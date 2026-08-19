use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    #[default]
    ReadOnly,
    Write,
    Destructive,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    Success,
    Denied,
    Failed,
    Timeout,
    Cancelled,
    InvalidInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub outcome: ToolOutcome,
    pub content: String,
    pub truncated: bool,
    pub content_bytes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ToolReceipt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessReplayPolicy {
    Never,
    UserApproved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolReceipt {
    Observation {
        resource: String,
        version_digest: String,
    },
    Mutation {
        execution_id: String,
        resource: String,
        before_digest: String,
        after_digest: String,
    },
    Artifact {
        execution_id: String,
        artifact_id: String,
        content_digest: String,
    },
    Process {
        execution_id: String,
        program: String,
        exit_code: i32,
        replay_policy: ProcessReplayPolicy,
    },
}

impl ToolReceipt {
    pub fn is_effect(&self) -> bool {
        !matches!(self, Self::Observation { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolIntent {
    pub execution_id: String,
    pub run_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub risk: ToolRisk,
    pub arguments: Value,
    pub approval_id: Option<String>,
    pub approved: bool,
    pub resource: Option<String>,
    pub before_digest: Option<String>,
    pub expected_after_digest: Option<String>,
    pub replay_policy: Option<ProcessReplayPolicy>,
}

impl ToolIntent {
    #[doc(hidden)]
    pub fn for_test(execution_id: &str, call_id: &str, tool_name: &str) -> Self {
        Self {
            execution_id: execution_id.into(),
            run_id: "run-test".into(),
            call_id: call_id.into(),
            tool_name: tool_name.into(),
            risk: ToolRisk::Write,
            arguments: Value::Object(Default::default()),
            approval_id: None,
            approved: false,
            resource: None,
            before_digest: None,
            expected_after_digest: None,
            replay_policy: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolIntentPrecondition {
    pub resource: Option<String>,
    pub before_digest: Option<String>,
    pub expected_after_digest: Option<String>,
    pub replay_policy: Option<ProcessReplayPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolHandlerOutput {
    pub sanitized_content: String,
    pub receipt: ToolReceipt,
}

impl ToolHandlerOutput {
    pub fn new(content: impl Into<String>, receipt: ToolReceipt) -> Self {
        Self {
            sanitized_content: content.into(),
            receipt,
        }
    }
}

impl std::ops::Deref for ToolHandlerOutput {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.sanitized_content
    }
}

impl PartialEq<&str> for ToolHandlerOutput {
    fn eq(&self, other: &&str) -> bool {
        self.sanitized_content == *other
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolMatcher {
    Exact(String),
    Prefix(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRule {
    pub matcher: ToolMatcher,
    pub risk: Option<ToolRisk>,
    pub decision: PermissionDecision,
}

#[derive(Debug, Clone, Default)]
pub struct PermissionPolicy {
    rules: Vec<PermissionRule>,
}

impl PermissionPolicy {
    pub fn new(rules: Vec<PermissionRule>) -> Self {
        Self { rules }
    }

    pub fn evaluate(&self, name: &str, risk: ToolRisk) -> PermissionDecision {
        self.rules
            .iter()
            .find(|rule| {
                rule.risk.is_none_or(|expected| expected == risk)
                    && match &rule.matcher {
                        ToolMatcher::Exact(expected) => expected == name,
                        ToolMatcher::Prefix(prefix) => name.starts_with(prefix),
                    }
            })
            .map(|rule| rule.decision)
            .unwrap_or(PermissionDecision::Deny)
    }
}

pub(super) fn restrict_decision(
    application: PermissionDecision,
    run: Option<PermissionDecision>,
) -> PermissionDecision {
    match (application, run) {
        (PermissionDecision::Deny, _) | (_, Some(PermissionDecision::Deny)) => {
            PermissionDecision::Deny
        }
        (PermissionDecision::Ask, _) | (_, Some(PermissionDecision::Ask)) => {
            PermissionDecision::Ask
        }
        (PermissionDecision::Allow, _) => PermissionDecision::Allow,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolApprovalRequest {
    pub run_id: String,
    pub approval_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub risk: ToolRisk,
    pub summary: Option<String>,
}

#[async_trait]
pub trait ToolCancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;

    async fn cancelled(&self) {
        while !self.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[derive(Debug, Default)]
pub struct NeverCancel;

#[async_trait]
impl ToolCancellation for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[async_trait]
pub trait ToolApprovalResolver: Send + Sync {
    async fn resolve(&self, request: ToolApprovalRequest) -> PermissionDecision;
}

#[derive(Debug, Default)]
pub struct DenyAllApprovals;

#[async_trait]
impl ToolApprovalResolver for DenyAllApprovals {
    async fn resolve(&self, _: ToolApprovalRequest) -> PermissionDecision {
        PermissionDecision::Deny
    }
}

#[derive(Clone)]
pub struct ToolExecutionContext {
    pub run_id: String,
    pub call_id: String,
    pub execution_id: String,
    pub cancellation: Arc<dyn ToolCancellation>,
}

#[derive(Debug, Error)]
#[error("tool handler failed")]
pub struct ToolHandlerError {
    sanitized_content: Option<String>,
}

// Keep the existing `Err(ToolHandlerError)` construction source-compatible while allowing
// trusted built-in handlers to return bounded, secret-free recovery hints to the model.
#[allow(non_upper_case_globals)]
pub const ToolHandlerError: ToolHandlerError = ToolHandlerError {
    sanitized_content: None,
};

impl ToolHandlerError {
    pub fn sanitized(content: impl Into<String>) -> Self {
        Self {
            sanitized_content: Some(content.into()),
        }
    }

    pub fn sanitized_content(&self) -> Option<&str> {
        self.sanitized_content.as_deref()
    }
}

#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn prepare_intent(
        &self,
        _: &ToolExecutionContext,
        _: &Value,
    ) -> Result<ToolIntentPrecondition, ToolHandlerError> {
        Ok(ToolIntentPrecondition::default())
    }

    async fn execute(
        &self,
        context: ToolExecutionContext,
        arguments: Value,
    ) -> Result<ToolHandlerOutput, ToolHandlerError>;

    fn summarize_arguments(&self, _: &Value) -> Option<String> {
        None
    }

    fn sanitize_result(&self, content: String) -> String {
        content
    }
}

pub trait ToolIntentJournal: Send + Sync {
    /// Must durably persist and fsync the complete intent before returning.
    fn record_intent(&self, intent: &ToolIntent) -> Result<(), ToolExecutionError>;
    /// Must durably persist and fsync the receipt before returning.
    fn record_receipt(
        &self,
        intent: &ToolIntent,
        receipt: &ToolReceipt,
    ) -> Result<(), ToolExecutionError>;
    /// Must durably remove an intent whose read-only execution is known to have
    /// produced no side effect before returning.
    fn record_no_effect(
        &self,
        intent: &ToolIntent,
        outcome: ToolOutcome,
    ) -> Result<(), ToolExecutionError>;
}

#[derive(Debug, Default)]
pub struct NoopToolIntentJournal;

impl ToolIntentJournal for NoopToolIntentJournal {
    fn record_intent(&self, _: &ToolIntent) -> Result<(), ToolExecutionError> {
        Ok(())
    }

    fn record_receipt(&self, _: &ToolIntent, _: &ToolReceipt) -> Result<(), ToolExecutionError> {
        Ok(())
    }

    fn record_no_effect(&self, _: &ToolIntent, _: ToolOutcome) -> Result<(), ToolExecutionError> {
        Ok(())
    }
}
