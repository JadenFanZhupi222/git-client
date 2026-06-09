use serde::{Deserialize, Serialize};

/// 一个工作树(`git worktree`):主工作树或链接(linked)工作树。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorktreeInfo {
    /// 绝对路径。
    pub path: String,
    /// 该工作树 HEAD 指向的提交 SHA。
    pub head_sha: String,
    /// 检出的分支短名(如 "main");分离头 / 裸仓库为空。
    pub branch: String,
    /// 是否主工作树(`git worktree list` 的第一条)。
    pub is_main: bool,
    /// 是否当前打开的这个工作树(路径与打开的仓库一致)。
    pub is_current: bool,
    /// 分离头(detached HEAD,没有检出分支)。
    pub detached: bool,
    /// 被锁定(`git worktree lock`,通常是可移动介质上的工作树)。
    pub locked: bool,
    /// 裸仓库(bare,没有工作区文件)。
    pub bare: bool,
}
