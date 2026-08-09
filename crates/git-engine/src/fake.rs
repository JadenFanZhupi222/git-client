use git_core::model::{
    AheadBehind, BlameLine, BranchDeleteImpact, BranchInfo, Commit, CommitRef, ConflictSides,
    FetchOutcome, FileChange, FileDiff, FileEntry, LineHistoryEntry, MergeOutcome, PullOutcome,
    PushOutcome, RebaseAction, RebaseStep, ReflogEntry, RemoteInfo, RepoState, ResetMode,
    Signature, SignatureInfo, StashEntry, SubmoduleInfo, SyncCommits, WorkingTreeStatus,
    WorktreeInfo,
};
use git_core::{GitBackend, GitError};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 测试/演示用的假后端。阶段 1 起带内部状态,记录被 stage/commit 的调用供断言。
/// ⚠️ 用 Mutex 而非 RefCell:GitBackend 要求 Send + Sync,RefCell 是 !Sync 编译不过。
/// Mutex 提供"跨线程安全的内部可变性"。
#[derive(Default)]
pub struct FakeBackend {
    canned_status: Mutex<Vec<FileEntry>>,
    staged: Mutex<Vec<PathBuf>>,
    unstaged: Mutex<Vec<PathBuf>>,
    commits: Mutex<Vec<String>>,
    commit_error: Mutex<Option<String>>,
    canned_log: Mutex<Vec<Commit>>,
    canned_commit_files: Mutex<Vec<FileChange>>,
    canned_branch: Mutex<Option<String>>,
    canned_file_diff: Mutex<FileDiff>,
    canned_signature: Mutex<SignatureInfo>,
    canned_submodules: Mutex<Vec<SubmoduleInfo>>,
    submodule_ops: Mutex<Vec<String>>,
    canned_worktrees: Mutex<Vec<WorktreeInfo>>,
    canned_sparse: Mutex<Vec<String>>,
    canned_branches: Mutex<Vec<BranchInfo>>,
    canned_refs: Mutex<Vec<CommitRef>>,
    canned_ahead_behind: Mutex<Option<AheadBehind>>,
    canned_sync_commits: Mutex<SyncCommits>,
    canned_remotes: Mutex<Vec<String>>,
    // 记录远程管理写操作(add/remove/rename),供测试断言转发。
    remote_ops: Mutex<Vec<String>>,
    // 合并:记录被合并进当前分支的名字 + 可预置结果。
    merge_ops: Mutex<Vec<String>>,
    canned_merge: Mutex<Option<MergeOutcome>>,
    // onboarding:记录 init/clone 调用(如 ["init /a", "clone url→/b"])。
    onboard_ops: Mutex<Vec<String>>,
    checked_out: Mutex<Vec<String>>,
    created: Mutex<Vec<String>>,
    deleted: Mutex<Vec<String>>,
    canned_fetch: Mutex<Option<FetchOutcome>>,
    fetch_calls: Mutex<u32>,
    canned_pull: Mutex<Option<PullOutcome>>,
    pull_calls: Mutex<u32>,
    canned_push: Mutex<Option<PushOutcome>>,
    push_calls: Mutex<u32>,
    staged_hunks: Mutex<Vec<(String, usize)>>,
    unstaged_hunks: Mutex<Vec<(String, usize)>>,
    upstreams_set: Mutex<Vec<String>>,
    canned_stashes: Mutex<Vec<StashEntry>>,
    stash_ops: Mutex<Vec<String>>,
    canned_repo_state: Mutex<Option<RepoState>>, // None → Clean
    conflict_ops: Mutex<Vec<String>>,
    canned_conflict_sides: Mutex<Option<ConflictSides>>,
    tag_ops: Mutex<Vec<String>>,
    rebase_ops: Mutex<Vec<String>>,
    canned_reflog: Mutex<Vec<ReflogEntry>>,
    canned_branch_delete_impact: Mutex<BranchDeleteImpact>,
    // 可移动的 HEAD oid:Some 时 head_commit 返回它;reset 会把它设到目标。
    // 供 RepoContext 的 Undo/Redo 时间线测试模拟 HEAD 真实移动。
    head_oid: Mutex<Option<String>>,
    // 读路径调用计数:供缓存测试断言「命中不打后端 / 失效后重打」。
    status_calls: Mutex<u32>,
    log_calls: Mutex<u32>,
    commit_file_diff_calls: Mutex<u32>,
    working_diff_calls: Mutex<u32>,
    blame_calls: Mutex<u32>,
    refs_calls: Mutex<u32>,
    // M6.3:三条 CLI 读路径的缓存测试用计数 + 预置返回值。
    file_history_calls: Mutex<u32>,
    line_history_calls: Mutex<u32>,
    pickaxe_calls: Mutex<u32>,
    canned_line_history: Mutex<Vec<LineHistoryEntry>>,
    // M6.2:read_blob 预置返回(图片字节流)。
    canned_blob: Mutex<Vec<u8>>,
}

impl FakeBackend {
    /// 预置一份 status 返回值,供 app-service 的 DTO 映射测试。
    pub fn with_status(entries: Vec<FileEntry>) -> Self {
        let fb = Self::default();
        *fb.canned_status.lock().unwrap() = entries;
        fb
    }
    pub fn staged_files(&self) -> Vec<PathBuf> {
        self.staged.lock().unwrap().clone()
    }
    pub fn unstaged_files(&self) -> Vec<PathBuf> {
        self.unstaged.lock().unwrap().clone()
    }
    pub fn commit_messages(&self) -> Vec<String> {
        self.commits.lock().unwrap().clone()
    }
    pub fn fail_commit_with(self, message: impl Into<String>) -> Self {
        *self.commit_error.lock().unwrap() = Some(message.into());
        self
    }
    pub fn with_log(self, commits: Vec<Commit>) -> Self {
        *self.canned_log.lock().unwrap() = commits;
        self
    }
    /// 预置 read_blob 返回的字节(M6.2 图片字节流测试)。
    pub fn with_blob(self, bytes: Vec<u8>) -> Self {
        *self.canned_blob.lock().unwrap() = bytes;
        self
    }
    pub fn with_commit_files(self, files: Vec<FileChange>) -> Self {
        *self.canned_commit_files.lock().unwrap() = files;
        self
    }
    pub fn with_branch(self, branch: Option<String>) -> Self {
        *self.canned_branch.lock().unwrap() = branch;
        self
    }
    pub fn with_signature(self, sig: SignatureInfo) -> Self {
        *self.canned_signature.lock().unwrap() = sig;
        self
    }
    pub fn with_submodules(self, subs: Vec<SubmoduleInfo>) -> Self {
        *self.canned_submodules.lock().unwrap() = subs;
        self
    }
    /// 断言用:记录被 update 的子模块路径(按调用顺序)。
    pub fn submodule_ops(&self) -> Vec<String> {
        self.submodule_ops.lock().unwrap().clone()
    }
    pub fn with_worktrees(self, wts: Vec<WorktreeInfo>) -> Self {
        *self.canned_worktrees.lock().unwrap() = wts;
        self
    }
    pub fn with_sparse_patterns(self, patterns: Vec<String>) -> Self {
        *self.canned_sparse.lock().unwrap() = patterns;
        self
    }
    pub fn with_file_diff(self, diff: FileDiff) -> Self {
        *self.canned_file_diff.lock().unwrap() = diff;
        self
    }
    pub fn with_branches(self, branches: Vec<BranchInfo>) -> Self {
        *self.canned_branches.lock().unwrap() = branches;
        self
    }
    pub fn with_refs(self, refs: Vec<CommitRef>) -> Self {
        *self.canned_refs.lock().unwrap() = refs;
        self
    }
    pub fn with_ahead_behind(self, ab: AheadBehind) -> Self {
        *self.canned_ahead_behind.lock().unwrap() = Some(ab);
        self
    }
    pub fn with_sync_commits(self, sync: SyncCommits) -> Self {
        *self.canned_sync_commits.lock().unwrap() = sync;
        self
    }
    pub fn with_remotes(self, remotes: Vec<String>) -> Self {
        *self.canned_remotes.lock().unwrap() = remotes;
        self
    }
    /// 测试断言:已记录的远程管理写操作(如 ["add upstream", "remove origin"])。
    pub fn remote_ops(&self) -> Vec<String> {
        self.remote_ops.lock().unwrap().clone()
    }
    pub fn with_merge(self, outcome: MergeOutcome) -> Self {
        *self.canned_merge.lock().unwrap() = Some(outcome);
        self
    }
    /// 测试断言:已合并进当前分支的分支名列表。
    pub fn merge_ops(&self) -> Vec<String> {
        self.merge_ops.lock().unwrap().clone()
    }
    /// 测试断言:已记录的 init/clone 调用。
    pub fn onboard_ops(&self) -> Vec<String> {
        self.onboard_ops.lock().unwrap().clone()
    }
    pub fn with_reflog(self, entries: Vec<ReflogEntry>) -> Self {
        *self.canned_reflog.lock().unwrap() = entries;
        self
    }
    /// 预置某次「删分支影响预览」的返回。供二次确认 / 未合并安全网测试。
    pub fn with_branch_delete_impact(self, impact: BranchDeleteImpact) -> Self {
        *self.canned_branch_delete_impact.lock().unwrap() = impact;
        self
    }
    /// 链式预置 status 条目(可与 with_head / with_reflog 组合)。供「撤销前脏工作区护栏」测试:
    /// 非空 entries 即代表工作区有未提交改动,hard 还原应被拒绝。
    pub fn with_status_entries(self, entries: Vec<FileEntry>) -> Self {
        *self.canned_status.lock().unwrap() = entries;
        self
    }
    /// 预置可移动 HEAD 的初始 oid(之后 reset 会改它)。供 Undo/Redo 时间线测试。
    pub fn with_head(self, oid: &str) -> Self {
        *self.head_oid.lock().unwrap() = Some(oid.to_string());
        self
    }
    /// 断言用:当前(可能被 reset 移动过的)HEAD oid。
    pub fn head_oid(&self) -> Option<String> {
        self.head_oid.lock().unwrap().clone()
    }
    /// 断言用:记录被 checkout 的分支名(按调用顺序)。
    pub fn checked_out_branches(&self) -> Vec<String> {
        self.checked_out.lock().unwrap().clone()
    }
    pub fn created_branches(&self) -> Vec<String> {
        self.created.lock().unwrap().clone()
    }
    pub fn deleted_branches(&self) -> Vec<String> {
        self.deleted.lock().unwrap().clone()
    }
    pub fn with_fetch(self, outcome: FetchOutcome) -> Self {
        *self.canned_fetch.lock().unwrap() = Some(outcome);
        self
    }
    pub fn fetch_call_count(&self) -> u32 {
        *self.fetch_calls.lock().unwrap()
    }
    pub fn with_pull(self, outcome: PullOutcome) -> Self {
        *self.canned_pull.lock().unwrap() = Some(outcome);
        self
    }
    pub fn pull_call_count(&self) -> u32 {
        *self.pull_calls.lock().unwrap()
    }
    pub fn with_push(self, outcome: PushOutcome) -> Self {
        *self.canned_push.lock().unwrap() = Some(outcome);
        self
    }
    pub fn push_call_count(&self) -> u32 {
        *self.push_calls.lock().unwrap()
    }
    pub fn status_call_count(&self) -> u32 {
        *self.status_calls.lock().unwrap()
    }
    pub fn log_call_count(&self) -> u32 {
        *self.log_calls.lock().unwrap()
    }
    pub fn commit_file_diff_call_count(&self) -> u32 {
        *self.commit_file_diff_calls.lock().unwrap()
    }
    pub fn working_diff_call_count(&self) -> u32 {
        *self.working_diff_calls.lock().unwrap()
    }
    pub fn blame_call_count(&self) -> u32 {
        *self.blame_calls.lock().unwrap()
    }
    pub fn refs_call_count(&self) -> u32 {
        *self.refs_calls.lock().unwrap()
    }
    pub fn file_history_call_count(&self) -> u32 {
        *self.file_history_calls.lock().unwrap()
    }
    pub fn line_history_call_count(&self) -> u32 {
        *self.line_history_calls.lock().unwrap()
    }
    pub fn pickaxe_call_count(&self) -> u32 {
        *self.pickaxe_calls.lock().unwrap()
    }
    pub fn staged_hunks(&self) -> Vec<(String, usize)> {
        self.staged_hunks.lock().unwrap().clone()
    }
    pub fn unstaged_hunks(&self) -> Vec<(String, usize)> {
        self.unstaged_hunks.lock().unwrap().clone()
    }
    pub fn upstreams_set(&self) -> Vec<String> {
        self.upstreams_set.lock().unwrap().clone()
    }
    pub fn with_stashes(self, stashes: Vec<StashEntry>) -> Self {
        *self.canned_stashes.lock().unwrap() = stashes;
        self
    }
    pub fn stash_ops(&self) -> Vec<String> {
        self.stash_ops.lock().unwrap().clone()
    }
    pub fn with_repo_state(self, state: RepoState) -> Self {
        *self.canned_repo_state.lock().unwrap() = Some(state);
        self
    }
    pub fn conflict_ops(&self) -> Vec<String> {
        self.conflict_ops.lock().unwrap().clone()
    }
    pub fn tag_ops(&self) -> Vec<String> {
        self.tag_ops.lock().unwrap().clone()
    }
    pub fn rebase_ops(&self) -> Vec<String> {
        self.rebase_ops.lock().unwrap().clone()
    }
    pub fn with_conflict_sides(self, sides: ConflictSides) -> Self {
        *self.canned_conflict_sides.lock().unwrap() = Some(sides);
        self
    }
}

impl GitBackend for FakeBackend {
    fn open(&self, _path: &Path) -> Result<(), GitError> {
        Ok(())
    }

    fn init(&self, path: &Path) -> Result<(), GitError> {
        self.onboard_ops
            .lock()
            .unwrap()
            .push(format!("init {}", path.display()));
        Ok(())
    }

    fn clone_repo(&self, url: &str, dst: &Path) -> Result<(), GitError> {
        if url.trim().is_empty() {
            return Err(GitError::InvalidUrl);
        }
        self.onboard_ops
            .lock()
            .unwrap()
            .push(format!("clone {}→{}", url, dst.display()));
        Ok(())
    }

    fn head_commit(&self, _path: &Path) -> Result<Commit, GitError> {
        let id = self
            .head_oid
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| "0123456789abcdef0123456789abcdef01234567".into());
        Ok(Commit {
            short_id: id.chars().take(7).collect(),
            id,
            summary: "这是来自 FakeBackend 的假提交".into(),
            body: String::new(),
            author: Signature {
                name: "测试者".into(),
                email: "test@example.com".into(),
            },
            timestamp: 1_700_000_000,
            parents: vec![],
        })
    }

    fn status(&self, _path: &Path) -> Result<WorkingTreeStatus, GitError> {
        *self.status_calls.lock().unwrap() += 1;
        Ok(WorkingTreeStatus {
            entries: self.canned_status.lock().unwrap().clone(),
        })
    }

    fn stage(&self, _path: &Path, file: &Path) -> Result<(), GitError> {
        self.staged.lock().unwrap().push(file.to_path_buf());
        Ok(())
    }

    fn unstage(&self, _path: &Path, file: &Path) -> Result<(), GitError> {
        self.unstaged.lock().unwrap().push(file.to_path_buf());
        Ok(())
    }

    fn commit(&self, _path: &Path, message: &str) -> Result<String, GitError> {
        if let Some(error) = self.commit_error.lock().unwrap().take() {
            return Err(GitError::Backend(error));
        }
        self.commits.lock().unwrap().push(message.to_string());
        Ok("fake000000000000000000000000000000000000".to_string())
    }
    fn amend_commit(&self, _path: &Path, message: Option<&str>) -> Result<String, GitError> {
        self.commits
            .lock()
            .unwrap()
            .push(format!("amend:{}", message.unwrap_or("<keep>")));
        Ok("fakeamend00000000000000000000000000000000".to_string())
    }

    fn log(
        &self,
        _path: &Path,
        limit: usize,
        skip: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<Commit>, GitError> {
        if cancelled() {
            return Err(GitError::Cancelled);
        }
        *self.log_calls.lock().unwrap() += 1;
        // 尊重 skip/limit,使增量分页(commit_graph_page)在测试里可被真实驱动。
        Ok(self
            .canned_log
            .lock()
            .unwrap()
            .iter()
            .skip(skip)
            .take(limit)
            .cloned()
            .collect())
    }
    fn reflog(&self, _path: &Path, limit: usize) -> Result<Vec<ReflogEntry>, GitError> {
        Ok(self
            .canned_reflog
            .lock()
            .unwrap()
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }
    fn search_commits(
        &self,
        _path: &Path,
        query: &str,
        limit: usize,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<Commit>, GitError> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .canned_log
            .lock()
            .unwrap()
            .iter()
            .filter(|c| {
                c.id.starts_with(&q)
                    || c.summary.to_lowercase().contains(&q)
                    || c.body.to_lowercase().contains(&q)
                    || c.author.name.to_lowercase().contains(&q)
                    || c.author.email.to_lowercase().contains(&q)
            })
            .take(limit)
            .cloned()
            .collect())
    }
    fn file_history(
        &self,
        _path: &Path,
        _file: &str,
        limit: usize,
    ) -> Result<Vec<Commit>, GitError> {
        *self.file_history_calls.lock().unwrap() += 1;
        Ok(self
            .canned_log
            .lock()
            .unwrap()
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }
    fn line_history(
        &self,
        _path: &Path,
        _file: &str,
        _start: u32,
        _end: u32,
    ) -> Result<Vec<LineHistoryEntry>, GitError> {
        *self.line_history_calls.lock().unwrap() += 1;
        Ok(self.canned_line_history.lock().unwrap().clone())
    }
    fn pickaxe(
        &self,
        _path: &Path,
        _query: &str,
        _regex: bool,
        limit: usize,
    ) -> Result<Vec<Commit>, GitError> {
        *self.pickaxe_calls.lock().unwrap() += 1;
        Ok(self
            .canned_log
            .lock()
            .unwrap()
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }
    fn read_blob(&self, _path: &Path, _oid: &str) -> Result<Vec<u8>, GitError> {
        Ok(self.canned_blob.lock().unwrap().clone())
    }
    fn commit_files(&self, _path: &Path, _commit_id: &str) -> Result<Vec<FileChange>, GitError> {
        Ok(self.canned_commit_files.lock().unwrap().clone())
    }
    fn compare_files(
        &self,
        _path: &Path,
        _from: &str,
        _to: &str,
    ) -> Result<Vec<FileChange>, GitError> {
        Ok(self.canned_commit_files.lock().unwrap().clone())
    }
    fn compare_file_diff(
        &self,
        _path: &Path,
        _from: &str,
        _to: &str,
        _file: &str,
    ) -> Result<FileDiff, GitError> {
        Ok(self.canned_file_diff.lock().unwrap().clone())
    }
    fn current_branch(&self, _path: &Path) -> Result<Option<String>, GitError> {
        Ok(self.canned_branch.lock().unwrap().clone())
    }
    fn commit_file_diff(
        &self,
        _path: &Path,
        _commit_id: &str,
        _file: &str,
    ) -> Result<FileDiff, GitError> {
        *self.commit_file_diff_calls.lock().unwrap() += 1;
        Ok(self.canned_file_diff.lock().unwrap().clone())
    }
    fn working_diff(&self, _path: &Path, _file: &str, _staged: bool) -> Result<FileDiff, GitError> {
        *self.working_diff_calls.lock().unwrap() += 1;
        Ok(self.canned_file_diff.lock().unwrap().clone())
    }
    fn stage_hunk(&self, _path: &Path, file: &str, hunk_index: usize) -> Result<(), GitError> {
        self.staged_hunks
            .lock()
            .unwrap()
            .push((file.to_string(), hunk_index));
        Ok(())
    }
    fn unstage_hunk(&self, _path: &Path, file: &str, hunk_index: usize) -> Result<(), GitError> {
        self.unstaged_hunks
            .lock()
            .unwrap()
            .push((file.to_string(), hunk_index));
        Ok(())
    }
    fn stage_lines(
        &self,
        _path: &Path,
        file: &str,
        hunk_index: usize,
        lines: &[usize],
    ) -> Result<(), GitError> {
        self.staged_hunks
            .lock()
            .unwrap()
            .push((format!("{file}#{hunk_index}:{lines:?}"), lines.len()));
        Ok(())
    }
    fn branches(&self, _path: &Path) -> Result<Vec<BranchInfo>, GitError> {
        Ok(self.canned_branches.lock().unwrap().clone())
    }
    fn refs(&self, _path: &Path) -> Result<Vec<CommitRef>, GitError> {
        *self.refs_calls.lock().unwrap() += 1;
        Ok(self.canned_refs.lock().unwrap().clone())
    }
    fn repo_state(&self, _path: &Path) -> Result<RepoState, GitError> {
        Ok(self
            .canned_repo_state
            .lock()
            .unwrap()
            .unwrap_or(RepoState::Clean))
    }
    fn blame(
        &self,
        _path: &Path,
        _file: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<BlameLine>, GitError> {
        if cancelled() {
            return Err(GitError::Cancelled);
        }
        *self.blame_calls.lock().unwrap() += 1;
        Ok(Vec::new())
    }
    fn commit_signature(&self, _path: &Path, _commit_id: &str) -> Result<SignatureInfo, GitError> {
        Ok(self.canned_signature.lock().unwrap().clone())
    }
    fn list_submodules(&self, _path: &Path) -> Result<Vec<SubmoduleInfo>, GitError> {
        Ok(self.canned_submodules.lock().unwrap().clone())
    }
    fn update_submodule(&self, _path: &Path, path: &str) -> Result<(), GitError> {
        self.submodule_ops.lock().unwrap().push(path.to_string());
        Ok(())
    }
    fn list_worktrees(&self, _path: &Path) -> Result<Vec<WorktreeInfo>, GitError> {
        Ok(self.canned_worktrees.lock().unwrap().clone())
    }
    fn sparse_checkout_patterns(&self, _path: &Path) -> Result<Vec<String>, GitError> {
        Ok(self.canned_sparse.lock().unwrap().clone())
    }
    fn conflict_sides(&self, _path: &Path, _file: &str) -> Result<ConflictSides, GitError> {
        Ok(self
            .canned_conflict_sides
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(ConflictSides {
                base: None,
                ours: None,
                theirs: None,
            }))
    }
    fn resolve_ours(&self, _path: &Path, file: &str) -> Result<(), GitError> {
        self.conflict_ops
            .lock()
            .unwrap()
            .push(format!("ours:{file}"));
        Ok(())
    }
    fn resolve_theirs(&self, _path: &Path, file: &str) -> Result<(), GitError> {
        self.conflict_ops
            .lock()
            .unwrap()
            .push(format!("theirs:{file}"));
        Ok(())
    }
    fn continue_op(&self, _path: &Path) -> Result<(), GitError> {
        self.conflict_ops.lock().unwrap().push("continue".into());
        Ok(())
    }
    fn abort_op(&self, _path: &Path) -> Result<(), GitError> {
        self.conflict_ops.lock().unwrap().push("abort".into());
        Ok(())
    }
    fn revert(&self, _path: &Path, commit_id: &str) -> Result<(), GitError> {
        self.conflict_ops
            .lock()
            .unwrap()
            .push(format!("revert:{commit_id}"));
        Ok(())
    }
    fn cherry_pick(&self, _path: &Path, commit_id: &str) -> Result<(), GitError> {
        self.conflict_ops
            .lock()
            .unwrap()
            .push(format!("cherry-pick:{commit_id}"));
        Ok(())
    }
    fn create_tag(
        &self,
        _path: &Path,
        name: &str,
        commit_id: &str,
        message: Option<&str>,
    ) -> Result<(), GitError> {
        let m = message.unwrap_or("");
        self.tag_ops
            .lock()
            .unwrap()
            .push(format!("create:{name}@{commit_id}:{m}"));
        Ok(())
    }
    fn delete_tag(&self, _path: &Path, name: &str) -> Result<(), GitError> {
        self.tag_ops.lock().unwrap().push(format!("delete:{name}"));
        Ok(())
    }
    fn reset(&self, _path: &Path, commit_id: &str, mode: ResetMode) -> Result<(), GitError> {
        let m = match mode {
            ResetMode::Soft => "soft",
            ResetMode::Mixed => "mixed",
            ResetMode::Hard => "hard",
        };
        self.tag_ops
            .lock()
            .unwrap()
            .push(format!("reset:{m}:{commit_id}"));
        // 模拟 reset 移动 HEAD 到目标(供 Undo/Redo 时间线测试)。
        *self.head_oid.lock().unwrap() = Some(commit_id.to_string());
        Ok(())
    }
    fn interactive_rebase(
        &self,
        _path: &Path,
        base: Option<&str>,
        steps: &[RebaseStep],
    ) -> Result<(), GitError> {
        let mut ops = self.rebase_ops.lock().unwrap();
        ops.push(format!("base:{}", base.unwrap_or("--root")));
        for s in steps {
            let a = match &s.action {
                RebaseAction::Pick => "pick".to_string(),
                RebaseAction::Reword(m) => format!("reword:{m}"),
                RebaseAction::Squash(m) => format!("squash:{m}"),
                RebaseAction::Fixup => "fixup".to_string(),
                RebaseAction::Drop => "drop".to_string(),
            };
            ops.push(format!("{}:{a}", s.sha));
        }
        Ok(())
    }
    fn ahead_behind(&self, _path: &Path) -> Result<Option<AheadBehind>, GitError> {
        Ok(*self.canned_ahead_behind.lock().unwrap())
    }
    fn remotes(&self, _path: &Path) -> Result<Vec<String>, GitError> {
        Ok(self.canned_remotes.lock().unwrap().clone())
    }
    fn remote_list(&self, _path: &Path) -> Result<Vec<RemoteInfo>, GitError> {
        Ok(self
            .canned_remotes
            .lock()
            .unwrap()
            .iter()
            .map(|name| RemoteInfo {
                name: name.clone(),
                url: format!("https://example.com/{name}.git"),
            })
            .collect())
    }
    fn add_remote(&self, _path: &Path, name: &str, url: &str) -> Result<(), GitError> {
        let name = name.trim();
        if name.is_empty() || url.trim().is_empty() {
            return Err(GitError::InvalidRemoteName);
        }
        if self
            .canned_remotes
            .lock()
            .unwrap()
            .iter()
            .any(|r| r == name)
        {
            return Err(GitError::RemoteAlreadyExists(name.to_string()));
        }
        self.canned_remotes.lock().unwrap().push(name.to_string());
        self.remote_ops.lock().unwrap().push(format!("add {name}"));
        Ok(())
    }
    fn remove_remote(&self, _path: &Path, name: &str) -> Result<(), GitError> {
        let mut remotes = self.canned_remotes.lock().unwrap();
        if !remotes.iter().any(|r| r == name) {
            return Err(GitError::RemoteNotFound(name.to_string()));
        }
        remotes.retain(|r| r != name);
        drop(remotes);
        self.remote_ops
            .lock()
            .unwrap()
            .push(format!("remove {name}"));
        Ok(())
    }
    fn rename_remote(&self, _path: &Path, old: &str, new: &str) -> Result<(), GitError> {
        let new = new.trim();
        if new.is_empty() {
            return Err(GitError::InvalidRemoteName);
        }
        let mut remotes = self.canned_remotes.lock().unwrap();
        if !remotes.iter().any(|r| r == old) {
            return Err(GitError::RemoteNotFound(old.to_string()));
        }
        if old != new && remotes.iter().any(|r| r == new) {
            return Err(GitError::RemoteAlreadyExists(new.to_string()));
        }
        for r in remotes.iter_mut() {
            if r == old {
                *r = new.to_string();
            }
        }
        drop(remotes);
        self.remote_ops
            .lock()
            .unwrap()
            .push(format!("rename {old} {new}"));
        Ok(())
    }
    fn sync_commits(&self, _path: &Path) -> Result<SyncCommits, GitError> {
        Ok(self.canned_sync_commits.lock().unwrap().clone())
    }
    fn set_upstream(&self, _path: &Path, upstream: &str) -> Result<(), GitError> {
        self.upstreams_set
            .lock()
            .unwrap()
            .push(upstream.to_string());
        Ok(())
    }
    fn stash_list(&self, _path: &Path) -> Result<Vec<StashEntry>, GitError> {
        Ok(self.canned_stashes.lock().unwrap().clone())
    }
    fn stash_save(&self, _path: &Path, message: Option<&str>) -> Result<(), GitError> {
        self.stash_ops
            .lock()
            .unwrap()
            .push(format!("save:{}", message.unwrap_or("")));
        Ok(())
    }
    fn stash_apply(&self, _path: &Path, index: usize) -> Result<(), GitError> {
        self.stash_ops
            .lock()
            .unwrap()
            .push(format!("apply:{index}"));
        Ok(())
    }
    fn stash_pop(&self, _path: &Path, index: usize) -> Result<(), GitError> {
        self.stash_ops.lock().unwrap().push(format!("pop:{index}"));
        Ok(())
    }
    fn stash_drop(&self, _path: &Path, index: usize) -> Result<(), GitError> {
        self.stash_ops.lock().unwrap().push(format!("drop:{index}"));
        Ok(())
    }
    fn checkout_branch(&self, _path: &Path, name: &str) -> Result<(), GitError> {
        self.checked_out.lock().unwrap().push(name.to_string());
        Ok(())
    }
    fn create_branch(&self, _path: &Path, name: &str) -> Result<(), GitError> {
        self.created.lock().unwrap().push(name.to_string());
        Ok(())
    }
    fn delete_branch(&self, _path: &Path, name: &str) -> Result<(), GitError> {
        self.deleted.lock().unwrap().push(name.to_string());
        Ok(())
    }
    fn merge_branch(&self, _path: &Path, name: &str) -> Result<MergeOutcome, GitError> {
        self.merge_ops.lock().unwrap().push(name.to_string());
        Ok(self
            .canned_merge
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(MergeOutcome {
                summary: "Already up to date.".into(),
                fast_forward: false,
            }))
    }
    fn branch_delete_impact(
        &self,
        _path: &Path,
        _name: &str,
    ) -> Result<BranchDeleteImpact, GitError> {
        Ok(self.canned_branch_delete_impact.lock().unwrap().clone())
    }
    fn fetch(&self, _path: &Path, remote: Option<&str>) -> Result<FetchOutcome, GitError> {
        *self.fetch_calls.lock().unwrap() += 1;
        Ok(self
            .canned_fetch
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(FetchOutcome {
                remote: remote.unwrap_or("").to_string(),
                summary: "已是最新".to_string(),
            }))
    }
    fn pull(
        &self,
        _path: &Path,
        _remote: Option<&str>,
        _rebase: bool,
    ) -> Result<PullOutcome, GitError> {
        *self.pull_calls.lock().unwrap() += 1;
        Ok(self
            .canned_pull
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(PullOutcome {
                summary: "已是最新".to_string(),
            }))
    }
    fn push(&self, _path: &Path, _remote: Option<&str>) -> Result<PushOutcome, GitError> {
        *self.push_calls.lock().unwrap() += 1;
        Ok(self
            .canned_push
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(PushOutcome {
                summary: "Everything up-to-date".to_string(),
                set_upstream: false,
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::model::{FileState, Signature};

    #[test]
    fn records_stage_and_commit() {
        let fb = FakeBackend::default();
        fb.stage(Path::new("/r"), Path::new("a.txt")).unwrap();
        assert_eq!(fb.staged_files(), vec![std::path::PathBuf::from("a.txt")]);

        let sha = fb.commit(Path::new("/r"), "msg").unwrap();
        assert!(!sha.is_empty());
        assert_eq!(fb.commit_messages(), vec!["msg".to_string()]);
    }

    #[test]
    fn fake_returns_canned_log_and_files() {
        let commit = Commit {
            id: "x".into(),
            short_id: "x".into(),
            summary: "s".into(),
            body: "".into(),
            author: Signature {
                name: "n".into(),
                email: "e".into(),
            },
            timestamp: 1,
            parents: vec![],
        };
        let fb = FakeBackend::default()
            .with_log(vec![commit])
            .with_commit_files(vec![FileChange {
                path: "a".into(),
                status: FileState::Added,
                additions: 1,
                deletions: 0,
            }])
            .with_branch(Some("main".into()));
        assert_eq!(fb.log(Path::new("/r"), 10, 0, &|| false).unwrap().len(), 1);
        assert_eq!(fb.commit_files(Path::new("/r"), "x").unwrap()[0].path, "a");
        assert_eq!(
            fb.current_branch(Path::new("/r")).unwrap(),
            Some("main".into())
        );
    }
}
