use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use agent_runtime::{
    DenyAllApprovals, ModelOutput, ModelProvider, ModelRequest, ModelResponse, ModelUsage,
    NeverCancel, NoopAgentEventSink, ProviderCapabilities, ProviderDescriptor, ProviderError,
    StructuredOutputSupport, ToolCall, ToolCallingSupport, ToolRunLimits, TranscriptItem,
    UsageSupport,
};
use agent_session::{
    compact_working_set, verify_completion_candidate, AgentBudgetAccount, AgentCompletionCandidate,
    AgentGoal, AgentGoalStatus, AgentSession, AgentSliceBoundary, AgentSliceOutcome,
    AgentSliceRequest, AgentTurnRequest, ContextLimits, ContextPlanner, ModelBudgetLimit,
    ModelRequestBudget, PauseReason, ProgressTracker, SessionEngine, SessionEngineConfig,
    SessionMessage, SessionRole, SessionStore, SessionStoreLimits, VerificationDecision,
};
use agent_tools::{build_builtin_tool_pack, BuiltinToolConfig};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Serialize)]
struct EvaluationReport {
    context: ContextMetrics,
    budget_preflight: BudgetPreflightMetrics,
    bounded_filesystem: FilesystemMetrics,
    no_progress: NoProgressMetrics,
    compaction: ModelServiceMetrics,
    verifier_repair: ModelServiceMetrics,
    restart_recovery: RestartMetrics,
}

#[derive(Debug, Serialize)]
struct ContextMetrics {
    original_history_messages: usize,
    planned_transcript_items: usize,
    estimated_input_tokens: u64,
    input_budget_tokens: u64,
}

#[derive(Debug, Serialize)]
struct BudgetPreflightMetrics {
    provider_requests: usize,
    model_rounds: u32,
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Debug, Serialize)]
struct FilesystemMetrics {
    provider_requests: usize,
    tool_calls: u32,
    full_file_bytes: usize,
    returned_content_bytes: usize,
    returned_lines: usize,
}

#[derive(Debug, Serialize)]
struct NoProgressMetrics {
    provider_requests: usize,
    tool_calls: u32,
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    final_request_tools_disabled: bool,
}

#[derive(Debug, Serialize)]
struct ModelServiceMetrics {
    provider_requests: usize,
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
}

#[derive(Debug, Serialize)]
struct RestartMetrics {
    provider_requests: usize,
    retained_input_tokens: u64,
    revision_delta: u64,
}

struct FixtureProvider {
    responses: Mutex<VecDeque<Result<ModelResponse, ProviderError>>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl FixtureProvider {
    fn new(responses: impl IntoIterator<Item = ModelResponse>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into_iter().map(Ok).collect()),
            requests: Mutex::new(Vec::new()),
        })
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

#[async_trait]
impl ModelProvider for FixtureProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        fixture_descriptor(32_000)
    }

    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, ProviderError> {
        self.requests.lock().unwrap().push(request.clone());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(ProviderError::InvalidResponse("fixture exhausted".into())))
    }
}

fn fixture_descriptor(context_window_tokens: u64) -> ProviderDescriptor {
    ProviderDescriptor {
        provider_id: "fixture".into(),
        model_id: "fixture-model".into(),
        capabilities: ProviderCapabilities {
            structured_output: StructuredOutputSupport::JsonSchema,
            tool_calling: ToolCallingSupport::Serial,
            can_disable_tools: true,
            requires_reasoning_replay: false,
            context_window_tokens,
            max_output_tokens: 2_048,
            usage: UsageSupport::InputOutputTokens,
        },
    }
}

fn response(output: ModelOutput, input: u64, cached: u64, output_tokens: u64) -> ModelResponse {
    ModelResponse {
        output,
        usage: ModelUsage {
            input_tokens: input,
            cached_input_tokens: cached,
            output_tokens,
            tool_calls: 0,
        },
    }
}

fn engine_config() -> SessionEngineConfig {
    SessionEngineConfig {
        context: ContextLimits {
            explicit_context_tokens: Some(32_000),
            safety_margin_tokens: 256,
            reserved_output_tokens: 512,
            ..ContextLimits::default()
        },
        tool_run: ToolRunLimits {
            max_model_rounds: 6,
            max_tool_calls: 8,
            ..ToolRunLimits::default()
        },
        ..SessionEngineConfig::default()
    }
}

fn sessions(id: &str) -> Arc<SessionStore> {
    let sessions = Arc::new(SessionStore::new(SessionStoreLimits::default()).unwrap());
    sessions
        .create(id, "Work only inside the configured repository.")
        .unwrap();
    sessions
}

fn token_budget(limit_tokens: u64) -> AgentBudgetAccount {
    AgentBudgetAccount::new(
        "fixture-model",
        None,
        ModelBudgetLimit::Tokens { limit_tokens },
    )
    .unwrap()
}

fn goal_with_candidate() -> AgentGoal {
    AgentGoal {
        goal_id: "eval-goal".into(),
        session_id: "eval-session".into(),
        objective: "Verify the evaluated result".into(),
        model_id: "fixture-model".into(),
        repository_identity: "eval-repository".into(),
        created_at_ms: 1,
        updated_at_ms: 1,
        revision: 1,
        status: AgentGoalStatus::Running,
        pause_reason: None,
        block_reason: None,
        usage_by_model: BTreeMap::from([("fixture-model".into(), token_budget(100_000))]),
        steering_messages: Vec::new(),
        checkpoint: agent_session::AgentCheckpoint::empty("digest", 1),
        completion_candidate: Some(AgentCompletionCandidate {
            text: "The evaluated work is complete.".into(),
            remaining_work: Vec::new(),
            created_at_ms: 1,
            model_responses: 2,
            used_tools: true,
            verification: None,
        }),
        result: None,
    }
}

fn evaluate_context() -> ContextMetrics {
    let original_history_messages = 16;
    let session = AgentSession {
        session_id: "context-eval".into(),
        revision: 8,
        system_instruction: "Keep repository evidence bounded.".into(),
        memory_summary: None,
        recent_messages: (0..original_history_messages)
            .map(|index| SessionMessage {
                role: if index % 2 == 0 {
                    SessionRole::User
                } else {
                    SessionRole::Assistant
                },
                content: format!("history-{index}:{}", "x".repeat(2_000)),
            })
            .collect(),
    };
    let input_budget_tokens = 8_000 - 512 - 512;
    let planner = ContextPlanner::new(ContextLimits {
        explicit_context_tokens: Some(8_000),
        safety_margin_tokens: 512,
        reserved_output_tokens: 512,
        max_compacted_memory_bytes: 2_048,
        ..ContextLimits::default()
    })
    .unwrap();
    let planned = planner
        .plan(
            &fixture_descriptor(8_000),
            &session,
            &[TranscriptItem::User(
                "Summarize the bounded evidence.".into(),
            )],
            Vec::new(),
            &[],
            None,
            512,
        )
        .unwrap();

    assert!(planned.estimated_input_tokens <= input_budget_tokens);
    assert!(planned.transcript.len() < original_history_messages + 2);
    assert!(planned.transcript.iter().any(|item| matches!(
        item,
        TranscriptItem::System(text) if text.contains("<memory-data>")
    )));

    ContextMetrics {
        original_history_messages,
        planned_transcript_items: planned.transcript.len(),
        estimated_input_tokens: planned.estimated_input_tokens,
        input_budget_tokens,
    }
}

async fn evaluate_budget_preflight() -> BudgetPreflightMetrics {
    let workspace = tempfile::tempdir().unwrap();
    let artifacts = tempfile::tempdir().unwrap();
    let pack = build_builtin_tool_pack(BuiltinToolConfig::local_only(
        workspace.path().into(),
        artifacts.path().into(),
    ))
    .unwrap();
    let provider = FixtureProvider::new([response(
        ModelOutput::FinalText {
            text: "must not be requested".into(),
        },
        10,
        0,
        2,
    )]);
    let engine = SessionEngine::new(
        provider.clone(),
        sessions("budget-eval"),
        pack.registry,
        pack.policy,
        Arc::new(DenyAllApprovals),
        Arc::new(NoopAgentEventSink),
        engine_config(),
    )
    .unwrap();
    let mut turn = AgentTurnRequest::text("budget-eval", "budget-run", "Inspect the tree.", 256);
    turn.request_budget = Some(ModelRequestBudget::Tokens {
        remaining_tokens: 0,
        input_safety_percent: 100,
    });
    let outcome = engine
        .run_goal_slice(
            AgentSliceRequest {
                turn,
                resume_transcript: Vec::new(),
                working_summary: None,
                progress: ProgressTracker::default(),
                slice_index: 0,
                execution_sequence: 0,
            },
            Arc::new(NeverCancel),
        )
        .await
        .unwrap();
    let AgentSliceOutcome::Checkpoint(checkpoint) = outcome else {
        panic!("a zero request budget must checkpoint before provider I/O");
    };
    assert_eq!(checkpoint.boundary, AgentSliceBoundary::Budget);
    assert_eq!(provider.request_count(), 0);
    assert_eq!(checkpoint.model_rounds, 0);
    assert_eq!(checkpoint.usage, ModelUsage::default());

    BudgetPreflightMetrics {
        provider_requests: provider.request_count(),
        model_rounds: checkpoint.model_rounds,
        input_tokens: checkpoint.usage.input_tokens,
        output_tokens: checkpoint.usage.output_tokens,
    }
}

async fn evaluate_bounded_filesystem() -> FilesystemMetrics {
    let workspace = tempfile::tempdir().unwrap();
    let artifacts = tempfile::tempdir().unwrap();
    let source = (1..=1_000)
        .map(|line| format!("pub const VALUE_{line}: usize = {line};\n"))
        .collect::<String>();
    let full_file_bytes = source.len();
    std::fs::write(workspace.path().join("large.rs"), source).unwrap();
    let pack = build_builtin_tool_pack(BuiltinToolConfig::local_only(
        workspace.path().into(),
        artifacts.path().into(),
    ))
    .unwrap();
    let provider = FixtureProvider::new([
        response(
            ModelOutput::ToolCalls {
                calls: vec![ToolCall::with_call_id(
                    "filesystem.list",
                    "list-root",
                    json!({"path":"."}),
                )],
            },
            40,
            0,
            5,
        ),
        response(
            ModelOutput::ToolCalls {
                calls: vec![ToolCall::with_call_id(
                    "filesystem.read",
                    "read-window",
                    json!({
                        "path":"large.rs",
                        "start_line":1,
                        "line_count":20,
                        "max_bytes":2048
                    }),
                )],
            },
            50,
            10,
            5,
        ),
        response(
            ModelOutput::FinalText {
                text: "Inspected a bounded source window.".into(),
            },
            60,
            20,
            10,
        ),
    ]);
    let engine = SessionEngine::new(
        provider.clone(),
        sessions("filesystem-eval"),
        pack.registry,
        pack.policy,
        Arc::new(DenyAllApprovals),
        Arc::new(NoopAgentEventSink),
        engine_config(),
    )
    .unwrap();
    let result = engine
        .run_turn(
            AgentTurnRequest::text(
                "filesystem-eval",
                "filesystem-run",
                "Inspect a bounded source window.",
                256,
            ),
            Arc::new(NeverCancel),
        )
        .await
        .unwrap();
    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[1].transcript.iter().any(|item| matches!(
        item,
        TranscriptItem::ToolResult { name, content, .. }
            if name == "filesystem.list" && content.contains("large.rs")
    )));
    let read_content = requests[2]
        .transcript
        .iter()
        .find_map(|item| match item {
            TranscriptItem::ToolResult { name, content, .. } if name == "filesystem.read" => {
                Some(content)
            }
            _ => None,
        })
        .expect("bounded read result");
    let (metadata, returned_content) = read_content.split_once('\n').unwrap();
    let window: serde_json::Value = serde_json::from_str(metadata).unwrap();
    let returned_lines =
        window["end_line"].as_u64().unwrap() - window["start_line"].as_u64().unwrap() + 1;
    assert_eq!(returned_lines, 20);
    assert!(returned_content.len() <= 2_048);
    assert!(returned_content.len() < full_file_bytes);
    assert_eq!(result.usage.tool_calls, 2);

    FilesystemMetrics {
        provider_requests: requests.len(),
        tool_calls: result.usage.tool_calls,
        full_file_bytes,
        returned_content_bytes: returned_content.len(),
        returned_lines: usize::try_from(returned_lines).unwrap(),
    }
}

async fn evaluate_no_progress() -> NoProgressMetrics {
    let workspace = tempfile::tempdir().unwrap();
    let artifacts = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("evidence.txt"), "stable evidence").unwrap();
    let pack = build_builtin_tool_pack(BuiltinToolConfig::local_only(
        workspace.path().into(),
        artifacts.path().into(),
    ))
    .unwrap();
    let mut responses = (0..3)
        .map(|index| {
            response(
                ModelOutput::ToolCalls {
                    calls: vec![ToolCall::with_call_id(
                        "filesystem.read",
                        format!("read-{index}"),
                        json!({"path":"evidence.txt"}),
                    )],
                },
                100,
                40,
                10,
            )
        })
        .collect::<Vec<_>>();
    responses.push(response(
        ModelOutput::FinalText {
            text: "No additional evidence is available.".into(),
        },
        150,
        60,
        20,
    ));
    let provider = FixtureProvider::new(responses);
    let engine = SessionEngine::new(
        provider.clone(),
        sessions("no-progress-eval"),
        pack.registry,
        pack.policy,
        Arc::new(DenyAllApprovals),
        Arc::new(NoopAgentEventSink),
        engine_config(),
    )
    .unwrap();
    let result = engine
        .run_turn(
            AgentTurnRequest::text(
                "no-progress-eval",
                "no-progress-run",
                "Find more evidence.",
                256,
            ),
            Arc::new(NeverCancel),
        )
        .await
        .unwrap();
    let requests = provider.requests.lock().unwrap();
    let final_request_tools_disabled = requests.last().is_some_and(|request| {
        request.tools.is_empty()
            && request.transcript.iter().any(|item| {
                matches!(
                    item,
                    TranscriptItem::System(text)
                        if text.contains("repeated without producing new evidence")
                )
            })
    });
    assert_eq!(requests.len(), 4);
    assert_eq!(result.usage.tool_calls, 3);
    assert_eq!(result.usage.input_tokens, 450);
    assert_eq!(result.usage.cached_input_tokens, 180);
    assert_eq!(result.usage.output_tokens, 50);
    assert!(final_request_tools_disabled);

    NoProgressMetrics {
        provider_requests: requests.len(),
        tool_calls: result.usage.tool_calls,
        input_tokens: result.usage.input_tokens,
        cached_input_tokens: result.usage.cached_input_tokens,
        output_tokens: result.usage.output_tokens,
        final_request_tools_disabled,
    }
}

fn compaction_transcript() -> Vec<TranscriptItem> {
    (0..3)
        .flat_map(|index| {
            [
                TranscriptItem::AssistantToolCalls(vec![ToolCall::with_call_id(
                    "filesystem.read",
                    format!("compact-{index}"),
                    json!({"path":"README.md"}),
                )]),
                TranscriptItem::ToolResult {
                    name: "filesystem.read".into(),
                    call_id: format!("compact-{index}"),
                    content: format!("evidence-{index}"),
                    counts_toward_budget: true,
                },
            ]
        })
        .collect()
}

async fn evaluate_compaction() -> ModelServiceMetrics {
    let provider = FixtureProvider::new([response(
        ModelOutput::FinalText {
            text: r#"{"working_summary":"bounded facts","next_actions":["finish"]}"#.into(),
        },
        90,
        30,
        20,
    )]);
    let attempt = compact_working_set(
        provider.clone(),
        "",
        &compaction_transcript(),
        &ModelRequestBudget::Tokens {
            remaining_tokens: 100_000,
            input_safety_percent: 100,
        },
    )
    .await
    .unwrap();
    let output = attempt.output.expect("valid compaction output");
    assert_eq!(provider.request_count(), 1);
    assert_eq!(output.recent_transcript.len(), 4);
    assert_eq!(output.summary, "bounded facts");
    assert_eq!(attempt.usage.input_tokens, 90);
    assert_eq!(attempt.usage.cached_input_tokens, 30);
    assert_eq!(attempt.usage.output_tokens, 20);

    ModelServiceMetrics {
        provider_requests: provider.request_count(),
        input_tokens: attempt.usage.input_tokens,
        cached_input_tokens: attempt.usage.cached_input_tokens,
        output_tokens: attempt.usage.output_tokens,
    }
}

async fn evaluate_verifier_repair() -> ModelServiceMetrics {
    let provider = FixtureProvider::new([
        response(
            ModelOutput::FinalText {
                text: "not json".into(),
            },
            10,
            0,
            2,
        ),
        response(
            ModelOutput::FinalText {
                text: r#"{"decision":"accepted","gaps":[],"evidence_ids":["e1"]}"#.into(),
            },
            11,
            3,
            4,
        ),
    ]);
    let verified = verify_completion_candidate(provider.clone(), &goal_with_candidate())
        .await
        .unwrap();
    assert_eq!(provider.request_count(), 2);
    assert_eq!(verified.result.decision, VerificationDecision::Accepted);
    assert_eq!(verified.usage.input_tokens, 21);
    assert_eq!(verified.usage.cached_input_tokens, 3);
    assert_eq!(verified.usage.output_tokens, 6);

    ModelServiceMetrics {
        provider_requests: provider.request_count(),
        input_tokens: verified.usage.input_tokens,
        cached_input_tokens: verified.usage.cached_input_tokens,
        output_tokens: verified.usage.output_tokens,
    }
}

fn evaluate_restart_recovery() -> RestartMetrics {
    let provider = FixtureProvider::new([]);
    let mut goal = goal_with_candidate();
    goal.completion_candidate = None;
    goal.active_budget_mut()
        .unwrap()
        .record_usage(&ModelUsage {
            input_tokens: 123,
            cached_input_tokens: 45,
            output_tokens: 12,
            tool_calls: 1,
        })
        .unwrap();
    let revision_before = goal.revision;
    goal.prepare_for_restart(2);

    assert_eq!(goal.status, AgentGoalStatus::Paused);
    assert_eq!(goal.pause_reason, Some(PauseReason::AppRestarted));
    assert_eq!(provider.request_count(), 0);
    assert_eq!(goal.active_budget().unwrap().usage.input_tokens, 123);

    RestartMetrics {
        provider_requests: provider.request_count(),
        retained_input_tokens: goal.active_budget().unwrap().usage.input_tokens,
        revision_delta: goal.revision - revision_before,
    }
}

#[tokio::test]
async fn provider_neutral_agent_evaluation_baseline() {
    let report = EvaluationReport {
        context: evaluate_context(),
        budget_preflight: evaluate_budget_preflight().await,
        bounded_filesystem: evaluate_bounded_filesystem().await,
        no_progress: evaluate_no_progress().await,
        compaction: evaluate_compaction().await,
        verifier_repair: evaluate_verifier_repair().await,
        restart_recovery: evaluate_restart_recovery(),
    };

    println!(
        "AGENT_EVAL_REPORT={}",
        serde_json::to_string(&report).unwrap()
    );
}
