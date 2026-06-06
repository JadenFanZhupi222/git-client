//! app-service:应用层。把领域能力组织成产品用例。
//! 关键点:它依赖 `dyn GitBackend`(trait 对象),而不是任何具体后端 ——
//! 后端通过构造函数注入(依赖注入),所以测试时能塞 FakeBackend。

use git_core::{GitBackend, GitError};
use ipc_types::{
    BranchDto, CommitDto, FetchResultDto, FileChangeDto, FileDiffDto, GraphRowDto, PullResultDto,
    StatusDto,
};
use std::path::Path;
use std::sync::Arc;

pub mod graph;
pub mod watcher;

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

    /// 用例:提交历史,时间倒序,limit/skip 分页。
    pub fn log(
        &self,
        repo_path: &Path,
        limit: usize,
        skip: usize,
    ) -> Result<Vec<CommitDto>, GitError> {
        let commits = self.backend.log(repo_path, limit, skip)?;
        Ok(commits.into_iter().map(CommitDto::from).collect())
    }

    /// 用例:提交图谱。取 HEAD 起 limit 条提交,算 lane 布局后返回。
    /// 从头(skip=0)整段计算,保证泳道一致。
    pub fn commit_graph(
        &self,
        repo_path: &Path,
        limit: usize,
    ) -> Result<Vec<GraphRowDto>, GitError> {
        let commits = self.backend.log(repo_path, limit, 0)?;
        Ok(crate::graph::layout(&commits))
    }

    /// 用例:某提交改动的文件列表。
    pub fn commit_files(
        &self,
        repo_path: &Path,
        commit_id: &str,
    ) -> Result<Vec<FileChangeDto>, GitError> {
        let files = self.backend.commit_files(repo_path, commit_id)?;
        Ok(files.into_iter().map(FileChangeDto::from).collect())
    }

    /// 用例:某提交中单个文件的行级 diff。
    pub fn commit_file_diff(
        &self,
        repo_path: &Path,
        commit_id: &str,
        file: &str,
    ) -> Result<FileDiffDto, GitError> {
        let diff = self.backend.commit_file_diff(repo_path, commit_id, file)?;
        Ok(FileDiffDto::from(diff))
    }

    /// 用例:当前 HEAD 分支短名;分离头/空仓库返回 None。
    pub fn current_branch(&self, repo_path: &Path) -> Result<Option<String>, GitError> {
        self.backend.current_branch(repo_path)
    }

    /// 用例:列出本地分支。
    pub fn branches(&self, repo_path: &Path) -> Result<Vec<BranchDto>, GitError> {
        let list = self.backend.branches(repo_path)?;
        Ok(list.into_iter().map(BranchDto::from).collect())
    }

    /// 用例:切换分支。空名在本层拦截。
    pub fn checkout_branch(&self, repo_path: &Path, name: &str) -> Result<(), GitError> {
        if name.trim().is_empty() {
            return Err(GitError::InvalidBranchName);
        }
        self.backend.checkout_branch(repo_path, name)
    }

    /// 用例:新建分支(在 HEAD 上)。`checkout=true` 时建完即切过去 ——
    /// 对应「新建并切换」这个最常见流程。空名在本层拦截。
    pub fn create_branch(
        &self,
        repo_path: &Path,
        name: &str,
        checkout: bool,
    ) -> Result<(), GitError> {
        if name.trim().is_empty() {
            return Err(GitError::InvalidBranchName);
        }
        self.backend.create_branch(repo_path, name)?;
        if checkout {
            self.backend.checkout_branch(repo_path, name)?;
        }
        Ok(())
    }

    /// 用例:删除本地分支。
    pub fn delete_branch(&self, repo_path: &Path, name: &str) -> Result<(), GitError> {
        if name.trim().is_empty() {
            return Err(GitError::InvalidBranchName);
        }
        self.backend.delete_branch(repo_path, name)
    }

    /// 用例:从远程 fetch。remote=None 用默认远程。
    pub fn fetch(
        &self,
        repo_path: &Path,
        remote: Option<&str>,
    ) -> Result<FetchResultDto, GitError> {
        let outcome = self.backend.fetch(repo_path, remote)?;
        Ok(FetchResultDto::from(outcome))
    }

    /// 用例:pull(fetch + merge)。remote=None 用上游。
    pub fn pull(&self, repo_path: &Path, remote: Option<&str>) -> Result<PullResultDto, GitError> {
        let outcome = self.backend.pull(repo_path, remote)?;
        Ok(PullResultDto::from(outcome))
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
    use git_core::model::{Commit, FileChange, FileEntry, FileState, Signature};
    use git_engine::FakeBackend;

    fn fake_commit(summary: &str) -> Commit {
        Commit {
            id: "i".into(),
            short_id: "i".into(),
            summary: summary.into(),
            body: "".into(),
            author: Signature {
                name: "n".into(),
                email: "e".into(),
            },
            timestamp: 1,
            parents: vec![],
        }
    }

    #[test]
    fn log_returns_commit_dtos() {
        let fb = FakeBackend::default().with_log(vec![fake_commit("hi")]);
        let svc = RepoService::new(Arc::new(fb));
        let dtos = svc.log(Path::new("/r"), 10, 0).unwrap();
        assert_eq!(dtos.len(), 1);
        assert_eq!(dtos[0].summary, "hi");
    }

    #[test]
    fn commit_graph_lays_out_log() {
        use git_core::model::{Commit, Signature};
        let mk = |id: &str, parents: Vec<&str>| Commit {
            id: id.into(),
            short_id: id.into(),
            summary: "s".into(),
            body: String::new(),
            author: Signature {
                name: "n".into(),
                email: "e".into(),
            },
            timestamp: 0,
            parents: parents.iter().map(|s| s.to_string()).collect(),
        };
        let fb = FakeBackend::default().with_log(vec![mk("a", vec!["b"]), mk("b", vec![])]);
        let svc = RepoService::new(Arc::new(fb));
        let rows = svc.commit_graph(Path::new("/r"), 10).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].commit.id, "a");
        assert_eq!(rows[0].commit.parents, vec!["b".to_string()]);
        assert_eq!(rows[0].column, 0);
    }

    #[test]
    fn commit_file_diff_maps_dto() {
        use git_core::model::{DiffLine, DiffLineKind, FileDiff, Hunk};
        let fb = FakeBackend::default().with_file_diff(FileDiff {
            path: "a.txt".into(),
            is_binary: false,
            hunks: vec![Hunk {
                header: "@@ -1 +1 @@".into(),
                lines: vec![DiffLine {
                    kind: DiffLineKind::Addition,
                    old_lineno: None,
                    new_lineno: Some(1),
                    content: "hi".into(),
                }],
            }],
        });
        let svc = RepoService::new(Arc::new(fb));
        let dto = svc.commit_file_diff(Path::new("/r"), "x", "a.txt").unwrap();
        assert_eq!(dto.path, "a.txt");
        assert!(!dto.is_binary);
        assert_eq!(dto.hunks.len(), 1);
        assert_eq!(dto.hunks[0].lines[0].kind, "add");
        assert_eq!(dto.hunks[0].lines[0].new_lineno, Some(1));
    }

    #[test]
    fn commit_files_maps_dto() {
        let fb = FakeBackend::default().with_commit_files(vec![FileChange {
            path: "a".into(),
            status: FileState::Modified,
        }]);
        let svc = RepoService::new(Arc::new(fb));
        let dtos = svc.commit_files(Path::new("/r"), "x").unwrap();
        assert_eq!(dtos[0].status, "modified");
    }

    #[test]
    fn current_branch_forwards() {
        let fb = FakeBackend::default().with_branch(Some("main".into()));
        let svc = RepoService::new(Arc::new(fb));
        assert_eq!(
            svc.current_branch(Path::new("/r")).unwrap(),
            Some("main".into())
        );
    }

    #[test]
    fn branches_map_to_dto() {
        use git_core::model::BranchInfo;
        let fb = FakeBackend::default().with_branches(vec![
            BranchInfo {
                name: "main".into(),
                is_head: true,
            },
            BranchInfo {
                name: "dev".into(),
                is_head: false,
            },
        ]);
        let svc = RepoService::new(Arc::new(fb));
        let dtos = svc.branches(Path::new("/r")).unwrap();
        assert_eq!(dtos.len(), 2);
        assert_eq!(dtos[0].name, "main");
        assert!(dtos[0].is_head);
        assert!(!dtos[1].is_head);
    }

    #[test]
    fn checkout_forwards_to_backend() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.checkout_branch(Path::new("/r"), "dev").unwrap();
        assert_eq!(fb.checked_out_branches(), vec!["dev".to_string()]);
    }

    #[test]
    fn checkout_rejects_empty_name() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        let err = svc.checkout_branch(Path::new("/r"), "  ").unwrap_err();
        assert!(matches!(err, GitError::InvalidBranchName));
        assert!(fb.checked_out_branches().is_empty(), "空名不应下探后端");
    }

    #[test]
    fn create_branch_without_checkout() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.create_branch(Path::new("/r"), "feat/x", false).unwrap();
        assert_eq!(fb.created_branches(), vec!["feat/x".to_string()]);
        assert!(fb.checked_out_branches().is_empty(), "checkout=false 不应切换");
    }

    #[test]
    fn create_branch_with_checkout_also_switches() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.create_branch(Path::new("/r"), "feat/y", true).unwrap();
        assert_eq!(fb.created_branches(), vec!["feat/y".to_string()]);
        assert_eq!(fb.checked_out_branches(), vec!["feat/y".to_string()]);
    }

    #[test]
    fn delete_branch_forwards() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.delete_branch(Path::new("/r"), "old").unwrap();
        assert_eq!(fb.deleted_branches(), vec!["old".to_string()]);
    }

    #[test]
    fn fetch_forwards_and_maps_dto() {
        use git_core::model::FetchOutcome;
        let fb = FakeBackend::default().with_fetch(FetchOutcome {
            remote: "origin".into(),
            summary: "已是最新".into(),
        });
        let svc = RepoService::new(Arc::new(fb));
        let dto = svc.fetch(Path::new("/r"), None).unwrap();
        assert_eq!(dto.remote, "origin");
        assert_eq!(dto.summary, "已是最新");
    }

    #[test]
    fn fetch_counts_backend_call() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.fetch(Path::new("/r"), Some("origin")).unwrap();
        assert_eq!(fb.fetch_call_count(), 1);
    }

    #[test]
    fn pull_forwards_and_maps_dto() {
        use git_core::model::PullOutcome;
        let fb = FakeBackend::default().with_pull(PullOutcome {
            summary: "Fast-forward".into(),
        });
        let svc = RepoService::new(Arc::new(fb));
        let dto = svc.pull(Path::new("/r"), None).unwrap();
        assert_eq!(dto.summary, "Fast-forward");
    }

    #[test]
    fn pull_counts_backend_call() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.pull(Path::new("/r"), None).unwrap();
        assert_eq!(fb.pull_call_count(), 1);
    }

    #[test]
    fn create_branch_rejects_empty_name() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        let err = svc.create_branch(Path::new("/r"), " ", true).unwrap_err();
        assert!(matches!(err, GitError::InvalidBranchName));
        assert!(fb.created_branches().is_empty());
    }

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
