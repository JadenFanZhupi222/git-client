use git_core::model::{Commit, FileChange, FileEntry, Signature, WorkingTreeStatus};
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
