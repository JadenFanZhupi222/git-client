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

#[path = "contracts/agent.rs"]
mod agent;
#[path = "contracts/git_error.rs"]
mod git_error;
#[path = "contracts/review_issue.rs"]
mod review_issue;

pub use agent::*;
pub use git_error::*;
pub use review_issue::*;
