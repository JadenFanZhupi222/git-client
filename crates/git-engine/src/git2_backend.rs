use std::path::Path;
use git_core::{GitBackend, GitError};
use git_core::model::{Commit, Signature, WorkingTreeStatus, FileEntry, FileState};

/// 基于 git2(libgit2 绑定)的后端。阶段 0/1 用它实现读写。
/// 注意:git2 是同步阻塞的 —— 调用方必须在 spawn_blocking 里使用它!
#[derive(Default)]
pub struct Git2Backend;

impl GitBackend for Git2Backend {
    fn open(&self, path: &Path) -> Result<(), GitError> {
        git2::Repository::open(path)
            .map(|_| ())
            .map_err(|e| GitError::RepoNotFound(e.to_string()))
    }

    fn head_commit(&self, path: &Path) -> Result<Commit, GitError> {
        let repo = git2::Repository::open(path)
            .map_err(|e| GitError::RepoNotFound(e.to_string()))?;

        let head = repo.head().map_err(|_| GitError::NoHead)?;
        let commit = head.peel_to_commit()
            .map_err(|e| GitError::Backend(e.to_string()))?;

        let id = commit.id().to_string();
        let author = commit.author();

        Ok(Commit {
            short_id: id.chars().take(7).collect(),
            id,
            summary: commit.summary().unwrap_or("").to_string(),
            body: commit.body().unwrap_or("").to_string(),
            author: Signature {
                name: author.name().unwrap_or("").to_string(),
                email: author.email().unwrap_or("").to_string(),
            },
            timestamp: commit.time().seconds(),
            parents: commit.parent_ids().map(|oid| oid.to_string()).collect(),
        })
    }

    fn status(&self, path: &Path) -> Result<WorkingTreeStatus, GitError> {
        let repo = git2::Repository::open(path)
            .map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let statuses = repo.statuses(None)
            .map_err(|e| GitError::Backend(e.to_string()))?;

        let mut entries = Vec::new();
        for entry in statuses.iter() {
            let s = entry.status();
            let (state, staged) = if s.is_index_new() {
                (FileState::Added, true)
            } else if s.is_index_modified() {
                (FileState::Modified, true)
            } else if s.is_wt_new() {
                (FileState::Untracked, false)
            } else if s.is_wt_modified() {
                (FileState::Modified, false)
            } else if s.is_wt_deleted() || s.is_index_deleted() {
                (FileState::Deleted, s.is_index_deleted())
            } else if s.is_conflicted() {
                (FileState::Conflicted, false)
            } else {
                continue;
            };
            entries.push(FileEntry {
                path: entry.path().unwrap_or("").to_string(),
                state,
                staged,
            });
        }
        Ok(WorkingTreeStatus { entries })
    }
}
