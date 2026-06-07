use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 本地与远程跟踪分支的提交差集,供图谱标记「未 push / 未 pull」用。
///
/// - `outgoing`:本地分支可达、但所有远程跟踪分支都不可达的提交 SHA = **已 commit 未 push**。
///   等价于 `git rev-list --branches --not --remotes`。
/// - `incoming`:远程跟踪分支可达、但本地分支都不可达的提交 SHA = **已 fetch 未 pull/merge**。
///   等价于 `git rev-list --remotes --not --branches`。
///
/// 仓库无任何远程跟踪引用时两集合均空(无从对比)。
///
/// ⚠️ **语义边界**:基于「**全部**本地分支 vs **全部**远程跟踪分支」,而非「当前分支 vs 其
/// upstream」。因此别的从没 push 的本地分支的独有提交也会进 `outgoing`。实践中图谱只渲染
/// HEAD 可达提交,这些提交多数落在窗口外,表现通常正确;但合并过其它分支时可能误标。
/// 如需严格「当前分支」语义,应改为 `HEAD --not --remotes`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncCommits {
    pub outgoing: HashSet<String>,
    pub incoming: HashSet<String>,
}

/// 本地分支信息(阶段 2a 最小集:名字 + 是否当前)。
/// 后续切片会扩展 upstream / ahead-behind 等字段。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchInfo {
    /// 短名,如 "main" / "feat/graph"。
    pub name: String,
    /// 是否为当前 HEAD 所在分支。
    pub is_head: bool,
}

/// 当前分支相对其上游(upstream)的领先/落后提交数。
/// 无上游时上层返回 None,不构造本结构。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AheadBehind {
    /// 本地有、上游没有的提交数(需要 push)。
    pub ahead: usize,
    /// 上游有、本地没有的提交数(需要 pull)。
    pub behind: usize,
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
    /// 标签 refs/tags/*。
    Tag,
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
