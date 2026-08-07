//! ipc-types:前后端共享的数据契约(DTO)。
//! 生产项目里这里会接入 specta/ts-rs,从这些结构体自动生成 TypeScript 类型,
//! 让前后端类型在编译期对齐。阶段 0 先保持简单。

use git_core::model::{
    AheadBehind, BlameLine, BranchDeleteImpact, BranchInfo, Commit, CommitRef, ConflictSides,
    DiffLine, DiffLineKind, FetchOutcome, FileChange, FileDiff, FileEntry, FileState, Hunk,
    ImageRef, LineHistoryEntry, MergeOutcome, PullOutcome, PushOutcome, RefKind, ReflogEntry,
    RemoteInfo, Seg, SignatureInfo, SignatureStatus, StashEntry, SubmoduleInfo, SubmoduleStatus,
    WorkingTreeStatus, WorktreeInfo,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub enum CredentialKindDto {
    Deepseek,
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

/// 传给前端的提交 DTO。这里特意和领域模型 Commit 分开:
/// 领域模型可以很丰富,DTO 只暴露前端真正需要的字段。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct CommitDto {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    /// 提交信息正文(首行 summary 之后的部分);无正文时为空串。
    pub body: String,
    pub author_name: String,
    pub author_email: String,
    #[ts(type = "number")]
    pub timestamp: i64,
    pub parents: Vec<String>,
}

impl From<Commit> for CommitDto {
    fn from(c: Commit) -> Self {
        CommitDto {
            id: c.id,
            short_id: c.short_id,
            summary: c.summary,
            body: c.body,
            author_name: c.author.name,
            author_email: c.author.email,
            timestamp: c.timestamp,
            parents: c.parents,
        }
    }
}

/// 一条 reflog 记录 DTO。`new_oid` 是"重置回这一步"时 reset 的目标提交。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ReflogEntryDto {
    pub index: usize,
    pub selector: String,
    pub new_oid: String,
    pub new_short: String,
    pub message: String,
    pub committer_name: String,
    #[ts(type = "number")]
    pub timestamp: i64,
}

impl From<ReflogEntry> for ReflogEntryDto {
    fn from(e: ReflogEntry) -> Self {
        ReflogEntryDto {
            index: e.index,
            selector: e.selector,
            new_oid: e.new_oid,
            new_short: e.new_short,
            message: e.message,
            committer_name: e.committer.name,
            timestamp: e.timestamp,
        }
    }
}

/// 一步 Undo / Redo 的描述:`label`=操作中文名,`target_short`=移动后 HEAD 的短 SHA。
/// 既用于 `undo`/`redo` 的返回(已执行),也用于 [`UndoStateDto`] 里描述「下一步能做什么」。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct UndoStepDto {
    /// 操作的中文名,如 "提交"、"重置(reset)"。
    pub label: String,
    /// 这一步移动后 HEAD 指向的提交短 SHA(供 toast/tooltip 显示"回到 abc1234")。
    pub target_short: String,
    /// 这一步用的还原语义:`true` = 忠实还原了工作区(reset --hard,撤销 reset/cherry-pick 等),
    /// `false` = 只动 HEAD、内容回暂存区(reset --soft,撤销提交)。仅用于 toast 文案精确化。
    pub worktree_restored: bool,
}

/// 撤销/重做的当前可用性。驱动顶栏「撤销」「重做」两个按钮的显隐与文案。
/// `None` = 该方向无可用项(按钮不显);来自 `RepoContext` 内的操作时间线 + 光标。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct UndoStateDto {
    /// 后退一步(撤销最近一次操作)。
    pub can_undo: Option<UndoStepDto>,
    /// 前进一步(重做刚撤销的操作)。
    pub can_redo: Option<UndoStepDto>,
}

/// 操作日志的一项:本工具做过的一次写操作的落点。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct OpLogEntryDto {
    /// 操作中文名,如 "提交"、"cherry-pick";时间线基点为 "起点"。
    pub label: String,
    /// 该操作后 HEAD 的短 SHA。
    pub target_short: String,
    /// Unix 时间戳(秒),供「几分钟前」显示。
    #[ts(type = "number")]
    pub timestamp: i64,
}

/// 操作日志面板数据:本会话写操作时间线(oldest→newest)+ 当前光标位置。
/// 点击第 i 项 = goto(i),沿时间线 reset --soft 跳过去。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct OpLogDto {
    pub entries: Vec<OpLogEntryDto>,
    /// 当前 HEAD 所在的项下标(高亮「现在在哪」)。
    pub current: usize,
}

/// 跨 IPC 边界的错误:带错误码(前端做逻辑分支)+ 友好信息 + 是否可重试。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct IpcError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct AgentIpcErrorDto {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    pub diagnostic_id: String,
}

impl AgentIpcErrorDto {
    pub fn from_ipc(error: IpcError, diagnostic_id: impl Into<String>) -> Self {
        Self {
            code: error.code,
            message: error.message,
            recoverable: error.recoverable,
            diagnostic_id: diagnostic_id.into(),
        }
    }
}

/// 工作区单个文件状态的 DTO。state 用字符串,前端直接渲染徽章。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct FileEntryDto {
    pub path: String,
    pub state: String, // modified | added | deleted | renamed | untracked | conflicted
    pub staged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct StatusDto {
    pub entries: Vec<FileEntryDto>,
}

impl From<FileEntry> for FileEntryDto {
    fn from(e: FileEntry) -> Self {
        let state = match e.state {
            FileState::Added => "added",
            FileState::Modified => "modified",
            FileState::Deleted => "deleted",
            FileState::Renamed => "renamed",
            FileState::Untracked => "untracked",
            FileState::Conflicted => "conflicted",
        };
        FileEntryDto {
            path: e.path,
            state: state.to_string(),
            staged: e.staged,
        }
    }
}

impl From<WorkingTreeStatus> for StatusDto {
    fn from(s: WorkingTreeStatus) -> Self {
        StatusDto {
            entries: s.entries.into_iter().map(FileEntryDto::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct FileChangeDto {
    pub path: String,
    pub status: String, // added | modified | deleted | renamed | untracked | conflicted
    pub additions: usize,
    pub deletions: usize,
}

impl From<FileChange> for FileChangeDto {
    fn from(c: FileChange) -> Self {
        let status = match c.status {
            FileState::Added => "added",
            FileState::Modified => "modified",
            FileState::Deleted => "deleted",
            FileState::Renamed => "renamed",
            FileState::Untracked => "untracked",
            FileState::Conflicted => "conflicted",
        };
        FileChangeDto {
            path: c.path,
            status: status.to_string(),
            additions: c.additions,
            deletions: c.deletions,
        }
    }
}

/// 当前分支相对上游的领先/落后 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct AheadBehindDto {
    pub ahead: usize,
    pub behind: usize,
}

impl From<AheadBehind> for AheadBehindDto {
    fn from(a: AheadBehind) -> Self {
        AheadBehindDto {
            ahead: a.ahead,
            behind: a.behind,
        }
    }
}

/// blame 一行 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct BlameLineDto {
    pub line_no: u32,
    pub commit_id: String,
    pub short_id: String,
    pub author_name: String,
    #[ts(type = "number")]
    pub timestamp: i64,
    pub content: String,
}

impl From<BlameLine> for BlameLineDto {
    fn from(l: BlameLine) -> Self {
        BlameLineDto {
            line_no: l.line_no,
            commit_id: l.commit_id,
            short_id: l.short_id,
            author_name: l.author_name,
            timestamp: l.timestamp,
            content: l.content,
        }
    }
}

/// 提交签名 DTO。status: "none" | "good" | "unverified" | "bad"。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct SignatureInfoDto {
    pub status: String,
    pub signer: String,
}

impl From<SignatureInfo> for SignatureInfoDto {
    fn from(s: SignatureInfo) -> Self {
        let status = match s.status {
            SignatureStatus::None => "none",
            SignatureStatus::Good => "good",
            SignatureStatus::Unverified => "unverified",
            SignatureStatus::Bad => "bad",
        };
        SignatureInfoDto {
            status: status.into(),
            signer: s.signer,
        }
    }
}

/// 子模块 DTO。status: "uninitialized" | "up-to-date" | "modified" | "conflict"。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct SubmoduleInfoDto {
    pub path: String,
    pub url: String,
    pub head_sha: String,
    pub short_sha: String,
    pub status: String,
    pub describe: String,
}

impl From<SubmoduleInfo> for SubmoduleInfoDto {
    fn from(s: SubmoduleInfo) -> Self {
        let status = match s.status {
            SubmoduleStatus::Uninitialized => "uninitialized",
            SubmoduleStatus::UpToDate => "up-to-date",
            SubmoduleStatus::Modified => "modified",
            SubmoduleStatus::Conflict => "conflict",
        };
        SubmoduleInfoDto {
            short_sha: s.head_sha.chars().take(7).collect(),
            path: s.path,
            url: s.url,
            head_sha: s.head_sha,
            status: status.into(),
            describe: s.describe,
        }
    }
}

/// 工作树 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct WorktreeInfoDto {
    pub path: String,
    pub head_sha: String,
    pub short_sha: String,
    pub branch: String,
    pub is_main: bool,
    pub is_current: bool,
    pub detached: bool,
    pub locked: bool,
    pub bare: bool,
}

impl From<WorktreeInfo> for WorktreeInfoDto {
    fn from(w: WorktreeInfo) -> Self {
        WorktreeInfoDto {
            short_sha: w.head_sha.chars().take(7).collect(),
            path: w.path,
            head_sha: w.head_sha,
            branch: w.branch,
            is_main: w.is_main,
            is_current: w.is_current,
            detached: w.detached,
            locked: w.locked,
            bare: w.bare,
        }
    }
}

/// 冲突文件三方内容 DTO(base/ours/theirs,某方缺失为 null)。供三栏合并编辑器用。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ConflictSidesDto {
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
}

impl From<ConflictSides> for ConflictSidesDto {
    fn from(s: ConflictSides) -> Self {
        ConflictSidesDto {
            base: s.base,
            ours: s.ours,
            theirs: s.theirs,
        }
    }
}

/// 一条贮藏 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct StashDto {
    pub index: usize,
    pub message: String,
}

impl From<StashEntry> for StashDto {
    fn from(s: StashEntry) -> Self {
        StashDto {
            index: s.index,
            message: s.message,
        }
    }
}

/// 本地分支 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct BranchDto {
    pub name: String,
    pub is_head: bool,
}

impl From<BranchInfo> for BranchDto {
    fn from(b: BranchInfo) -> Self {
        BranchDto {
            name: b.name,
            is_head: b.is_head,
        }
    }
}

/// 删分支影响预览 DTO:`unmerged_commits`>0 时前端做强危险二次确认。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct BranchDeleteImpactDto {
    pub unmerged_commits: usize,
    pub sample_summaries: Vec<String>,
}

impl From<BranchDeleteImpact> for BranchDeleteImpactDto {
    fn from(i: BranchDeleteImpact) -> Self {
        BranchDeleteImpactDto {
            unmerged_commits: i.unmerged_commits,
            sample_summaries: i.sample_summaries,
        }
    }
}

/// 一个远程的展示信息 DTO(名字 + fetch URL)。供「管理远程」面板。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct RemoteInfoDto {
    pub name: String,
    pub url: String,
}

impl From<RemoteInfo> for RemoteInfoDto {
    fn from(r: RemoteInfo) -> Self {
        RemoteInfoDto {
            name: r.name,
            url: r.url,
        }
    }
}

/// 一次 fetch 的结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct FetchResultDto {
    pub remote: String,
    pub summary: String,
}

impl From<FetchOutcome> for FetchResultDto {
    fn from(o: FetchOutcome) -> Self {
        FetchResultDto {
            remote: o.remote,
            summary: o.summary,
        }
    }
}

/// 一次 pull 的成功结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct PullResultDto {
    pub summary: String,
}

impl From<PullOutcome> for PullResultDto {
    fn from(o: PullOutcome) -> Self {
        PullResultDto { summary: o.summary }
    }
}

/// 一次「合并某分支进当前分支」的成功结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct MergeResultDto {
    pub summary: String,
    /// 本次是否为快进合并(未产生合并提交)。
    pub fast_forward: bool,
}

impl From<MergeOutcome> for MergeResultDto {
    fn from(o: MergeOutcome) -> Self {
        MergeResultDto {
            summary: o.summary,
            fast_forward: o.fast_forward,
        }
    }
}

/// 一次 push 的成功结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct PushResultDto {
    pub summary: String,
    /// 本次是否顺带建立了上游(首次 push 自动 -u 时为 true,前端可特别提示)。
    pub set_upstream: bool,
}

impl From<PushOutcome> for PushResultDto {
    fn from(o: PushOutcome) -> Self {
        PushResultDto {
            summary: o.summary,
            set_upstream: o.set_upstream,
        }
    }
}

/// 图谱中一段连线:在某行单元格内,从 `from` 列连到 `to` 列。
/// 上半段语义为 顶边→节点(中点),下半段为 节点(中点)→底边。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct GraphSegDto {
    pub from: u32,
    pub to: u32,
    pub color: u32,
}

/// 指向某 commit 的引用 DTO。kind:head | local | remote。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct RefDto {
    pub name: String,
    pub kind: String,
}

impl From<CommitRef> for RefDto {
    fn from(r: CommitRef) -> Self {
        let kind = match r.kind {
            RefKind::Head => "head",
            RefKind::LocalBranch => "local",
            RefKind::RemoteBranch => "remote",
            RefKind::Tag => "tag",
        };
        RefDto {
            name: r.name,
            kind: kind.to_string(),
        }
    }
}

/// 图谱一行:嵌入提交信息 + 节点列/颜色 + 上下半段连线 + 指向该提交的引用。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct GraphRowDto {
    pub commit: CommitDto,
    pub column: u32,
    pub color: u32,
    pub top: Vec<GraphSegDto>,
    pub bottom: Vec<GraphSegDto>,
    /// 指向本行提交的引用(分支/远程/HEAD);多数行为空。
    pub refs: Vec<RefDto>,
    /// 同步标记:"" 普通 | "outgoing" 已 commit 未 push | "incoming" 已 fetch 未 pull。
    pub sync: String,
}

/// 行内一段 DTO。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct SegDto {
    pub text: String,
    pub changed: bool,
}

impl From<Seg> for SegDto {
    fn from(s: Seg) -> Self {
        SegDto {
            text: s.text,
            changed: s.changed,
        }
    }
}

/// 行级 diff 的一行 DTO。kind:context | add | del。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct DiffLineDto {
    pub kind: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
    /// 行内词级段;None = 整行着色。
    #[ts(optional)]
    pub emphasis: Option<Vec<SegDto>>,
}

impl From<DiffLine> for DiffLineDto {
    fn from(l: DiffLine) -> Self {
        let kind = match l.kind {
            DiffLineKind::Context => "context",
            DiffLineKind::Addition => "add",
            DiffLineKind::Deletion => "del",
        };
        DiffLineDto {
            kind: kind.to_string(),
            old_lineno: l.old_lineno,
            new_lineno: l.new_lineno,
            content: l.content,
            emphasis: l
                .emphasis
                .map(|segs| segs.into_iter().map(SegDto::from).collect()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct HunkDto {
    pub header: String,
    pub lines: Vec<DiffLineDto>,
}

impl From<Hunk> for HunkDto {
    fn from(h: Hunk) -> Self {
        HunkDto {
            header: h.header,
            lines: h.lines.into_iter().map(DiffLineDto::from).collect(),
        }
    }
}

/// 一侧图片的取图句柄(M6.2:不再内联 base64)。前端用 `(mime, oid)` + 文件路径经
/// `read_image` 命令取原始字节转 Blob URL。`oid` 空串表示读工作区文件。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct ImageRefDto {
    pub mime: String,
    pub oid: String,
}

impl From<ImageRef> for ImageRefDto {
    fn from(i: ImageRef) -> Self {
        ImageRefDto {
            mime: i.mime,
            oid: i.oid,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct FileDiffDto {
    pub path: String,
    pub is_binary: bool,
    pub too_large: bool,
    pub is_lfs_pointer: bool,
    pub lfs_size: String,
    pub is_image: bool,
    pub old_image: Option<ImageRefDto>,
    pub new_image: Option<ImageRefDto>,
    pub hunks: Vec<HunkDto>,
}

impl From<FileDiff> for FileDiffDto {
    fn from(d: FileDiff) -> Self {
        FileDiffDto {
            path: d.path,
            is_binary: d.is_binary,
            too_large: d.too_large,
            is_lfs_pointer: d.is_lfs_pointer,
            lfs_size: d.lfs_size,
            is_image: d.is_image,
            old_image: d.old_image.map(ImageRefDto::from),
            new_image: d.new_image.map(ImageRefDto::from),
            hunks: d.hunks.into_iter().map(HunkDto::from).collect(),
        }
    }
}

/// 行历史的一条:某提交 + 它对选中行范围的 diff。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/src/bindings/")]
pub struct LineHistoryEntryDto {
    pub commit: CommitDto,
    pub diff: FileDiffDto,
}

impl From<LineHistoryEntry> for LineHistoryEntryDto {
    fn from(e: LineHistoryEntry) -> Self {
        LineHistoryEntryDto {
            commit: CommitDto::from(e.commit),
            diff: FileDiffDto::from(e.diff),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::model::{FileEntry, FileState, WorkingTreeStatus};

    #[test]
    fn maps_file_change_to_dto() {
        use git_core::model::{FileChange, FileState};
        let dto = FileChangeDto::from(FileChange {
            path: "a.rs".into(),
            status: FileState::Deleted,
            additions: 0,
            deletions: 5,
        });
        assert_eq!(dto.path, "a.rs");
        assert_eq!(dto.status, "deleted");
        assert_eq!(dto.additions, 0);
        assert_eq!(dto.deletions, 5);
    }

    #[test]
    fn maps_reflog_entry_to_dto() {
        use git_core::model::{ReflogEntry, Signature};
        let dto = ReflogEntryDto::from(ReflogEntry {
            index: 0,
            selector: "HEAD@{0}".into(),
            new_oid: "abcdef1234567890".into(),
            new_short: "abcdef1".into(),
            message: "commit: hello".into(),
            committer: Signature {
                name: "Tester".into(),
                email: "t@e".into(),
            },
            timestamp: 42,
        });
        assert_eq!(dto.selector, "HEAD@{0}");
        assert_eq!(dto.new_short, "abcdef1");
        assert_eq!(dto.committer_name, "Tester");
        assert_eq!(dto.timestamp, 42);
    }

    #[test]
    fn maps_status_to_dto_with_string_state() {
        let st = WorkingTreeStatus {
            entries: vec![
                FileEntry {
                    path: "a.txt".into(),
                    state: FileState::Modified,
                    staged: false,
                },
                FileEntry {
                    path: "b.txt".into(),
                    state: FileState::Added,
                    staged: true,
                },
            ],
        };
        let dto = StatusDto::from(st);
        assert_eq!(dto.entries.len(), 2);
        assert_eq!(dto.entries[0].state, "modified");
        assert!(!dto.entries[0].staged);
        assert_eq!(dto.entries[1].state, "added");
    }
}
