use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use agent_runtime::{ModelPricing, ModelUsage, ToolIntent, ToolReceipt};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_ID_BYTES: usize = 128;
const MAX_OBJECTIVE_BYTES: usize = 64 * 1024;
const MAX_STEERING_BYTES: usize = 64 * 1024;
const MAX_SUMMARY_BYTES: usize = 64 * 1024;
const MAX_EVIDENCE: usize = 512;
const MAX_RECEIPTS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentGoalStatus {
    Queued,
    Running,
    AwaitingApproval,
    Pausing,
    Paused,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

impl AgentGoalStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    User,
    AppRestarted,
    Budget,
    ProviderUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockReason {
    WorkspaceConflict,
    AmbiguousToolEffect,
    NoProgress,
    VerifierRejected,
    CheckpointCorrupt,
    StorageLocked,
    RunawayGuard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceSnapshot {
    pub currency: String,
    pub input_cache_hit_per_million_micros: u64,
    pub input_cache_miss_per_million_micros: u64,
    pub output_per_million_micros: u64,
    pub source_url: String,
    pub source_version: String,
    pub checked_at: String,
}

impl From<ModelPricing> for PriceSnapshot {
    fn from(value: ModelPricing) -> Self {
        Self {
            currency: value.currency,
            input_cache_hit_per_million_micros: value.input_cache_hit_per_million_micros,
            input_cache_miss_per_million_micros: value.input_cache_miss_per_million_micros,
            output_per_million_micros: value.output_per_million_micros,
            source_url: value.source_url,
            source_version: value.source_version,
            checked_at: value.checked_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelBudgetLimit {
    CostMicros { currency: String, limit_micros: u64 },
    Tokens { limit_tokens: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBudgetAccount {
    pub model_id: String,
    pub usage: ModelUsage,
    pub price: Option<PriceSnapshot>,
    pub limit: ModelBudgetLimit,
    pub spent_micros: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetCharge {
    pub spent_micros: u64,
    pub exceeded: bool,
}

impl AgentBudgetAccount {
    pub fn new(
        model_id: impl Into<String>,
        price: Option<PriceSnapshot>,
        limit: ModelBudgetLimit,
    ) -> Result<Self, GoalError> {
        let model_id = model_id.into();
        validate_id(&model_id)?;
        validate_budget(&price, &limit)?;
        Ok(Self {
            model_id,
            usage: ModelUsage::default(),
            price,
            limit,
            spent_micros: 0,
        })
    }

    pub fn record_usage(&mut self, usage: &ModelUsage) -> Result<BudgetCharge, GoalError> {
        self.usage.input_tokens = self
            .usage
            .input_tokens
            .checked_add(usage.input_tokens)
            .ok_or(GoalError::Capacity)?;
        self.usage.cached_input_tokens = self
            .usage
            .cached_input_tokens
            .checked_add(usage.cached_input_tokens.min(usage.input_tokens))
            .ok_or(GoalError::Capacity)?;
        self.usage.output_tokens = self
            .usage
            .output_tokens
            .checked_add(usage.output_tokens)
            .ok_or(GoalError::Capacity)?;
        self.usage.tool_calls = self
            .usage
            .tool_calls
            .checked_add(usage.tool_calls)
            .ok_or(GoalError::Capacity)?;
        self.spent_micros = self.calculate_spent_micros()?;
        Ok(BudgetCharge {
            spent_micros: self.spent_micros,
            exceeded: self.is_exceeded(),
        })
    }

    pub fn extend(&mut self, next: ModelBudgetLimit) -> Result<(), GoalError> {
        match (&self.limit, &next) {
            (
                ModelBudgetLimit::CostMicros {
                    currency,
                    limit_micros,
                },
                ModelBudgetLimit::CostMicros {
                    currency: next_currency,
                    limit_micros: next_limit,
                },
            ) if currency == next_currency && next_limit > limit_micros => {}
            (
                ModelBudgetLimit::Tokens { limit_tokens },
                ModelBudgetLimit::Tokens {
                    limit_tokens: next_limit,
                },
            ) if next_limit > limit_tokens => {}
            _ => return Err(GoalError::InvalidBudget),
        }
        self.limit = next;
        Ok(())
    }

    pub fn is_exceeded(&self) -> bool {
        match self.limit {
            ModelBudgetLimit::CostMicros { limit_micros, .. } => self.spent_micros >= limit_micros,
            ModelBudgetLimit::Tokens { limit_tokens } => {
                self.usage
                    .input_tokens
                    .saturating_add(self.usage.output_tokens)
                    >= limit_tokens
            }
        }
    }

    fn calculate_spent_micros(&self) -> Result<u64, GoalError> {
        let Some(price) = &self.price else {
            return Ok(0);
        };
        let cached = self.usage.cached_input_tokens.min(self.usage.input_tokens);
        let misses = self.usage.input_tokens.saturating_sub(cached);
        let numerator = u128::from(cached)
            .checked_mul(u128::from(price.input_cache_hit_per_million_micros))
            .and_then(|value| {
                value.checked_add(
                    u128::from(misses) * u128::from(price.input_cache_miss_per_million_micros),
                )
            })
            .and_then(|value| {
                value.checked_add(
                    u128::from(self.usage.output_tokens)
                        * u128::from(price.output_per_million_micros),
                )
            })
            .ok_or(GoalError::Capacity)?;
        u64::try_from(numerator.div_ceil(1_000_000)).map_err(|_| GoalError::Capacity)
    }
}

fn validate_budget(
    price: &Option<PriceSnapshot>,
    limit: &ModelBudgetLimit,
) -> Result<(), GoalError> {
    match limit {
        ModelBudgetLimit::CostMicros {
            currency,
            limit_micros,
        } if *limit_micros > 0
            && price
                .as_ref()
                .is_some_and(|snapshot| snapshot.currency == *currency) =>
        {
            Ok(())
        }
        ModelBudgetLimit::Tokens { limit_tokens } if *limit_tokens > 0 && price.is_none() => Ok(()),
        _ => Err(GoalError::InvalidBudget),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteeringMessage {
    pub sequence: u64,
    pub created_at_ms: u64,
    pub content: String,
    pub injected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkingEvidence {
    pub source: String,
    pub digest: String,
    pub content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCheckpoint {
    pub slice_index: u32,
    pub working_summary: String,
    pub recent_transcript: Vec<agent_runtime::TranscriptItem>,
    pub evidence: Vec<WorkingEvidence>,
    pub pending_intents: Vec<ToolIntent>,
    pub receipts: Vec<ToolReceipt>,
    #[serde(default)]
    pub superseded_execution_ids: Vec<String>,
    pub verifier_gaps: Vec<String>,
    pub next_actions: Vec<String>,
    #[serde(default)]
    pub progress: ProgressTracker,
    pub repository_digest: String,
    pub saved_at_ms: u64,
}

impl AgentCheckpoint {
    pub fn empty(repository_digest: impl Into<String>, now_ms: u64) -> Self {
        Self {
            slice_index: 0,
            working_summary: String::new(),
            recent_transcript: Vec::new(),
            evidence: Vec::new(),
            pending_intents: Vec::new(),
            receipts: Vec::new(),
            superseded_execution_ids: Vec::new(),
            verifier_gaps: Vec::new(),
            next_actions: Vec::new(),
            progress: ProgressTracker::default(),
            repository_digest: repository_digest.into(),
            saved_at_ms: now_ms,
        }
    }

    pub fn validate(&self) -> Result<(), GoalError> {
        if self.working_summary.len() > MAX_SUMMARY_BYTES
            || self.evidence.len() > MAX_EVIDENCE
            || self.receipts.len() > MAX_RECEIPTS
            || self.superseded_execution_ids.len() > MAX_RECEIPTS
            || self.pending_intents.len() > MAX_RECEIPTS
            || self.repository_digest.is_empty()
        {
            return Err(GoalError::InvalidContent);
        }
        Ok(())
    }

    pub fn compact_covered_evidence(&mut self) {
        for evidence in self.evidence.iter_mut().rev().skip(2) {
            evidence.content = None;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCompletionCandidate {
    pub text: String,
    pub remaining_work: Vec<String>,
    pub created_at_ms: u64,
    pub model_responses: u32,
    pub used_tools: bool,
    #[serde(default)]
    pub verification: Option<VerificationResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationDecision {
    Accepted,
    Continue,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationResult {
    pub decision: VerificationDecision,
    pub gaps: Vec<String>,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentGoalResult {
    pub text: String,
    pub committed_at_ms: u64,
    pub verifier: Option<VerificationResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentGoal {
    pub goal_id: String,
    pub session_id: String,
    pub objective: String,
    pub model_id: String,
    pub repository_identity: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub revision: u64,
    pub status: AgentGoalStatus,
    pub pause_reason: Option<PauseReason>,
    pub block_reason: Option<BlockReason>,
    pub usage_by_model: BTreeMap<String, AgentBudgetAccount>,
    pub steering_messages: Vec<SteeringMessage>,
    pub checkpoint: AgentCheckpoint,
    pub completion_candidate: Option<AgentCompletionCandidate>,
    pub result: Option<AgentGoalResult>,
}

impl AgentGoal {
    pub fn active_budget_mut(&mut self) -> Result<&mut AgentBudgetAccount, GoalError> {
        self.usage_by_model
            .get_mut(&self.model_id)
            .ok_or(GoalError::InvalidBudget)
    }

    pub fn requires_independent_verifier(&self) -> bool {
        let Some(candidate) = &self.completion_candidate else {
            return false;
        };
        candidate.used_tools
            || candidate.model_responses > 1
            || self.checkpoint.slice_index > 0
            || !self.steering_messages.is_empty()
            || self.checkpoint.receipts.iter().any(ToolReceipt::is_effect)
    }

    pub fn prepare_for_restart(&mut self, now_ms: u64) {
        if !self.status.is_terminal() && self.status != AgentGoalStatus::Blocked {
            self.status = AgentGoalStatus::Paused;
            self.pause_reason = Some(PauseReason::AppRestarted);
            self.block_reason = None;
            self.revision = self.revision.saturating_add(1);
            self.updated_at_ms = now_ms;
            self.checkpoint
                .pending_intents
                .iter_mut()
                .for_each(|intent| {
                    intent.approval_id = None;
                    intent.approved = false;
                });
        }
    }

    pub fn clear_active_working_data(&mut self) {
        self.completion_candidate = None;
        self.checkpoint.pending_intents.clear();
        self.checkpoint.recent_transcript.clear();
        self.checkpoint.working_summary.clear();
        self.checkpoint.next_actions.clear();
        self.checkpoint.verifier_gaps.clear();
        for evidence in &mut self.checkpoint.evidence {
            evidence.content = None;
        }
        for steering in &mut self.steering_messages {
            steering.content.clear();
            steering.injected = true;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableAgentSession {
    pub format_version: u16,
    pub session_id: String,
    pub repository_identity: String,
    pub revision: u64,
    pub memory_summary: Option<String>,
    pub recent_messages: Vec<crate::SessionMessage>,
    pub active_goal: Option<AgentGoal>,
}

impl DurableAgentSession {
    pub const FORMAT_VERSION: u16 = 1;

    pub fn new(session_id: impl Into<String>, repository_identity: impl Into<String>) -> Self {
        Self {
            format_version: Self::FORMAT_VERSION,
            session_id: session_id.into(),
            repository_identity: repository_identity.into(),
            revision: 0,
            memory_summary: None,
            recent_messages: Vec::new(),
            active_goal: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GoalSliceLimits {
    pub max_active_ms: u64,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_sanitized_tool_result_bytes: usize,
    pub runaway_model_rounds: u32,
    pub runaway_tool_calls: u32,
}

impl Default for GoalSliceLimits {
    fn default() -> Self {
        Self {
            max_active_ms: 120_000,
            max_input_tokens: 250_000,
            max_output_tokens: 16_000,
            max_sanitized_tool_result_bytes: 2 * 1024 * 1024,
            runaway_model_rounds: 512,
            runaway_tool_calls: 1_024,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressTracker {
    pub last_digest: Option<String>,
    pub consecutive_no_progress: u8,
    pub recovery_attempted: bool,
}

impl ProgressTracker {
    pub fn observe(&mut self, digest: impl Into<String>) -> ProgressAction {
        let digest = digest.into();
        if self.last_digest.as_deref() != Some(&digest) {
            self.last_digest = Some(digest);
            self.consecutive_no_progress = 0;
            self.recovery_attempted = false;
            return ProgressAction::Continue;
        }
        self.consecutive_no_progress = self.consecutive_no_progress.saturating_add(1);
        if !self.recovery_attempted && self.consecutive_no_progress >= 4 {
            self.recovery_attempted = true;
            self.consecutive_no_progress = 0;
            ProgressAction::RecoverySlice
        } else if self.recovery_attempted && self.consecutive_no_progress >= 2 {
            ProgressAction::Block
        } else {
            ProgressAction::Continue
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressAction {
    Continue,
    RecoverySlice,
    Block,
}

pub trait GoalPersistence: Send + Sync + 'static {
    fn load(&self, session_id: &str) -> Result<Option<DurableAgentSession>, GoalError>;
    fn save(&self, session: &DurableAgentSession) -> Result<(), GoalError>;
    fn remove(&self, session_id: &str) -> Result<(), GoalError>;
}

pub struct GoalRepository<P: GoalPersistence> {
    persistence: P,
    sessions: Mutex<HashMap<String, DurableAgentSession>>,
}

impl<P: GoalPersistence> GoalRepository<P> {
    pub fn new(persistence: P) -> Self {
        Self {
            persistence,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn load_or_create(
        &self,
        session_id: &str,
        repository_identity: &str,
    ) -> Result<DurableAgentSession, GoalError> {
        validate_id(session_id)?;
        validate_id(repository_identity)?;
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(session) = sessions.get(session_id) {
            return Ok(session.clone());
        }
        let session = self
            .persistence
            .load(session_id)?
            .unwrap_or_else(|| DurableAgentSession::new(session_id, repository_identity));
        if session.format_version != DurableAgentSession::FORMAT_VERSION
            || session.repository_identity != repository_identity
        {
            return Err(GoalError::UnsupportedVersion);
        }
        sessions.insert(session_id.to_owned(), session.clone());
        Ok(session)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_goal(
        &self,
        session_id: &str,
        repository_identity: &str,
        goal_id: &str,
        objective: String,
        model_id: String,
        budget: AgentBudgetAccount,
        repository_digest: String,
        now_ms: u64,
    ) -> Result<AgentGoal, GoalError> {
        validate_id(goal_id)?;
        validate_id(&model_id)?;
        validate_content(&objective, MAX_OBJECTIVE_BYTES)?;
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let session = sessions
            .get_mut(session_id)
            .ok_or(GoalError::SessionNotLoaded)?;
        if session
            .active_goal
            .as_ref()
            .is_some_and(|goal| !goal.status.is_terminal())
        {
            return Err(GoalError::Busy);
        }
        let mut usage_by_model = BTreeMap::new();
        usage_by_model.insert(model_id.clone(), budget);
        let goal = AgentGoal {
            goal_id: goal_id.to_owned(),
            session_id: session_id.to_owned(),
            objective,
            model_id,
            repository_identity: repository_identity.to_owned(),
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            revision: 0,
            status: AgentGoalStatus::Queued,
            pause_reason: None,
            block_reason: None,
            usage_by_model,
            steering_messages: Vec::new(),
            checkpoint: AgentCheckpoint::empty(repository_digest, now_ms),
            completion_candidate: None,
            result: None,
        };
        session.active_goal = Some(goal.clone());
        session.revision = session.revision.checked_add(1).ok_or(GoalError::Capacity)?;
        persist_rollback(&self.persistence, session, |session| {
            session.active_goal = None;
            session.revision = session.revision.saturating_sub(1);
        })?;
        Ok(goal)
    }

    pub fn snapshot(&self, session_id: &str) -> Result<DurableAgentSession, GoalError> {
        self.sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(session_id)
            .cloned()
            .ok_or(GoalError::SessionNotLoaded)
    }

    pub fn reset_session(&self, session_id: &str) -> Result<DurableAgentSession, GoalError> {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let session = sessions
            .get_mut(session_id)
            .ok_or(GoalError::SessionNotLoaded)?;
        if session
            .active_goal
            .as_ref()
            .is_some_and(|goal| !goal.status.is_terminal())
        {
            return Err(GoalError::Busy);
        }
        let before = session.clone();
        session.active_goal = None;
        session.memory_summary = None;
        session.recent_messages.clear();
        session.revision = session.revision.checked_add(1).ok_or(GoalError::Capacity)?;
        if let Err(error) = self.persistence.save(session) {
            *session = before;
            return Err(error);
        }
        Ok(session.clone())
    }

    pub fn mutate_goal<R>(
        &self,
        session_id: &str,
        expected_revision: u64,
        now_ms: u64,
        mutation: impl FnOnce(&mut AgentGoal) -> Result<R, GoalError>,
    ) -> Result<R, GoalError> {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let session = sessions
            .get_mut(session_id)
            .ok_or(GoalError::SessionNotLoaded)?;
        let before = session.clone();
        let goal = session
            .active_goal
            .as_mut()
            .ok_or(GoalError::GoalNotFound)?;
        if goal.revision != expected_revision {
            return Err(GoalError::RevisionConflict);
        }
        let result = mutation(goal)?;
        goal.revision = goal.revision.checked_add(1).ok_or(GoalError::Capacity)?;
        goal.updated_at_ms = now_ms;
        session.revision = session.revision.checked_add(1).ok_or(GoalError::Capacity)?;
        if let Err(error) = self.persistence.save(session) {
            *session = before;
            return Err(error);
        }
        Ok(result)
    }

    pub fn mutate_goal_current<R>(
        &self,
        session_id: &str,
        now_ms: u64,
        mutation: impl FnOnce(&mut AgentGoal) -> Result<R, GoalError>,
    ) -> Result<R, GoalError> {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let session = sessions
            .get_mut(session_id)
            .ok_or(GoalError::SessionNotLoaded)?;
        let before = session.clone();
        let goal = session
            .active_goal
            .as_mut()
            .ok_or(GoalError::GoalNotFound)?;
        let result = mutation(goal)?;
        goal.revision = goal.revision.checked_add(1).ok_or(GoalError::Capacity)?;
        goal.updated_at_ms = now_ms;
        session.revision = session.revision.checked_add(1).ok_or(GoalError::Capacity)?;
        if let Err(error) = self.persistence.save(session) {
            *session = before;
            return Err(error);
        }
        Ok(result)
    }

    pub fn commit_goal_result(
        &self,
        session_id: &str,
        result: AgentGoalResult,
        now_ms: u64,
    ) -> Result<DurableAgentSession, GoalError> {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let session = sessions
            .get_mut(session_id)
            .ok_or(GoalError::SessionNotLoaded)?;
        let before = session.clone();
        let goal = session
            .active_goal
            .as_mut()
            .ok_or(GoalError::GoalNotFound)?;
        if goal.status.is_terminal() {
            return Err(GoalError::Terminal);
        }
        let user = goal.objective.clone();
        let assistant = result.text.clone();
        goal.result = Some(result);
        goal.status = AgentGoalStatus::Completed;
        goal.pause_reason = None;
        goal.block_reason = None;
        goal.revision = goal.revision.checked_add(1).ok_or(GoalError::Capacity)?;
        goal.updated_at_ms = now_ms;
        goal.clear_active_working_data();
        session.recent_messages.extend([
            crate::SessionMessage {
                role: crate::SessionRole::User,
                content: user,
            },
            crate::SessionMessage {
                role: crate::SessionRole::Assistant,
                content: assistant,
            },
        ]);
        if session.recent_messages.len() > 16 {
            session
                .recent_messages
                .drain(..session.recent_messages.len().saturating_sub(16));
        }
        session.revision = session.revision.checked_add(1).ok_or(GoalError::Capacity)?;
        if let Err(error) = self.persistence.save(session) {
            *session = before;
            return Err(error);
        }
        Ok(session.clone())
    }

    pub fn steer(
        &self,
        session_id: &str,
        expected_revision: u64,
        content: String,
        now_ms: u64,
    ) -> Result<SteeringMessage, GoalError> {
        validate_content(&content, MAX_STEERING_BYTES)?;
        self.mutate_goal(session_id, expected_revision, now_ms, |goal| {
            if goal.status.is_terminal() {
                return Err(GoalError::Terminal);
            }
            let message = SteeringMessage {
                sequence: goal.steering_messages.len() as u64 + 1,
                created_at_ms: now_ms,
                content,
                injected: false,
            };
            goal.steering_messages.push(message.clone());
            Ok(message)
        })
    }

    pub fn prepare_loaded_sessions_for_restart(&self, now_ms: u64) -> Result<(), GoalError> {
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for session in sessions.values_mut() {
            let before = session.clone();
            if let Some(goal) = &mut session.active_goal {
                let revision = goal.revision;
                goal.prepare_for_restart(now_ms);
                if goal.revision != revision {
                    session.revision = session.revision.saturating_add(1);
                    if let Err(error) = self.persistence.save(session) {
                        *session = before;
                        return Err(error);
                    }
                }
            }
        }
        Ok(())
    }
}

fn persist_rollback(
    persistence: &impl GoalPersistence,
    session: &mut DurableAgentSession,
    rollback: impl FnOnce(&mut DurableAgentSession),
) -> Result<(), GoalError> {
    if let Err(error) = persistence.save(session) {
        rollback(session);
        return Err(error);
    }
    Ok(())
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GoalError {
    #[error("invalid identifier")]
    InvalidId,
    #[error("invalid content")]
    InvalidContent,
    #[error("invalid budget")]
    InvalidBudget,
    #[error("goal already active")]
    Busy,
    #[error("session is not loaded")]
    SessionNotLoaded,
    #[error("goal not found")]
    GoalNotFound,
    #[error("goal revision conflict")]
    RevisionConflict,
    #[error("goal is terminal")]
    Terminal,
    #[error("capacity exceeded")]
    Capacity,
    #[error("checkpoint key unavailable")]
    KeyUnavailable,
    #[error("checkpoint storage is locked")]
    StorageLocked,
    #[error("checkpoint is corrupt")]
    CheckpointCorrupt,
    #[error("checkpoint version is unsupported")]
    UnsupportedVersion,
    #[error("checkpoint storage is unavailable")]
    StorageUnavailable,
}

fn validate_id(value: &str) -> Result<(), GoalError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(GoalError::InvalidId)
    }
}

fn validate_content(value: &str, max_bytes: usize) -> Result<(), GoalError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.contains('\0') {
        Err(GoalError::InvalidContent)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Clone, Default)]
    struct MemoryPersistence(Arc<Mutex<HashMap<String, DurableAgentSession>>>);

    impl GoalPersistence for MemoryPersistence {
        fn load(&self, session_id: &str) -> Result<Option<DurableAgentSession>, GoalError> {
            Ok(self.0.lock().unwrap().get(session_id).cloned())
        }

        fn save(&self, session: &DurableAgentSession) -> Result<(), GoalError> {
            self.0
                .lock()
                .unwrap()
                .insert(session.session_id.clone(), session.clone());
            Ok(())
        }

        fn remove(&self, session_id: &str) -> Result<(), GoalError> {
            self.0.lock().unwrap().remove(session_id);
            Ok(())
        }
    }

    fn price() -> PriceSnapshot {
        PriceSnapshot {
            currency: "CNY".into(),
            input_cache_hit_per_million_micros: 20_000,
            input_cache_miss_per_million_micros: 1_000_000,
            output_per_million_micros: 2_000_000,
            source_url: "https://example.test".into(),
            source_version: "v1".into(),
            checked_at: "2026-08-19".into(),
        }
    }

    fn budget() -> AgentBudgetAccount {
        AgentBudgetAccount::new(
            "deepseek-v4-flash",
            Some(price()),
            ModelBudgetLimit::CostMicros {
                currency: "CNY".into(),
                limit_micros: 1_000_000,
            },
        )
        .unwrap()
    }

    #[test]
    fn cached_and_miss_usage_charge_against_a_model_price_snapshot() {
        let mut account = budget();
        let charge = account
            .record_usage(&ModelUsage {
                input_tokens: 1_000_000,
                cached_input_tokens: 900_000,
                output_tokens: 100_000,
                tool_calls: 0,
            })
            .unwrap();
        assert_eq!(charge.spent_micros, 318_000);
        assert!(!charge.exceeded);
        assert!(account
            .extend(ModelBudgetLimit::CostMicros {
                currency: "CNY".into(),
                limit_micros: 2_000_000,
            })
            .is_ok());
        assert_eq!(
            account
                .extend(ModelBudgetLimit::CostMicros {
                    currency: "CNY".into(),
                    limit_micros: 1_500_000,
                })
                .unwrap_err(),
            GoalError::InvalidBudget
        );
    }

    #[test]
    fn unpriced_models_require_token_budgets() {
        assert!(AgentBudgetAccount::new(
            "future-model",
            None,
            ModelBudgetLimit::Tokens { limit_tokens: 10 }
        )
        .is_ok());
        assert_eq!(
            AgentBudgetAccount::new(
                "future-model",
                None,
                ModelBudgetLimit::CostMicros {
                    currency: "USD".into(),
                    limit_micros: 1,
                }
            )
            .unwrap_err(),
            GoalError::InvalidBudget
        );
    }

    #[test]
    fn repository_allows_only_one_nonterminal_goal_and_uses_cas() {
        let persistence = MemoryPersistence::default();
        let repository = GoalRepository::new(persistence);
        repository.load_or_create("session", "repo-1").unwrap();
        let goal = repository
            .create_goal(
                "session",
                "repo-1",
                "goal-1",
                "inspect".into(),
                "deepseek-v4-flash".into(),
                budget(),
                "workspace-v1".into(),
                1,
            )
            .unwrap();
        assert_eq!(goal.status, AgentGoalStatus::Queued);
        assert_eq!(
            repository
                .create_goal(
                    "session",
                    "repo-1",
                    "goal-2",
                    "other".into(),
                    "deepseek-v4-flash".into(),
                    budget(),
                    "workspace-v1".into(),
                    2,
                )
                .unwrap_err(),
            GoalError::Busy
        );
        assert_eq!(
            repository
                .steer("session", 9, "new direction".into(), 2)
                .unwrap_err(),
            GoalError::RevisionConflict
        );
        let steering = repository
            .steer("session", 0, "new direction".into(), 2)
            .unwrap();
        assert_eq!(steering.sequence, 1);
        assert_eq!(repository.snapshot("session").unwrap().revision, 2);
    }

    #[test]
    fn restart_pauses_without_scheduling_work_and_expires_approval() {
        let persistence = MemoryPersistence::default();
        let repository = GoalRepository::new(persistence);
        repository.load_or_create("session", "repo-1").unwrap();
        repository
            .create_goal(
                "session",
                "repo-1",
                "goal-1",
                "inspect".into(),
                "deepseek-v4-flash".into(),
                budget(),
                "workspace-v1".into(),
                1,
            )
            .unwrap();
        repository
            .mutate_goal("session", 0, 2, |goal| {
                goal.status = AgentGoalStatus::Running;
                goal.checkpoint.pending_intents.push(ToolIntent::for_test(
                    "exec-1",
                    "call-1",
                    "filesystem.write",
                ));
                goal.checkpoint.pending_intents[0].approval_id = Some("old-approval".into());
                goal.checkpoint.pending_intents[0].approved = true;
                Ok(())
            })
            .unwrap();
        repository.prepare_loaded_sessions_for_restart(3).unwrap();
        let goal = repository.snapshot("session").unwrap().active_goal.unwrap();
        assert_eq!(goal.status, AgentGoalStatus::Paused);
        assert_eq!(goal.pause_reason, Some(PauseReason::AppRestarted));
        assert_eq!(goal.checkpoint.pending_intents[0].approval_id, None);
        assert!(!goal.checkpoint.pending_intents[0].approved);
    }

    #[test]
    fn no_progress_uses_recovery_then_blocks_without_completing() {
        let mut tracker = ProgressTracker::default();
        assert_eq!(tracker.observe("same"), ProgressAction::Continue);
        for _ in 0..3 {
            assert_eq!(tracker.observe("same"), ProgressAction::Continue);
        }
        assert_eq!(tracker.observe("same"), ProgressAction::RecoverySlice);
        assert_eq!(tracker.observe("same"), ProgressAction::Continue);
        assert_eq!(tracker.observe("same"), ProgressAction::Block);
    }

    #[test]
    fn slice_defaults_are_not_business_turn_limits() {
        let limits = GoalSliceLimits::default();
        assert_eq!(limits.max_active_ms, 120_000);
        assert_eq!(limits.max_input_tokens, 250_000);
        assert_eq!(limits.max_output_tokens, 16_000);
        assert!(limits.runaway_model_rounds > 64);
        assert!(limits.runaway_tool_calls > 128);
    }

    #[test]
    fn multi_slice_candidate_requires_verification_and_terminal_cleanup_removes_working_content() {
        let repository = GoalRepository::new(MemoryPersistence::default());
        repository.load_or_create("session", "repo-1").unwrap();
        repository
            .create_goal(
                "session",
                "repo-1",
                "goal-1",
                "inspect".into(),
                "deepseek-v4-flash".into(),
                budget(),
                "workspace-v1".into(),
                1,
            )
            .unwrap();
        repository
            .mutate_goal_current("session", 2, |goal| {
                goal.checkpoint.slice_index = 1;
                goal.checkpoint.working_summary = "working marker".into();
                goal.completion_candidate = Some(AgentCompletionCandidate {
                    text: "candidate".into(),
                    remaining_work: Vec::new(),
                    created_at_ms: 2,
                    model_responses: 1,
                    used_tools: false,
                    verification: None,
                });
                assert!(goal.requires_independent_verifier());
                Ok(())
            })
            .unwrap();
        let session = repository
            .commit_goal_result(
                "session",
                AgentGoalResult {
                    text: "canonical".into(),
                    committed_at_ms: 3,
                    verifier: Some(VerificationResult {
                        decision: VerificationDecision::Accepted,
                        gaps: Vec::new(),
                        evidence_ids: Vec::new(),
                    }),
                },
                3,
            )
            .unwrap();
        let goal = session.active_goal.unwrap();
        assert_eq!(goal.status, AgentGoalStatus::Completed);
        assert!(goal.completion_candidate.is_none());
        assert!(goal.checkpoint.working_summary.is_empty());
        assert_eq!(session.recent_messages[1].content, "canonical");
    }
}
