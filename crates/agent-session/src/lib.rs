mod compaction;
mod completion;
mod context;
mod engine;
mod goal;
mod rag;
mod session;

pub use compaction::{compact_working_set, CompactionAttempt, WorkingCompaction};
pub use completion::{
    validate_completion_candidate, verifier_continuation_action, verifier_feedback_message,
    verifier_requests_fit_budget, verify_completion_candidate, CandidateVerification,
    CompletionError, VerifierContinuationAction,
};
pub use context::{
    estimate_request_tokens, estimate_text_tokens, CalibratedTokenEstimator, ContextError,
    ContextLimits, ContextPlanner, PlannedContext, RequestTokenEstimator,
};
pub use engine::{
    AgentLoopPolicy, AgentSliceBoundary, AgentSliceCheckpoint, AgentSliceOutcome,
    AgentSliceRequest, AgentTurnRequest, AgentTurnResult, SessionEngine, SessionEngineConfig,
    SessionEngineError,
};
pub use goal::{
    AgentBudgetAccount, AgentCheckpoint, AgentCompletionCandidate, AgentGoal, AgentGoalResult,
    AgentGoalStatus, BlockReason, BudgetCharge, DurableAgentSession, GoalError, GoalPersistence,
    GoalRepository, GoalSliceLimits, ModelBudgetLimit, ModelRequestBudget, PauseReason,
    PriceSnapshot, ProgressTracker, SteeringMessage, VerificationDecision, VerificationResult,
    WorkingEvidence,
};
pub use rag::{InMemoryRagIndex, NoopRagRetriever, RagChunk, RagError, RagRetriever};
pub use session::{
    AgentSession, ExtractiveMemoryCompactor, MemoryCompactor, SessionError, SessionLease,
    SessionMessage, SessionRole, SessionStore, SessionStoreLimits,
};
