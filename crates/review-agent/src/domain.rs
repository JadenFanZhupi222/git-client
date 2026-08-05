use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path};
use thiserror::Error;

pub const MAX_AUTO_FILES: usize = 30;
pub const MAX_PATCH_BYTES: usize = 200_000;
pub const MAX_MODEL_ROUNDS: usize = 8;
pub const MAX_TOOL_CALLS: usize = 20;
pub const MAX_READ_LINES: u32 = 400;
pub const MAX_TOOL_OUTPUT_BYTES: usize = 300_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewTarget {
    pub owner: String,
    pub repo: String,
    pub pull_number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewFile {
    pub path: String,
    pub patch: Option<String>,
    pub patch_bytes: usize,
    pub reviewable: bool,
}

impl ReviewFile {
    pub fn from_patch(
        path: impl Into<String>,
        patch: impl Into<String>,
    ) -> Result<Self, ReviewError> {
        let path = path.into();
        validate_repository_path(&path)?;
        let patch = patch.into();
        let patch_bytes = patch.len();
        Ok(Self {
            path,
            patch: Some(patch),
            patch_bytes,
            reviewable: patch_bytes <= MAX_PATCH_BYTES,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewPreflight {
    pub head_sha: String,
    pub files: Vec<ReviewFile>,
    pub total_patch_bytes: usize,
    pub requires_selection: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRunInput {
    pub run_id: String,
    pub target: ReviewTarget,
    pub expected_head_sha: String,
    pub selected_files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReviewSide {
    LEFT,
    RIGHT,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewFinding {
    pub id: String,
    pub severity: Severity,
    pub path: String,
    pub side: ReviewSide,
    pub line: u32,
    pub title: String,
    pub failure_scenario: String,
    pub explanation: String,
    pub draft_comment: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_calls: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRunResult {
    pub run_id: String,
    pub head_sha: String,
    pub summary: String,
    pub reviewed_files: Vec<String>,
    pub findings: Vec<ReviewFinding>,
    pub usage: ReviewUsage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitReview {
    pub target: ReviewTarget,
    pub head_sha: String,
    pub findings: Vec<ReviewFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedReview {
    pub review_id: u64,
    pub html_url: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ReviewError {
    #[error("AI API key is missing")]
    AiKeyMissing,
    #[error("GitHub token is missing")]
    GithubTokenMissing,
    #[error("authentication failed")]
    AuthFailed,
    #[error("service rate limit exceeded")]
    RateLimited,
    #[error("network request failed: {0}")]
    NetworkError(String),
    #[error("pull request head changed")]
    PrUpdated,
    #[error("review budget exceeded")]
    ReviewBudgetExceeded,
    #[error("invalid model output: {0}")]
    InvalidModelOutput(String),
    #[error("review cancelled")]
    Cancelled,
    #[error("review publish failed: {0}")]
    ReviewPublishFailed(String),
}

impl ReviewError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::AiKeyMissing => "AI_KEY_MISSING",
            Self::GithubTokenMissing => "GITHUB_TOKEN_MISSING",
            Self::AuthFailed => "AUTH_FAILED",
            Self::RateLimited => "RATE_LIMITED",
            Self::NetworkError(_) => "NETWORK_ERROR",
            Self::PrUpdated => "PR_UPDATED",
            Self::ReviewBudgetExceeded => "REVIEW_BUDGET_EXCEEDED",
            Self::InvalidModelOutput(_) => "INVALID_MODEL_OUTPUT",
            Self::Cancelled => "CANCELLED",
            Self::ReviewPublishFailed(_) => "REVIEW_PUBLISH_FAILED",
        }
    }
}

pub fn validate_repository_path(path: &str) -> Result<(), ReviewError> {
    if path.is_empty() || path.contains('\\') || path.contains('\0') {
        return Err(ReviewError::InvalidModelOutput(
            "unsafe repository path".into(),
        ));
    }
    let parsed = Path::new(path);
    if parsed.is_absolute()
        || path.as_bytes().get(1) == Some(&b':')
        || parsed.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ReviewError::InvalidModelOutput(
            "unsafe repository path".into(),
        ));
    }
    Ok(())
}

pub fn map_patch_lines(patch: &str) -> Result<HashSet<(String, u32)>, ReviewError> {
    let mut result = HashSet::new();
    let mut old = 0u32;
    let mut new = 0u32;
    let mut in_hunk = false;
    for text in patch.lines() {
        if let Some(header) = text.strip_prefix("@@ ") {
            let end = header
                .find(" @@")
                .ok_or_else(|| ReviewError::InvalidModelOutput("invalid patch hunk".into()))?;
            let mut ranges = header[..end].split_whitespace();
            old = parse_hunk_start(ranges.next(), '-')?;
            new = parse_hunk_start(ranges.next(), '+')?;
            in_hunk = true;
        } else if in_hunk && text.starts_with('-') {
            result.insert(("LEFT".into(), old));
            old += 1;
        } else if in_hunk && text.starts_with('+') {
            result.insert(("RIGHT".into(), new));
            new += 1;
        } else if in_hunk && !text.starts_with('\\') {
            result.insert(("LEFT".into(), old));
            result.insert(("RIGHT".into(), new));
            old += 1;
            new += 1;
        }
    }
    Ok(result)
}

fn parse_hunk_start(value: Option<&str>, prefix: char) -> Result<u32, ReviewError> {
    value
        .and_then(|v| v.strip_prefix(prefix))
        .and_then(|v| v.split(',').next())
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ReviewError::InvalidModelOutput("invalid patch hunk".into()))
}
