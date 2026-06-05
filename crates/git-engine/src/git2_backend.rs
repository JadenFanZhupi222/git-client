use std::path::Path;
use git_core::{GitBackend, GitError};
use git_core::model::{Commit, Signature, WorkingTreeStatus, FileEntry, FileState};

/// git2 的 add_path/remove_path 要求"仓库根相对路径"。
/// 若传入绝对路径,用 workdir 前缀剥成相对路径;否则原样返回。
/// 这个泄漏细节锁在适配器层,不污染上层。
fn to_repo_relative(repo: &git2::Repository, file: &Path) -> std::path::PathBuf {
    if file.is_absolute() {
        if let Some(wd) = repo.workdir() {
            if let Ok(stripped) = file.strip_prefix(wd) {
                return stripped.to_path_buf();
            }
        }
    }
    file.to_path_buf()
}

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

    fn stage(&self, path: &Path, file: &Path) -> Result<(), GitError> {
        let repo = git2::Repository::open(path)
            .map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let rel = to_repo_relative(&repo, file);
        let mut index = repo.index().map_err(|e| GitError::Backend(e.to_string()))?;
        // 已知限制(阶段 1):add_path 只覆盖修改/新增;暂存"已删除文件"需 remove_path,留待后续。
        index.add_path(&rel).map_err(|e| GitError::Backend(e.to_string()))?;
        index.write().map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(())
    }

    fn unstage(&self, path: &Path, file: &Path) -> Result<(), GitError> {
        let repo = git2::Repository::open(path)
            .map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let rel = to_repo_relative(&repo, file);

        // ⚠️ 精确判断 unborn:只认 UnbornBranch 这个错误码,别拿 head() 任何报错当无 HEAD。
        match repo.head() {
            Ok(head_ref) => {
                // 有 HEAD:把该文件的 index 条目重置回 HEAD 版本。
                let head_commit = head_ref
                    .peel_to_commit()
                    .map_err(|e| GitError::Backend(e.to_string()))?;
                let obj = head_commit.into_object();
                let spec = rel.to_string_lossy().into_owned();
                repo.reset_default(Some(&obj), [spec.as_str()])
                    .map_err(|e| GitError::Backend(e.to_string()))?;
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
                // 无 HEAD(首次提交前):没有可重置的目标,直接从 index 删除条目。
                let mut index = repo.index().map_err(|e| GitError::Backend(e.to_string()))?;
                index.remove_path(&rel).map_err(|e| GitError::Backend(e.to_string()))?;
                index.write().map_err(|e| GitError::Backend(e.to_string()))?;
            }
            Err(e) => return Err(GitError::Backend(e.to_string())),
        }
        Ok(())
    }

    fn commit(&self, _path: &Path, _message: &str) -> Result<String, GitError> {
        todo!("Task 6")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::model::FileState;

    /// 建一个临时真仓库,并配好提交身份。
    fn init_repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test").unwrap();
        cfg.set_str("user.email", "test@example.com").unwrap();
        let path = dir.path().to_path_buf();
        (dir, path)
    }

    fn write(dir: &Path, name: &str, contents: &str) {
        std::fs::write(dir.join(name), contents).unwrap();
    }

    #[test]
    fn stage_marks_file_staged() {
        let (_tmp, repo) = init_repo();
        write(&repo, "a.txt", "hello");
        let backend = Git2Backend::default();

        backend.stage(&repo, Path::new("a.txt")).unwrap();

        let status = backend.status(&repo).unwrap();
        let entry = status.entries.iter().find(|e| e.path == "a.txt").unwrap();
        assert!(entry.staged, "stage 后应标记 staged");
        assert_eq!(entry.state, FileState::Added);
    }

    #[test]
    fn unstage_with_head_reverts_to_committed() {
        let (_tmp, repo) = init_repo();
        let backend = Git2Backend::default();
        // 先建一个初始提交(让仓库有 HEAD)
        write(&repo, "a.txt", "v1");
        backend.stage(&repo, Path::new("a.txt")).unwrap();
        backend.commit(&repo, "init").unwrap();
        // 改动并暂存,再取消暂存
        write(&repo, "a.txt", "v2");
        backend.stage(&repo, Path::new("a.txt")).unwrap();
        backend.unstage(&repo, Path::new("a.txt")).unwrap();

        let status = backend.status(&repo).unwrap();
        let entry = status.entries.iter().find(|e| e.path == "a.txt").unwrap();
        assert!(!entry.staged, "取消暂存后应回到未暂存");
    }

    #[test]
    fn unstage_without_head_removes_from_index() {
        let (_tmp, repo) = init_repo();
        let backend = Git2Backend::default();
        // 全新仓库,没有任何提交(unborn HEAD)
        write(&repo, "a.txt", "hello");
        backend.stage(&repo, Path::new("a.txt")).unwrap();
        backend.unstage(&repo, Path::new("a.txt")).unwrap();

        let status = backend.status(&repo).unwrap();
        let entry = status.entries.iter().find(|e| e.path == "a.txt").unwrap();
        assert!(!entry.staged, "无 HEAD 时取消暂存应把条目从 index 移除");
        assert_eq!(entry.state, FileState::Untracked);
    }
}
