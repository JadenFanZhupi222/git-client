use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct AgentEventDto {
    pub run_id: String,
    #[ts(type = "number")]
    pub sequence: u64,
    pub attempt_id: u32,
    pub event_type: String,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub response_id: Option<String>,
    pub delta: Option<String>,
    pub artifact_type: Option<String>,
    pub artifact_field: Option<String>,
    pub artifact_index: Option<u32>,
    pub call_id: Option<String>,
    pub tool_name: Option<String>,
    pub usage: Option<ReviewUsageDto>,
    pub error_code: Option<String>,
    pub will_retry: Option<bool>,
    pub approval_id: Option<String>,
    pub risk: Option<String>,
    pub approval_summary: Option<String>,
    pub decision: Option<String>,
    pub tool_outcome: Option<String>,
    #[ts(type = "number | null")]
    pub duration_ms: Option<u64>,
    pub content_bytes: Option<usize>,
    pub truncated: Option<bool>,
    pub tool_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub enum ToolApprovalDecisionDto {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ToolApprovalResolutionDto {
    pub run_id: String,
    pub approval_id: String,
    pub decision: ToolApprovalDecisionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct AgentSessionTurnInputDto {
    pub repo_path: String,
    pub run_id: String,
    pub model_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct AgentSessionTurnResultDto {
    pub session_id: String,
    pub run_id: String,
    #[ts(type = "number")]
    pub revision: u64,
    pub final_text: String,
    pub usage: ReviewUsageDto,
    pub model_rounds: u32,
    pub retrieval_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct AgentSessionMessageDto {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct AgentSessionSnapshotDto {
    pub session_id: String,
    #[ts(type = "number")]
    pub revision: u64,
    pub memory_summary: Option<String>,
    pub recent_messages: Vec<AgentSessionMessageDto>,
    pub active_goal: Option<AgentGoalSnapshotDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct AgentGoalUsageDto {
    pub model_id: String,
    pub currency: Option<String>,
    #[ts(type = "number")]
    pub input_tokens: u64,
    #[ts(type = "number")]
    pub cached_input_tokens: u64,
    #[ts(type = "number")]
    pub output_tokens: u64,
    pub tool_calls: u32,
    #[ts(type = "number")]
    pub spent_micros: u64,
    #[ts(type = "number | null")]
    pub limit_micros: Option<u64>,
    #[ts(type = "number | null")]
    pub limit_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct AgentGoalSnapshotDto {
    pub goal_id: String,
    pub session_id: String,
    #[ts(type = "number")]
    pub revision: u64,
    pub objective: String,
    pub model_id: String,
    pub status: String,
    pub pause_reason: Option<String>,
    pub block_reason: Option<String>,
    pub usage_by_model: Vec<AgentGoalUsageDto>,
    pub slice_index: u32,
    pub steering_count: usize,
    pub completion_candidate_pending: bool,
    pub final_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct CreateAgentGoalInputDto {
    pub repo_path: String,
    pub goal_id: String,
    pub model_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct AgentGoalMutationInputDto {
    pub repo_path: String,
    pub goal_id: String,
    #[ts(type = "number")]
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct SteerAgentGoalInputDto {
    pub repo_path: String,
    pub goal_id: String,
    #[ts(type = "number")]
    pub expected_revision: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ResumeAgentGoalInputDto {
    pub repo_path: String,
    pub goal_id: String,
    #[ts(type = "number")]
    pub expected_revision: u64,
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ExtendAgentBudgetInputDto {
    pub repo_path: String,
    pub goal_id: String,
    #[ts(type = "number")]
    pub expected_revision: u64,
    pub model_id: String,
    pub currency: Option<String>,
    #[ts(type = "number | null")]
    pub new_limit_micros: Option<u64>,
    #[ts(type = "number | null")]
    pub new_limit_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct AgentGoalEventDto {
    pub goal_id: String,
    #[ts(type = "number")]
    pub revision: u64,
    pub event_type: String,
    pub status: Option<String>,
    pub reason: Option<String>,
    pub model_id: Option<String>,
    #[ts(type = "number | null")]
    pub spent_micros: Option<u64>,
    #[ts(type = "number | null")]
    pub limit_micros: Option<u64>,
    pub receipt_digest: Option<String>,
    pub size_bytes: Option<usize>,
}
