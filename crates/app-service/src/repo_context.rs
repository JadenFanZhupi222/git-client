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
use crate::undo_nav::UndoNav;
use crate::watcher::ChangeKind;
use git_core::model::{RebaseStep, ResetMode};
use git_core::{GitBackend, GitError, UndoKind};
use ipc_types::{
    AheadBehindDto, BlameLineDto, BranchDeleteImpactDto, BranchDto, CommitDto, ConflictSidesDto,
    FetchResultDto, FileChangeDto, FileDiffDto, GraphRowDto, OpLogDto, OpLogEntryDto,
    PullResultDto, PushResultDto, RefDto, ReflogEntryDto, SignatureInfoDto, StashDto, StatusDto,
    SubmoduleInfoDto, UndoStateDto, UndoStepDto, WorktreeInfoDto,
};
use lru::LruCache;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// 每个「重读操作类」一个代次计数器(M1.3 取消)。
///
/// 新请求 `fetch_add` 推进计数 → 被它取代的旧请求,其闭包看到「当前代次 ≠ 自己的」
/// 即返回 `GitError::Cancelled`,尽快从阻塞线程池放手。每类一个、互不干扰:切分支触发
/// 的 graph 重算不会误取消正在跑的 blame。计数只在「缓存未命中=真正下探后端」时推进,
/// 缓存命中瞬回、不打扰任何在跑的请求。
#[derive(Default)]
struct CancelGens {
    log: AtomicU64,
    graph: AtomicU64,
    blame: AtomicU64,
}

/// 同时缓存几个不同 limit 的图谱(「加载更多」会换 limit)。LRU 控上限防内存膨胀。
const GRAPH_CACHE_CAP: usize = 8;
/// 按 key 缓存的多条目缓存(diff/blame/log/compare)的容量上限:浏览多文件时够用。
const ENTRY_CACHE_CAP: usize = 32;

fn lru<K: std::hash::Hash + Eq, V>(cap: usize) -> Mutex<LruCache<K, V>> {
    Mutex::new(LruCache::new(NonZeroUsize::new(cap).unwrap()))
}

/// 图谱累加器(M1.5 增量泳道):`rows` 是已算好的前缀(含 refs/sync),`state` 是
/// 续算泳道的断点,`complete` 表示已到历史开端。「加载更多」时只续算尾段并 append,
/// 不重算全量;`GitRef` 失效时整体重置。
#[derive(Default)]
struct GraphAccum {
    rows: Vec<GraphRowDto>,
    state: crate::graph::LayoutState,
    complete: bool,
}

/// 每个仓库一份读缓存(M1.2)。挂在 [`RepoContext`] 内,随上下文长驻。
///
/// 切 tab / 重渲染会反复读同样的数据,这里命中即瞬回;失效由 [`RepoContext::invalidate`]
/// (文件监听 + 自身写操作)按「变化域」精准驱动。`LruCache` 非线程安全,故各包一层
/// `Mutex` —— 锁只在查/存的一瞬持有,后端调用本身不持锁(不违反「慢操作持锁」铁律)。
///
/// 三类失效语义:
/// - **不可变(按 SHA 寻址)**:`commit_files`/`commit_diff` —— 提交内容永不变,永不失效,只靠 LRU 淘汰。
/// - **worktree 域**:`status`/`working_diff` —— 工作区/暂存区变化即失效。
/// - **ref 域**:`graph`/`log`/`refs`/`branches`/`current_branch`/`ahead_behind`/`compare_*` —— 引用/提交变化即失效。
/// - `blame` 两域都清(未提交改动会改变它的「尚未提交」行)。
struct RepoCache {
    // 不可变(SHA 寻址,永不失效)
    commit_files: Mutex<LruCache<String, Vec<FileChangeDto>>>,
    commit_diff: Mutex<LruCache<(String, String), FileDiffDto>>,
    // worktree 域
    status: Mutex<Option<StatusDto>>,
    working_diff: Mutex<LruCache<(String, bool), FileDiffDto>>,
    // ref 域
    graph: Mutex<GraphAccum>, // M1.5:增量累加器(非 LRU)
    log: Mutex<LruCache<(usize, usize), Vec<CommitDto>>>,
    refs: Mutex<Option<Vec<RefDto>>>,
    branches: Mutex<Option<Vec<BranchDto>>>,
    current_branch: Mutex<Option<Option<String>>>,
    ahead_behind: Mutex<Option<Option<AheadBehindDto>>>,
    compare_files: Mutex<LruCache<(String, String), Vec<FileChangeDto>>>,
    compare_diff: Mutex<LruCache<(String, String, String), FileDiffDto>>,
    // 两域都清
    blame: Mutex<LruCache<String, Vec<BlameLineDto>>>,
}

impl Default for RepoCache {
    fn default() -> Self {
        Self {
            commit_files: lru(ENTRY_CACHE_CAP),
            commit_diff: lru(ENTRY_CACHE_CAP),
            status: Mutex::new(None),
            working_diff: lru(ENTRY_CACHE_CAP),
            graph: Mutex::new(GraphAccum::default()),
            log: lru(GRAPH_CACHE_CAP),
            refs: Mutex::new(None),
            branches: Mutex::new(None),
            current_branch: Mutex::new(None),
            ahead_behind: Mutex::new(None),
            compare_files: lru(ENTRY_CACHE_CAP),
            compare_diff: lru(ENTRY_CACHE_CAP),
            blame: lru(ENTRY_CACHE_CAP),
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
    cancel: CancelGens,
    /// 多级 Undo/Redo 的操作时间线 + 光标(M2.1)。本工具做的写操作记进来,
    /// 撤销/重做沿它移动光标。`Mutex` 保护:写操作罕见、用户串行,无锁竞争。
    nav: Mutex<UndoNav>,
}

impl RepoContext {
    fn new(backend: Arc<dyn GitBackend>, path: PathBuf) -> Self {
        Self {
            service: RepoService::new(backend),
            path,
            cache: RepoCache::default(),
            cancel: CancelGens::default(),
            nav: Mutex::new(UndoNav::default()),
        }
    }

    /// 此上下文绑定的仓库路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 按「变化域」失效缓存。文件监听(外部改动)与自身写操作共用一套语义。
    /// 不可变缓存(commit_files/commit_diff,按 SHA 寻址)永不在此清除。
    pub fn invalidate(&self, kind: ChangeKind) {
        let c = &self.cache;
        match kind {
            // 工作区/暂存区变 → status / 工作区 diff 失效(不动 graph/log,免得切个文件就重算)。
            ChangeKind::WorkingTree | ChangeKind::Index => {
                *c.status.lock().unwrap() = None;
                c.working_diff.lock().unwrap().clear();
                c.blame.lock().unwrap().clear();
            }
            // 引用/提交变(commit/切分支/fetch)→ 历史、引用、比较类全失效。
            ChangeKind::GitRef => {
                *c.graph.lock().unwrap() = GraphAccum::default();
                c.log.lock().unwrap().clear();
                *c.refs.lock().unwrap() = None;
                *c.branches.lock().unwrap() = None;
                *c.current_branch.lock().unwrap() = None;
                *c.ahead_behind.lock().unwrap() = None;
                c.compare_files.lock().unwrap().clear();
                c.compare_diff.lock().unwrap().clear();
                c.blame.lock().unwrap().clear();
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
        let key = (limit, skip);
        if let Some(hit) = self.cache.log.lock().unwrap().get(&key).cloned() {
            return Ok(hit);
        }
        // 缓存未命中 = 一次真正的重读:推进代次取消上一次仍在跑的 log,并把
        // 「是否被取代」做成闭包传进后端循环。被取代时提前返回 Cancelled,不写缓存。
        let mine = self.cancel.log.fetch_add(1, Ordering::SeqCst) + 1;
        let cancelled = || self.cancel.log.load(Ordering::SeqCst) != mine;
        let v = self.service.log(&self.path, limit, skip, &cancelled)?;
        self.cache.log.lock().unwrap().put(key, v.clone());
        Ok(v)
    }
    /// 增量图谱(M1.5):已算够 / 已到底则切片瞬回;否则只续算尾段 [已算..limit]。
    pub fn commit_graph(&self, limit: usize) -> Result<Vec<GraphRowDto>, GitError> {
        // 快路径:持锁极短。够了或已到历史开端 → 切片返回,零计算、不取消任何在跑请求。
        let (skip, mut state) = {
            let accum = self.cache.graph.lock().unwrap();
            if accum.complete || limit <= accum.rows.len() {
                let n = limit.min(accum.rows.len());
                return Ok(accum.rows[..n].to_vec());
            }
            (accum.rows.len(), accum.state.clone())
        };
        // 续算尾段(真正的重读):**锁外**计算,不阻塞其它 graph 请求,且可被新请求取消。
        let mine = self.cancel.graph.fetch_add(1, Ordering::SeqCst) + 1;
        let cancelled = || self.cancel.graph.load(Ordering::SeqCst) != mine;
        let take = limit - skip;
        let new_rows = self
            .service
            .commit_graph_page(&self.path, skip, take, &mut state, &cancelled)?;
        // 提交:重新持锁。若期间 accum 被失效清空 / 被另一请求推进(len 变了),丢弃本次
        // 结果(失效后前端会重新请求,自愈),只返回当前能给的切片,绝不错拼。
        let mut accum = self.cache.graph.lock().unwrap();
        if accum.rows.len() == skip {
            if new_rows.len() < take {
                accum.complete = true; // log 返回不足请求数 → 已到历史开端
            }
            accum.state = state;
            accum.rows.extend(new_rows);
        }
        let n = limit.min(accum.rows.len());
        Ok(accum.rows[..n].to_vec())
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
    /// 某文件的提交历史(git log --follow)。按需触发、结果短,不缓存。
    pub fn file_history(&self, file: &str, limit: usize) -> Result<Vec<CommitDto>, GitError> {
        self.service.file_history(&self.path, file, limit)
    }
    pub fn reflog(&self, limit: usize) -> Result<Vec<ReflogEntryDto>, GitError> {
        self.service.reflog(&self.path, limit)
    }
    /// 不可变:某提交的文件列表按 SHA 缓存,永不失效(提交内容不变)。
    pub fn commit_files(&self, commit_id: &str) -> Result<Vec<FileChangeDto>, GitError> {
        if let Some(hit) = self
            .cache
            .commit_files
            .lock()
            .unwrap()
            .get(commit_id)
            .cloned()
        {
            return Ok(hit);
        }
        let v = self.service.commit_files(&self.path, commit_id)?;
        self.cache
            .commit_files
            .lock()
            .unwrap()
            .put(commit_id.to_string(), v.clone());
        Ok(v)
    }
    /// 不可变:某提交中单文件的 diff 按 (SHA, file) 缓存,永不失效。
    pub fn commit_file_diff(&self, commit_id: &str, file: &str) -> Result<FileDiffDto, GitError> {
        let key = (commit_id.to_string(), file.to_string());
        if let Some(hit) = self.cache.commit_diff.lock().unwrap().get(&key).cloned() {
            return Ok(hit);
        }
        let v = self.service.commit_file_diff(&self.path, commit_id, file)?;
        self.cache.commit_diff.lock().unwrap().put(key, v.clone());
        Ok(v)
    }
    pub fn compare_files(&self, from: &str, to: &str) -> Result<Vec<FileChangeDto>, GitError> {
        let key = (from.to_string(), to.to_string());
        if let Some(hit) = self.cache.compare_files.lock().unwrap().get(&key).cloned() {
            return Ok(hit);
        }
        let v = self.service.compare_files(&self.path, from, to)?;
        self.cache.compare_files.lock().unwrap().put(key, v.clone());
        Ok(v)
    }
    pub fn compare_file_diff(
        &self,
        from: &str,
        to: &str,
        file: &str,
    ) -> Result<FileDiffDto, GitError> {
        let key = (from.to_string(), to.to_string(), file.to_string());
        if let Some(hit) = self.cache.compare_diff.lock().unwrap().get(&key).cloned() {
            return Ok(hit);
        }
        let v = self.service.compare_file_diff(&self.path, from, to, file)?;
        self.cache.compare_diff.lock().unwrap().put(key, v.clone());
        Ok(v)
    }
    pub fn working_diff(&self, file: &str, staged: bool) -> Result<FileDiffDto, GitError> {
        let key = (file.to_string(), staged);
        if let Some(hit) = self.cache.working_diff.lock().unwrap().get(&key).cloned() {
            return Ok(hit);
        }
        let v = self.service.working_diff(&self.path, file, staged)?;
        self.cache.working_diff.lock().unwrap().put(key, v.clone());
        Ok(v)
    }
    /// 某提交的签名状态。一次 CLI 调用、签名按 SHA 不变,前端 query 已按 commit 缓存,
    /// 这里不再额外缓存(避免给 RepoCache 加字段),按需直读。
    pub fn commit_signature(&self, commit_id: &str) -> Result<SignatureInfoDto, GitError> {
        self.service.commit_signature(&self.path, commit_id)
    }
    /// 子模块列表(只读)。一次 CLI 调用,前端 query 已缓存,这里不额外缓存。
    pub fn list_submodules(&self) -> Result<Vec<SubmoduleInfoDto>, GitError> {
        self.service.list_submodules(&self.path)
    }
    /// 工作树列表(只读)。前端 query 已缓存,这里不额外缓存。
    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfoDto>, GitError> {
        self.service.list_worktrees(&self.path)
    }
    /// 稀疏检出范围(只读)。
    pub fn sparse_checkout_patterns(&self) -> Result<Vec<String>, GitError> {
        self.service.sparse_checkout_patterns(&self.path)
    }
    pub fn current_branch(&self) -> Result<Option<String>, GitError> {
        if let Some(hit) = self.cache.current_branch.lock().unwrap().clone() {
            return Ok(hit);
        }
        let v = self.service.current_branch(&self.path)?;
        *self.cache.current_branch.lock().unwrap() = Some(v.clone());
        Ok(v)
    }
    pub fn branches(&self) -> Result<Vec<BranchDto>, GitError> {
        if let Some(hit) = self.cache.branches.lock().unwrap().clone() {
            return Ok(hit);
        }
        let v = self.service.branches(&self.path)?;
        *self.cache.branches.lock().unwrap() = Some(v.clone());
        Ok(v)
    }
    pub fn ahead_behind(&self) -> Result<Option<AheadBehindDto>, GitError> {
        if let Some(hit) = self.cache.ahead_behind.lock().unwrap().clone() {
            return Ok(hit);
        }
        let v = self.service.ahead_behind(&self.path)?;
        *self.cache.ahead_behind.lock().unwrap() = Some(v.clone());
        Ok(v)
    }
    pub fn remotes(&self) -> Result<Vec<String>, GitError> {
        self.service.remotes(&self.path)
    }
    pub fn refs(&self) -> Result<Vec<RefDto>, GitError> {
        if let Some(hit) = self.cache.refs.lock().unwrap().clone() {
            return Ok(hit);
        }
        let v = self.service.refs(&self.path)?;
        *self.cache.refs.lock().unwrap() = Some(v.clone());
        Ok(v)
    }
    pub fn repo_state(&self) -> Result<String, GitError> {
        self.service.repo_state(&self.path)
    }
    pub fn blame(&self, file: &str) -> Result<Vec<BlameLineDto>, GitError> {
        if let Some(hit) = self.cache.blame.lock().unwrap().get(file).cloned() {
            return Ok(hit);
        }
        let mine = self.cancel.blame.fetch_add(1, Ordering::SeqCst) + 1;
        let cancelled = || self.cancel.blame.load(Ordering::SeqCst) != mine;
        let v = self.service.blame(&self.path, file, &cancelled)?;
        self.cache
            .blame
            .lock()
            .unwrap()
            .put(file.to_string(), v.clone());
        Ok(v)
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
        let before = self.head_oid_opt();
        let out = self.service.commit(&self.path, message)?;
        self.after_write(true, true);
        self.record_nav(before, "提交", UndoKind::Uncommit);
        Ok(out)
    }
    pub fn amend_commit(&self, message: Option<&str>) -> Result<String, GitError> {
        let before = self.head_oid_opt();
        let out = self.service.amend_commit(&self.path, message)?;
        self.after_write(true, true);
        self.record_nav(before, "修改提交(amend)", UndoKind::Uncommit);
        Ok(out)
    }
    pub fn set_upstream(&self, upstream: &str) -> Result<(), GitError> {
        self.service.set_upstream(&self.path, upstream)?;
        self.after_write(false, true);
        Ok(())
    }
    /// 初始化/更新子模块。会改子模块工作区内容(从超级项目看是该目录的状态变化)→
    /// 失效 worktree 域(status / 工作区 diff)。不动超级项目的引用。
    pub fn update_submodule(&self, path: &str) -> Result<(), GitError> {
        self.service.update_submodule(&self.path, path)?;
        self.after_write(true, false);
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
    /// 删分支影响预览(只读,删除前调)。不缓存:仅在用户点删除时按需算一次。
    pub fn branch_delete_impact(&self, name: &str) -> Result<BranchDeleteImpactDto, GitError> {
        self.service.branch_delete_impact(&self.path, name)
    }
    pub fn fetch(&self, remote: Option<&str>) -> Result<FetchResultDto, GitError> {
        let out = self.service.fetch(&self.path, remote)?;
        self.after_write(false, true);
        Ok(out)
    }
    pub fn pull(&self, remote: Option<&str>, rebase: bool) -> Result<PullResultDto, GitError> {
        let before = self.head_oid_opt();
        let out = self.service.pull(&self.path, remote, rebase)?;
        self.after_write(true, true);
        self.record_nav(before, "拉取(pull)", UndoKind::Restore);
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
        let before = self.head_oid_opt();
        self.service.cherry_pick(&self.path, commit_id)?;
        self.after_write(true, true);
        self.record_nav(before, "cherry-pick", UndoKind::Restore);
        Ok(())
    }
    pub fn revert(&self, commit_id: &str) -> Result<(), GitError> {
        let before = self.head_oid_opt();
        self.service.revert(&self.path, commit_id)?;
        self.after_write(true, true);
        self.record_nav(before, "回退提交(revert)", UndoKind::Restore);
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
        let before = self.head_oid_opt();
        self.service.reset(&self.path, commit_id, mode)?;
        self.after_write(true, true);
        self.record_nav(before, "重置(reset)", UndoKind::Restore);
        Ok(())
    }

    // ---- 多级 Undo/Redo(M2.1)----

    /// 读当前 HEAD 的完整 oid;空仓库(无 HEAD)→ None。
    fn head_oid_opt(&self) -> Option<String> {
        self.service.head_commit(&self.path).ok().map(|c| c.id)
    }

    /// 记录一次「我们刚做的」HEAD 移动:捕获操作后 HEAD,连同操作前 `before` + 还原语义
    /// `kind` 喂给时间线。在写方法成功 + 失效缓存之后调用。HEAD 没动(如 up-to-date 的
    /// pull)由 nav 内部忽略。`kind` 决定将来撤销这一步时用 soft(提交类)还是 hard(改写类)。
    fn record_nav(&self, before: Option<String>, label: &str, kind: UndoKind) {
        if let Some(after) = self.head_oid_opt() {
            self.nav
                .lock()
                .unwrap()
                .record(before.as_deref(), &after, label, kind, Self::now());
        }
    }

    /// 当前 Unix 时间戳(秒);时钟异常兜底为 0。给操作日志的点打时间戳。
    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// 冷启动 / 外部改动后重建时间线用的提示:reflog 顶项可安全撤销时给
    /// `(上一步 oid, 操作中文名, 还原语义)`,供 [`UndoNav::sync`] 兜出一次撤销。
    /// 带上 `kind`,这样冷启动后撤销一次 reset 也会正确走 hard(而非留残渣的 soft)。
    fn bootstrap_hint(&self) -> Result<Option<(String, String, UndoKind)>, GitError> {
        let entries = self.service.reflog(&self.path, 2)?;
        let (Some(top), Some(prev)) = (entries.first(), entries.get(1)) else {
            return Ok(None);
        };
        Ok(git_core::classify_undoable(&top.message)
            .map(|(l, k)| (prev.new_oid.clone(), l.to_string(), k)))
    }

    fn short(oid: &str) -> String {
        oid.chars().take(7).collect()
    }

    /// 把一步导航(撤销/重做/跳转)落到后端 reset。**核心**:按还原语义选 soft / hard。
    ///
    /// - `kind == Uncommit`(撤销提交)→ `reset --soft`:只退 HEAD,内容回暂存区,永不丢活。
    /// - `kind == Restore`(撤销 reset/cherry-pick/…)或 `force_hard`(操作日志里跨多步跳转)
    ///   → `reset --hard`:忠实还原工作区到目标点,消除残渣(修掉「撤销 hard reset 后暂存区
    ///   凭空多文件」的 bug)。**但 hard 会覆盖未提交改动**,故先查工作区:有脏改动就拒绝
    ///   (`UncommittedChanges`),让用户先提交/贮藏 —— 守住「永不丢工作」。
    ///
    /// 返回是否真的还原了工作区(true=走了 hard),供前端 toast 精确措辞。
    fn apply_nav(
        &self,
        target_oid: &str,
        kind: UndoKind,
        force_hard: bool,
    ) -> Result<bool, GitError> {
        let hard = force_hard || kind == UndoKind::Restore;
        if hard {
            // 拿新鲜 status(绕过缓存):hard 会丢弃所有未提交改动,有就拒绝。
            let dirty = self.service.status(&self.path)?.entries.len();
            if dirty > 0 {
                return Err(GitError::UncommittedChanges { count: dirty });
            }
        }
        let mode = if hard {
            ResetMode::Hard
        } else {
            ResetMode::Soft
        };
        self.service.reset(&self.path, target_oid, mode)?;
        Ok(hard)
    }

    /// 撤销/重做的当前可用性。先把时间线对齐真实 HEAD(必要时从 reflog 重建),
    /// 再读出前后两个方向。只读,不改仓库。
    pub fn undo_state(&self) -> Result<UndoStateDto, GitError> {
        let Some(head) = self.head_oid_opt() else {
            return Ok(UndoStateDto {
                can_undo: None,
                can_redo: None,
            });
        };
        // 已对齐则免读 reflog(切 tab/历史刷新会频繁调到这里)。
        let aligned = self.nav.lock().unwrap().is_aligned(&head);
        let boot = if aligned {
            None
        } else {
            self.bootstrap_hint()?
        };
        let mut nav = self.nav.lock().unwrap();
        nav.sync(&head, boot, Self::now());
        let to_dto = |s: crate::undo_nav::NavStep| UndoStepDto {
            label: s.label,
            target_short: Self::short(&s.target_oid),
            // 预览:Restore 类撤销会还原工作区(hard),提交类不会(soft)。
            worktree_restored: s.kind == UndoKind::Restore,
        };
        Ok(UndoStateDto {
            can_undo: nav.can_undo().map(to_dto),
            can_redo: nav.can_redo().map(to_dto),
        })
    }

    /// 操作日志:本工具本会话做过的写操作时间线 + 当前光标(高亮「现在在哪」)。
    /// 与 undo_state 同样先对齐真实 HEAD。点击某项 → [`RepoContext::goto`]。
    pub fn op_log(&self) -> Result<OpLogDto, GitError> {
        let Some(head) = self.head_oid_opt() else {
            return Ok(OpLogDto {
                entries: Vec::new(),
                current: 0,
            });
        };
        let aligned = self.nav.lock().unwrap().is_aligned(&head);
        let boot = if aligned {
            None
        } else {
            self.bootstrap_hint()?
        };
        let mut nav = self.nav.lock().unwrap();
        nav.sync(&head, boot, Self::now());
        Ok(OpLogDto {
            entries: nav
                .items()
                .into_iter()
                .map(|it| OpLogEntryDto {
                    label: it.label,
                    target_short: Self::short(&it.oid),
                    timestamp: it.timestamp,
                })
                .collect(),
            current: nav.cursor(),
        })
    }

    /// 跳到操作日志的第 `index` 项。按还原语义 reset(撤销/重做的泛化)。已在该点或越界
    /// → NothingToUndo(无需移动)。**跨多步跳转一律 hard 忠实还原**:soft 只退 HEAD,跨多个
    /// 提交后会让中间差异全堆进暂存区。脏工作区时 hard 被拒(`UncommittedChanges`),不丢活。
    pub fn goto(&self, index: usize) -> Result<UndoStepDto, GitError> {
        let head = self.head_oid_opt().ok_or(GitError::NoHead)?;
        let boot = self.bootstrap_hint()?;
        let (step, multi) = {
            let mut nav = self.nav.lock().unwrap();
            nav.sync(&head, boot, Self::now());
            let cur = nav.cursor();
            let step = nav.point_at(index).ok_or(GitError::NothingToUndo)?;
            (step, index.abs_diff(cur) > 1)
        };
        // reset 失败(含脏工作区拒绝)→ 提前返回,不动光标、不失效缓存。
        let restored = self.apply_nav(&step.target_oid, step.kind, multi)?;
        self.nav.lock().unwrap().commit_goto(index);
        self.after_write(true, true);
        Ok(UndoStepDto {
            label: step.label,
            target_short: Self::short(&step.target_oid),
            worktree_restored: restored,
        })
    }

    /// 撤销一步:回到光标左邻点。提交类走 soft(内容回暂存区,保留活);reset/cherry-pick 等
    /// 改写类走 hard 忠实还原(脏工作区时拒绝,守住不丢活)。无可撤销 → NothingToUndo。
    pub fn undo(&self) -> Result<UndoStepDto, GitError> {
        let head = self.head_oid_opt().ok_or(GitError::NoHead)?;
        let boot = self.bootstrap_hint()?;
        let step = {
            let mut nav = self.nav.lock().unwrap();
            nav.sync(&head, boot, Self::now());
            nav.can_undo().ok_or(GitError::NothingToUndo)?
        };
        // git 重活在锁外做;模式由 apply_nav 按 step.kind 决定。
        let restored = self.apply_nav(&step.target_oid, step.kind, false)?;
        self.nav.lock().unwrap().commit_undo();
        self.after_write(true, true);
        Ok(UndoStepDto {
            label: step.label,
            target_short: Self::short(&step.target_oid),
            worktree_restored: restored,
        })
    }

    /// 重做一步:回到光标右邻点。模式与撤销镜像(被重做的操作是提交类→soft,改写类→hard)。
    /// 无可重做 → NothingToRedo。
    pub fn redo(&self) -> Result<UndoStepDto, GitError> {
        let head = self.head_oid_opt().ok_or(GitError::NoHead)?;
        let boot = self.bootstrap_hint()?;
        let step = {
            let mut nav = self.nav.lock().unwrap();
            nav.sync(&head, boot, Self::now());
            nav.can_redo().ok_or(GitError::NothingToRedo)?
        };
        let restored = self.apply_nav(&step.target_oid, step.kind, false)?;
        self.nav.lock().unwrap().commit_redo();
        self.after_write(true, true);
        Ok(UndoStepDto {
            label: step.label,
            target_short: Self::short(&step.target_oid),
            worktree_restored: restored,
        })
    }
    pub fn interactive_rebase(
        &self,
        base: Option<&str>,
        steps: &[RebaseStep],
    ) -> Result<(), GitError> {
        let before = self.head_oid_opt();
        self.service.interactive_rebase(&self.path, base, steps)?;
        self.after_write(true, true);
        self.record_nav(before, "变基(rebase)", UndoKind::Restore);
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
    fn graph_increments_and_invalidates() {
        // M1.5:limit 增长只续算新尾段;切片不打后端;探到底标记 complete;
        // GitRef 失效后重算;WorkingTree 不影响 graph。
        let mk = |id: &str| {
            use git_core::model::{Commit, Signature};
            Commit {
                id: id.into(),
                short_id: id.into(),
                summary: String::new(),
                body: String::new(),
                author: Signature {
                    name: "n".into(),
                    email: "e".into(),
                },
                timestamp: 0,
                parents: vec![],
            }
        };
        let fb = Arc::new(FakeBackend::default().with_log(vec![mk("a"), mk("b"), mk("c")]));
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));

        assert_eq!(ctx.commit_graph(2).unwrap().len(), 2);
        assert_eq!(fb.log_call_count(), 1, "首次取 2 条:log 一次");
        assert_eq!(ctx.commit_graph(2).unwrap().len(), 2);
        assert_eq!(fb.log_call_count(), 1, "limit 不变:切片命中,不打后端");
        assert_eq!(ctx.commit_graph(3).unwrap().len(), 3);
        assert_eq!(
            fb.log_call_count(),
            2,
            "加载更多:只续算尾段 [2..3],再 log 一次"
        );
        assert_eq!(ctx.commit_graph(10).unwrap().len(), 3);
        assert_eq!(
            fb.log_call_count(),
            3,
            "探到历史开端再 log 一次(随后标记 complete)"
        );
        assert_eq!(ctx.commit_graph(10).unwrap().len(), 3);
        assert_eq!(fb.log_call_count(), 3, "complete 后切片,不再 log");

        // 工作区变化不该动 graph
        ctx.invalidate(ChangeKind::WorkingTree);
        ctx.commit_graph(2).unwrap();
        assert_eq!(fb.log_call_count(), 3, "WorkingTree 失效不该影响 graph");

        // 引用变化(commit / 切分支 / fetch)→ 累加器重置,重算
        ctx.invalidate(ChangeKind::GitRef);
        assert_eq!(ctx.commit_graph(2).unwrap().len(), 2);
        assert_eq!(fb.log_call_count(), 4, "GitRef 失效后重算");
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

    // ---- slice 2:其余读路径 ----

    #[test]
    fn commit_diff_is_immutable_never_invalidated() {
        let fb = Arc::new(FakeBackend::default());
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));
        ctx.commit_file_diff("sha1", "a.txt").unwrap();
        ctx.commit_file_diff("sha1", "a.txt").unwrap();
        assert_eq!(
            fb.commit_file_diff_call_count(),
            1,
            "同 (SHA,file) 命中缓存"
        );
        // 即便引用变化,按 SHA 寻址的内容永不变 → 不失效
        ctx.invalidate(ChangeKind::GitRef);
        ctx.commit_file_diff("sha1", "a.txt").unwrap();
        assert_eq!(
            fb.commit_file_diff_call_count(),
            1,
            "不可变缓存不该被 ref 失效"
        );
        // 不同 file 是不同 key
        ctx.commit_file_diff("sha1", "b.txt").unwrap();
        assert_eq!(fb.commit_file_diff_call_count(), 2);
    }

    #[test]
    fn working_diff_is_worktree_domain() {
        let fb = Arc::new(FakeBackend::default());
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));
        ctx.working_diff("a.txt", false).unwrap();
        ctx.working_diff("a.txt", false).unwrap();
        assert_eq!(fb.working_diff_call_count(), 1);
        // ref 域变化不该动工作区 diff
        ctx.invalidate(ChangeKind::GitRef);
        ctx.working_diff("a.txt", false).unwrap();
        assert_eq!(
            fb.working_diff_call_count(),
            1,
            "GitRef 不该使 working_diff 失效"
        );
        // 工作区变化才失效
        ctx.invalidate(ChangeKind::WorkingTree);
        ctx.working_diff("a.txt", false).unwrap();
        assert_eq!(fb.working_diff_call_count(), 2);
        // staged 不同是不同 key
        ctx.working_diff("a.txt", true).unwrap();
        assert_eq!(fb.working_diff_call_count(), 3);
    }

    #[test]
    fn blame_cleared_by_both_domains() {
        let fb = Arc::new(FakeBackend::default());
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));
        ctx.blame("a.txt").unwrap();
        ctx.blame("a.txt").unwrap();
        assert_eq!(fb.blame_call_count(), 1);
        // 未提交改动会改变 blame 的「尚未提交」行 → 工作区域也清
        ctx.invalidate(ChangeKind::WorkingTree);
        ctx.blame("a.txt").unwrap();
        assert_eq!(fb.blame_call_count(), 2);
        ctx.invalidate(ChangeKind::GitRef);
        ctx.blame("a.txt").unwrap();
        assert_eq!(fb.blame_call_count(), 3);
    }

    #[test]
    fn refs_is_single_value_ref_domain() {
        let fb = Arc::new(FakeBackend::default());
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));
        ctx.refs().unwrap();
        ctx.refs().unwrap();
        assert_eq!(fb.refs_call_count(), 1);
        // 工作区变化不该动 refs
        ctx.invalidate(ChangeKind::WorkingTree);
        ctx.refs().unwrap();
        assert_eq!(fb.refs_call_count(), 1, "WorkingTree 不该使 refs 失效");
        // 引用变化才失效
        ctx.invalidate(ChangeKind::GitRef);
        ctx.refs().unwrap();
        assert_eq!(fb.refs_call_count(), 2);
    }

    // ---- M2.1 多级 Undo/Redo(RepoContext 把纯时间线接到后端 reset)----

    fn reflog_top(msg: &str, prev_oid: &str) -> Vec<git_core::model::ReflogEntry> {
        use git_core::model::{ReflogEntry, Signature};
        let sig = Signature {
            name: "n".into(),
            email: "e".into(),
        };
        vec![
            ReflogEntry {
                index: 0,
                selector: "HEAD@{0}".into(),
                new_oid: "head".into(),
                new_short: "head".into(),
                message: msg.into(),
                committer: sig.clone(),
                timestamp: 0,
            },
            ReflogEntry {
                index: 1,
                selector: "HEAD@{1}".into(),
                new_oid: prev_oid.into(),
                new_short: prev_oid.chars().take(7).collect(),
                message: "older".into(),
                committer: sig,
                timestamp: 0,
            },
        ]
    }

    #[test]
    fn undo_then_redo_walks_head_via_reset_soft() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        // 冷启动:HEAD 在 B,reflog 顶项是可撤销的 commit、上一步是 A。
        let fb = Arc::new(
            FakeBackend::default()
                .with_head(&b)
                .with_reflog(reflog_top("commit: 改了点东西", &a)),
        );
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));

        // 初始:可撤销那次提交,暂无重做。
        let st = ctx.undo_state().unwrap();
        assert_eq!(st.can_undo.as_ref().map(|s| s.label.as_str()), Some("提交"));
        assert_eq!(st.can_undo.unwrap().target_short, "aaaaaaa");
        assert!(st.can_redo.is_none());

        // 撤销 → reset --soft 到 A,HEAD 移到 A。
        let done = ctx.undo().unwrap();
        assert_eq!(done.label, "提交");
        assert_eq!(done.target_short, "aaaaaaa");
        assert_eq!(fb.head_oid().as_deref(), Some(a.as_str()));
        assert_eq!(fb.tag_ops(), vec![format!("reset:soft:{a}")]);

        // 撤销后:不能再撤(已到底),但能重做回 B。
        let st = ctx.undo_state().unwrap();
        assert!(st.can_undo.is_none());
        assert_eq!(st.can_redo.as_ref().map(|s| s.label.as_str()), Some("提交"));
        assert_eq!(st.can_redo.unwrap().target_short, "bbbbbbb");

        // 重做 → reset --soft 回 B。
        let done = ctx.redo().unwrap();
        assert_eq!(done.target_short, "bbbbbbb");
        assert_eq!(fb.head_oid().as_deref(), Some(b.as_str()));

        // 回到原位:又能撤销、不能重做。无重做时再 redo → NothingToRedo。
        let st = ctx.undo_state().unwrap();
        assert!(st.can_undo.is_some());
        assert!(st.can_redo.is_none());
        assert!(matches!(ctx.redo(), Err(GitError::NothingToRedo)));
    }

    #[test]
    fn undo_unavailable_when_top_op_not_undoable() {
        let fb = Arc::new(
            FakeBackend::default()
                .with_head(&"b".repeat(40))
                .with_reflog(reflog_top("checkout: moving from x to y", &"a".repeat(40))),
        );
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));
        // checkout 不可用 reset 撤销 → 无撤销项,且 undo 报错、绝不动 HEAD。
        assert!(ctx.undo_state().unwrap().can_undo.is_none());
        assert!(matches!(ctx.undo(), Err(GitError::NothingToUndo)));
        assert!(fb.tag_ops().is_empty());
    }

    #[test]
    fn op_log_lists_timeline_and_goto_jumps() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let fb = Arc::new(
            FakeBackend::default()
                .with_head(&b)
                .with_reflog(reflog_top("commit: 改了点东西", &a)),
        );
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));

        // 冷启动 bootstrap:时间线 [起点(A), 提交(B)],当前在 B(index 1)。
        let log = ctx.op_log().unwrap();
        assert_eq!(log.entries.len(), 2);
        assert_eq!(log.current, 1);
        assert_eq!(log.entries[0].label, "起点");
        assert_eq!(log.entries[0].target_short, "aaaaaaa");
        assert_eq!(log.entries[1].label, "提交");
        assert_eq!(log.entries[1].target_short, "bbbbbbb");

        // 点第 0 项 → 跳回 A(reset --soft),光标到 0。
        let done = ctx.goto(0).unwrap();
        assert_eq!(done.target_short, "aaaaaaa");
        assert_eq!(fb.head_oid().as_deref(), Some(a.as_str()));
        assert_eq!(ctx.op_log().unwrap().current, 0);

        // 跳到当前项 → NothingToUndo(无需移动),不再 reset。
        let resets_before = fb.tag_ops().len();
        assert!(matches!(ctx.goto(0), Err(GitError::NothingToUndo)));
        assert_eq!(fb.tag_ops().len(), resets_before, "跳到原地不该 reset");

        // 点回第 1 项 → 跳到 B,光标回 1。
        ctx.goto(1).unwrap();
        assert_eq!(fb.head_oid().as_deref(), Some(b.as_str()));
        assert_eq!(ctx.op_log().unwrap().current, 1);
    }

    // ---- 还原语义:撤销 reset/cherry-pick 等改写类必须 hard,否则暂存区留残渣 ----

    #[test]
    fn undo_of_reset_uses_hard_to_faithfully_restore() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        // 冷启动:HEAD 在 B,reflog 顶项是一次 reset、上一步是 A。工作区干净。
        let fb = Arc::new(
            FakeBackend::default()
                .with_head(&b)
                .with_reflog(reflog_top("reset: moving to A", &a)),
        );
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));

        // 撤销这次 reset:必须走 hard 忠实还原(不是 soft —— soft 会把 A..B 差异留在暂存区,
        // 正是真机里「撤销 hard reset 后暂存区凭空多文件」的 bug)。
        let done = ctx.undo().unwrap();
        assert_eq!(done.label, "重置(reset)");
        assert!(done.worktree_restored, "撤销 reset 应还原工作区(hard)");
        assert_eq!(fb.head_oid().as_deref(), Some(a.as_str()));
        assert_eq!(
            fb.tag_ops(),
            vec![format!("reset:hard:{a}")],
            "撤销 reset 必须 hard,不能 soft"
        );
    }

    #[test]
    fn undo_hard_path_refuses_when_worktree_dirty() {
        use git_core::model::{FileEntry, FileState};
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        // 顶项是 reset(撤销需 hard),但工作区有 1 处未提交改动 → hard 会冲掉它。
        let fb = Arc::new(
            FakeBackend::default()
                .with_head(&b)
                .with_reflog(reflog_top("reset: moving to A", &a))
                .with_status_entries(vec![FileEntry {
                    path: "dirty.txt".into(),
                    state: FileState::Modified,
                    staged: false,
                }]),
        );
        let ctx = RepoRegistry::new(fb.clone()).context(Path::new("/r"));

        // 必须拒绝并报 UncommittedChanges,绝不 reset、绝不动 HEAD(永不丢工作)。
        assert!(matches!(
            ctx.undo(),
            Err(GitError::UncommittedChanges { count: 1 })
        ));
        assert!(fb.tag_ops().is_empty(), "拒绝时不该执行任何 reset");
        assert_eq!(fb.head_oid().as_deref(), Some(b.as_str()), "HEAD 不该移动");
    }
}
