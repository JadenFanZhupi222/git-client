use crate::error::GitError;
use crate::model::{Commit, WorkingTreeStatus};
use std::path::Path;

/// 端口(Port):所有 git 后端必须实现它。
/// 上层只依赖这个 trait,不依赖任何具体实现 —— 这是六边形架构的核心。
///
/// `Send + Sync`:声明这个对象可以安全地跨线程使用(我们要放进多线程环境)。
pub trait GitBackend: Send + Sync {
    /// 打开仓库,顺手验证它是不是个有效仓库。
    fn open(&self, path: &Path) -> Result<(), GitError>;

    /// 读 HEAD 指向的提交。阶段 0 的验证目标。
    fn head_commit(&self, path: &Path) -> Result<Commit, GitError>;

    /// 工作区状态(阶段 1 用)。
    fn status(&self, path: &Path) -> Result<WorkingTreeStatus, GitError>;

    /// 文件级暂存:把工作区某文件当前内容加入 index。路径为仓库根相对路径。
    fn stage(&self, repo: &Path, file: &Path) -> Result<(), GitError>;

    /// 取消暂存:把某文件从 index 撤回(有/无 HEAD 语义不同,见适配器实现)。
    fn unstage(&self, repo: &Path, file: &Path) -> Result<(), GitError>;

    /// 提交 index 内容,返回新 commit 的完整 SHA。
    fn commit(&self, repo: &Path, message: &str) -> Result<String, GitError>;
}
