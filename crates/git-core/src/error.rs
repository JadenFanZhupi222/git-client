use thiserror::Error;

/// 领域错误:用类型精确表达"会出什么错"。
/// 上层(app-service / IPC)会把它翻译成给用户看的信息。
#[derive(Debug, Error)]
pub enum GitError {
    #[error("仓库未找到: {0}")]
    RepoNotFound(String),

    #[error("HEAD 不存在(可能是空仓库)")]
    NoHead,

    #[error("操作被取消")]
    Cancelled,

    #[error("没有已暂存的改动可提交")]
    NothingToCommit,

    #[error("提交信息不能为空")]
    EmptyCommitMessage,

    #[error("git 身份未配置,请先设置 user.name / user.email")]
    EmptySignature,

    /// 兜底:底层 git 库返回的、我们还没细分的错误。
    #[error("底层 git 错误: {0}")]
    Backend(String),
}
