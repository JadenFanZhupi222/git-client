use crate::cli_backend::CliBackend;
use crate::git2_backend::Git2Backend;
use git_core::model::{
    AheadBehind, BlameLine, BranchInfo, Commit, CommitRef, ConflictSides, FetchOutcome, FileChange,
    FileDiff, PullOutcome, PushOutcome, RepoState, StashEntry, SyncCommits, WorkingTreeStatus,
};
use git_core::{GitBackend, GitError};
use std::path::Path;

/// 组合后端:对外是一个 GitBackend,内部按操作路由。
/// 既有(读 + 本地写)方法走 git2;网络方法(fetch)走 CLI。
#[derive(Default)]
pub struct CompositeBackend {
    git2: Git2Backend,
    cli: CliBackend,
}

impl GitBackend for CompositeBackend {
    fn open(&self, repo: &Path) -> Result<(), GitError> {
        self.git2.open(repo)
    }
    fn head_commit(&self, repo: &Path) -> Result<Commit, GitError> {
        self.git2.head_commit(repo)
    }
    fn status(&self, repo: &Path) -> Result<WorkingTreeStatus, GitError> {
        self.git2.status(repo)
    }
    fn stage(&self, repo: &Path, file: &Path) -> Result<(), GitError> {
        self.git2.stage(repo, file)
    }
    fn unstage(&self, repo: &Path, file: &Path) -> Result<(), GitError> {
        self.git2.unstage(repo, file)
    }
    fn commit(&self, repo: &Path, message: &str) -> Result<String, GitError> {
        self.git2.commit(repo, message)
    }
    fn log(&self, repo: &Path, limit: usize, skip: usize) -> Result<Vec<Commit>, GitError> {
        self.git2.log(repo, limit, skip)
    }
    fn search_commits(
        &self,
        repo: &Path,
        query: &str,
        limit: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<Commit>, GitError> {
        self.git2.search_commits(repo, query, limit, cancelled)
    }
    fn commit_files(&self, repo: &Path, commit_id: &str) -> Result<Vec<FileChange>, GitError> {
        self.git2.commit_files(repo, commit_id)
    }
    fn commit_file_diff(
        &self,
        repo: &Path,
        commit_id: &str,
        file: &str,
    ) -> Result<FileDiff, GitError> {
        self.git2.commit_file_diff(repo, commit_id, file)
    }
    fn working_diff(&self, repo: &Path, file: &str, staged: bool) -> Result<FileDiff, GitError> {
        self.git2.working_diff(repo, file, staged)
    }
    fn current_branch(&self, repo: &Path) -> Result<Option<String>, GitError> {
        self.git2.current_branch(repo)
    }
    fn branches(&self, repo: &Path) -> Result<Vec<BranchInfo>, GitError> {
        self.git2.branches(repo)
    }
    fn refs(&self, repo: &Path) -> Result<Vec<CommitRef>, GitError> {
        self.git2.refs(repo)
    }
    fn repo_state(&self, repo: &Path) -> Result<RepoState, GitError> {
        self.git2.repo_state(repo)
    }
    fn blame(&self, repo: &Path, file: &str) -> Result<Vec<BlameLine>, GitError> {
        self.git2.blame(repo, file)
    }
    fn conflict_sides(&self, repo: &Path, file: &str) -> Result<ConflictSides, GitError> {
        self.git2.conflict_sides(repo, file)
    }
    fn resolve_ours(&self, repo: &Path, file: &str) -> Result<(), GitError> {
        self.cli.resolve_ours(repo, file)
    }
    fn resolve_theirs(&self, repo: &Path, file: &str) -> Result<(), GitError> {
        self.cli.resolve_theirs(repo, file)
    }
    fn continue_op(&self, repo: &Path) -> Result<(), GitError> {
        // 读状态(git2)决定走哪条 continue,再交给 CLI 执行。
        let state = self.git2.repo_state(repo)?;
        self.cli.continue_op(repo, state)
    }
    fn abort_op(&self, repo: &Path) -> Result<(), GitError> {
        let state = self.git2.repo_state(repo)?;
        self.cli.abort_op(repo, state)
    }
    fn cherry_pick(&self, repo: &Path, commit_id: &str) -> Result<(), GitError> {
        self.cli.cherry_pick(repo, commit_id)
    }
    fn revert(&self, repo: &Path, commit_id: &str) -> Result<(), GitError> {
        self.cli.revert(repo, commit_id)
    }
    fn ahead_behind(&self, repo: &Path) -> Result<Option<AheadBehind>, GitError> {
        self.git2.ahead_behind(repo)
    }
    fn remotes(&self, repo: &Path) -> Result<Vec<String>, GitError> {
        self.git2.remotes(repo)
    }
    fn sync_commits(&self, repo: &Path) -> Result<SyncCommits, GitError> {
        self.git2.sync_commits(repo)
    }
    fn set_upstream(&self, repo: &Path, upstream: &str) -> Result<(), GitError> {
        self.git2.set_upstream(repo, upstream)
    }
    fn checkout_branch(&self, repo: &Path, name: &str) -> Result<(), GitError> {
        self.git2.checkout_branch(repo, name)
    }
    fn create_branch(&self, repo: &Path, name: &str) -> Result<(), GitError> {
        self.git2.create_branch(repo, name)
    }
    fn delete_branch(&self, repo: &Path, name: &str) -> Result<(), GitError> {
        self.git2.delete_branch(repo, name)
    }

    // 网络操作走 CLI 后端。
    fn fetch(&self, repo: &Path, remote: Option<&str>) -> Result<FetchOutcome, GitError> {
        self.cli.fetch(repo, remote)
    }
    fn pull(
        &self,
        repo: &Path,
        remote: Option<&str>,
        rebase: bool,
    ) -> Result<PullOutcome, GitError> {
        self.cli.pull(repo, remote, rebase)
    }
    fn push(&self, repo: &Path, remote: Option<&str>) -> Result<PushOutcome, GitError> {
        self.cli.push(repo, remote)
    }
    fn stage_hunk(&self, repo: &Path, file: &str, hunk_index: usize) -> Result<(), GitError> {
        self.cli.stage_hunk(repo, file, hunk_index)
    }
    fn unstage_hunk(&self, repo: &Path, file: &str, hunk_index: usize) -> Result<(), GitError> {
        self.cli.unstage_hunk(repo, file, hunk_index)
    }
    fn stage_lines(&self, repo: &Path, file: &str, hunk_index: usize, lines: &[usize]) -> Result<(), GitError> {
        // git2 按选定行重建 patch(行序与前端一致),CLI 应用到 index。
        let patch = self.git2.build_line_stage_patch(repo, file, hunk_index, lines)?;
        self.cli.apply_cached_patch(repo, &patch)
    }

    // 贮藏走 CLI 后端。
    fn stash_list(&self, repo: &Path) -> Result<Vec<StashEntry>, GitError> {
        self.cli.stash_list(repo)
    }
    fn stash_save(&self, repo: &Path, message: Option<&str>) -> Result<(), GitError> {
        self.cli.stash_save(repo, message)
    }
    fn stash_apply(&self, repo: &Path, index: usize) -> Result<(), GitError> {
        self.cli.stash_apply(repo, index)
    }
    fn stash_pop(&self, repo: &Path, index: usize) -> Result<(), GitError> {
        self.cli.stash_pop(repo, index)
    }
    fn stash_drop(&self, repo: &Path, index: usize) -> Result<(), GitError> {
        self.cli.stash_drop(repo, index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
    }

    #[test]
    fn delegates_branches_to_git2() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "main", "."]);
        git(dir.path(), &["config", "user.email", "t@e"]);
        git(dir.path(), &["config", "user.name", "t"]);
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-m", "c1"]);

        let composite = CompositeBackend::default();
        let git2 = Git2Backend;
        let via_composite: Vec<String> = composite
            .branches(dir.path())
            .unwrap()
            .into_iter()
            .map(|b| b.name)
            .collect();
        let via_git2: Vec<String> = git2
            .branches(dir.path())
            .unwrap()
            .into_iter()
            .map(|b| b.name)
            .collect();
        assert_eq!(via_composite, via_git2, "composite 应把 branches 透传给 git2");
        assert!(via_composite.contains(&"main".to_string()));
    }

    #[test]
    fn stage_lines_stages_only_selected_line() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "main", "."]);
        git(dir.path(), &["config", "user.email", "t@e"]);
        git(dir.path(), &["config", "user.name", "t"]);
        std::fs::write(dir.path().join("f.txt"), "a\nb\nc\n").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-m", "c1"]);
        // 改首行和末行(同一 hunk):a→A,c→C。hunk 行序:0:-a 1:+A 2: b 3:-c 4:+C
        std::fs::write(dir.path().join("f.txt"), "A\nb\nC\n").unwrap();

        let composite = CompositeBackend::default();
        // 只暂存第一处改动(-a / +A)
        composite.stage_lines(dir.path(), "f.txt", 0, &[0, 1]).unwrap();

        let cached = String::from_utf8(
            std::process::Command::new("git")
                .current_dir(dir.path())
                .args(["diff", "--cached"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let unstaged = String::from_utf8(
            std::process::Command::new("git")
                .current_dir(dir.path())
                .args(["diff"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        assert!(cached.contains("+A") && !cached.contains("+C"), "已暂存应只含 A:\n{cached}");
        assert!(unstaged.contains("+C") && !unstaged.contains("+A"), "未暂存应只剩 C:\n{unstaged}");
    }
}
