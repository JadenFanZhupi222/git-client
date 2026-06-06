use crate::error::GitError;
use crate::model::{BranchInfo, Commit, FileChange, FileDiff, WorkingTreeStatus};
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

    /// 提交历史,时间倒序(新→旧)。limit/skip 分页。
    fn log(&self, repo: &Path, limit: usize, skip: usize) -> Result<Vec<Commit>, GitError>;

    /// 某提交相对第一个父的改动文件(文件级)。
    fn commit_files(&self, repo: &Path, commit_id: &str) -> Result<Vec<FileChange>, GitError>;

    /// 某提交中单个文件相对第一个父的行级 diff。
    /// `file` 为仓库根相对路径(git 风格,正斜杠)。
    fn commit_file_diff(
        &self,
        repo: &Path,
        commit_id: &str,
        file: &str,
    ) -> Result<FileDiff, GitError>;

    /// 当前 HEAD 分支短名(如 "main");分离头/空仓库为 None。
    fn current_branch(&self, repo: &Path) -> Result<Option<String>, GitError>;

    /// 列出本地分支(名 + 是否当前),按名字升序。
    fn branches(&self, repo: &Path) -> Result<Vec<BranchInfo>, GitError>;

    /// 切换到已有的本地分支(更新工作区 + 移动 HEAD)。
    /// 工作区有冲突改动时应失败而非覆盖(安全 checkout)。
    fn checkout_branch(&self, repo: &Path, name: &str) -> Result<(), GitError>;

    /// 在当前 HEAD 上新建本地分支(不切换)。同名已存在 → 错误。
    fn create_branch(&self, repo: &Path, name: &str) -> Result<(), GitError>;

    /// 删除本地分支。不能删除当前所在分支。
    fn delete_branch(&self, repo: &Path, name: &str) -> Result<(), GitError>;
}
