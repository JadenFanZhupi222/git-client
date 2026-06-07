use crate::error::GitError;
use crate::model::{
    AheadBehind, BlameLine, BranchInfo, Commit, CommitRef, ConflictSides, FetchOutcome, FileChange,
    FileDiff, PullOutcome, PushOutcome, RepoState, StashEntry, SyncCommits, WorkingTreeStatus,
};
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

    /// 从 HEAD 搜索提交(时间倒序),大小写不敏感匹配 summary/body/作者名/邮箱/SHA 前缀。
    /// 最多返回 limit 条。空 query / 不支持 → 空。默认 Unsupported。
    ///
    /// `cancelled`:遍历大仓库历史时定期回调,返回 true 则尽快中断并返回
    /// `GitError::Cancelled`(上层在用户改了搜索词时取消上一次,避免占满阻塞线程池)。
    fn search_commits(
        &self,
        _repo: &Path,
        _query: &str,
        _limit: usize,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<Commit>, GitError> {
        Err(GitError::Unsupported)
    }

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

    /// 工作区某文件的行级 diff:`staged=false` 为 index↔工作区(未暂存改动,
    /// 含未跟踪文件),`staged=true` 为 HEAD↔index(已暂存改动)。
    fn working_diff(&self, repo: &Path, file: &str, staged: bool) -> Result<FileDiff, GitError>;

    /// 当前 HEAD 分支短名(如 "main");分离头/空仓库为 None。
    fn current_branch(&self, repo: &Path) -> Result<Option<String>, GitError>;

    /// 列出本地分支(名 + 是否当前),按名字升序。
    fn branches(&self, repo: &Path) -> Result<Vec<BranchInfo>, GitError>;

    /// 列出仓库引用(本地分支 / 远程跟踪分支 / HEAD),含各自指向的 commit SHA。
    /// 供图谱在对应提交上渲染分支/远程标签。
    fn refs(&self, repo: &Path) -> Result<Vec<CommitRef>, GitError>;

    /// 仓库当前状态(干净 / 合并中 / 变基中 / cherry-pick / revert)。
    fn repo_state(&self, repo: &Path) -> Result<RepoState, GitError>;

    /// 逐行 blame:返回 `file`(仓库根相对路径)每行最后修改的提交信息。
    fn blame(&self, repo: &Path, file: &str) -> Result<Vec<BlameLine>, GitError>;

    // ---- 冲突解决:整文件采用一边走 CLI;默认 Unsupported ----

    /// 整文件采用「我方(ours)」并标记已解决(checkout --ours + add)。
    fn resolve_ours(&self, _repo: &Path, _file: &str) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }
    /// 整文件采用「对方(theirs)」并标记已解决(checkout --theirs + add)。
    fn resolve_theirs(&self, _repo: &Path, _file: &str) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }
    /// 读冲突文件的三方内容(base/ours/theirs,来自 index stage 1/2/3 的 blob)。
    /// 供三栏合并编辑器使用。默认 Unsupported。
    fn conflict_sides(&self, _repo: &Path, _file: &str) -> Result<ConflictSides, GitError> {
        Err(GitError::Unsupported)
    }
    /// 继续进行中的操作(合并提交 / rebase --continue / cherry-pick --continue)。
    /// 仍有未解决冲突 → MergeConflict。默认 Unsupported。
    fn continue_op(&self, _repo: &Path) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }
    /// 中止进行中的操作(merge/rebase/cherry-pick --abort)。默认 Unsupported。
    fn abort_op(&self, _repo: &Path) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }

    /// 把指定提交拣选(cherry-pick)到当前分支。冲突 → MergeConflict
    /// (进入 cherry-pick 中,可用 continue_op/abort_op)。默认 Unsupported。
    fn cherry_pick(&self, _repo: &Path, _commit_id: &str) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }

    /// 回滚(revert)指定提交:生成一个抵消其改动的新提交。冲突 → MergeConflict
    /// (进入 reverting 中,可用 continue_op/abort_op)。默认 Unsupported。
    fn revert(&self, _repo: &Path, _commit_id: &str) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }

    /// 当前分支相对上游的领先/落后数。无上游 / 分离头 / 空仓库 → None。
    fn ahead_behind(&self, repo: &Path) -> Result<Option<AheadBehind>, GitError>;

    /// 本地与远程跟踪分支的提交差集(未 push / 未 pull),供图谱逐行标记。
    /// 无远程跟踪引用 → 两集合均空。默认实现返回空(无网络/不支持的后端)。
    fn sync_commits(&self, _repo: &Path) -> Result<SyncCommits, GitError> {
        Ok(SyncCommits::default())
    }

    /// 列出远程名(如 ["origin", "upstream"]),按 git 返回顺序。
    fn remotes(&self, repo: &Path) -> Result<Vec<String>, GitError>;

    /// 把当前分支的上游设为 `upstream`(形如 "origin/main");该远程跟踪分支须已存在。
    fn set_upstream(&self, repo: &Path, upstream: &str) -> Result<(), GitError>;

    // ---- 贮藏(stash):有状态的复杂流程,走 CLI 后端;默认 Unsupported ----

    /// 列出贮藏(stash@{0} 在前)。
    fn stash_list(&self, _repo: &Path) -> Result<Vec<StashEntry>, GitError> {
        Err(GitError::Unsupported)
    }
    /// 贮藏当前工作区改动(含已暂存)。无改动 → NothingToStash。
    fn stash_save(&self, _repo: &Path, _message: Option<&str>) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }
    /// 应用第 index 条贮藏(不从列表移除)。冲突 → MergeConflict。
    fn stash_apply(&self, _repo: &Path, _index: usize) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }
    /// 弹出第 index 条贮藏(应用并移除)。冲突 → MergeConflict。
    fn stash_pop(&self, _repo: &Path, _index: usize) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }
    /// 删除第 index 条贮藏。
    fn stash_drop(&self, _repo: &Path, _index: usize) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }

    /// 切换到已有的本地分支(更新工作区 + 移动 HEAD)。
    /// 工作区有冲突改动时应失败而非覆盖(安全 checkout)。
    fn checkout_branch(&self, repo: &Path, name: &str) -> Result<(), GitError>;

    /// 在当前 HEAD 上新建本地分支(不切换)。同名已存在 → 错误。
    fn create_branch(&self, repo: &Path, name: &str) -> Result<(), GitError>;

    /// 删除本地分支。不能删除当前所在分支。
    fn delete_branch(&self, repo: &Path, name: &str) -> Result<(), GitError>;

    /// 从远程拉取更新(更新远程跟踪分支,不改工作区/当前分支)。
    /// remote = None 时用 git 默认远程(通常当前分支的 upstream / origin)。
    /// 默认实现返回 Unsupported —— 不做网络的后端无需覆盖。
    fn fetch(&self, _repo: &Path, _remote: Option<&str>) -> Result<FetchOutcome, GitError> {
        Err(GitError::Unsupported)
    }

    /// 从上游 pull。`rebase=false` 为 fetch+merge,`true` 为 fetch+rebase。
    /// 更新工作区与当前分支。冲突 → MergeConflict;无上游 → NoUpstream。
    /// 默认实现返回 Unsupported。
    fn pull(
        &self,
        _repo: &Path,
        _remote: Option<&str>,
        _rebase: bool,
    ) -> Result<PullOutcome, GitError> {
        Err(GitError::Unsupported)
    }

    /// 暂存单个 hunk(把工作区某文件第 `hunk_index` 个未暂存块加入 index)。
    /// 复杂的局部 patch 应用走 CLI 后端;默认实现返回 Unsupported。
    fn stage_hunk(&self, _repo: &Path, _file: &str, _hunk_index: usize) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }

    /// 取消暂存单个 hunk(把 index 里某文件第 `hunk_index` 个已暂存块撤回)。
    /// 默认实现返回 Unsupported。
    fn unstage_hunk(&self, _repo: &Path, _file: &str, _hunk_index: usize) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }

    /// 暂存某未暂存 hunk 中的指定行(`lines` 为该 hunk 内行下标,与 working_diff 一致)。
    /// 默认实现返回 Unsupported。
    fn stage_lines(
        &self,
        _repo: &Path,
        _file: &str,
        _hunk_index: usize,
        _lines: &[usize],
    ) -> Result<(), GitError> {
        Err(GitError::Unsupported)
    }

    /// 把当前分支推到远程。remote = None 用默认远程(通常 origin)。
    /// 当前分支无上游时自动 `-u` 建立跟踪;被拒(non-fast-forward)→ PushRejected。
    /// 默认实现返回 Unsupported —— 不做网络的后端无需覆盖。
    fn push(&self, _repo: &Path, _remote: Option<&str>) -> Result<PushOutcome, GitError> {
        Err(GitError::Unsupported)
    }
}
