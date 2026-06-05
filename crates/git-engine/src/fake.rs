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
        Ok(Vec::new())
    }
    fn commit_files(&self, _path: &Path, _commit_id: &str) -> Result<Vec<FileChange>, GitError> {
        Ok(Vec::new())
    }
    fn current_branch(&self, _path: &Path) -> Result<Option<String>, GitError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_stage_and_commit() {
        let fb = FakeBackend::default();
        fb.stage(Path::new("/r"), Path::new("a.txt")).unwrap();
        assert_eq!(fb.staged_files(), vec![std::path::PathBuf::from("a.txt")]);

        let sha = fb.commit(Path::new("/r"), "msg").unwrap();
        assert!(!sha.is_empty());
        assert_eq!(fb.commit_messages(), vec!["msg".to_string()]);
    }
}
