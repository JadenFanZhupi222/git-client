//! app-service:应用层。把领域能力组织成产品用例。
//! 关键点:它依赖 `dyn GitBackend`(trait 对象),而不是任何具体后端 ——
//! 后端通过构造函数注入(依赖注入),所以测试时能塞 FakeBackend。

use git_core::{GitBackend, GitError};
use ipc_types::CommitDto;
use ipc_types::StatusDto;
use std::path::Path;
use std::sync::Arc;

/// 仓库服务。生产版本里它会演化成第 4 部分讲的 RepoActor(独占状态 + 消息驱动)。
/// 阶段 0 先用最简单的形式跑通分层。
pub struct RepoService {
    backend: Arc<dyn GitBackend>,
}

impl RepoService {
    /// 依赖注入:谁创建 service,谁决定用哪个后端。
    pub fn new(backend: Arc<dyn GitBackend>) -> Self {
        Self { backend }
    }

    /// 用例:读取 HEAD 提交并转成给前端的 DTO。
    pub fn head_commit(&self, repo_path: &Path) -> Result<CommitDto, GitError> {
        tracing::info!(path = %repo_path.display(), "读取 HEAD");
        let commit = self.backend.head_commit(repo_path)?;
        Ok(CommitDto::from(commit))
    }

    /// 用例:读工作区状态并转 DTO。
    pub fn status(&self, repo_path: &Path) -> Result<StatusDto, GitError> {
        tracing::info!(path = %repo_path.display(), "读取 status");
        let st = self.backend.status(repo_path)?;
        Ok(StatusDto::from(st))
    }

    /// 用例:暂存某文件(路径为仓库根相对)。
    pub fn stage(&self, repo_path: &Path, file: &Path) -> Result<(), GitError> {
        self.backend.stage(repo_path, file)
    }

    /// 用例:取消暂存某文件。
    pub fn unstage(&self, repo_path: &Path, file: &Path) -> Result<(), GitError> {
        self.backend.unstage(repo_path, file)
    }

    /// 用例:提交。空白信息在本层拦截,不下探后端。
    pub fn commit(&self, repo_path: &Path, message: &str) -> Result<String, GitError> {
        if message.trim().is_empty() {
            return Err(GitError::EmptyCommitMessage);
        }
        self.backend.commit(repo_path, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::model::{FileEntry, FileState};
    use git_engine::FakeBackend;

    #[test]
    fn head_commit_via_fake_backend() {
        // 注入假后端 —— 不碰真实仓库,测试毫秒级且确定。
        let service = RepoService::new(Arc::new(FakeBackend::default()));
        let dto = service.head_commit(Path::new("/whatever")).unwrap();
        assert_eq!(dto.short_id, "0123456");
        assert_eq!(dto.author_name, "测试者");
    }

    #[test]
    fn status_maps_to_dto() {
        let fb = FakeBackend::with_status(vec![FileEntry {
            path: "a.txt".into(),
            state: FileState::Modified,
            staged: false,
        }]);
        let service = RepoService::new(Arc::new(fb));
        let dto = service.status(Path::new("/r")).unwrap();
        assert_eq!(dto.entries.len(), 1);
        assert_eq!(dto.entries[0].state, "modified");
    }

    #[test]
    fn stage_calls_backend() {
        let fb = Arc::new(FakeBackend::default());
        let service = RepoService::new(fb.clone());
        service.stage(Path::new("/r"), Path::new("a.txt")).unwrap();
        assert_eq!(fb.staged_files(), vec![std::path::PathBuf::from("a.txt")]);
    }

    #[test]
    fn commit_rejects_empty_message() {
        let service = RepoService::new(Arc::new(FakeBackend::default()));
        let err = service.commit(Path::new("/r"), "   ").unwrap_err();
        assert!(matches!(err, GitError::EmptyCommitMessage));
    }

    #[test]
    fn commit_forwards_nonempty_message() {
        let fb = Arc::new(FakeBackend::default());
        let service = RepoService::new(fb.clone());
        let sha = service.commit(Path::new("/r"), "real msg").unwrap();
        assert!(!sha.is_empty());
        assert_eq!(fb.commit_messages(), vec!["real msg".to_string()]);
    }
}
