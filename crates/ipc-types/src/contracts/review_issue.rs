use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub enum CredentialKindDto {
    Deepseek,
    Openai,
    Anthropic,
    Github,
    Gitlab,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ReviewTargetDto {
    pub owner: String,
    pub repo: String,
    #[ts(type = "number")]
    pub pull_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ReviewFileDto {
    pub path: String,
    pub patch_bytes: usize,
    pub reviewable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ReviewPreflightDto {
    pub head_sha: String,
    pub files: Vec<ReviewFileDto>,
    pub total_patch_bytes: usize,
    pub requires_selection: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub enum ReviewLanguageDto {
    SimplifiedChinese,
    English,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ReviewModelOptionDto {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub provider_id: String,
    pub capabilities: ReviewModelCapabilitiesDto,
    pub pricing: Option<ReviewModelPricingDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ReviewModelCapabilitiesDto {
    pub supports_tool_calling: bool,
    pub supports_structured_output: bool,
    #[ts(type = "number")]
    pub context_window_tokens: u64,
    #[ts(type = "number")]
    pub max_output_tokens: u64,
    pub reports_usage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ReviewModelPricingDto {
    pub currency: String,
    #[ts(type = "number")]
    pub input_cache_hit_per_million_micros: u64,
    #[ts(type = "number")]
    pub input_cache_miss_per_million_micros: u64,
    #[ts(type = "number")]
    pub output_per_million_micros: u64,
    pub source_url: String,
    pub source_version: String,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ReviewRunInputDto {
    pub run_id: String,
    pub target: ReviewTargetDto,
    pub expected_head_sha: String,
    pub selected_files: Vec<String>,
    pub model_id: String,
    pub output_language: ReviewLanguageDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ReviewFindingDto {
    pub id: String,
    pub severity: String,
    pub path: String,
    pub side: String,
    pub line: u32,
    pub title: String,
    pub failure_scenario: String,
    pub explanation: String,
    pub draft_comment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ReviewUsageDto {
    #[ts(type = "number")]
    pub input_tokens: u64,
    #[ts(type = "number")]
    pub output_tokens: u64,
    pub tool_calls: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ReviewRunResultDto {
    pub run_id: String,
    pub head_sha: String,
    pub summary: String,
    pub reviewed_files: Vec<String>,
    pub findings: Vec<ReviewFindingDto>,
    pub usage: ReviewUsageDto,
    pub model_id: String,
    #[ts(type = "number")]
    pub duration_ms: u64,
    pub diagnostic_id: String,
    pub provider_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct SubmitReviewDto {
    pub target: ReviewTargetDto,
    pub head_sha: String,
    pub findings: Vec<ReviewFindingDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct PublishedReviewDto {
    #[ts(type = "number")]
    pub review_id: u64,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ReviewProgressEventDto {
    pub run_id: String,
    pub stage: String,
    pub tool_name: Option<String>,
    pub tool_calls: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ChangePlanInputDto {
    pub run_id: String,
    pub repo_path: String,
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub enum ChangeWarningSeverityDto {
    Info,
    Warning,
    Blocker,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ChangeWarningDto {
    pub code: String,
    pub severity: ChangeWarningSeverityDto,
    pub message: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ChangePlanFileDto {
    pub path: String,
    pub state: String,
    pub staged: bool,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ChangeCommitGroupDto {
    pub id: String,
    pub title: String,
    pub rationale: String,
    pub commit_message: String,
    pub files: Vec<ChangePlanFileDto>,
    pub executable: bool,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ChangePlanResultDto {
    pub snapshot_id: String,
    pub summary: String,
    pub warnings: Vec<ChangeWarningDto>,
    pub groups: Vec<ChangeCommitGroupDto>,
    pub enhanced: bool,
    pub usage: ReviewUsageDto,
    pub model_id: String,
    pub provider_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct CommitChangeGroupInputDto {
    pub run_id: String,
    pub repo_path: String,
    pub snapshot_id: String,
    pub group_id: String,
    pub commit_message: String,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ChangeGroupCommitResultDto {
    pub sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct HistoryInvestigationInputDto {
    pub run_id: String,
    pub repo_path: String,
    pub question: String,
    pub file: Option<String>,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct HistoryInvestigationFindingDto {
    pub title: String,
    pub explanation: String,
    pub commit_ids: Vec<String>,
    pub paths: Vec<String>,
    pub evidence_links: Vec<HistoryEvidenceLinkDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct HistoryEvidenceLinkDto {
    pub commit_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct HistoryInvestigationResultDto {
    pub snapshot_id: String,
    pub summary: String,
    pub confidence: String,
    pub findings: Vec<HistoryInvestigationFindingDto>,
    pub caveats: Vec<String>,
    pub search_terms: Vec<String>,
    pub evidence_sources: Vec<String>,
    pub evidence_commit_count: usize,
    pub usage: ReviewUsageDto,
    pub model_id: String,
    pub provider_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueRepositoryTargetDto {
    pub owner: String,
    pub repo: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueTargetDto {
    pub owner: String,
    pub repo: String,
    #[ts(type = "number")]
    pub issue_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueLabelDto {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueSummaryDto {
    #[ts(type = "number")]
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: Option<String>,
    pub updated_at: String,
    pub comments: u32,
    pub labels: Vec<IssueLabelDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueCommentDto {
    pub author: Option<String>,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueSnapshotDto {
    pub updated_at: String,
    pub comments: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueContextDto {
    pub issue: IssueSummaryDto,
    pub body: String,
    pub comments: Vec<IssueCommentDto>,
    pub comments_truncated: bool,
    pub available_labels: Vec<IssueLabelDto>,
    pub similar_issues: Vec<IssueSummaryDto>,
    pub snapshot: IssueSnapshotDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueTriageInputDto {
    pub run_id: String,
    pub target: IssueTargetDto,
    pub expected_updated_at: String,
    pub expected_comments: u32,
    pub model_id: String,
    pub output_language: ReviewLanguageDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueTriageProposalDto {
    pub summary: String,
    pub category: String,
    pub priority: String,
    pub confidence: f64,
    pub suggested_labels: Vec<String>,
    #[ts(type = "Array<number>")]
    pub suspected_duplicate_numbers: Vec<u64>,
    pub suggested_reply: String,
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueTriageResultDto {
    pub run_id: String,
    pub snapshot: IssueSnapshotDto,
    pub comments_analyzed: usize,
    pub comments_truncated: bool,
    pub proposal: IssueTriageProposalDto,
    pub usage: ReviewUsageDto,
    pub model_id: String,
    #[ts(type = "number")]
    pub duration_ms: u64,
    pub diagnostic_id: String,
    pub provider_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueTriagePublishInputDto {
    pub publish_id: String,
    pub confirmed: bool,
    pub target: IssueTargetDto,
    pub expected_snapshot: IssueSnapshotDto,
    pub labels: Vec<String>,
    pub reply: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueTriagePublishActionResultDto {
    pub action_id: String,
    pub kind: String,
    pub label: Option<String>,
    pub status: String,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IssueTriagePublishResultDto {
    pub publish_id: String,
    pub snapshot: Option<IssueSnapshotDto>,
    pub actions: Vec<IssueTriagePublishActionResultDto>,
}

impl From<IssueRepositoryTargetDto> for review_agent::IssueRepositoryTarget {
    fn from(value: IssueRepositoryTargetDto) -> Self {
        Self {
            owner: value.owner,
            repo: value.repo,
        }
    }
}

impl From<IssueTargetDto> for review_agent::IssueTarget {
    fn from(value: IssueTargetDto) -> Self {
        Self {
            owner: value.owner,
            repo: value.repo,
            issue_number: value.issue_number,
        }
    }
}

impl From<review_agent::IssueLabel> for IssueLabelDto {
    fn from(value: review_agent::IssueLabel) -> Self {
        Self {
            name: value.name,
            color: value.color,
        }
    }
}

impl From<review_agent::IssueSummary> for IssueSummaryDto {
    fn from(value: review_agent::IssueSummary) -> Self {
        Self {
            number: value.number,
            title: value.title,
            url: value.url,
            author: value.author,
            updated_at: value.updated_at,
            comments: value.comments,
            labels: value.labels.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<review_agent::IssueComment> for IssueCommentDto {
    fn from(value: review_agent::IssueComment) -> Self {
        Self {
            author: value.author,
            body: value.body,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<review_agent::IssueSnapshot> for IssueSnapshotDto {
    fn from(value: review_agent::IssueSnapshot) -> Self {
        Self {
            updated_at: value.updated_at,
            comments: value.comments,
        }
    }
}

impl From<IssueSnapshotDto> for review_agent::IssueSnapshot {
    fn from(value: IssueSnapshotDto) -> Self {
        Self {
            updated_at: value.updated_at,
            comments: value.comments,
        }
    }
}

impl From<review_agent::IssueContext> for IssueContextDto {
    fn from(value: review_agent::IssueContext) -> Self {
        Self {
            issue: value.issue.into(),
            body: value.body,
            comments: value.comments.into_iter().map(Into::into).collect(),
            comments_truncated: value.comments_truncated,
            available_labels: value.available_labels.into_iter().map(Into::into).collect(),
            similar_issues: value.similar_issues.into_iter().map(Into::into).collect(),
            snapshot: value.snapshot.into(),
        }
    }
}

impl From<IssueTriageInputDto> for review_agent::IssueTriageInput {
    fn from(value: IssueTriageInputDto) -> Self {
        Self {
            run_id: value.run_id,
            target: value.target.into(),
            expected_updated_at: value.expected_updated_at,
            expected_comments: value.expected_comments,
            output_language: match value.output_language {
                ReviewLanguageDto::SimplifiedChinese => {
                    review_agent::ReviewLanguage::SimplifiedChinese
                }
                ReviewLanguageDto::English => review_agent::ReviewLanguage::English,
            },
        }
    }
}

impl From<review_agent::IssueTriageProposal> for IssueTriageProposalDto {
    fn from(value: review_agent::IssueTriageProposal) -> Self {
        Self {
            summary: value.summary,
            category: value.category,
            priority: value.priority,
            confidence: value.confidence,
            suggested_labels: value.suggested_labels,
            suspected_duplicate_numbers: value.suspected_duplicate_numbers,
            suggested_reply: value.suggested_reply,
            rationale: value.rationale,
        }
    }
}

impl From<review_agent::IssueTriageResult> for IssueTriageResultDto {
    fn from(value: review_agent::IssueTriageResult) -> Self {
        Self {
            run_id: value.run_id,
            snapshot: value.snapshot.into(),
            comments_analyzed: value.comments_analyzed,
            comments_truncated: value.comments_truncated,
            proposal: value.proposal.into(),
            usage: value.usage.into(),
            model_id: value.model_id,
            duration_ms: value.duration_ms,
            diagnostic_id: value.diagnostic_id,
            provider_attempts: value.provider_attempts,
        }
    }
}

impl From<IssueTriagePublishInputDto> for review_agent::IssueTriagePublishInput {
    fn from(value: IssueTriagePublishInputDto) -> Self {
        Self {
            publish_id: value.publish_id,
            confirmed: value.confirmed,
            target: value.target.into(),
            expected_snapshot: value.expected_snapshot.into(),
            labels: value.labels,
            reply: value.reply,
        }
    }
}

impl From<review_agent::IssueTriagePublishActionResult> for IssueTriagePublishActionResultDto {
    fn from(value: review_agent::IssueTriagePublishActionResult) -> Self {
        Self {
            action_id: value.action_id,
            kind: match value.kind {
                review_agent::IssueTriagePublishActionKind::Label => "label",
                review_agent::IssueTriagePublishActionKind::Comment => "comment",
            }
            .into(),
            label: value.label,
            status: match value.status {
                review_agent::IssueTriagePublishActionStatus::Applied => "applied",
                review_agent::IssueTriagePublishActionStatus::AlreadyApplied => "already_applied",
                review_agent::IssueTriagePublishActionStatus::Failed => "failed",
            }
            .into(),
            error_code: value.error_code,
        }
    }
}

impl From<review_agent::IssueTriagePublishResult> for IssueTriagePublishResultDto {
    fn from(value: review_agent::IssueTriagePublishResult) -> Self {
        Self {
            publish_id: value.publish_id,
            snapshot: value.snapshot.map(Into::into),
            actions: value.actions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<review_agent::ReviewTarget> for ReviewTargetDto {
    fn from(v: review_agent::ReviewTarget) -> Self {
        Self {
            owner: v.owner,
            repo: v.repo,
            pull_number: v.pull_number,
        }
    }
}
impl From<ReviewTargetDto> for review_agent::ReviewTarget {
    fn from(v: ReviewTargetDto) -> Self {
        Self {
            owner: v.owner,
            repo: v.repo,
            pull_number: v.pull_number,
        }
    }
}
impl From<review_agent::ReviewFile> for ReviewFileDto {
    fn from(v: review_agent::ReviewFile) -> Self {
        Self {
            path: v.path,
            patch_bytes: v.patch_bytes,
            reviewable: v.reviewable,
        }
    }
}
impl From<review_agent::ReviewPreflight> for ReviewPreflightDto {
    fn from(v: review_agent::ReviewPreflight) -> Self {
        Self {
            head_sha: v.head_sha,
            files: v.files.into_iter().map(Into::into).collect(),
            total_patch_bytes: v.total_patch_bytes,
            requires_selection: v.requires_selection,
        }
    }
}
impl From<ReviewRunInputDto> for review_agent::ReviewRunInput {
    fn from(v: ReviewRunInputDto) -> Self {
        Self {
            run_id: v.run_id,
            target: v.target.into(),
            expected_head_sha: v.expected_head_sha,
            selected_files: v.selected_files,
            output_language: match v.output_language {
                ReviewLanguageDto::SimplifiedChinese => {
                    review_agent::ReviewLanguage::SimplifiedChinese
                }
                ReviewLanguageDto::English => review_agent::ReviewLanguage::English,
            },
        }
    }
}
impl From<review_agent::ReviewFinding> for ReviewFindingDto {
    fn from(v: review_agent::ReviewFinding) -> Self {
        Self {
            id: v.id,
            severity: format!("{:?}", v.severity).to_lowercase(),
            path: v.path,
            side: format!("{:?}", v.side),
            line: v.line,
            title: v.title,
            failure_scenario: v.failure_scenario,
            explanation: v.explanation,
            draft_comment: v.draft_comment,
        }
    }
}
impl TryFrom<ReviewFindingDto> for review_agent::ReviewFinding {
    type Error = String;
    fn try_from(v: ReviewFindingDto) -> Result<Self, Self::Error> {
        Ok(Self {
            id: v.id,
            severity: match v.severity.as_str() {
                "high" => review_agent::Severity::High,
                "medium" => review_agent::Severity::Medium,
                "low" => review_agent::Severity::Low,
                _ => return Err("invalid severity".into()),
            },
            path: v.path,
            side: match v.side.as_str() {
                "LEFT" => review_agent::ReviewSide::LEFT,
                "RIGHT" => review_agent::ReviewSide::RIGHT,
                _ => return Err("invalid review side".into()),
            },
            line: v.line,
            title: v.title,
            failure_scenario: v.failure_scenario,
            explanation: v.explanation,
            draft_comment: v.draft_comment,
        })
    }
}
impl From<review_agent::ReviewUsage> for ReviewUsageDto {
    fn from(v: review_agent::ReviewUsage) -> Self {
        Self {
            input_tokens: v.input_tokens,
            output_tokens: v.output_tokens,
            tool_calls: v.tool_calls,
        }
    }
}

impl From<review_agent::AgentEvent> for AgentEventDto {
    fn from(event: review_agent::AgentEvent) -> Self {
        let mut dto = Self {
            run_id: event.run_id,
            sequence: event.sequence,
            attempt_id: event.attempt_id,
            event_type: String::new(),
            provider_id: None,
            model_id: None,
            response_id: None,
            delta: None,
            artifact_type: None,
            artifact_field: None,
            artifact_index: None,
            call_id: None,
            tool_name: None,
            usage: None,
            error_code: None,
            will_retry: None,
            approval_id: None,
            risk: None,
            approval_summary: None,
            decision: None,
            tool_outcome: None,
            duration_ms: None,
            content_bytes: None,
            truncated: None,
            tool_error: None,
        };
        match event.kind {
            review_agent::AgentEventKind::ModelAttemptStarted {
                provider_id,
                model_id,
            } => {
                dto.event_type = "model_attempt_started".into();
                dto.provider_id = Some(provider_id);
                dto.model_id = Some(model_id);
            }
            review_agent::AgentEventKind::ModelResponseStarted { response_id } => {
                dto.event_type = "model_response_started".into();
                dto.response_id = response_id;
            }
            review_agent::AgentEventKind::OutputTextDelta { delta } => {
                dto.event_type = "output_text_delta".into();
                dto.delta = Some(delta);
            }
            review_agent::AgentEventKind::ArtifactTextDelta {
                artifact_type,
                field,
                item_index,
                delta,
            } => {
                dto.event_type = "artifact_text_delta".into();
                dto.artifact_type = Some(artifact_type);
                dto.artifact_field = Some(field);
                dto.artifact_index = item_index;
                dto.delta = Some(delta);
            }
            review_agent::AgentEventKind::ArtifactTextReset {
                artifact_type,
                field,
                item_index,
            } => {
                dto.event_type = "artifact_text_reset".into();
                dto.artifact_type = Some(artifact_type);
                dto.artifact_field = Some(field);
                dto.artifact_index = item_index;
            }
            review_agent::AgentEventKind::ToolCallStarted { call_id, name } => {
                dto.event_type = "tool_call_started".into();
                dto.call_id = Some(call_id);
                dto.tool_name = Some(name);
            }
            review_agent::AgentEventKind::ToolArgumentsDelta { call_id, .. } => {
                // Partial provider arguments are deliberately not projected across IPC. They are
                // observational parser state and may contain file content or secrets.
                dto.event_type = "tool_call_progress".into();
                dto.call_id = Some(call_id);
            }
            review_agent::AgentEventKind::ToolValidationFailed {
                call_id,
                tool_name,
                error,
            } => {
                dto.event_type = "tool_validation_failed".into();
                dto.call_id = Some(call_id);
                dto.tool_name = tool_name;
                dto.tool_error = Some(error);
            }
            review_agent::AgentEventKind::ToolApprovalRequested {
                approval_id,
                call_id,
                tool_name,
                risk,
                summary,
            } => {
                dto.event_type = "tool_approval_requested".into();
                dto.approval_id = Some(approval_id);
                dto.call_id = Some(call_id);
                dto.tool_name = Some(tool_name);
                dto.risk = Some(tool_risk_name(risk).into());
                dto.approval_summary = summary;
            }
            review_agent::AgentEventKind::ToolApprovalResolved {
                approval_id,
                call_id,
                decision,
            } => {
                dto.event_type = "tool_approval_resolved".into();
                dto.approval_id = Some(approval_id);
                dto.call_id = Some(call_id);
                dto.decision = Some(permission_decision_name(decision).into());
            }
            review_agent::AgentEventKind::ToolExecutionStarted {
                call_id,
                tool_name,
                risk,
            } => {
                dto.event_type = "tool_call_ready".into();
                dto.call_id = Some(call_id);
                dto.tool_name = Some(tool_name);
                dto.risk = Some(tool_risk_name(risk).into());
            }
            review_agent::AgentEventKind::ToolExecutionCompleted {
                call_id,
                tool_name,
                outcome,
                duration_ms,
                content_bytes,
                truncated,
            } => {
                dto.event_type = "tool_execution_completed".into();
                dto.call_id = Some(call_id);
                dto.tool_name = Some(tool_name);
                dto.tool_outcome = Some(tool_outcome_name(outcome).into());
                dto.duration_ms = Some(duration_ms);
                dto.content_bytes = Some(content_bytes);
                dto.truncated = Some(truncated);
            }
            review_agent::AgentEventKind::UsageUpdated { usage } => {
                dto.event_type = "usage_updated".into();
                dto.usage = Some(usage.into());
            }
            review_agent::AgentEventKind::ModelResponseCompleted => {
                dto.event_type = "model_response_completed".into();
            }
            review_agent::AgentEventKind::ModelAttemptFailed { error, will_retry } => {
                dto.event_type = "model_attempt_failed".into();
                dto.error_code = Some(
                    match error {
                        review_agent::AgentErrorCode::CredentialMissing => "credential_missing",
                        review_agent::AgentErrorCode::AuthenticationFailed => {
                            "authentication_failed"
                        }
                        review_agent::AgentErrorCode::QuotaExceeded => "quota_exceeded",
                        review_agent::AgentErrorCode::InvalidRequest => "invalid_request",
                        review_agent::AgentErrorCode::RateLimited => "rate_limited",
                        review_agent::AgentErrorCode::Network => "network",
                        review_agent::AgentErrorCode::OutputTruncated => "output_truncated",
                        review_agent::AgentErrorCode::InvalidResponse => "invalid_response",
                    }
                    .into(),
                );
                dto.will_retry = Some(will_retry);
            }
        }
        dto
    }
}

fn tool_risk_name(risk: review_agent::ToolRisk) -> &'static str {
    match risk {
        review_agent::ToolRisk::ReadOnly => "read_only",
        review_agent::ToolRisk::Write => "write",
        review_agent::ToolRisk::Destructive => "destructive",
        review_agent::ToolRisk::External => "external",
    }
}

fn permission_decision_name(decision: review_agent::PermissionDecision) -> &'static str {
    match decision {
        review_agent::PermissionDecision::Allow => "allow",
        review_agent::PermissionDecision::Deny => "deny",
        review_agent::PermissionDecision::Ask => "deny",
    }
}

fn tool_outcome_name(outcome: review_agent::ToolOutcome) -> &'static str {
    match outcome {
        review_agent::ToolOutcome::Success => "success",
        review_agent::ToolOutcome::Denied => "denied",
        review_agent::ToolOutcome::Failed => "failed",
        review_agent::ToolOutcome::Timeout => "timeout",
        review_agent::ToolOutcome::Cancelled => "cancelled",
        review_agent::ToolOutcome::InvalidInput => "invalid_input",
    }
}
impl From<review_agent::ReviewRunResult> for ReviewRunResultDto {
    fn from(v: review_agent::ReviewRunResult) -> Self {
        Self {
            run_id: v.run_id,
            head_sha: v.head_sha,
            summary: v.summary,
            reviewed_files: v.reviewed_files,
            findings: v.findings.into_iter().map(Into::into).collect(),
            usage: v.usage.into(),
            model_id: v.model_id,
            duration_ms: v.duration_ms,
            diagnostic_id: v.diagnostic_id,
            provider_attempts: v.provider_attempts,
        }
    }
}
impl TryFrom<SubmitReviewDto> for review_agent::SubmitReview {
    type Error = String;
    fn try_from(v: SubmitReviewDto) -> Result<Self, Self::Error> {
        Ok(Self {
            target: v.target.into(),
            head_sha: v.head_sha,
            findings: v
                .findings
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}
impl From<review_agent::PublishedReview> for PublishedReviewDto {
    fn from(v: review_agent::PublishedReview) -> Self {
        Self {
            review_id: v.review_id,
            html_url: v.html_url,
        }
    }
}

#[cfg(test)]
mod review_dto_contract_tests {
    use super::*;

    fn assert_public_shape_has_no_sensitive_fields(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    assert!(
                        ![
                            "secret",
                            "token",
                            "key",
                            "patch",
                            "prompt",
                            "content",
                            "reasoning"
                        ]
                        .contains(&key.as_str()),
                        "sensitive field {key}"
                    );
                    assert_public_shape_has_no_sensitive_fields(child);
                }
            }
            serde_json::Value::Array(values) => values
                .iter()
                .for_each(assert_public_shape_has_no_sensitive_fields),
            _ => {}
        }
    }

    #[test]
    fn review_file_dto_omits_raw_patch_and_secrets() {
        let source =
            review_agent::ReviewFile::from_patch("src/lib.rs", "@@ -1 +1 @@\n-a\n+b").unwrap();
        let json = serde_json::to_string(&ReviewFileDto::from(source)).unwrap();
        assert_eq!(
            json,
            r#"{"path":"src/lib.rs","patch_bytes":17,"reviewable":true}"#
        );
        assert!(!json.contains("\"patch\":"));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn credential_kind_is_a_stable_lowercase_string() {
        assert_eq!(
            serde_json::to_string(&CredentialKindDto::Deepseek).unwrap(),
            "\"deepseek\""
        );
        assert_eq!(
            serde_json::to_string(&CredentialKindDto::Openai).unwrap(),
            "\"openai\""
        );
        assert_eq!(
            serde_json::to_string(&CredentialKindDto::Anthropic).unwrap(),
            "\"anthropic\""
        );
        assert_eq!(
            serde_json::to_string(&CredentialKindDto::Github).unwrap(),
            "\"github\""
        );
        assert_eq!(
            serde_json::to_string(&CredentialKindDto::Gitlab).unwrap(),
            "\"gitlab\""
        );
    }

    #[test]
    fn review_run_input_contains_no_credentials_or_content() {
        let input = ReviewRunInputDto {
            run_id: "run-1".into(),
            target: ReviewTargetDto {
                owner: "o".into(),
                repo: "r".into(),
                pull_number: 7,
            },
            expected_head_sha: "abc".into(),
            selected_files: vec!["src/lib.rs".into()],
            model_id: "deepseek-v4-flash".into(),
            output_language: ReviewLanguageDto::English,
        };
        let json = serde_json::to_string(&input).unwrap();
        for forbidden in ["token", "key", "prompt", "content", "patch"] {
            assert!(!json.contains(forbidden));
        }
    }

    #[test]
    fn agent_event_dto_has_a_stable_flat_streaming_shape() {
        let dto = AgentEventDto::from(review_agent::AgentEvent {
            run_id: "run-1".into(),
            sequence: 7,
            attempt_id: 2,
            kind: review_agent::AgentEventKind::ModelAttemptFailed {
                error: review_agent::AgentErrorCode::RateLimited,
                will_retry: true,
            },
        });
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({
                "run_id": "run-1",
                "sequence": 7,
                "attempt_id": 2,
                "event_type": "model_attempt_failed",
                "provider_id": null,
                "model_id": null,
                "response_id": null,
                "delta": null,
                "artifact_type": null,
                "artifact_field": null,
                "artifact_index": null,
                "call_id": null,
                "tool_name": null,
                "usage": null,
                "error_code": "rate_limited",
                "will_retry": true
                ,"approval_id": null
                ,"risk": null
                ,"approval_summary": null
                ,"decision": null
                ,"tool_outcome": null
                ,"duration_ms": null
                ,"content_bytes": null
                ,"truncated": null
                ,"tool_error": null
            })
        );
    }

    #[test]
    fn tool_approval_event_exposes_only_sanitized_metadata() {
        let dto = AgentEventDto::from(review_agent::AgentEvent {
            run_id: "run-1".into(),
            sequence: 9,
            attempt_id: 1,
            kind: review_agent::AgentEventKind::ToolApprovalRequested {
                approval_id: "approval-1".into(),
                call_id: "call-1".into(),
                tool_name: "filesystem.write".into(),
                risk: review_agent::ToolRisk::Write,
                summary: Some("Write one repository file".into()),
            },
        });
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("tool_approval_requested"));
        assert!(json.contains("Write one repository file"));
        for forbidden in ["arguments", "result", "prompt", "api_key", "provider_body"] {
            assert!(!json.contains(forbidden));
        }
    }

    #[test]
    fn partial_tool_arguments_are_never_projected_to_ipc() {
        let marker = "PARTIAL_ARGUMENT_SECRET_MARKER";
        let dto = AgentEventDto::from(review_agent::AgentEvent {
            run_id: "run-1".into(),
            sequence: 10,
            attempt_id: 1,
            kind: review_agent::AgentEventKind::ToolArgumentsDelta {
                call_id: "call-1".into(),
                delta: format!(r#"{{"content":"{marker}""#),
            },
        });
        assert_eq!(dto.event_type, "tool_call_progress");
        assert_eq!(dto.call_id.as_deref(), Some("call-1"));
        assert_eq!(dto.delta, None);
        assert!(!serde_json::to_string(&dto).unwrap().contains(marker));
    }

    #[test]
    fn artifact_reset_uses_the_same_flat_target_fields_as_artifact_deltas() {
        let dto = AgentEventDto::from(review_agent::AgentEvent {
            run_id: "run-1".into(),
            sequence: 8,
            attempt_id: 1,
            kind: review_agent::AgentEventKind::ArtifactTextReset {
                artifact_type: "history_investigation".into(),
                field: "finding_title".into(),
                item_index: Some(2),
            },
        });
        assert_eq!(dto.event_type, "artifact_text_reset");
        assert_eq!(dto.artifact_type.as_deref(), Some("history_investigation"));
        assert_eq!(dto.artifact_field.as_deref(), Some("finding_title"));
        assert_eq!(dto.artifact_index, Some(2));
        assert_eq!(dto.delta, None);
    }

    #[test]
    fn issue_publish_dtos_preserve_confirmation_and_stable_action_strings() {
        let input = IssueTriagePublishInputDto {
            publish_id: "batch-1".into(),
            confirmed: true,
            target: IssueTargetDto {
                owner: "acme".into(),
                repo: "rocket".into(),
                issue_number: 7,
            },
            expected_snapshot: IssueSnapshotDto {
                updated_at: "now".into(),
                comments: 1,
            },
            labels: vec!["bug".into()],
            reply: Some("Thanks".into()),
        };
        let domain = review_agent::IssueTriagePublishInput::from(input);
        assert!(domain.confirmed);
        assert_eq!(domain.labels, ["bug"]);

        let result = IssueTriagePublishResultDto::from(review_agent::IssueTriagePublishResult {
            publish_id: "batch-1".into(),
            snapshot: Some(review_agent::IssueSnapshot {
                updated_at: "later".into(),
                comments: 2,
            }),
            actions: vec![review_agent::IssueTriagePublishActionResult {
                action_id: "comment".into(),
                kind: review_agent::IssueTriagePublishActionKind::Comment,
                label: None,
                status: review_agent::IssueTriagePublishActionStatus::AlreadyApplied,
                error_code: None,
            }],
        });
        assert_eq!(result.actions[0].kind, "comment");
        assert_eq!(result.actions[0].status, "already_applied");
        assert_public_shape_has_no_sensitive_fields(&serde_json::to_value(result).unwrap());
    }

    #[test]
    fn every_review_dto_has_the_expected_public_shape_and_converts() {
        let target = review_agent::ReviewTarget {
            owner: "owner".into(),
            repo: "repo".into(),
            pull_number: 42,
        };
        let target_dto = ReviewTargetDto::from(target.clone());
        assert_eq!(
            serde_json::to_value(&target_dto).unwrap(),
            serde_json::json!({"owner":"owner","repo":"repo","pull_number":42})
        );
        assert_eq!(review_agent::ReviewTarget::from(target_dto.clone()), target);

        let file =
            review_agent::ReviewFile::from_patch("src/lib.rs", "@@ -1 +1 @@\n-a\n+b").unwrap();
        let file_dto = ReviewFileDto::from(file.clone());
        assert_eq!(
            serde_json::to_value(&file_dto).unwrap(),
            serde_json::json!({"path":"src/lib.rs","patch_bytes":17,"reviewable":true})
        );
        let preflight_dto = ReviewPreflightDto::from(review_agent::ReviewPreflight {
            head_sha: "abc".into(),
            files: vec![file],
            total_patch_bytes: 17,
            requires_selection: false,
        });
        assert_eq!(
            serde_json::to_value(&preflight_dto).unwrap(),
            serde_json::json!({"head_sha":"abc","files":[{"path":"src/lib.rs","patch_bytes":17,"reviewable":true}],"total_patch_bytes":17,"requires_selection":false})
        );

        let run_input = ReviewRunInputDto {
            run_id: "run".into(),
            target: target_dto.clone(),
            expected_head_sha: "abc".into(),
            selected_files: vec!["src/lib.rs".into()],
            model_id: "deepseek-v4-flash".into(),
            output_language: ReviewLanguageDto::English,
        };
        let domain_input = review_agent::ReviewRunInput::from(run_input.clone());
        assert_eq!(domain_input.run_id, "run");
        assert_eq!(
            domain_input.output_language,
            review_agent::ReviewLanguage::English
        );
        assert_eq!(run_input.model_id, "deepseek-v4-flash");
        assert_eq!(
            serde_json::to_value(&run_input).unwrap()["selected_files"],
            serde_json::json!(["src/lib.rs"])
        );

        let finding = review_agent::ReviewFinding {
            id: "finding".into(),
            severity: review_agent::Severity::High,
            path: "src/lib.rs".into(),
            side: review_agent::ReviewSide::RIGHT,
            line: 2,
            title: "Title".into(),
            failure_scenario: "Scenario".into(),
            explanation: "Explanation".into(),
            draft_comment: "Draft".into(),
        };
        let finding_dto = ReviewFindingDto::from(finding.clone());
        assert_eq!(
            serde_json::to_value(&finding_dto).unwrap(),
            serde_json::json!({"id":"finding","severity":"high","path":"src/lib.rs","side":"RIGHT","line":2,"title":"Title","failure_scenario":"Scenario","explanation":"Explanation","draft_comment":"Draft"})
        );
        assert_eq!(
            review_agent::ReviewFinding::try_from(finding_dto.clone()).unwrap(),
            finding
        );

        let usage = review_agent::ReviewUsage {
            input_tokens: 10,
            cached_input_tokens: 0,
            output_tokens: 4,
            tool_calls: 1,
        };
        let usage_dto = ReviewUsageDto::from(usage.clone());
        assert_eq!(
            serde_json::to_value(&usage_dto).unwrap(),
            serde_json::json!({"input_tokens":10,"output_tokens":4,"tool_calls":1})
        );
        let result_dto = ReviewRunResultDto::from(review_agent::ReviewRunResult {
            run_id: "run".into(),
            head_sha: "abc".into(),
            summary: "One issue.".into(),
            reviewed_files: vec!["src/lib.rs".into()],
            findings: vec![finding.clone()],
            usage,
            model_id: "fixture-model".into(),
            duration_ms: 1250,
            diagnostic_id: "diag-0123456789abcdef".into(),
            provider_attempts: 2,
        });
        assert_eq!(
            serde_json::to_value(&result_dto).unwrap(),
            serde_json::json!({
                "run_id":"run",
                "head_sha":"abc",
                "summary":"One issue.",
                "reviewed_files":["src/lib.rs"],
                "findings":[{"id":"finding","severity":"high","path":"src/lib.rs","side":"RIGHT","line":2,"title":"Title","failure_scenario":"Scenario","explanation":"Explanation","draft_comment":"Draft"}],
                "usage":{"input_tokens":10,"output_tokens":4,"tool_calls":1},
                "model_id":"fixture-model",
                "duration_ms":1250,
                "diagnostic_id":"diag-0123456789abcdef",
                "provider_attempts":2
            })
        );

        let submit_dto = SubmitReviewDto {
            target: target_dto,
            head_sha: "abc".into(),
            findings: vec![finding_dto],
        };
        let submit = review_agent::SubmitReview::try_from(submit_dto.clone()).unwrap();
        assert_eq!(submit.findings, vec![finding]);
        let published_dto = PublishedReviewDto::from(review_agent::PublishedReview {
            review_id: 7,
            html_url: Some("https://example.invalid/7".into()),
        });
        assert_eq!(
            serde_json::to_value(&published_dto).unwrap(),
            serde_json::json!({"review_id":7,"html_url":"https://example.invalid/7"})
        );
        let progress = ReviewProgressEventDto {
            run_id: "run".into(),
            stage: "tool_call".into(),
            tool_name: Some("read_file".into()),
            tool_calls: Some(1),
        };
        assert_eq!(
            serde_json::to_value(&progress).unwrap(),
            serde_json::json!({"run_id":"run","stage":"tool_call","tool_name":"read_file","tool_calls":1})
        );

        for value in [
            serde_json::to_value(preflight_dto).unwrap(),
            serde_json::to_value(run_input).unwrap(),
            serde_json::to_value(result_dto).unwrap(),
            serde_json::to_value(submit_dto).unwrap(),
            serde_json::to_value(published_dto).unwrap(),
            serde_json::to_value(progress).unwrap(),
        ] {
            assert_public_shape_has_no_sensitive_fields(&value);
        }
    }
}
