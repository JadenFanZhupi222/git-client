use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use agent_runtime::{
    AgentEvent, AgentEventSink, ModelOutput, ModelProvider, ModelRequest, ModelResponse,
    ModelUsage, NeverCancel, PermissionDecision, ProviderCapabilities, ProviderDescriptor,
    ProviderError, StructuredOutputSupport, ToolApprovalRequest, ToolApprovalResolver, ToolCall,
    ToolCallingSupport, ToolRunLimits, UsageSupport,
};
use agent_session::{
    AgentTurnRequest, ContextLimits, SessionEngine, SessionEngineConfig, SessionStore,
    SessionStoreLimits,
};
use agent_tools::{build_builtin_tool_pack, BuiltinToolConfig};
use async_trait::async_trait;
use serde_json::json;

struct FixtureProvider {
    responses: Mutex<VecDeque<ModelResponse>>,
    requests: Mutex<Vec<ModelRequest>>,
}

#[async_trait]
impl ModelProvider for FixtureProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: "fixture".into(),
            model_id: "fixture-model".into(),
            capabilities: ProviderCapabilities {
                structured_output: StructuredOutputSupport::None,
                tool_calling: ToolCallingSupport::Serial,
                can_disable_tools: true,
                requires_reasoning_replay: false,
                context_window_tokens: 32_000,
                max_output_tokens: 2_048,
                usage: UsageSupport::InputOutputTokens,
            },
        }
    }

    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, ProviderError> {
        self.requests.lock().unwrap().push(request.clone());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::InvalidResponse("fixture exhausted".into()))
    }
}

#[derive(Default)]
struct AllowAndRecord(Mutex<Vec<ToolApprovalRequest>>);

#[async_trait]
impl ToolApprovalResolver for AllowAndRecord {
    async fn resolve(&self, request: ToolApprovalRequest) -> PermissionDecision {
        self.0.lock().unwrap().push(request);
        PermissionDecision::Allow
    }
}

#[derive(Default)]
struct RecordingEvents(Mutex<Vec<AgentEvent>>);

impl AgentEventSink for RecordingEvents {
    fn emit(&self, event: AgentEvent) {
        self.0.lock().unwrap().push(event);
    }
}

fn model_response(output: ModelOutput) -> ModelResponse {
    ModelResponse {
        output,
        usage: ModelUsage {
            input_tokens: 20,
            cached_input_tokens: 0,
            output_tokens: 5,
            tool_calls: 0,
        },
    }
}

#[tokio::test]
async fn real_s4_tools_run_through_s3_contract_without_persisting_tool_content() {
    let workspace = tempfile::tempdir().unwrap();
    let artifacts = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("note.txt"), "before private evidence").unwrap();
    let pack = build_builtin_tool_pack(BuiltinToolConfig::local_only(
        workspace.path().into(),
        artifacts.path().into(),
    ))
    .unwrap();
    let provider = Arc::new(FixtureProvider {
        responses: Mutex::new(
            vec![
                model_response(ModelOutput::ToolCalls {
                    calls: vec![ToolCall::with_call_id(
                        "filesystem.read",
                        "read-call",
                        json!({"path":"note.txt"}),
                    )],
                }),
                model_response(ModelOutput::ToolCalls {
                    calls: vec![ToolCall::with_call_id(
                        "patch.apply",
                        "patch-call",
                        json!({
                            "path":"note.txt",
                            "expected":"before private evidence",
                            "replacement":"after"
                        }),
                    )],
                }),
                model_response(ModelOutput::FinalText {
                    text: "Updated note.txt.".into(),
                }),
            ]
            .into(),
        ),
        requests: Mutex::new(Vec::new()),
    });
    let sessions = Arc::new(SessionStore::new(SessionStoreLimits::default()).unwrap());
    sessions
        .create("session", "Work only inside the configured repository.")
        .unwrap();
    let approvals = Arc::new(AllowAndRecord::default());
    let events = Arc::new(RecordingEvents::default());
    let engine = SessionEngine::new(
        provider.clone(),
        sessions.clone(),
        pack.registry,
        pack.policy,
        approvals.clone(),
        events.clone(),
        SessionEngineConfig {
            context: ContextLimits {
                explicit_context_tokens: Some(32_000),
                safety_margin_tokens: 256,
                reserved_output_tokens: 512,
                ..ContextLimits::default()
            },
            tool_run: ToolRunLimits {
                max_model_rounds: 4,
                max_tool_calls: 4,
                ..ToolRunLimits::default()
            },
            max_final_bytes: 4_096,
            ..SessionEngineConfig::default()
        },
    )
    .unwrap();

    let result = engine
        .run_turn(
            AgentTurnRequest::text("session", "run", "Update the note.", 512),
            Arc::new(NeverCancel),
        )
        .await
        .unwrap();

    assert_eq!(result.final_text, "Updated note.txt.");
    assert_eq!(result.usage.tool_calls, 2);
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("note.txt")).unwrap(),
        "after"
    );
    let approvals = approvals.0.lock().unwrap();
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0].tool_name, "patch.apply");

    let requests = provider.requests.lock().unwrap();
    assert!(requests[1].transcript.iter().any(|item| matches!(
        item,
        agent_runtime::TranscriptItem::ToolResult { content, .. }
            if content == "before private evidence"
    )));
    let session_json = serde_json::to_string(&sessions.get("session").unwrap()).unwrap();
    assert!(!session_json.contains("private evidence"));
    assert!(!session_json.contains("patch-call"));
    let event_json = serde_json::to_string(&*events.0.lock().unwrap()).unwrap();
    assert!(!event_json.contains("private evidence"));
    assert!(!event_json.contains("before"));
}
