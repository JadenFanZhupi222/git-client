use serde::{Deserialize, Serialize};

/// 本地分支信息(阶段 2a 最小集:名字 + 是否当前)。
/// 后续切片会扩展 upstream / ahead-behind 等字段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchInfo {
    /// 短名,如 "main" / "feat/graph"。
    pub name: String,
    /// 是否为当前 HEAD 所在分支。
    pub is_head: bool,
}

/// 引用的种类,决定图谱里徽章的样式。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RefKind {
    /// HEAD 指向的当前分支(或分离头)。
    Head,
    /// 本地分支 refs/heads/*。
    LocalBranch,
    /// 远程跟踪分支 refs/remotes/*(如 origin/main)。
    RemoteBranch,
}

/// 一个指向某 commit 的引用。图谱据此在对应提交上画分支/远程标签。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitRef {
    /// 显示名:本地 "main"、远程 "origin/main"、HEAD 用其当前分支短名(分离头为 "HEAD")。
    pub name: String,
    pub kind: RefKind,
    /// 指向的 commit 完整 SHA。
    pub target: String,
}
