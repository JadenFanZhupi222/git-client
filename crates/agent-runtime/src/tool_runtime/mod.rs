use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

use crate::ToolCall;

const HARD_MAX_RESULT_BYTES: usize = 1024 * 1024;
const TRUNCATION_MARKER: &str = "\n[tool result truncated]";

mod executor;
mod policy;
mod run_limits;
mod schema;

pub use executor::*;
pub use policy::*;
pub use run_limits::*;
pub use schema::*;

use policy::restrict_decision;
#[cfg(test)]
use schema::CompiledSchema;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct EchoHandler(Arc<AtomicUsize>);

    #[async_trait]
    impl ToolHandler for EchoHandler {
        async fn execute(
            &self,
            context: ToolExecutionContext,
            arguments: Value,
        ) -> Result<ToolHandlerOutput, ToolHandlerError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(ToolHandlerOutput::new(
                arguments["text"].as_str().unwrap_or_default(),
                ToolReceipt::Observation {
                    resource: context.call_id,
                    version_digest: "test-digest".into(),
                },
            ))
        }

        fn summarize_arguments(&self, _: &Value) -> Option<String> {
            Some("sanitized summary".into())
        }
    }

    fn definition() -> crate::ToolDefinition {
        crate::ToolDefinition {
            name: "test.echo".into(),
            description: "Echo validated text".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "minLength": 1, "maxLength": 40},
                    "count": {"type": "integer", "minimum": 1, "maximum": 3},
                    "tags": {"type": "array", "items": {"type": "string"}, "uniqueItems": true}
                },
                "required": ["text"],
                "additionalProperties": false
            }),
            risk: ToolRisk::ReadOnly,
            timeout_ms: 100,
            max_result_bytes: 64,
        }
    }

    fn allow_policy() -> PermissionPolicy {
        PermissionPolicy::new(vec![PermissionRule {
            matcher: ToolMatcher::Exact("test.echo".into()),
            risk: Some(ToolRisk::ReadOnly),
            decision: PermissionDecision::Allow,
        }])
    }

    #[test]
    fn tool_contracts_round_trip_with_first_class_call_id() {
        let call = ToolCall::with_call_id("test.echo", "call-1", json!({"text":"ok"}));
        let encoded = serde_json::to_string(&call).unwrap();
        assert!(!encoded.contains("_call_id"));
        assert_eq!(serde_json::from_str::<ToolCall>(&encoded).unwrap(), call);
        let result = ToolResult {
            call_id: "call-1".into(),
            name: "test.echo".into(),
            outcome: ToolOutcome::Success,
            content: "ok".into(),
            truncated: false,
            content_bytes: 2,
            receipt: None,
        };
        assert_eq!(
            serde_json::from_str::<ToolResult>(&serde_json::to_string(&result).unwrap()).unwrap(),
            result
        );
    }

    #[test]
    fn registry_rejects_duplicates_and_unsupported_schema_keywords() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::default();
        registry
            .register(definition(), Arc::new(EchoHandler(Arc::clone(&calls))))
            .unwrap();
        assert_eq!(
            registry
                .register(definition(), Arc::new(EchoHandler(calls)))
                .unwrap_err(),
            ToolRegistrationError::DuplicateName
        );
        let mut invalid = definition();
        invalid.name = "test.invalid".into();
        invalid.input_schema["$ref"] = json!("https://example.test/schema");
        assert_eq!(
            registry
                .register(
                    invalid,
                    Arc::new(EchoHandler(Arc::new(AtomicUsize::new(0))))
                )
                .unwrap_err(),
            ToolRegistrationError::InvalidSchema("unsupported_keyword")
        );
    }

    #[test]
    fn controlled_schema_profile_validates_composition_and_all_value_families() {
        let schema = CompiledSchema::compile(&json!({
            "type": "object",
            "properties": {
                "mode": {"enum": ["read", "write"]},
                "enabled": {"const": true},
                "target": {"oneOf": [
                    {"type": "string", "pattern": "^[a-z]+$", "minLength": 2, "maxLength": 8},
                    {"type": "integer", "minimum": 2, "exclusiveMaximum": 10, "multipleOf": 2}
                ]},
                "items": {"type": "array", "items": {"type": "boolean"}, "minItems": 1, "maxItems": 2, "uniqueItems": true},
                "none": {"type": "null"}
            },
            "required": ["mode", "enabled", "target", "items", "none"],
            "minProperties": 5,
            "maxProperties": 5,
            "additionalProperties": false
        })).unwrap();
        schema.validate(&json!({
            "mode": "read", "enabled": true, "target": "repo", "items": [true, false], "none": null
        })).unwrap();
        schema
            .validate(&json!({
                "mode": "write", "enabled": true, "target": 4, "items": [true], "none": null
            }))
            .unwrap();
        for invalid_value in [
            json!({"mode":"other", "enabled":true, "target":"repo", "items":[true], "none":null}),
            json!({"mode":"read", "enabled":false, "target":"repo", "items":[true], "none":null}),
            json!({"mode":"read", "enabled":true, "target":"R", "items":[true], "none":null}),
            json!({"mode":"read", "enabled":true, "target":3, "items":[true], "none":null}),
            json!({"mode":"read", "enabled":true, "target":"repo", "items":[true,true], "none":null}),
        ] {
            assert!(schema.validate(&invalid_value).is_err());
        }
    }

    #[test]
    fn policy_uses_first_match_and_run_restriction_never_broadens() {
        let policy = PermissionPolicy::new(vec![
            PermissionRule {
                matcher: ToolMatcher::Exact("filesystem.delete".into()),
                risk: None,
                decision: PermissionDecision::Deny,
            },
            PermissionRule {
                matcher: ToolMatcher::Prefix("filesystem.".into()),
                risk: None,
                decision: PermissionDecision::Allow,
            },
        ]);
        assert_eq!(
            policy.evaluate("filesystem.delete", ToolRisk::Destructive),
            PermissionDecision::Deny
        );
        assert_eq!(
            policy.evaluate("filesystem.read", ToolRisk::ReadOnly),
            PermissionDecision::Allow
        );
        assert_eq!(
            policy.evaluate("web.fetch", ToolRisk::External),
            PermissionDecision::Deny
        );
        assert_eq!(
            restrict_decision(PermissionDecision::Allow, Some(PermissionDecision::Ask)),
            PermissionDecision::Ask
        );
        assert_eq!(
            restrict_decision(PermissionDecision::Deny, Some(PermissionDecision::Allow)),
            PermissionDecision::Deny
        );
    }

    #[tokio::test]
    async fn invalid_input_never_reaches_handler_or_permission_execution() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::default();
        registry
            .register(definition(), Arc::new(EchoHandler(Arc::clone(&calls))))
            .unwrap();
        let executor = ToolExecutor::new(Arc::new(registry), allow_policy());
        let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel));
        let error = executor
            .execute(
                &run,
                ToolCall::with_call_id("test.echo", "bad", json!({"text":"ok", "secret":true})),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ToolExecutionError::InvalidInput {
                code: "additional_property",
                ..
            }
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn policy_defaults_to_deny_and_run_policy_cannot_broaden_it() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::default();
        registry
            .register(definition(), Arc::new(EchoHandler(Arc::clone(&calls))))
            .unwrap();
        let executor = ToolExecutor::new(Arc::new(registry), PermissionPolicy::default());
        let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel))
            .with_policy(allow_policy());
        let result = executor
            .execute(
                &run,
                ToolCall::with_call_id("test.echo", "denied", json!({"text":"ok"})),
            )
            .await
            .unwrap();
        assert_eq!(result.outcome, ToolOutcome::Denied);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    struct Approval(PermissionDecision);

    #[async_trait]
    impl ToolApprovalResolver for Approval {
        async fn resolve(&self, _: ToolApprovalRequest) -> PermissionDecision {
            self.0
        }
    }

    #[tokio::test]
    async fn ask_requires_one_shot_allow_before_handler_execution() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::default();
        registry
            .register(definition(), Arc::new(EchoHandler(Arc::clone(&calls))))
            .unwrap();
        let ask = PermissionPolicy::new(vec![PermissionRule {
            matcher: ToolMatcher::Prefix("test.".into()),
            risk: None,
            decision: PermissionDecision::Ask,
        }]);
        let executor = ToolExecutor::new(Arc::new(registry), ask)
            .with_approvals(Arc::new(Approval(PermissionDecision::Allow)));
        let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel));
        let result = executor
            .execute(
                &run,
                ToolCall::with_call_id("test.echo", "approved", json!({"text":"ok"})),
            )
            .await
            .unwrap();
        assert_eq!(result.outcome, ToolOutcome::Success);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    struct ToggleCancel(AtomicBool);

    #[async_trait]
    impl ToolCancellation for ToggleCancel {
        fn is_cancelled(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    struct SlowHandler;

    #[async_trait]
    impl ToolHandler for SlowHandler {
        async fn execute(
            &self,
            _: ToolExecutionContext,
            _: Value,
        ) -> Result<ToolHandlerOutput, ToolHandlerError> {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(ToolHandlerOutput::new(
                "late",
                ToolReceipt::Observation {
                    resource: "slow".into(),
                    version_digest: "test-digest".into(),
                },
            ))
        }
    }

    #[tokio::test]
    async fn timeout_and_budgets_fail_closed() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut slow = definition();
        slow.name = "test.slow".into();
        slow.timeout_ms = 5;
        let mut registry = ToolRegistry::default();
        registry.register(slow, Arc::new(SlowHandler)).unwrap();
        let policy = PermissionPolicy::new(vec![PermissionRule {
            matcher: ToolMatcher::Exact("test.slow".into()),
            risk: None,
            decision: PermissionDecision::Allow,
        }]);
        let executor = ToolExecutor::new(Arc::new(registry), policy).with_journal(Arc::new(
            RecordingJournal {
                order: Arc::clone(&order),
                fail_intent: false,
            },
        ));
        let run = ToolRun::new(
            "run",
            ToolRunLimits {
                max_tool_calls: 1,
                ..ToolRunLimits::default()
            },
            Arc::new(ToggleCancel(AtomicBool::new(false))),
        );
        assert_eq!(
            executor
                .execute(
                    &run,
                    ToolCall::with_call_id("test.slow", "one", json!({"text":"ok"}))
                )
                .await
                .unwrap_err(),
            ToolExecutionError::Timeout
        );
        assert_eq!(*order.lock().unwrap(), vec!["intent", "no_effect"]);
        assert_eq!(
            executor
                .execute(
                    &run,
                    ToolCall::with_call_id("test.slow", "two", json!({"text":"ok"}))
                )
                .await
                .unwrap_err(),
            ToolExecutionError::BudgetExceeded("tool_calls")
        );
        for _ in 0..run.limits.max_model_rounds {
            run.begin_model_round().unwrap();
        }
        assert_eq!(
            run.begin_model_round().unwrap_err(),
            ToolExecutionError::BudgetExceeded("model_rounds")
        );
    }

    #[tokio::test]
    async fn result_is_redacted_then_utf8_safely_truncated_and_counted() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut definition = definition();
        definition.max_result_bytes = 32;
        let mut registry = ToolRegistry::default();
        registry
            .register(definition, Arc::new(EchoHandler(calls)))
            .unwrap();
        let executor = ToolExecutor::new(Arc::new(registry), allow_policy())
            .with_secret_literals(vec!["super-secret".into()]);
        let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel));
        let result = executor
            .execute(
                &run,
                ToolCall::with_call_id(
                    "test.echo",
                    "redact",
                    json!({"text":"super-secret你好你好你好你好"}),
                ),
            )
            .await
            .unwrap();
        assert!(!result.content.contains("super-secret"));
        assert!(result.truncated);
        assert!(result.content_bytes <= 32);
        assert!(std::str::from_utf8(result.content.as_bytes()).is_ok());
    }

    struct OrderedHandler(Arc<Mutex<Vec<&'static str>>>);

    #[async_trait]
    impl ToolHandler for OrderedHandler {
        async fn execute(
            &self,
            context: ToolExecutionContext,
            _: Value,
        ) -> Result<ToolHandlerOutput, ToolHandlerError> {
            self.0.lock().unwrap().push("effect");
            Ok(ToolHandlerOutput::new(
                "safe",
                ToolReceipt::Observation {
                    resource: context.call_id,
                    version_digest: "digest".into(),
                },
            ))
        }
    }

    struct RecordingJournal {
        order: Arc<Mutex<Vec<&'static str>>>,
        fail_intent: bool,
    }

    impl ToolIntentJournal for RecordingJournal {
        fn record_intent(&self, _: &ToolIntent) -> Result<(), ToolExecutionError> {
            self.order.lock().unwrap().push("intent");
            if self.fail_intent {
                Err(ToolExecutionError::IntentPersistence)
            } else {
                Ok(())
            }
        }

        fn record_receipt(
            &self,
            _: &ToolIntent,
            _: &ToolReceipt,
        ) -> Result<(), ToolExecutionError> {
            self.order.lock().unwrap().push("receipt");
            Ok(())
        }

        fn record_no_effect(
            &self,
            _: &ToolIntent,
            _: ToolOutcome,
        ) -> Result<(), ToolExecutionError> {
            self.order.lock().unwrap().push("no_effect");
            Ok(())
        }
    }

    struct FailingHandler;

    #[async_trait]
    impl ToolHandler for FailingHandler {
        async fn execute(
            &self,
            _: ToolExecutionContext,
            _: Value,
        ) -> Result<ToolHandlerOutput, ToolHandlerError> {
            Err(ToolHandlerError)
        }
    }

    struct RecoverableFailingHandler;

    #[async_trait]
    impl ToolHandler for RecoverableFailingHandler {
        async fn execute(
            &self,
            _: ToolExecutionContext,
            _: Value,
        ) -> Result<ToolHandlerOutput, ToolHandlerError> {
            Err(ToolHandlerError::sanitized(
                r#"{"error":"bounded_failure","actual_bytes":42}"#,
            ))
        }
    }

    #[tokio::test]
    async fn read_only_failure_resolves_intent_without_effect() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut registry = ToolRegistry::default();
        registry
            .register(definition(), Arc::new(FailingHandler))
            .unwrap();
        let executor = ToolExecutor::new(Arc::new(registry), allow_policy()).with_journal(
            Arc::new(RecordingJournal {
                order: Arc::clone(&order),
                fail_intent: false,
            }),
        );
        let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel));
        let result = executor
            .execute(
                &run,
                ToolCall::with_call_id("test.echo", "failed-read", json!({"text":"ok"})),
            )
            .await
            .unwrap();
        assert_eq!(result.outcome, ToolOutcome::Failed);
        assert_eq!(*order.lock().unwrap(), vec!["intent", "no_effect"]);
    }

    #[tokio::test]
    async fn trusted_handler_failure_returns_sanitized_recovery_content() {
        let mut registry = ToolRegistry::default();
        registry
            .register(definition(), Arc::new(RecoverableFailingHandler))
            .unwrap();
        let executor = ToolExecutor::new(Arc::new(registry), allow_policy());
        let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel));
        let result = executor
            .execute(
                &run,
                ToolCall::with_call_id("test.echo", "recoverable", json!({"text":"ok"})),
            )
            .await
            .unwrap();
        assert_eq!(result.outcome, ToolOutcome::Failed);
        assert_eq!(
            result.content,
            r#"{"error":"bounded_failure","actual_bytes":42}"#
        );
    }

    #[tokio::test]
    async fn read_only_cancellation_resolves_intent_without_effect() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut slow = definition();
        slow.name = "test.slow-cancel".into();
        slow.timeout_ms = 2_000;
        let mut registry = ToolRegistry::default();
        registry.register(slow, Arc::new(SlowHandler)).unwrap();
        let policy = PermissionPolicy::new(vec![PermissionRule {
            matcher: ToolMatcher::Exact("test.slow-cancel".into()),
            risk: Some(ToolRisk::ReadOnly),
            decision: PermissionDecision::Allow,
        }]);
        let executor = ToolExecutor::new(Arc::new(registry), policy).with_journal(Arc::new(
            RecordingJournal {
                order: Arc::clone(&order),
                fail_intent: false,
            },
        ));
        let cancellation = Arc::new(ToggleCancel(AtomicBool::new(false)));
        let trigger = Arc::clone(&cancellation);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            trigger.0.store(true, Ordering::SeqCst);
        });
        let run = ToolRun::new("run", ToolRunLimits::default(), cancellation);
        let error = executor
            .execute(
                &run,
                ToolCall::with_call_id("test.slow-cancel", "cancelled-read", json!({"text":"ok"})),
            )
            .await
            .unwrap_err();
        assert_eq!(error, ToolExecutionError::Cancelled);
        assert_eq!(*order.lock().unwrap(), vec!["intent", "no_effect"]);
    }

    #[tokio::test]
    async fn mutation_timeout_preserves_intent_for_ambiguous_recovery() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut slow = definition();
        slow.name = "test.write-slow".into();
        slow.risk = ToolRisk::Write;
        slow.timeout_ms = 5;
        let mut registry = ToolRegistry::default();
        registry.register(slow, Arc::new(SlowHandler)).unwrap();
        let policy = PermissionPolicy::new(vec![PermissionRule {
            matcher: ToolMatcher::Exact("test.write-slow".into()),
            risk: Some(ToolRisk::Write),
            decision: PermissionDecision::Allow,
        }]);
        let executor = ToolExecutor::new(Arc::new(registry), policy).with_journal(Arc::new(
            RecordingJournal {
                order: Arc::clone(&order),
                fail_intent: false,
            },
        ));
        let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel));
        assert_eq!(
            executor
                .execute(
                    &run,
                    ToolCall::with_call_id("test.write-slow", "write", json!({"text":"ok"})),
                )
                .await
                .unwrap_err(),
            ToolExecutionError::Timeout
        );
        assert_eq!(*order.lock().unwrap(), vec!["intent"]);
    }

    #[tokio::test]
    async fn durable_intent_precedes_effect_and_receipt_and_failed_intent_prevents_execution() {
        for fail_intent in [false, true] {
            let order = Arc::new(Mutex::new(Vec::new()));
            let mut registry = ToolRegistry::default();
            registry
                .register(definition(), Arc::new(OrderedHandler(Arc::clone(&order))))
                .unwrap();
            let executor = ToolExecutor::new(Arc::new(registry), allow_policy()).with_journal(
                Arc::new(RecordingJournal {
                    order: Arc::clone(&order),
                    fail_intent,
                }),
            );
            let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel));
            let result = executor
                .execute(
                    &run,
                    ToolCall::with_call_id("test.echo", "ordered", json!({"text":"ok"})),
                )
                .await;
            if fail_intent {
                assert_eq!(result.unwrap_err(), ToolExecutionError::IntentPersistence);
                assert_eq!(*order.lock().unwrap(), vec!["intent"]);
            } else {
                assert_eq!(result.unwrap().outcome, ToolOutcome::Success);
                assert_eq!(*order.lock().unwrap(), vec!["intent", "effect", "receipt"]);
            }
        }
    }
}
