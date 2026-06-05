//! app-service:应用层。把领域能力组织成产品用例。
//! 关键点:它依赖 `dyn GitBackend`(trait 对象),而不是任何具体后端 ——
//! 后端通过构造函数注入(依赖注入),所以测试时能塞 FakeBackend。

use std::path::Path;
use std::sync::Arc;
use git_core::{GitBackend, GitError};
use ipc_types::CommitDto;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_engine::FakeBackend;

    #[test]
    fn head_commit_via_fake_backend() {
        // 注入假后端 —— 不碰真实仓库,测试毫秒级且确定。
        let service = RepoService::new(Arc::new(FakeBackend::default()));
        let dto = service.head_commit(Path::new("/whatever")).unwrap();
        assert_eq!(dto.short_id, "0123456");
        assert_eq!(dto.author_name, "测试者");
    }
}
