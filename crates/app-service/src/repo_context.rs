//! 长驻仓库上下文 + 注册表(M1.1)。
//!
//! 背景:此前每个 Tauri 命令都 `RepoService::new(...)` 并把 `repo_path` 透传给
//! 每次后端调用 —— 没有任何可挂缓存的长驻对象。这里立起注入点:
//!
//! - [`RepoContext`]:绑定到「一个仓库 + 一个共享后端」的上下文。方法不再收
//!   `repo_path`,而是用自身持有的 `path`。**M1.2 的缓存就挂在这些方法体里**
//!   (查缓存命中即返回,未命中再下探 `service`)。
//! - [`RepoRegistry`]:`打开的仓库路径 → Arc<RepoContext>` 的注册表。经 Tauri
//!   `State` 注入,命令按 `repo_path` 路由到同一个长驻上下文,不再每次新建。
//!
//! 并发铁律:[`RepoRegistry::context`] 只在「查/插 HashMap」的一瞬持锁,git 重活
//! 在锁释放后才在拿到的 `Arc<RepoContext>` 上跑 —— 绝不持锁做阻塞操作。

use crate::RepoService;
use git_core::model::{RebaseStep, ResetMode};
use git_core::{GitBackend, GitError};
use ipc_types::{
    AheadBehindDto, BlameLineDto, BranchDto, CommitDto, ConflictSidesDto, FetchResultDto,
    FileChangeDto, FileDiffDto, GraphRowDto, PullResultDto, PushResultDto, RefDto, ReflogEntryDto,
    StashDto, StatusDto,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 绑定到「单个仓库 + 共享后端」的长驻上下文。
///
/// 每个方法 = `RepoService` 同名方法去掉 `repo_path` 参数(改用 `self.path`)。
/// 这层薄转发现在行为与旧代码完全一致;M1.2 会在这里加缓存检查。
pub struct RepoContext {
    service: RepoService,
    path: PathBuf,
}

impl RepoContext {
    fn new(backend: Arc<dyn GitBackend>, path: PathBuf) -> Self {
        Self {
            service: RepoService::new(backend),
            path,
        }
    }

    /// 此上下文绑定的仓库路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    // ---- 读 ----
    pub fn head_commit(&self) -> Result<CommitDto, GitError> {
        self.service.head_commit(&self.path)
    }
    pub fn status(&self) -> Result<StatusDto, GitError> {
        self.service.status(&self.path)
    }
    pub fn log(&self, limit: usize, skip: usize) -> Result<Vec<CommitDto>, GitError> {
        self.service.log(&self.path, limit, skip)
    }
    pub fn commit_graph(&self, limit: usize) -> Result<Vec<GraphRowDto>, GitError> {
        self.service.commit_graph(&self.path, limit)
    }
    pub fn search_commits(
        &self,
        query: &str,
        limit: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<CommitDto>, GitError> {
        self.service
            .search_commits(&self.path, query, limit, cancelled)
    }
    pub fn reflog(&self, limit: usize) -> Result<Vec<ReflogEntryDto>, GitError> {
        self.service.reflog(&self.path, limit)
    }
    pub fn commit_files(&self, commit_id: &str) -> Result<Vec<FileChangeDto>, GitError> {
        self.service.commit_files(&self.path, commit_id)
    }
    pub fn commit_file_diff(&self, commit_id: &str, file: &str) -> Result<FileDiffDto, GitError> {
        self.service.commit_file_diff(&self.path, commit_id, file)
    }
    pub fn compare_files(&self, from: &str, to: &str) -> Result<Vec<FileChangeDto>, GitError> {
        self.service.compare_files(&self.path, from, to)
    }
    pub fn compare_file_diff(
        &self,
        from: &str,
        to: &str,
        file: &str,
    ) -> Result<FileDiffDto, GitError> {
        self.service.compare_file_diff(&self.path, from, to, file)
    }
    pub fn working_diff(&self, file: &str, staged: bool) -> Result<FileDiffDto, GitError> {
        self.service.working_diff(&self.path, file, staged)
    }
    pub fn current_branch(&self) -> Result<Option<String>, GitError> {
        self.service.current_branch(&self.path)
    }
    pub fn branches(&self) -> Result<Vec<BranchDto>, GitError> {
        self.service.branches(&self.path)
    }
    pub fn ahead_behind(&self) -> Result<Option<AheadBehindDto>, GitError> {
        self.service.ahead_behind(&self.path)
    }
    pub fn remotes(&self) -> Result<Vec<String>, GitError> {
        self.service.remotes(&self.path)
    }
    pub fn refs(&self) -> Result<Vec<RefDto>, GitError> {
        self.service.refs(&self.path)
    }
    pub fn repo_state(&self) -> Result<String, GitError> {
        self.service.repo_state(&self.path)
    }
    pub fn blame(&self, file: &str) -> Result<Vec<BlameLineDto>, GitError> {
        self.service.blame(&self.path, file)
    }
    pub fn conflict_sides(&self, file: &str) -> Result<ConflictSidesDto, GitError> {
        self.service.conflict_sides(&self.path, file)
    }
    pub fn stash_list(&self) -> Result<Vec<StashDto>, GitError> {
        self.service.stash_list(&self.path)
    }

    // ---- 写 ----
    pub fn stage(&self, file: &Path) -> Result<(), GitError> {
        self.service.stage(&self.path, file)
    }
    pub fn unstage(&self, file: &Path) -> Result<(), GitError> {
        self.service.unstage(&self.path, file)
    }
    pub fn stage_hunk(&self, file: &str, hunk_index: usize) -> Result<(), GitError> {
        self.service.stage_hunk(&self.path, file, hunk_index)
    }
    pub fn stage_lines(
        &self,
        file: &str,
        hunk_index: usize,
        lines: &[usize],
    ) -> Result<(), GitError> {
        self.service
            .stage_lines(&self.path, file, hunk_index, lines)
    }
    pub fn unstage_hunk(&self, file: &str, hunk_index: usize) -> Result<(), GitError> {
        self.service.unstage_hunk(&self.path, file, hunk_index)
    }
    pub fn commit(&self, message: &str) -> Result<String, GitError> {
        self.service.commit(&self.path, message)
    }
    pub fn amend_commit(&self, message: Option<&str>) -> Result<String, GitError> {
        self.service.amend_commit(&self.path, message)
    }
    pub fn set_upstream(&self, upstream: &str) -> Result<(), GitError> {
        self.service.set_upstream(&self.path, upstream)
    }
    pub fn checkout_branch(&self, name: &str) -> Result<(), GitError> {
        self.service.checkout_branch(&self.path, name)
    }
    pub fn create_branch(&self, name: &str, checkout: bool) -> Result<(), GitError> {
        self.service.create_branch(&self.path, name, checkout)
    }
    pub fn delete_branch(&self, name: &str) -> Result<(), GitError> {
        self.service.delete_branch(&self.path, name)
    }
    pub fn fetch(&self, remote: Option<&str>) -> Result<FetchResultDto, GitError> {
        self.service.fetch(&self.path, remote)
    }
    pub fn pull(&self, remote: Option<&str>, rebase: bool) -> Result<PullResultDto, GitError> {
        self.service.pull(&self.path, remote, rebase)
    }
    pub fn push(&self, remote: Option<&str>) -> Result<PushResultDto, GitError> {
        self.service.push(&self.path, remote)
    }
    pub fn resolve_ours(&self, file: &str) -> Result<(), GitError> {
        self.service.resolve_ours(&self.path, file)
    }
    pub fn resolve_theirs(&self, file: &str) -> Result<(), GitError> {
        self.service.resolve_theirs(&self.path, file)
    }
    pub fn continue_op(&self) -> Result<(), GitError> {
        self.service.continue_op(&self.path)
    }
    pub fn abort_op(&self) -> Result<(), GitError> {
        self.service.abort_op(&self.path)
    }
    pub fn cherry_pick(&self, commit_id: &str) -> Result<(), GitError> {
        self.service.cherry_pick(&self.path, commit_id)
    }
    pub fn revert(&self, commit_id: &str) -> Result<(), GitError> {
        self.service.revert(&self.path, commit_id)
    }
    pub fn create_tag(
        &self,
        name: &str,
        commit_id: &str,
        message: Option<&str>,
    ) -> Result<(), GitError> {
        self.service
            .create_tag(&self.path, name, commit_id, message)
    }
    pub fn delete_tag(&self, name: &str) -> Result<(), GitError> {
        self.service.delete_tag(&self.path, name)
    }
    pub fn reset(&self, commit_id: &str, mode: ResetMode) -> Result<(), GitError> {
        self.service.reset(&self.path, commit_id, mode)
    }
    pub fn interactive_rebase(
        &self,
        base: Option<&str>,
        steps: &[RebaseStep],
    ) -> Result<(), GitError> {
        self.service.interactive_rebase(&self.path, base, steps)
    }
    pub fn stash_save(&self, message: Option<&str>) -> Result<(), GitError> {
        self.service.stash_save(&self.path, message)
    }
    pub fn stash_apply(&self, index: usize) -> Result<(), GitError> {
        self.service.stash_apply(&self.path, index)
    }
    pub fn stash_pop(&self, index: usize) -> Result<(), GitError> {
        self.service.stash_pop(&self.path, index)
    }
    pub fn stash_drop(&self, index: usize) -> Result<(), GitError> {
        self.service.stash_drop(&self.path, index)
    }
}

/// 打开的仓库注册表:`repo_path → Arc<RepoContext>`。
///
/// 整个应用一个共享后端(`Arc<dyn GitBackend>`),启动时建一次;每个仓库一个长驻
/// 上下文,首次访问时惰性创建并缓存。经 Tauri `State` 注入,所有命令共享。
pub struct RepoRegistry {
    backend: Arc<dyn GitBackend>,
    contexts: Mutex<HashMap<PathBuf, Arc<RepoContext>>>,
}

impl RepoRegistry {
    /// 注入共享后端(生产为 `CompositeBackend`,测试可塞 `FakeBackend`)。
    pub fn new(backend: Arc<dyn GitBackend>) -> Self {
        Self {
            backend,
            contexts: Mutex::new(HashMap::new()),
        }
    }

    /// 取或建某仓库的长驻上下文。**只在查/插表时短暂持锁**,返回 `Arc` 后调用方
    /// 在锁外做 git 操作。同一路径多次调用返回同一个 `Arc`(M1.2 缓存才有意义)。
    pub fn context(&self, repo_path: &Path) -> Arc<RepoContext> {
        let mut map = self.contexts.lock().expect("registry mutex poisoned");
        if let Some(ctx) = map.get(repo_path) {
            return ctx.clone();
        }
        let ctx = Arc::new(RepoContext::new(
            self.backend.clone(),
            repo_path.to_path_buf(),
        ));
        map.insert(repo_path.to_path_buf(), ctx.clone());
        ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_engine::FakeBackend;

    #[test]
    fn same_path_returns_same_context() {
        let reg = RepoRegistry::new(Arc::new(FakeBackend::default()));
        let a = reg.context(Path::new("/r"));
        let b = reg.context(Path::new("/r"));
        // 同一路径 → 同一个 Arc(指针相等),证明上下文被复用而非每次新建。
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn different_paths_get_distinct_contexts() {
        let reg = RepoRegistry::new(Arc::new(FakeBackend::default()));
        let a = reg.context(Path::new("/r1"));
        let b = reg.context(Path::new("/r2"));
        assert!(!Arc::ptr_eq(&a, &b));
        assert_eq!(a.path(), Path::new("/r1"));
        assert_eq!(b.path(), Path::new("/r2"));
    }

    #[test]
    fn context_forwards_to_backend() {
        // 绑定路径的转发方法应等价于旧的 service.X(path, ...)。
        let fb = Arc::new(FakeBackend::default());
        let reg = RepoRegistry::new(fb.clone());
        let ctx = reg.context(Path::new("/r"));
        ctx.stage(Path::new("a.txt")).unwrap();
        assert_eq!(fb.staged_files(), vec![PathBuf::from("a.txt")]);
    }
}
