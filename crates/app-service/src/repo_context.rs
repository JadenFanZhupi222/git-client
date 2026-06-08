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
use crate::watcher::ChangeKind;
use git_core::model::{RebaseStep, ResetMode};
use git_core::{GitBackend, GitError};
use ipc_types::{
    AheadBehindDto, BlameLineDto, BranchDto, CommitDto, ConflictSidesDto, FetchResultDto,
    FileChangeDto, FileDiffDto, GraphRowDto, PullResultDto, PushResultDto, RefDto, ReflogEntryDto,
    StashDto, StatusDto,
};
use lru::LruCache;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 同时缓存几个不同 limit 的图谱(「加载更多」会换 limit)。LRU 控上限防内存膨胀。
const GRAPH_CACHE_CAP: usize = 8;

/// 每个仓库一份读缓存(M1.2)。挂在 [`RepoContext`] 内,随上下文长驻。
///
/// 切 tab / 重渲染会反复读同样的数据,这里命中即瞬回;失效由 [`RepoContext::invalidate`]
/// (文件监听 + 自身写操作)按「变化域」精准驱动。`LruCache` 非线程安全,故各包一层
/// `Mutex` —— 锁只在查/存的一瞬持有,后端调用本身不持锁(不违反「慢操作持锁」铁律)。
struct RepoCache {
    /// 工作区状态(单值)。工作区/暂存区变化即失效。
    status: Mutex<Option<StatusDto>>,
    /// 提交图谱,按 limit 缓存。引用/提交变化即整体失效。
    graph: Mutex<LruCache<usize, Vec<GraphRowDto>>>,
}

impl Default for RepoCache {
    fn default() -> Self {
        Self {
            status: Mutex::new(None),
            graph: Mutex::new(LruCache::new(NonZeroUsize::new(GRAPH_CACHE_CAP).unwrap())),
        }
    }
}

/// 绑定到「单个仓库 + 共享后端」的长驻上下文。
///
/// 每个方法 = `RepoService` 同名方法去掉 `repo_path` 参数(改用 `self.path`)。
/// 这层薄转发现在行为与旧代码完全一致;M1.2 会在这里加缓存检查。
pub struct RepoContext {
    service: RepoService,
    path: PathBuf,
    cache: RepoCache,
}

impl RepoContext {
    fn new(backend: Arc<dyn GitBackend>, path: PathBuf) -> Self {
        Self {
            service: RepoService::new(backend),
            path,
            cache: RepoCache::default(),
        }
    }

    /// 此上下文绑定的仓库路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 按「变化域」失效缓存。文件监听(外部改动)与自身写操作共用一套语义:
    /// - 工作区/暂存区变 → `status` 失效(不动 graph,免得切个文件就重算图谱);
    /// - 引用/提交变 → `graph` 失效。
    ///
    /// 后续 slice 会把更多缓存(diff/blame/log/refs…)按同样的域接进来。
    pub fn invalidate(&self, kind: ChangeKind) {
        match kind {
            ChangeKind::WorkingTree | ChangeKind::Index => {
                *self.cache.status.lock().unwrap() = None;
            }
            ChangeKind::GitRef => {
                self.cache.graph.lock().unwrap().clear();
            }
        }
    }

    /// 自身写操作成功后立即失效(watcher 有 200ms debounce 且异步,不能等它)。
    /// `worktree`=动了工作区/暂存区;`refs`=动了提交/分支/远程引用。
    fn after_write(&self, worktree: bool, refs: bool) {
        if worktree {
            self.invalidate(ChangeKind::WorkingTree);
        }
        if refs {
            self.invalidate(ChangeKind::GitRef);
        }
    }

    // ---- 读 ----
    pub fn head_commit(&self) -> Result<CommitDto, GitError> {
        self.service.head_commit(&self.path)
    }
    pub fn status(&self) -> Result<StatusDto, GitError> {
        if let Some(cached) = self.cache.status.lock().unwrap().clone() {
            return Ok(cached);
        }
        let st = self.service.status(&self.path)?;
        *self.cache.status.lock().unwrap() = Some(st.clone());
        Ok(st)
    }
    pub fn log(&self, limit: usize, skip: usize) -> Result<Vec<CommitDto>, GitError> {
        self.service.log(&self.path, limit, skip)
    }
    pub fn commit_graph(&self, limit: usize) -> Result<Vec<GraphRowDto>, GitError> {
        if let Some(cached) = self.cache.graph.lock().unwrap().get(&limit).cloned() {
            return Ok(cached);
        }
        let rows = self.service.commit_graph(&self.path, limit)?;
        self.cache.graph.lock().unwrap().put(limit, rows.clone());
        Ok(rows)
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
    // 每个写方法成功后调 after_write(worktree, refs) 立即失效对应域缓存。
    // 取舍偏保守:拿不准就多失效一点(顶多重算一次),绝不留过期数据。
    pub fn stage(&self, file: &Path) -> Result<(), GitError> {
        self.service.stage(&self.path, file)?;
        self.after_write(true, false);
        Ok(())
    }
    pub fn unstage(&self, file: &Path) -> Result<(), GitError> {
        self.service.unstage(&self.path, file)?;
        self.after_write(true, false);
        Ok(())
    }
    pub fn stage_hunk(&self, file: &str, hunk_index: usize) -> Result<(), GitError> {
        self.service.stage_hunk(&self.path, file, hunk_index)?;
        self.after_write(true, false);
        Ok(())
    }
    pub fn stage_lines(
        &self,
        file: &str,
        hunk_index: usize,
        lines: &[usize],
    ) -> Result<(), GitError> {
        self.service
            .stage_lines(&self.path, file, hunk_index, lines)?;
        self.after_write(true, false);
        Ok(())
    }
    pub fn unstage_hunk(&self, file: &str, hunk_index: usize) -> Result<(), GitError> {
        self.service.unstage_hunk(&self.path, file, hunk_index)?;
        self.after_write(true, false);
        Ok(())
    }
    pub fn commit(&self, message: &str) -> Result<String, GitError> {
        let out = self.service.commit(&self.path, message)?;
        self.after_write(true, true);
        Ok(out)
    }
    pub fn amend_commit(&self, message: Option<&str>) -> Result<String, GitError> {
        let out = self.service.amend_commit(&self.path, message)?;
        self.after_write(true, true);
        Ok(out)
    }
    pub fn set_upstream(&self, upstream: &str) -> Result<(), GitError> {
        self.service.set_upstream(&self.path, upstream)?;
        self.after_write(false, true);
        Ok(())
    }
    pub fn checkout_branch(&self, name: &str) -> Result<(), GitError> {
        self.service.checkout_branch(&self.path, name)?;
        self.after_write(true, true);
        Ok(())
    }
    pub fn create_branch(&self, name: &str, checkout: bool) -> Result<(), GitError> {
        self.service.create_branch(&self.path, name, checkout)?;
        self.after_write(checkout, true);
        Ok(())
    }
    pub fn delete_branch(&self, name: &str) -> Result<(), GitError> {
        self.service.delete_branch(&self.path, name)?;
        self.after_write(false, true);
        Ok(())
    }
    pub fn fetch(&self, remote: Option<&str>) -> Result<FetchResultDto, GitError> {
        let out = self.service.fetch(&self.path, remote)?;
        self.after_write(false, true);
        Ok(out)
    }
    pub fn pull(&self, remote: Option<&str>, rebase: bool) -> Result<PullResultDto, GitError> {
        let out = self.service.pull(&self.path, remote, rebase)?;
        self.after_write(true, true);
        Ok(out)
    }
    pub fn push(&self, remote: Option<&str>) -> Result<PushResultDto, GitError> {
        let out = self.service.push(&self.path, remote)?;
        self.after_write(false, true);
        Ok(out)
    }
    pub fn resolve_ours(&self, file: &str) -> Result<(), GitError> {
        self.service.resolve_ours(&self.path, file)?;
        self.after_write(true, false);
        Ok(())
    }
    pub fn resolve_theirs(&self, file: &str) -> Result<(), GitError> {
        self.service.resolve_theirs(&self.path, file)?;
        self.after_write(true, false);
        Ok(())
    }
    pub fn continue_op(&self) -> Result<(), GitError> {
        self.service.continue_op(&self.path)?;
        self.after_write(true, true);
        Ok(())
    }
    pub fn abort_op(&self) -> Result<(), GitError> {
        self.service.abort_op(&self.path)?;
        self.after_write(true, true);
        Ok(())
    }
    pub fn cherry_pick(&self, commit_id: &str) -> Result<(), GitError> {
        self.service.cherry_pick(&self.path, commit_id)?;
        self.after_write(true, true);
        Ok(())
    }
    pub fn revert(&self, commit_id: &str) -> Result<(), GitError> {
        self.service.revert(&self.path, commit_id)?;
        self.after_write(true, true);
        Ok(())
    }
    pub fn create_tag(
        &self,
        name: &str,
        commit_id: &str,
        message: Option<&str>,
    ) -> Result<(), GitError> {
        self.service
            .create_tag(&self.path, name, commit_id, message)?;
        self.after_write(false, true);
        Ok(())
    }
    pub fn delete_tag(&self, name: &str) -> Result<(), GitError> {
        self.service.delete_tag(&self.path, name)?;
        self.after_write(false, true);
        Ok(())
    }
    pub fn reset(&self, commit_id: &str, mode: ResetMode) -> Result<(), GitError> {
        self.service.reset(&self.path, commit_id, mode)?;
        self.after_write(true, true);
        Ok(())
    }
    pub fn interactive_rebase(
        &self,
        base: Option<&str>,
        steps: &[RebaseStep],
    ) -> Result<(), GitError> {
        self.service.interactive_rebase(&self.path, base, steps)?;
        self.after_write(true, true);
        Ok(())
    }
    pub fn stash_save(&self, message: Option<&str>) -> Result<(), GitError> {
        self.service.stash_save(&self.path, message)?;
        self.after_write(true, false);
        Ok(())
    }
    pub fn stash_apply(&self, index: usize) -> Result<(), GitError> {
        self.service.stash_apply(&self.path, index)?;
        self.after_write(true, false);
        Ok(())
    }
    pub fn stash_pop(&self, index: usize) -> Result<(), GitError> {
        self.service.stash_pop(&self.path, index)?;
        self.after_write(true, false);
        Ok(())
    }
    pub fn stash_drop(&self, index: usize) -> Result<(), GitError> {
        // 仅删除一条 stash 记录,不影响工作区/图谱(stash 列表缓存留到后续 slice)。
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

    // ---- M1.2 读缓存 + 失效 ----
    use crate::watcher::ChangeKind;

    fn fake_commit() -> git_core::model::Commit {
        use git_core::model::{Commit, Signature};
        Commit {
            id: "a".repeat(40),
            short_id: "aaaaaaa".into(),
            summary: "s".into(),
            body: String::new(),
            author: Signature {
                name: "n".into(),
                email: "e".into(),
            },
            timestamp: 0,
            parents: vec![],
        }
    }

    #[test]
    fn status_cached_until_invalidated() {
        let fb = Arc::new(FakeBackend::default());
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));
        ctx.status().unwrap();
        ctx.status().unwrap();
        assert_eq!(fb.status_call_count(), 1, "第二次应命中缓存,不打后端");
        // 工作区变化(文件监听 / 写操作)→ status 失效 → 重打
        ctx.invalidate(ChangeKind::WorkingTree);
        ctx.status().unwrap();
        assert_eq!(fb.status_call_count(), 2, "失效后应重新拉取");
    }

    #[test]
    fn graph_cached_per_limit_until_ref_change() {
        let fb = Arc::new(FakeBackend::default().with_log(vec![fake_commit()]));
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));
        ctx.commit_graph(50).unwrap();
        ctx.commit_graph(50).unwrap();
        assert_eq!(fb.log_call_count(), 1, "同 limit 第二次命中缓存");
        ctx.commit_graph(100).unwrap();
        assert_eq!(fb.log_call_count(), 2, "不同 limit 是不同 key,需重算");
        // 引用变化(commit / 切分支 / fetch)→ graph 失效
        ctx.invalidate(ChangeKind::GitRef);
        ctx.commit_graph(50).unwrap();
        assert_eq!(fb.log_call_count(), 3, "ref 失效后重算");
        // 工作区变化不应动 graph
        ctx.invalidate(ChangeKind::WorkingTree);
        ctx.commit_graph(50).unwrap();
        assert_eq!(fb.log_call_count(), 3, "WorkingTree 失效不该影响 graph");
    }

    #[test]
    fn writes_invalidate_correct_domains() {
        let fb = Arc::new(FakeBackend::default().with_log(vec![fake_commit()]));
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));
        ctx.status().unwrap();
        ctx.commit_graph(50).unwrap();
        // stage 只动暂存区 → 清 status,不清 graph
        ctx.stage(Path::new("a.txt")).unwrap();
        ctx.status().unwrap();
        ctx.commit_graph(50).unwrap();
        assert_eq!(fb.status_call_count(), 2, "stage 后 status 应重打");
        assert_eq!(fb.log_call_count(), 1, "stage 不该使 graph 失效");
        // commit 既清暂存区又产生新提交 → 两者都失效
        ctx.commit("msg").unwrap();
        ctx.status().unwrap();
        ctx.commit_graph(50).unwrap();
        assert_eq!(fb.status_call_count(), 3, "commit 后 status 应重打");
        assert_eq!(fb.log_call_count(), 2, "commit 后 graph 应重算");
    }
}
