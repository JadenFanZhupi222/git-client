//! app-service:应用层。把领域能力组织成产品用例。
//! 关键点:它依赖 `dyn GitBackend`(trait 对象),而不是任何具体后端 ——
//! 后端通过构造函数注入(依赖注入),所以测试时能塞 FakeBackend。

use git_core::{GitBackend, GitError};
use ipc_types::{
    AheadBehindDto, BlameLineDto, BranchDto, CommitDto, ConflictSidesDto, FetchResultDto,
    FileChangeDto, FileDiffDto, GraphRowDto, PullResultDto, PushResultDto, RefDto, StashDto,
    StatusDto,
};
use std::collections::HashMap;
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

    /// 用例:暂存某文件的第 hunk_index 个未暂存改动块。
    pub fn stage_hunk(
        &self,
        repo_path: &Path,
        file: &str,
        hunk_index: usize,
    ) -> Result<(), GitError> {
        self.backend.stage_hunk(repo_path, file, hunk_index)
    }

    /// 用例:暂存某未暂存 hunk 中的指定行。
    pub fn stage_lines(
        &self,
        repo_path: &Path,
        file: &str,
        hunk_index: usize,
        lines: &[usize],
    ) -> Result<(), GitError> {
        self.backend.stage_lines(repo_path, file, hunk_index, lines)
    }

    /// 用例:取消暂存某文件的第 hunk_index 个已暂存改动块。
    pub fn unstage_hunk(
        &self,
        repo_path: &Path,
        file: &str,
        hunk_index: usize,
    ) -> Result<(), GitError> {
        self.backend.unstage_hunk(repo_path, file, hunk_index)
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
    /// 从头(skip=0)整段计算,保证泳道一致。再把引用(分支/远程/HEAD)
    /// 按 SHA 挂到对应行,供前端渲染标签。
    pub fn commit_graph(
        &self,
        repo_path: &Path,
        limit: usize,
    ) -> Result<Vec<GraphRowDto>, GitError> {
        let commits = self.backend.log(repo_path, limit, 0)?;
        let refs = self.backend.refs(repo_path)?;
        let sync = self.backend.sync_commits(repo_path)?;
        let mut rows = crate::graph::layout(&commits);

        // 按目标 SHA 分组引用,然后挂到可见行上(指向窗口外提交的引用自然落空)。
        let mut by_sha: HashMap<String, Vec<RefDto>> = HashMap::new();
        for r in refs {
            by_sha
                .entry(r.target.clone())
                .or_default()
                .push(RefDto::from(r));
        }
        for row in &mut rows {
            if let Some(rs) = by_sha.remove(&row.commit.id) {
                row.refs = rs;
            }
            // 未 push / 未 pull 标记(两集合互斥;outgoing 优先无歧义)。
            if sync.outgoing.contains(&row.commit.id) {
                row.sync = "outgoing".to_string();
            } else if sync.incoming.contains(&row.commit.id) {
                row.sync = "incoming".to_string();
            }
        }
        Ok(rows)
    }

    /// 用例:从 HEAD 搜索提交(匹配 message/作者/SHA),扁平列表。空 query 返回空。
    /// `cancelled`:用户改搜索词时取消上一次遍历(见 GitBackend::search_commits)。
    pub fn search_commits(
        &self,
        repo_path: &Path,
        query: &str,
        limit: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<CommitDto>, GitError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let commits = self
            .backend
            .search_commits(repo_path, query, limit, cancelled)?;
        Ok(commits.into_iter().map(CommitDto::from).collect())
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

    /// 用例:工作区文件 diff(staged=false 未暂存 / true 已暂存)。
    pub fn working_diff(
        &self,
        repo_path: &Path,
        file: &str,
        staged: bool,
    ) -> Result<FileDiffDto, GitError> {
        let diff = self.backend.working_diff(repo_path, file, staged)?;
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

    /// 用例:当前分支相对上游的领先/落后;无上游返回 None。
    pub fn ahead_behind(&self, repo_path: &Path) -> Result<Option<AheadBehindDto>, GitError> {
        Ok(self
            .backend
            .ahead_behind(repo_path)?
            .map(AheadBehindDto::from))
    }

    /// 用例:列出远程名。
    pub fn remotes(&self, repo_path: &Path) -> Result<Vec<String>, GitError> {
        self.backend.remotes(repo_path)
    }

    /// 用例:把当前分支上游设为 upstream(形如 "origin/main")。
    pub fn set_upstream(&self, repo_path: &Path, upstream: &str) -> Result<(), GitError> {
        if upstream.trim().is_empty() {
            return Err(GitError::InvalidBranchName);
        }
        self.backend.set_upstream(repo_path, upstream)
    }

    // ---- 冲突 / 进行中操作 ----
    /// 仓库状态字符串:clean | merging | rebasing | cherry-picking | reverting | other。
    pub fn repo_state(&self, repo_path: &Path) -> Result<String, GitError> {
        use git_core::model::RepoState::*;
        Ok(match self.backend.repo_state(repo_path)? {
            Clean => "clean",
            Merging => "merging",
            Rebasing => "rebasing",
            CherryPicking => "cherry-picking",
            Reverting => "reverting",
            Other => "other",
        }
        .to_string())
    }
    /// 用例:逐行 blame。
    pub fn blame(&self, repo_path: &Path, file: &str) -> Result<Vec<BlameLineDto>, GitError> {
        Ok(self
            .backend
            .blame(repo_path, file)?
            .into_iter()
            .map(BlameLineDto::from)
            .collect())
    }
    /// 用例:读冲突文件三方内容,供三栏合并编辑器渲染。
    pub fn conflict_sides(
        &self,
        repo_path: &Path,
        file: &str,
    ) -> Result<ConflictSidesDto, GitError> {
        Ok(ConflictSidesDto::from(
            self.backend.conflict_sides(repo_path, file)?,
        ))
    }
    pub fn resolve_ours(&self, repo_path: &Path, file: &str) -> Result<(), GitError> {
        self.backend.resolve_ours(repo_path, file)
    }
    pub fn resolve_theirs(&self, repo_path: &Path, file: &str) -> Result<(), GitError> {
        self.backend.resolve_theirs(repo_path, file)
    }
    pub fn continue_op(&self, repo_path: &Path) -> Result<(), GitError> {
        self.backend.continue_op(repo_path)
    }
    pub fn abort_op(&self, repo_path: &Path) -> Result<(), GitError> {
        self.backend.abort_op(repo_path)
    }
    /// 把某提交拣选到当前分支。
    pub fn cherry_pick(&self, repo_path: &Path, commit_id: &str) -> Result<(), GitError> {
        if commit_id.trim().is_empty() {
            return Err(GitError::Backend("提交 ID 不能为空".into()));
        }
        self.backend.cherry_pick(repo_path, commit_id)
    }

    /// 回滚某提交(生成抵消其改动的新提交)。
    pub fn revert(&self, repo_path: &Path, commit_id: &str) -> Result<(), GitError> {
        if commit_id.trim().is_empty() {
            return Err(GitError::Backend("提交 ID 不能为空".into()));
        }
        self.backend.revert(repo_path, commit_id)
    }

    /// 在指定提交上打标签。name 空在本层拦截;message 空串视为轻量标签。
    pub fn create_tag(
        &self,
        repo_path: &Path,
        name: &str,
        commit_id: &str,
        message: Option<&str>,
    ) -> Result<(), GitError> {
        if name.trim().is_empty() {
            return Err(GitError::Backend("标签名不能为空".into()));
        }
        if commit_id.trim().is_empty() {
            return Err(GitError::Backend("提交 ID 不能为空".into()));
        }
        self.backend.create_tag(repo_path, name.trim(), commit_id, message)
    }

    /// 删除标签。name 空在本层拦截。
    pub fn delete_tag(&self, repo_path: &Path, name: &str) -> Result<(), GitError> {
        if name.trim().is_empty() {
            return Err(GitError::Backend("标签名不能为空".into()));
        }
        self.backend.delete_tag(repo_path, name.trim())
    }

    // ---- 贮藏 ----
    pub fn stash_list(&self, repo_path: &Path) -> Result<Vec<StashDto>, GitError> {
        Ok(self
            .backend
            .stash_list(repo_path)?
            .into_iter()
            .map(StashDto::from)
            .collect())
    }
    pub fn stash_save(&self, repo_path: &Path, message: Option<&str>) -> Result<(), GitError> {
        self.backend.stash_save(repo_path, message)
    }
    pub fn stash_apply(&self, repo_path: &Path, index: usize) -> Result<(), GitError> {
        self.backend.stash_apply(repo_path, index)
    }
    pub fn stash_pop(&self, repo_path: &Path, index: usize) -> Result<(), GitError> {
        self.backend.stash_pop(repo_path, index)
    }
    pub fn stash_drop(&self, repo_path: &Path, index: usize) -> Result<(), GitError> {
        self.backend.stash_drop(repo_path, index)
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

    /// 用例:pull。remote=None 用上游;rebase=true 走 fetch+rebase。
    pub fn pull(
        &self,
        repo_path: &Path,
        remote: Option<&str>,
        rebase: bool,
    ) -> Result<PullResultDto, GitError> {
        let outcome = self.backend.pull(repo_path, remote, rebase)?;
        Ok(PullResultDto::from(outcome))
    }

    /// 用例:push 当前分支。remote=None 用默认远程;首次自动建上游。
    pub fn push(&self, repo_path: &Path, remote: Option<&str>) -> Result<PushResultDto, GitError> {
        let outcome = self.backend.push(repo_path, remote)?;
        Ok(PushResultDto::from(outcome))
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
    fn commit_graph_attaches_refs_by_sha() {
        use git_core::model::{Commit, CommitRef, RefKind, Signature};
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
        let fb = FakeBackend::default()
            .with_log(vec![mk("a", vec!["b"]), mk("b", vec![])])
            .with_refs(vec![
                CommitRef {
                    name: "main".into(),
                    kind: RefKind::Head,
                    target: "a".into(),
                },
                CommitRef {
                    name: "origin/main".into(),
                    kind: RefKind::RemoteBranch,
                    target: "b".into(),
                },
            ]);
        let svc = RepoService::new(Arc::new(fb));
        let rows = svc.commit_graph(Path::new("/r"), 10).unwrap();
        // a 行挂 HEAD(kind=head, name=main)
        assert_eq!(rows[0].commit.id, "a");
        assert_eq!(rows[0].refs.len(), 1);
        assert_eq!(rows[0].refs[0].kind, "head");
        assert_eq!(rows[0].refs[0].name, "main");
        // b 行挂 origin/main(kind=remote)
        assert_eq!(rows[1].refs.len(), 1);
        assert_eq!(rows[1].refs[0].kind, "remote");
        assert_eq!(rows[1].refs[0].name, "origin/main");
    }

    #[test]
    fn search_commits_filters_and_limits() {
        use git_core::model::{Commit, Signature};
        let mk = |id: &str, summary: &str, author: &str| Commit {
            id: id.into(),
            short_id: id.chars().take(7).collect(),
            summary: summary.into(),
            body: String::new(),
            author: Signature {
                name: author.into(),
                email: format!("{author}@e"),
            },
            timestamp: 0,
            parents: vec![],
        };
        let fb = FakeBackend::default().with_log(vec![
            mk("aaa111", "fix login bug", "alice"),
            mk("bbb222", "add Login page", "bob"),
            mk("ccc333", "refactor utils", "carol"),
        ]);
        let svc = RepoService::new(Arc::new(fb));
        let never = || false;
        // 大小写不敏感匹配 summary
        let hits = svc
            .search_commits(Path::new("/r"), "login", 10, &never)
            .unwrap();
        assert_eq!(hits.len(), 2);
        // 按作者
        let by_author = svc
            .search_commits(Path::new("/r"), "carol", 10, &never)
            .unwrap();
        assert_eq!(by_author.len(), 1);
        assert_eq!(by_author[0].id, "ccc333");
        // SHA 前缀
        let by_sha = svc
            .search_commits(Path::new("/r"), "bbb", 10, &never)
            .unwrap();
        assert_eq!(by_sha.len(), 1);
        // limit 生效
        let limited = svc
            .search_commits(Path::new("/r"), "login", 1, &never)
            .unwrap();
        assert_eq!(limited.len(), 1);
        // 空 query → 空
        assert!(
            svc.search_commits(Path::new("/r"), "  ", 10, &never)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn commit_graph_marks_outgoing_and_incoming() {
        use git_core::model::{Commit, Signature, SyncCommits};
        use std::collections::HashSet;
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
        // 历史:a(未push)→ b(已同步)→ c(已同步);另有 d 仅在远程(未pull)。
        let fb = FakeBackend::default()
            .with_log(vec![
                mk("a", vec!["b"]),
                mk("b", vec!["c"]),
                mk("c", vec![]),
                mk("d", vec!["b"]),
            ])
            .with_sync_commits(SyncCommits {
                outgoing: HashSet::from(["a".to_string()]),
                incoming: HashSet::from(["d".to_string()]),
            });
        let svc = RepoService::new(Arc::new(fb));
        let rows = svc.commit_graph(Path::new("/r"), 10).unwrap();
        let by_id = |id: &str| rows.iter().find(|r| r.commit.id == id).unwrap();
        assert_eq!(by_id("a").sync, "outgoing");
        assert_eq!(by_id("d").sync, "incoming");
        assert_eq!(by_id("b").sync, "");
        assert_eq!(by_id("c").sync, "");
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
            additions: 3,
            deletions: 1,
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
    fn ahead_behind_forwards_and_maps() {
        use git_core::model::AheadBehind;
        let fb = FakeBackend::default().with_ahead_behind(AheadBehind {
            ahead: 2,
            behind: 3,
        });
        let svc = RepoService::new(Arc::new(fb));
        let ab = svc
            .ahead_behind(Path::new("/r"))
            .unwrap()
            .expect("应有结果");
        assert_eq!(ab.ahead, 2);
        assert_eq!(ab.behind, 3);
    }

    #[test]
    fn ahead_behind_none_passthrough() {
        let svc = RepoService::new(Arc::new(FakeBackend::default()));
        assert!(svc.ahead_behind(Path::new("/r")).unwrap().is_none());
    }

    #[test]
    fn remotes_forwards() {
        let fb = FakeBackend::default().with_remotes(vec!["origin".into(), "upstream".into()]);
        let svc = RepoService::new(Arc::new(fb));
        assert_eq!(
            svc.remotes(Path::new("/r")).unwrap(),
            vec!["origin", "upstream"]
        );
    }

    #[test]
    fn repo_state_maps_to_string() {
        use git_core::model::RepoState;
        let fb = FakeBackend::default().with_repo_state(RepoState::Merging);
        let svc = RepoService::new(Arc::new(fb));
        assert_eq!(svc.repo_state(Path::new("/r")).unwrap(), "merging");
    }

    #[test]
    fn conflict_sides_maps_to_dto() {
        use git_core::model::ConflictSides;
        let fb = FakeBackend::default().with_conflict_sides(ConflictSides {
            base: Some("b\n".into()),
            ours: Some("o\n".into()),
            theirs: None,
        });
        let svc = RepoService::new(Arc::new(fb));
        let dto = svc.conflict_sides(Path::new("/r"), "a.txt").unwrap();
        assert_eq!(dto.base.as_deref(), Some("b\n"));
        assert_eq!(dto.ours.as_deref(), Some("o\n"));
        assert_eq!(dto.theirs, None);
    }

    #[test]
    fn conflict_ops_forward_to_backend() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.resolve_ours(Path::new("/r"), "a.txt").unwrap();
        svc.resolve_theirs(Path::new("/r"), "b.txt").unwrap();
        svc.continue_op(Path::new("/r")).unwrap();
        svc.abort_op(Path::new("/r")).unwrap();
        svc.cherry_pick(Path::new("/r"), "abc123").unwrap();
        svc.revert(Path::new("/r"), "def456").unwrap();
        assert_eq!(
            fb.conflict_ops(),
            vec![
                "ours:a.txt",
                "theirs:b.txt",
                "continue",
                "abort",
                "cherry-pick:abc123",
                "revert:def456"
            ]
        );
        // 空 id 在 service 层拦截
        assert!(svc.revert(Path::new("/r"), "  ").is_err());
    }

    #[test]
    fn tag_ops_forward_and_validate() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.create_tag(Path::new("/r"), "v1.0", "abc123", None).unwrap();
        svc.create_tag(Path::new("/r"), " v1.1 ", "def456", Some("release")).unwrap();
        svc.delete_tag(Path::new("/r"), "v1.0").unwrap();
        assert_eq!(
            fb.tag_ops(),
            vec!["create:v1.0@abc123:", "create:v1.1@def456:release", "delete:v1.0"]
        );
        // 空名在 service 层拦截
        assert!(svc.create_tag(Path::new("/r"), "  ", "abc", None).is_err());
        assert!(svc.delete_tag(Path::new("/r"), "").is_err());
    }

    #[test]
    fn stash_list_maps_dto() {
        use git_core::model::StashEntry;
        let fb = FakeBackend::default().with_stashes(vec![StashEntry {
            index: 0,
            message: "wip".into(),
        }]);
        let svc = RepoService::new(Arc::new(fb));
        let list = svc.stash_list(Path::new("/r")).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].index, 0);
        assert_eq!(list[0].message, "wip");
    }

    #[test]
    fn stash_ops_forward_to_backend() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.stash_save(Path::new("/r"), Some("m")).unwrap();
        svc.stash_apply(Path::new("/r"), 1).unwrap();
        svc.stash_pop(Path::new("/r"), 2).unwrap();
        svc.stash_drop(Path::new("/r"), 3).unwrap();
        assert_eq!(fb.stash_ops(), vec!["save:m", "apply:1", "pop:2", "drop:3"]);
    }

    #[test]
    fn set_upstream_forwards_and_rejects_empty() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.set_upstream(Path::new("/r"), "origin/main").unwrap();
        assert_eq!(fb.upstreams_set(), vec!["origin/main".to_string()]);
        assert!(matches!(
            svc.set_upstream(Path::new("/r"), "  ").unwrap_err(),
            GitError::InvalidBranchName
        ));
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
        assert!(
            fb.checked_out_branches().is_empty(),
            "checkout=false 不应切换"
        );
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
        let dto = svc.pull(Path::new("/r"), None, false).unwrap();
        assert_eq!(dto.summary, "Fast-forward");
    }

    #[test]
    fn pull_counts_backend_call() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.pull(Path::new("/r"), None, false).unwrap();
        assert_eq!(fb.pull_call_count(), 1);
    }

    #[test]
    fn push_forwards_and_maps_dto() {
        use git_core::model::PushOutcome;
        let fb = FakeBackend::default().with_push(PushOutcome {
            summary: "main -> main".into(),
            set_upstream: true,
        });
        let svc = RepoService::new(Arc::new(fb));
        let dto = svc.push(Path::new("/r"), None).unwrap();
        assert_eq!(dto.summary, "main -> main");
        assert!(dto.set_upstream);
    }

    #[test]
    fn push_counts_backend_call() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.push(Path::new("/r"), Some("origin")).unwrap();
        assert_eq!(fb.push_call_count(), 1);
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
    fn stage_hunk_forwards_to_backend() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.stage_hunk(Path::new("/r"), "a.txt", 2).unwrap();
        assert_eq!(fb.staged_hunks(), vec![("a.txt".to_string(), 2)]);
    }

    #[test]
    fn unstage_hunk_forwards_to_backend() {
        let fb = Arc::new(FakeBackend::default());
        let svc = RepoService::new(fb.clone());
        svc.unstage_hunk(Path::new("/r"), "a.txt", 1).unwrap();
        assert_eq!(fb.unstaged_hunks(), vec![("a.txt".to_string(), 1)]);
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
