use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use agent_runtime::{
    redact_sensitive_text, AgentErrorCode, AgentEventClock, AgentEventEmitter, AgentEventKind,
    AgentEventSink, ModelOutput, ModelProvider, ModelRequest, ModelResponse, ModelUsage,
    PermissionPolicy, ProviderCapabilities, ResponseFormat, RetryPolicy, ToolApprovalResolver,
    ToolCall, ToolCancellation, ToolExecutionError, ToolExecutionEvent, ToolExecutionEventSink,
    ToolExecutor, ToolRegistry, ToolResult, ToolRun, ToolRunLimits, TranscriptItem,
};
use serde_json::Value;
use thiserror::Error;

use crate::context::{ContextError, ContextLimits, ContextPlanner};
use crate::rag::{NoopRagRetriever, RagError, RagRetriever};
use crate::session::{SessionError, SessionLease, SessionStore};

#[derive(Debug, Clone)]
pub struct SessionEngineConfig {
    pub context: ContextLimits,
    pub tool_run: ToolRunLimits,
    pub retry: RetryPolicy,
    pub max_total_input_tokens: u64,
    pub max_total_output_tokens: u64,
    pub max_user_bytes: usize,
    pub max_final_bytes: usize,
    pub max_run_duration: Duration,
}

impl Default for SessionEngineConfig {
    fn default() -> Self {
        Self {
            context: ContextLimits::default(),
            tool_run: ToolRunLimits::default(),
            retry: RetryPolicy::default(),
            max_total_input_tokens: 500_000,
            max_total_output_tokens: 100_000,
            max_user_bytes: 64 * 1024,
            max_final_bytes: 64 * 1024,
            max_run_duration: Duration::from_secs(5 * 60),
        }
    }
}

pub struct AgentTurnRequest {
    pub session_id: String,
    pub run_id: String,
    pub user_input: String,
    pub response_format: ResponseFormat,
    pub response_schema: Option<Value>,
    pub max_output_tokens: u32,
    pub run_policy: Option<PermissionPolicy>,
}

impl AgentTurnRequest {
    pub fn text(
        session_id: impl Into<String>,
        run_id: impl Into<String>,
        user_input: impl Into<String>,
        max_output_tokens: u32,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            run_id: run_id.into(),
            user_input: user_input.into(),
            response_format: ResponseFormat::Text,
            response_schema: None,
            max_output_tokens,
            run_policy: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTurnResult {
    pub session_id: String,
    pub run_id: String,
    pub revision: u64,
    pub final_text: String,
    pub usage: ModelUsage,
    pub model_rounds: u32,
    pub retrieval_count: usize,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SessionEngineError {
    #[error("session operation failed")]
    Session(SessionError),
    #[error("invalid turn input")]
    InvalidInput,
    #[error("agent run was cancelled")]
    Cancelled,
    #[error("agent run timed out")]
    Timeout,
    #[error("provider failed: {0:?}")]
    Provider(AgentErrorCode),
    #[error("context planning failed")]
    Context(ContextError),
    #[error("retrieval failed")]
    Retrieval,
    #[error("agent budget exceeded: {0}")]
    Budget(&'static str),
    #[error("invalid tool call: {0}")]
    InvalidToolCall(&'static str),
    #[error("tool execution failed: {0}")]
    Tool(&'static str),
    #[error("agent loop exhausted")]
    LoopExhausted,
    #[error("final response is invalid")]
    InvalidFinal,
}

impl From<SessionError> for SessionEngineError {
    fn from(value: SessionError) -> Self {
        Self::Session(value)
    }
}

impl From<ContextError> for SessionEngineError {
    fn from(value: ContextError) -> Self {
        Self::Context(value)
    }
}

pub struct SessionEngine {
    provider: Arc<dyn ModelProvider>,
    sessions: Arc<SessionStore>,
    registry: Arc<ToolRegistry>,
    policy: PermissionPolicy,
    approvals: Arc<dyn ToolApprovalResolver>,
    events: Arc<dyn AgentEventSink>,
    retriever: Arc<dyn RagRetriever>,
    planner: ContextPlanner,
    config: SessionEngineConfig,
    secret_literals: Vec<String>,
}

impl SessionEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        sessions: Arc<SessionStore>,
        registry: Arc<ToolRegistry>,
        policy: PermissionPolicy,
        approvals: Arc<dyn ToolApprovalResolver>,
        events: Arc<dyn AgentEventSink>,
        config: SessionEngineConfig,
    ) -> Result<Self, SessionEngineError> {
        if config.tool_run.max_model_rounds == 0
            || config.tool_run.max_tool_calls == 0
            || config.retry.max_attempts == 0
            || config.max_total_input_tokens == 0
            || config.max_total_output_tokens == 0
            || config.max_user_bytes == 0
            || config.max_final_bytes == 0
            || config.max_run_duration.is_zero()
        {
            return Err(SessionEngineError::InvalidInput);
        }
        let planner = ContextPlanner::new(config.context.clone())?;
        Ok(Self {
            provider,
            sessions,
            registry,
            policy,
            approvals,
            events,
            retriever: Arc::new(NoopRagRetriever),
            planner,
            config,
            secret_literals: Vec::new(),
        })
    }

    pub fn with_retriever(mut self, retriever: Arc<dyn RagRetriever>) -> Self {
        self.retriever = retriever;
        self
    }

    pub fn with_secret_literals(mut self, secrets: Vec<String>) -> Self {
        self.secret_literals = secrets
            .into_iter()
            .filter(|secret| !secret.is_empty())
            .collect();
        self
    }

    pub async fn run_turn(
        &self,
        request: AgentTurnRequest,
        cancellation: Arc<dyn ToolCancellation>,
    ) -> Result<AgentTurnResult, SessionEngineError> {
        validate_turn(&request, self.config.max_user_bytes)?;
        let lease = self
            .sessions
            .begin_turn(&request.session_id, &request.run_id)?;
        let result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(SessionEngineError::Cancelled),
            result = tokio::time::timeout(
                self.config.max_run_duration,
                self.run_leased(&lease, &request, Arc::clone(&cancellation)),
            ) => result.map_err(|_| SessionEngineError::Timeout).and_then(|result| result),
        };
        match result {
            Ok(completed) => {
                if cancellation.is_cancelled() {
                    let _ = self.sessions.abort_turn(&lease);
                    return Err(SessionEngineError::Cancelled);
                }
                let committed = self.sessions.commit_turn(
                    &lease,
                    redact_sensitive_text(request.user_input, &self.secret_literals),
                    redact_sensitive_text(completed.final_text.clone(), &self.secret_literals),
                );
                match committed {
                    Ok(session) => Ok(AgentTurnResult {
                        session_id: session.session_id,
                        run_id: lease.run_id.clone(),
                        revision: session.revision,
                        final_text: completed.final_text,
                        usage: completed.usage,
                        model_rounds: completed.model_rounds,
                        retrieval_count: completed.retrieval_count,
                    }),
                    Err(error) => {
                        let _ = self.sessions.abort_turn(&lease);
                        Err(SessionEngineError::Session(error))
                    }
                }
            }
            Err(error) => {
                let _ = self.sessions.abort_turn(&lease);
                Err(error)
            }
        }
    }

    async fn run_leased(
        &self,
        lease: &SessionLease,
        request: &AgentTurnRequest,
        cancellation: Arc<dyn ToolCancellation>,
    ) -> Result<CompletedTurn, SessionEngineError> {
        if cancellation.is_cancelled() {
            return Err(SessionEngineError::Cancelled);
        }
        let descriptor = self.provider.descriptor();
        validate_provider_request(descriptor.capabilities, request)?;
        if descriptor.capabilities.tool_calling != agent_runtime::ToolCallingSupport::None
            && !descriptor.capabilities.can_disable_tools
            && !self.registry.definitions().is_empty()
        {
            return Err(SessionEngineError::InvalidInput);
        }
        let retrieval = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(SessionEngineError::Cancelled),
            result = self.retriever.retrieve(&request.user_input, self.planner.max_rag_chunks()) => {
                result.map_err(map_rag_error)?
            }
        };
        let router = Arc::new(RunEventRouter::new(
            lease.run_id.clone(),
            Arc::clone(&self.events),
        ));
        let executor = ToolExecutor::new(Arc::clone(&self.registry), self.policy.clone())
            .with_approvals(Arc::clone(&self.approvals))
            .with_events(router.clone())
            .with_secret_literals(self.secret_literals.clone());
        let mut run_limits = self.config.tool_run.clone();
        run_limits.deadline = Some(Instant::now() + self.config.max_run_duration);
        let mut run = ToolRun::new(lease.run_id.clone(), run_limits, Arc::clone(&cancellation));
        if let Some(policy) = request.run_policy.clone() {
            run = run.with_policy(policy);
        }

        let all_tools = self
            .registry
            .definitions()
            .into_iter()
            .filter(|definition| {
                request.run_policy.as_ref().is_none_or(|policy| {
                    policy.evaluate(&definition.name, definition.risk)
                        != agent_runtime::PermissionDecision::Deny
                })
            })
            .collect::<Vec<_>>();
        let mut current = vec![TranscriptItem::User(request.user_input.clone())];
        let mut usage = ModelUsage::default();
        let mut seen_call_ids = HashSet::new();
        let mut tool_calls = 0u32;
        let mut model_rounds = 0u32;

        loop {
            run.begin_model_round().map_err(map_tool_error)?;
            model_rounds = model_rounds.saturating_add(1);
            let tools_enabled = descriptor.capabilities.tool_calling
                != agent_runtime::ToolCallingSupport::None
                && model_rounds < self.config.tool_run.max_model_rounds
                && tool_calls < self.config.tool_run.max_tool_calls;
            let tools = if tools_enabled {
                all_tools.as_slice()
            } else {
                &[]
            };
            let mut request_turn = current.clone();
            if !tools_enabled {
                request_turn.push(TranscriptItem::System(
                    "Tools are disabled for this final round. Return the final answer now using only the available evidence."
                        .into(),
                ));
            }
            let planned = self.planner.plan(
                descriptor.capabilities,
                &lease.snapshot,
                &request_turn,
                retrieval.clone(),
                tools,
                request.response_schema.as_ref(),
                request.max_output_tokens,
            )?;
            let retrieval_count = planned.retrieval_count;
            let model_request = ModelRequest {
                transcript: planned.transcript,
                tools: tools.to_vec(),
                response_format: request.response_format,
                response_schema: request.response_schema.clone(),
                max_output_tokens: request.max_output_tokens,
            };
            let response = self
                .respond_with_retry(&router, &model_request, Arc::clone(&cancellation))
                .await?;
            if cancellation.is_cancelled() {
                return Err(SessionEngineError::Cancelled);
            }
            accumulate_usage(&mut usage, &response, &self.config)?;

            match response.output {
                ModelOutput::FinalText { text } => {
                    validate_final(&text, request.response_format, self.config.max_final_bytes)?;
                    usage.tool_calls = tool_calls;
                    return Ok(CompletedTurn {
                        final_text: text,
                        usage,
                        model_rounds,
                        retrieval_count,
                    });
                }
                ModelOutput::ToolCalls { calls } => {
                    if !tools_enabled {
                        return Err(SessionEngineError::LoopExhausted);
                    }
                    validate_calls(&calls, &mut seen_call_ids)?;
                    let next_count = tool_calls
                        .checked_add(
                            u32::try_from(calls.len())
                                .map_err(|_| SessionEngineError::Budget("tool_calls"))?,
                        )
                        .ok_or(SessionEngineError::Budget("tool_calls"))?;
                    if next_count > self.config.tool_run.max_tool_calls {
                        return Err(SessionEngineError::Budget("tool_calls"));
                    }
                    tool_calls = next_count;
                    current.push(TranscriptItem::AssistantToolCalls(calls.clone()));
                    for call in calls {
                        let result = match executor.execute(&run, call.clone()).await {
                            Ok(result) => result,
                            Err(ToolExecutionError::UnknownTool) => synthetic_tool_result(
                                call,
                                "Tool is unavailable. Choose a listed tool or return the final answer.",
                            ),
                            Err(ToolExecutionError::InvalidInput { code, .. }) => synthetic_tool_result(
                                call,
                                &format!("Tool input was rejected ({code}). Correct the complete arguments or return the final answer."),
                            ),
                            Err(error) => return Err(map_tool_error(error)),
                        };
                        current.push(TranscriptItem::ToolResult {
                            name: result.name,
                            call_id: result.call_id,
                            content: result.content,
                            counts_toward_budget: true,
                        });
                    }
                }
            }
        }
    }

    async fn respond_with_retry(
        &self,
        router: &RunEventRouter,
        request: &ModelRequest,
        cancellation: Arc<dyn ToolCancellation>,
    ) -> Result<ModelResponse, SessionEngineError> {
        let descriptor = self.provider.descriptor();
        for failed_attempt in 1..=self.config.retry.max_attempts {
            let attempt_id = router.next_attempt();
            let emitter = router.emitter(attempt_id);
            emitter.emit(AgentEventKind::ModelAttemptStarted {
                provider_id: descriptor.provider_id.clone(),
                model_id: descriptor.model_id.clone(),
            });
            let response = tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(SessionEngineError::Cancelled),
                response = self.provider.respond_stream(request, &emitter) => response,
            };
            match response {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let will_retry =
                        error.is_transient() && failed_attempt < self.config.retry.max_attempts;
                    emitter.emit(AgentEventKind::ModelAttemptFailed {
                        error: AgentErrorCode::from(&error),
                        will_retry,
                    });
                    if !will_retry {
                        return Err(SessionEngineError::Provider(AgentErrorCode::from(&error)));
                    }
                    let delay = self
                        .config
                        .retry
                        .delay_after(failed_attempt, router.run_id());
                    tokio::select! {
                        biased;
                        _ = cancellation.cancelled() => return Err(SessionEngineError::Cancelled),
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
            }
        }
        Err(SessionEngineError::Provider(
            AgentErrorCode::InvalidResponse,
        ))
    }
}

struct CompletedTurn {
    final_text: String,
    usage: ModelUsage,
    model_rounds: u32,
    retrieval_count: usize,
}

struct RunEventRouter {
    run_id: String,
    clock: AgentEventClock,
    sink: Arc<dyn AgentEventSink>,
    next_attempt: AtomicU32,
    current_attempt: AtomicU32,
}

impl RunEventRouter {
    fn new(run_id: String, sink: Arc<dyn AgentEventSink>) -> Self {
        Self {
            run_id,
            clock: AgentEventClock::default(),
            sink,
            next_attempt: AtomicU32::new(1),
            current_attempt: AtomicU32::new(0),
        }
    }

    fn run_id(&self) -> &str {
        &self.run_id
    }

    fn next_attempt(&self) -> u32 {
        let attempt = self.next_attempt.fetch_add(1, Ordering::Relaxed);
        self.current_attempt.store(attempt, Ordering::Release);
        attempt
    }

    fn emitter(&self, attempt: u32) -> AgentEventEmitter<'_> {
        AgentEventEmitter::new(&self.run_id, attempt, &self.clock, self.sink.as_ref())
    }
}

impl ToolExecutionEventSink for RunEventRouter {
    fn emit(&self, event: ToolExecutionEvent) {
        let attempt = self.current_attempt.load(Ordering::Acquire);
        if attempt > 0 {
            self.emitter(attempt).emit(event.into_agent_event_kind());
        }
    }
}

fn validate_turn(
    request: &AgentTurnRequest,
    max_user_bytes: usize,
) -> Result<(), SessionEngineError> {
    if request.user_input.trim().is_empty()
        || request.user_input.len() > max_user_bytes
        || request.user_input.contains('\0')
        || request.max_output_tokens == 0
    {
        return Err(SessionEngineError::InvalidInput);
    }
    if request.response_format == ResponseFormat::Text && request.response_schema.is_some() {
        return Err(SessionEngineError::InvalidInput);
    }
    Ok(())
}

fn validate_provider_request(
    capabilities: ProviderCapabilities,
    request: &AgentTurnRequest,
) -> Result<(), SessionEngineError> {
    if (capabilities.max_output_tokens > 0
        && u64::from(request.max_output_tokens) > capabilities.max_output_tokens)
        || (request.response_format == ResponseFormat::JsonObject
            && capabilities.structured_output == agent_runtime::StructuredOutputSupport::None)
    {
        Err(SessionEngineError::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_calls(
    calls: &[ToolCall],
    seen_call_ids: &mut HashSet<String>,
) -> Result<(), SessionEngineError> {
    if calls.is_empty() {
        return Err(SessionEngineError::InvalidToolCall("empty_batch"));
    }
    let mut batch = HashSet::new();
    for call in calls {
        if call.call_id.trim().is_empty() || call.name.trim().is_empty() {
            return Err(SessionEngineError::InvalidToolCall("identity"));
        }
        if !batch.insert(call.call_id.clone()) || seen_call_ids.contains(&call.call_id) {
            return Err(SessionEngineError::InvalidToolCall("duplicate_call_id"));
        }
    }
    seen_call_ids.extend(batch);
    Ok(())
}

fn synthetic_tool_result(call: ToolCall, content: &str) -> ToolResult {
    ToolResult {
        call_id: call.call_id,
        name: call.name,
        outcome: agent_runtime::ToolOutcome::InvalidInput,
        content: content.into(),
        truncated: false,
        content_bytes: content.len(),
    }
}

fn validate_final(
    text: &str,
    response_format: ResponseFormat,
    max_bytes: usize,
) -> Result<(), SessionEngineError> {
    if text.trim().is_empty() || text.len() > max_bytes || text.contains('\0') {
        return Err(SessionEngineError::InvalidFinal);
    }
    if response_format == ResponseFormat::JsonObject
        && !serde_json::from_str::<Value>(text).is_ok_and(|value| value.is_object())
    {
        return Err(SessionEngineError::InvalidFinal);
    }
    Ok(())
}

fn accumulate_usage(
    total: &mut ModelUsage,
    response: &ModelResponse,
    config: &SessionEngineConfig,
) -> Result<(), SessionEngineError> {
    total.input_tokens = total
        .input_tokens
        .checked_add(response.usage.input_tokens)
        .ok_or(SessionEngineError::Budget("input_tokens"))?;
    total.output_tokens = total
        .output_tokens
        .checked_add(response.usage.output_tokens)
        .ok_or(SessionEngineError::Budget("output_tokens"))?;
    if total.input_tokens > config.max_total_input_tokens {
        return Err(SessionEngineError::Budget("input_tokens"));
    }
    if total.output_tokens > config.max_total_output_tokens {
        return Err(SessionEngineError::Budget("output_tokens"));
    }
    Ok(())
}

fn map_tool_error(error: ToolExecutionError) -> SessionEngineError {
    match error {
        ToolExecutionError::Cancelled => SessionEngineError::Cancelled,
        ToolExecutionError::Timeout => SessionEngineError::Tool("timeout"),
        ToolExecutionError::BudgetExceeded(budget) => SessionEngineError::Budget(budget),
        ToolExecutionError::InvalidCall(code) => SessionEngineError::InvalidToolCall(code),
        ToolExecutionError::UnknownTool => SessionEngineError::Tool("unknown"),
        ToolExecutionError::InvalidInput { code, .. } => SessionEngineError::Tool(code),
    }
}

fn map_rag_error(_: RagError) -> SessionEngineError {
    SessionEngineError::Retrieval
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use agent_runtime::{
        AgentEvent, DenyAllApprovals, ModelUsage, NeverCancel, NoopAgentEventSink,
        PermissionDecision, PermissionRule, ProviderDescriptor, ProviderError,
        StructuredOutputSupport, ToolCallingSupport, ToolHandler, ToolHandlerError, ToolMatcher,
        ToolRisk, UsageSupport,
    };
    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::session::{SessionStore, SessionStoreLimits};

    struct FixtureProvider {
        responses: Mutex<VecDeque<Result<ModelResponse, ProviderError>>>,
        requests: Mutex<Vec<ModelRequest>>,
    }

    struct HangingProvider;

    #[async_trait]
    impl ModelProvider for HangingProvider {
        fn descriptor(&self) -> ProviderDescriptor {
            FixtureProvider {
                responses: Mutex::new(VecDeque::new()),
                requests: Mutex::new(Vec::new()),
            }
            .descriptor()
        }

        async fn respond(&self, _: &ModelRequest) -> Result<ModelResponse, ProviderError> {
            std::future::pending().await
        }
    }

    struct AlreadyCancelled;

    #[async_trait]
    impl ToolCancellation for AlreadyCancelled {
        fn is_cancelled(&self) -> bool {
            true
        }
    }

    #[async_trait]
    impl ModelProvider for FixtureProvider {
        fn descriptor(&self) -> ProviderDescriptor {
            ProviderDescriptor {
                provider_id: "fixture".into(),
                model_id: "fixture-model".into(),
                capabilities: ProviderCapabilities {
                    structured_output: StructuredOutputSupport::JsonObject,
                    tool_calling: ToolCallingSupport::Serial,
                    can_disable_tools: true,
                    requires_reasoning_replay: false,
                    context_window_tokens: 16_000,
                    max_output_tokens: 1_024,
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
                .unwrap_or_else(|| Err(ProviderError::InvalidResponse("missing fixture".into())))
        }
    }

    #[derive(Default)]
    struct RecordingEvents(Mutex<Vec<AgentEvent>>);

    impl AgentEventSink for RecordingEvents {
        fn emit(&self, event: AgentEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[derive(Default)]
    struct CountingHandler(AtomicU32);

    #[async_trait]
    impl ToolHandler for CountingHandler {
        async fn execute(
            &self,
            _: agent_runtime::ToolExecutionContext,
            arguments: Value,
        ) -> Result<String, ToolHandlerError> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(arguments["text"].as_str().unwrap().to_owned())
        }
    }

    fn registry(handler: Arc<CountingHandler>) -> Arc<ToolRegistry> {
        let mut registry = ToolRegistry::default();
        registry
            .register(
                agent_runtime::ToolDefinition {
                    name: "echo".into(),
                    description: "Echo".into(),
                    input_schema: json!({
                        "type":"object",
                        "properties":{"text":{"type":"string"}},
                        "required":["text"],
                        "additionalProperties":false
                    }),
                    risk: ToolRisk::ReadOnly,
                    timeout_ms: 1_000,
                    max_result_bytes: 1_024,
                },
                handler,
            )
            .unwrap();
        Arc::new(registry)
    }

    fn policy() -> PermissionPolicy {
        PermissionPolicy::new(vec![PermissionRule {
            matcher: ToolMatcher::Exact("echo".into()),
            risk: Some(ToolRisk::ReadOnly),
            decision: PermissionDecision::Allow,
        }])
    }

    fn config(rounds: u32) -> SessionEngineConfig {
        SessionEngineConfig {
            context: ContextLimits {
                explicit_context_tokens: Some(16_000),
                safety_margin_tokens: 128,
                reserved_output_tokens: 256,
                ..ContextLimits::default()
            },
            tool_run: ToolRunLimits {
                max_model_rounds: rounds,
                max_tool_calls: 4,
                ..ToolRunLimits::default()
            },
            retry: RetryPolicy {
                max_attempts: 2,
                base_delay_ms: 0,
                max_delay_ms: 0,
                jitter_percent: 0,
            },
            max_total_input_tokens: 10_000,
            max_total_output_tokens: 10_000,
            max_user_bytes: 1_024,
            max_final_bytes: 1_024,
            max_run_duration: Duration::from_secs(5),
        }
    }

    fn sessions() -> Arc<SessionStore> {
        let store = Arc::new(SessionStore::new(SessionStoreLimits::default()).unwrap());
        store.create("session", "system").unwrap();
        store
    }

    fn fixture_provider(
        responses: Vec<Result<ModelResponse, ProviderError>>,
    ) -> Arc<FixtureProvider> {
        Arc::new(FixtureProvider {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        })
    }

    fn response(output: ModelOutput) -> Result<ModelResponse, ProviderError> {
        Ok(ModelResponse {
            output,
            usage: ModelUsage {
                input_tokens: 10,
                output_tokens: 5,
                tool_calls: 0,
            },
        })
    }

    #[tokio::test]
    async fn final_response_is_the_only_memory_commit() {
        let sessions = sessions();
        let provider = fixture_provider(vec![response(ModelOutput::FinalText {
            text: "answer".into(),
        })]);
        let engine = SessionEngine::new(
            provider,
            Arc::clone(&sessions),
            registry(Arc::new(CountingHandler::default())),
            policy(),
            Arc::new(DenyAllApprovals),
            Arc::new(NoopAgentEventSink),
            config(2),
        )
        .unwrap();
        let result = engine
            .run_turn(
                AgentTurnRequest::text("session", "run", "question", 256),
                Arc::new(NeverCancel),
            )
            .await
            .unwrap();
        assert_eq!(result.final_text, "answer");
        assert_eq!(result.revision, 1);
        assert_eq!(sessions.get("session").unwrap().recent_messages.len(), 2);
    }

    #[tokio::test]
    async fn invalid_schema_becomes_recoverable_result_without_handler_execution() {
        let handler = Arc::new(CountingHandler::default());
        let provider = fixture_provider(vec![
            response(ModelOutput::ToolCalls {
                calls: vec![ToolCall::with_call_id("echo", "call", json!({"bad":true}))],
            }),
            response(ModelOutput::FinalText {
                text: "recovered".into(),
            }),
        ]);
        let engine = SessionEngine::new(
            provider.clone(),
            sessions(),
            registry(Arc::clone(&handler)),
            policy(),
            Arc::new(DenyAllApprovals),
            Arc::new(NoopAgentEventSink),
            config(3),
        )
        .unwrap();
        engine
            .run_turn(
                AgentTurnRequest::text("session", "run", "question", 256),
                Arc::new(NeverCancel),
            )
            .await
            .unwrap();
        assert_eq!(handler.0.load(Ordering::Relaxed), 0);
        let requests = provider.requests.lock().unwrap();
        assert!(requests[1].transcript.iter().any(|item| matches!(
            item,
            TranscriptItem::ToolResult { content, .. } if content.contains("rejected")
        )));
    }

    #[tokio::test]
    async fn denied_tool_result_is_recoverable_without_handler_execution() {
        let handler = Arc::new(CountingHandler::default());
        let provider = fixture_provider(vec![
            response(ModelOutput::ToolCalls {
                calls: vec![ToolCall::with_call_id(
                    "echo",
                    "call",
                    json!({"text":"write"}),
                )],
            }),
            response(ModelOutput::FinalText {
                text: "denied safely".into(),
            }),
        ]);
        let ask = PermissionPolicy::new(vec![PermissionRule {
            matcher: ToolMatcher::Exact("echo".into()),
            risk: Some(ToolRisk::ReadOnly),
            decision: PermissionDecision::Ask,
        }]);
        let engine = SessionEngine::new(
            provider.clone(),
            sessions(),
            registry(Arc::clone(&handler)),
            ask,
            Arc::new(DenyAllApprovals),
            Arc::new(NoopAgentEventSink),
            config(3),
        )
        .unwrap();
        engine
            .run_turn(
                AgentTurnRequest::text("session", "run", "question", 256),
                Arc::new(NeverCancel),
            )
            .await
            .unwrap();
        assert_eq!(handler.0.load(Ordering::Relaxed), 0);
        assert!(provider.requests.lock().unwrap()[1]
            .transcript
            .iter()
            .any(|item| matches!(item, TranscriptItem::ToolResult { content, .. } if content == "Tool permission denied.")));
    }

    #[tokio::test]
    async fn duplicate_ids_and_final_round_tool_calls_abort_without_commit() {
        let sessions = sessions();
        let provider = fixture_provider(vec![response(ModelOutput::ToolCalls {
            calls: vec![
                ToolCall::with_call_id("echo", "same", json!({"text":"a"})),
                ToolCall::with_call_id("echo", "same", json!({"text":"b"})),
            ],
        })]);
        let engine = SessionEngine::new(
            provider,
            Arc::clone(&sessions),
            registry(Arc::new(CountingHandler::default())),
            policy(),
            Arc::new(DenyAllApprovals),
            Arc::new(NoopAgentEventSink),
            config(3),
        )
        .unwrap();
        assert_eq!(
            engine
                .run_turn(
                    AgentTurnRequest::text("session", "run", "question", 256),
                    Arc::new(NeverCancel),
                )
                .await
                .unwrap_err(),
            SessionEngineError::InvalidToolCall("duplicate_call_id")
        );
        assert_eq!(sessions.get("session").unwrap().revision, 0);

        let provider = fixture_provider(vec![response(ModelOutput::ToolCalls {
            calls: vec![ToolCall::with_call_id("echo", "new", json!({"text":"a"}))],
        })]);
        let engine = SessionEngine::new(
            provider.clone(),
            Arc::clone(&sessions),
            registry(Arc::new(CountingHandler::default())),
            policy(),
            Arc::new(DenyAllApprovals),
            Arc::new(NoopAgentEventSink),
            config(1),
        )
        .unwrap();
        assert_eq!(
            engine
                .run_turn(
                    AgentTurnRequest::text("session", "run-2", "question", 256),
                    Arc::new(NeverCancel),
                )
                .await
                .unwrap_err(),
            SessionEngineError::LoopExhausted
        );
        assert!(provider.requests.lock().unwrap()[0].tools.is_empty());
        assert_eq!(sessions.get("session").unwrap().revision, 0);
    }

    #[tokio::test]
    async fn transient_retry_uses_monotonic_sanitized_events() {
        let events = Arc::new(RecordingEvents::default());
        let provider = fixture_provider(vec![
            Err(ProviderError::Network("secret provider detail".into())),
            response(ModelOutput::FinalText {
                text: "answer".into(),
            }),
        ]);
        let engine = SessionEngine::new(
            provider,
            sessions(),
            registry(Arc::new(CountingHandler::default())),
            policy(),
            Arc::new(DenyAllApprovals),
            events.clone(),
            config(2),
        )
        .unwrap();
        engine
            .run_turn(
                AgentTurnRequest::text("session", "run", "question", 256),
                Arc::new(NeverCancel),
            )
            .await
            .unwrap();
        let events = events.0.lock().unwrap();
        assert!(events
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence));
        assert!(events.iter().any(|event| event.attempt_id == 1));
        assert!(events.iter().any(|event| event.attempt_id == 2));
        let encoded = serde_json::to_string(&*events).unwrap();
        assert!(!encoded.contains("secret provider detail"));
        assert!(!encoded.contains("question"));
    }

    #[tokio::test]
    async fn known_secrets_are_redacted_from_memory_but_not_the_canonical_result() {
        let sessions = sessions();
        let provider = fixture_provider(vec![response(ModelOutput::FinalText {
            text: "token=known-secret".into(),
        })]);
        let engine = SessionEngine::new(
            provider,
            Arc::clone(&sessions),
            registry(Arc::new(CountingHandler::default())),
            policy(),
            Arc::new(DenyAllApprovals),
            Arc::new(NoopAgentEventSink),
            config(2),
        )
        .unwrap()
        .with_secret_literals(vec!["known-secret".into()]);
        let result = engine
            .run_turn(
                AgentTurnRequest::text("session", "run", "api_key=known-secret", 256),
                Arc::new(NeverCancel),
            )
            .await
            .unwrap();
        assert_eq!(result.final_text, "token=known-secret");
        let memory = serde_json::to_string(&sessions.get("session").unwrap()).unwrap();
        assert!(!memory.contains("known-secret"));
        assert!(memory.contains("[REDACTED]"));
    }

    #[tokio::test]
    async fn run_timeout_aborts_the_lease_without_committing_memory() {
        let sessions = sessions();
        let mut short = config(2);
        short.max_run_duration = Duration::from_millis(20);
        let engine = SessionEngine::new(
            Arc::new(HangingProvider),
            Arc::clone(&sessions),
            registry(Arc::new(CountingHandler::default())),
            policy(),
            Arc::new(DenyAllApprovals),
            Arc::new(NoopAgentEventSink),
            short,
        )
        .unwrap();
        assert_eq!(
            engine
                .run_turn(
                    AgentTurnRequest::text("session", "run", "question", 256),
                    Arc::new(NeverCancel),
                )
                .await
                .unwrap_err(),
            SessionEngineError::Timeout
        );
        assert_eq!(sessions.get("session").unwrap().revision, 0);
        let lease = sessions.begin_turn("session", "next-run").unwrap();
        sessions.abort_turn(&lease).unwrap();
    }

    #[tokio::test]
    async fn cancellation_and_usage_budget_abort_without_provider_or_memory_commit() {
        let sessions = sessions();
        let provider = fixture_provider(vec![response(ModelOutput::FinalText {
            text: "should not run".into(),
        })]);
        let engine = SessionEngine::new(
            provider.clone(),
            Arc::clone(&sessions),
            registry(Arc::new(CountingHandler::default())),
            policy(),
            Arc::new(DenyAllApprovals),
            Arc::new(NoopAgentEventSink),
            config(2),
        )
        .unwrap();
        assert_eq!(
            engine
                .run_turn(
                    AgentTurnRequest::text("session", "cancelled", "question", 256),
                    Arc::new(AlreadyCancelled),
                )
                .await
                .unwrap_err(),
            SessionEngineError::Cancelled
        );
        assert!(provider.requests.lock().unwrap().is_empty());
        assert_eq!(sessions.get("session").unwrap().revision, 0);

        let provider = fixture_provider(vec![Ok(ModelResponse {
            output: ModelOutput::FinalText {
                text: "answer".into(),
            },
            usage: ModelUsage {
                input_tokens: 10_001,
                output_tokens: 1,
                tool_calls: 0,
            },
        })]);
        let engine = SessionEngine::new(
            provider,
            Arc::clone(&sessions),
            registry(Arc::new(CountingHandler::default())),
            policy(),
            Arc::new(DenyAllApprovals),
            Arc::new(NoopAgentEventSink),
            config(2),
        )
        .unwrap();
        assert_eq!(
            engine
                .run_turn(
                    AgentTurnRequest::text("session", "over-budget", "question", 256),
                    Arc::new(NeverCancel),
                )
                .await
                .unwrap_err(),
            SessionEngineError::Budget("input_tokens")
        );
        assert_eq!(sessions.get("session").unwrap().revision, 0);
    }

    #[tokio::test]
    async fn rag_is_injected_as_untrusted_data_and_counted() {
        let provider = fixture_provider(vec![response(ModelOutput::FinalText {
            text: "grounded".into(),
        })]);
        let retriever = crate::InMemoryRagIndex::new(vec![crate::RagChunk {
            id: "memory-doc".into(),
            source: "docs/memory".into(),
            content: "session memory compaction".into(),
            score: 0.0,
        }])
        .unwrap();
        let engine = SessionEngine::new(
            provider.clone(),
            sessions(),
            registry(Arc::new(CountingHandler::default())),
            policy(),
            Arc::new(DenyAllApprovals),
            Arc::new(NoopAgentEventSink),
            config(2),
        )
        .unwrap()
        .with_retriever(Arc::new(retriever));
        let result = engine
            .run_turn(
                AgentTurnRequest::text("session", "run", "How does memory compact?", 256),
                Arc::new(NeverCancel),
            )
            .await
            .unwrap();
        assert_eq!(result.retrieval_count, 1);
        let request = &provider.requests.lock().unwrap()[0];
        assert!(request.transcript.iter().any(|item| matches!(
            item,
            TranscriptItem::System(text)
                if text.contains("<rag-data>") && text.contains("untrusted data")
        )));
    }
}
