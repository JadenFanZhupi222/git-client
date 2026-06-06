use git_core::model::{
    AheadBehind, BlameLine, BranchInfo, Commit, CommitRef, FetchOutcome, FileChange, FileDiff,
    FileEntry, PullOutcome, PushOutcome, RepoState, StashEntry, Signature, WorkingTreeStatus,
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
    canned_log: Mutex<Vec<Commit>>,
    canned_commit_files: Mutex<Vec<FileChange>>,
    canned_branch: Mutex<Option<String>>,
    canned_file_diff: Mutex<FileDiff>,
    canned_branches: Mutex<Vec<BranchInfo>>,
    canned_refs: Mutex<Vec<CommitRef>>,
    canned_ahead_behind: Mutex<Option<AheadBehind>>,
    canned_remotes: Mutex<Vec<String>>,
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
    pub fn with_log(self, commits: Vec<Commit>) -> Self {
        *self.canned_log.lock().unwrap() = commits;
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
    pub fn with_remotes(self, remotes: Vec<String>) -> Self {
        *self.canned_remotes.lock().unwrap() = remotes;
        self
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
}

impl GitBackend for FakeBackend {
    fn open(&self, _path: &Path) -> Result<(), GitError> {
        Ok(())
    }

    fn head_commit(&self, _path: &Path) -> Result<Commit, GitError> {
        Ok(Commit {
            id: "0123456789abcdef0123456789abcdef01234567".into(),
            short_id: "0123456".into(),
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
        self.commits.lock().unwrap().push(message.to_string());
        Ok("fake000000000000000000000000000000000000".to_string())
    }

    fn log(&self, _path: &Path, _limit: usize, _skip: usize) -> Result<Vec<Commit>, GitError> {
        Ok(self.canned_log.lock().unwrap().clone())
    }
    fn commit_files(&self, _path: &Path, _commit_id: &str) -> Result<Vec<FileChange>, GitError> {
        Ok(self.canned_commit_files.lock().unwrap().clone())
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
        Ok(self.canned_file_diff.lock().unwrap().clone())
    }
    fn working_diff(&self, _path: &Path, _file: &str, _staged: bool) -> Result<FileDiff, GitError> {
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
    fn stage_lines(&self, _path: &Path, file: &str, hunk_index: usize, lines: &[usize]) -> Result<(), GitError> {
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
        Ok(self.canned_refs.lock().unwrap().clone())
    }
    fn repo_state(&self, _path: &Path) -> Result<RepoState, GitError> {
        Ok(self.canned_repo_state.lock().unwrap().unwrap_or(RepoState::Clean))
    }
    fn blame(&self, _path: &Path, _file: &str) -> Result<Vec<BlameLine>, GitError> {
        Ok(Vec::new())
    }
    fn resolve_ours(&self, _path: &Path, file: &str) -> Result<(), GitError> {
        self.conflict_ops.lock().unwrap().push(format!("ours:{file}"));
        Ok(())
    }
    fn resolve_theirs(&self, _path: &Path, file: &str) -> Result<(), GitError> {
        self.conflict_ops.lock().unwrap().push(format!("theirs:{file}"));
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
    fn cherry_pick(&self, _path: &Path, commit_id: &str) -> Result<(), GitError> {
        self.conflict_ops.lock().unwrap().push(format!("cherry-pick:{commit_id}"));
        Ok(())
    }
    fn ahead_behind(&self, _path: &Path) -> Result<Option<AheadBehind>, GitError> {
        Ok(*self.canned_ahead_behind.lock().unwrap())
    }
    fn remotes(&self, _path: &Path) -> Result<Vec<String>, GitError> {
        Ok(self.canned_remotes.lock().unwrap().clone())
    }
    fn set_upstream(&self, _path: &Path, upstream: &str) -> Result<(), GitError> {
        self.upstreams_set.lock().unwrap().push(upstream.to_string());
        Ok(())
    }
    fn stash_list(&self, _path: &Path) -> Result<Vec<StashEntry>, GitError> {
        Ok(self.canned_stashes.lock().unwrap().clone())
    }
    fn stash_save(&self, _path: &Path, message: Option<&str>) -> Result<(), GitError> {
        self.stash_ops.lock().unwrap().push(format!("save:{}", message.unwrap_or("")));
        Ok(())
    }
    fn stash_apply(&self, _path: &Path, index: usize) -> Result<(), GitError> {
        self.stash_ops.lock().unwrap().push(format!("apply:{index}"));
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
        assert_eq!(fb.log(Path::new("/r"), 10, 0).unwrap().len(), 1);
        assert_eq!(fb.commit_files(Path::new("/r"), "x").unwrap()[0].path, "a");
        assert_eq!(
            fb.current_branch(Path::new("/r")).unwrap(),
            Some("main".into())
        );
    }
}
