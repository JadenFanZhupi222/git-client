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

    #[error("没有可贮藏的改动")]
    NothingToStash,

    #[error("提交信息不能为空")]
    EmptyCommitMessage,

    #[error("git 身份未配置,请先设置 user.name / user.email")]
    EmptySignature,

    #[error("分支不存在: {0}")]
    BranchNotFound(String),

    #[error("分支已存在: {0}")]
    BranchAlreadyExists(String),

    #[error("分支名无效")]
    InvalidBranchName,

    #[error("不能删除当前所在的分支,请先切到别的分支")]
    CannotDeleteCurrentBranch,

    #[error("标签已存在: {0}")]
    TagAlreadyExists(String),

    #[error("工作区有未提交的改动,切换分支会被覆盖,请先提交或暂存")]
    CheckoutConflict,

    #[error("未找到 git 命令,请确认已安装 git 并在 PATH 中")]
    GitCliNotFound,

    #[error("认证失败,请检查凭据或 SSH key")]
    AuthFailed,

    #[error("网络错误,无法访问远程")]
    NetworkError,

    #[error("没有配置远程仓库")]
    NoRemote,

    #[error("远程已存在: {0}")]
    RemoteAlreadyExists(String),

    #[error("远程不存在: {0}")]
    RemoteNotFound(String),

    #[error("远程名无效")]
    InvalidRemoteName,

    #[error("目标目录非空: {0}")]
    DestinationNotEmpty(String),

    #[error("仓库地址无效")]
    InvalidUrl,

    #[error("当前分支没有设置上游分支(upstream),无法 pull")]
    NoUpstream,

    #[error("push 被拒绝:远程有你本地没有的提交,请先 Pull 再推")]
    PushRejected,

    #[error("合并冲突,涉及 {files} 个文件,请手动解决后提交")]
    MergeConflict { files: usize },

    #[error("没有可撤销的上一步操作")]
    NothingToUndo,

    #[error("没有可重做的操作")]
    NothingToRedo,

    #[error("有 {count} 处未提交改动,撤销该操作会还原工作区并丢失它们,请先提交或贮藏")]
    UncommittedChanges { count: usize },

    #[error("文件过大,已跳过(超过 {limit} 字节)")]
    FileTooLarge { limit: usize },

    #[error("二进制文件,无法逐行显示")]
    BinaryFile,

    #[error("该后端不支持此操作")]
    Unsupported,

    /// 兜底:底层 git 库返回的、我们还没细分的错误。
    #[error("底层 git 错误: {0}")]
    Backend(String),
}
