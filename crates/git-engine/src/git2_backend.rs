use git_core::model::{
    AheadBehind, BlameLine, BranchDeleteImpact, BranchInfo, Commit, CommitRef, ConflictSides,
    DiffLine, DiffLineKind, FileChange, FileDiff, FileEntry, FileState, Hunk, RefKind, ReflogEntry,
    RepoState, ResetMode, Signature, SyncCommits, WorkingTreeStatus,
};
use git_core::{GitBackend, GitError};
use std::collections::HashSet;
use std::path::Path;

/// git2 的 add_path/remove_path 要求"仓库根相对路径"。
/// 若传入绝对路径,用 workdir 前缀剥成相对路径;否则原样返回。
/// 这个泄漏细节锁在适配器层,不污染上层。
fn to_repo_relative(repo: &git2::Repository, file: &Path) -> std::path::PathBuf {
    if file.is_absolute()
        && let Some(wd) = repo.workdir()
    {
        // canonicalize 两边再剥前缀:macOS 临时目录 /var 是 /private/var 的符号链接,
        // 不规范化会导致 strip_prefix 误判失败。canonicalize 失败(如文件已删)时回退原值。
        let wd_c = wd.canonicalize().unwrap_or_else(|_| wd.to_path_buf());
        let file_c = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
        if let Ok(stripped) = file_c.strip_prefix(&wd_c) {
            return stripped.to_path_buf();
        }
    }
    file.to_path_buf()
}

/// 从 git2::Commit 构造领域 Commit。head_commit 与 log 共用(DRY)。
fn build_commit(c: &git2::Commit) -> Commit {
    let id = c.id().to_string();
    let author = c.author();
    Commit {
        short_id: id.chars().take(7).collect(),
        id,
        summary: c.summary().unwrap_or("").to_string(),
        body: c.body().unwrap_or("").to_string(),
        author: Signature {
            name: author.name().unwrap_or("").to_string(),
            email: author.email().unwrap_or("").to_string(),
        },
        timestamp: c.time().seconds(),
        parents: c.parent_ids().map(|oid| oid.to_string()).collect(),
    }
}

/// 建一个从 HEAD 出发、按时间倒序(新→旧)的 revwalk。
/// 空仓库(unborn HEAD)→ Ok(None);调用方据此返回空结果。
/// ⚠️ set_sorting 必须在 push_head 之前。log 与 search_commits 共用,避免漏改顺序。
fn head_revwalk(repo: &git2::Repository) -> Result<Option<git2::Revwalk<'_>>, GitError> {
    // 用 repo.head() 提前判断 unborn,避免 push_head 在不同 libgit2 版本上返回不一致的错误码。
    match repo.head() {
        Ok(_) => {}
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Ok(None),
        Err(e) => return Err(GitError::Backend(e.to_string())),
    }
    let mut walk = repo
        .revwalk()
        .map_err(|e| GitError::Backend(e.to_string()))?;
    walk.set_sorting(git2::Sort::TIME)
        .map_err(|e| GitError::Backend(e.to_string()))?;
    walk.push_head()
        .map_err(|e| GitError::Backend(e.to_string()))?;
    Ok(Some(walk))
}

/// 把一个 tree↔tree 的 git2::Diff 摊成文件级 FileChange 列表(含增删行数)。
/// commit_files(提交↔父)与 compare_files(任意两 revision)共用。
fn diff_to_changes(diff: &git2::Diff) -> Result<Vec<FileChange>, GitError> {
    let mut out = Vec::new();
    for (idx, delta) in diff.deltas().enumerate() {
        let status = match delta.status() {
            git2::Delta::Added | git2::Delta::Copied => FileState::Added,
            git2::Delta::Deleted => FileState::Deleted,
            git2::Delta::Renamed => FileState::Renamed,
            git2::Delta::Modified | git2::Delta::Typechange => FileState::Modified,
            _ => continue,
        };
        let p = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        // 本文件增删行数(diff --stat)。二进制 delta → Patch 为 None → 0/0。
        let (additions, deletions) = match git2::Patch::from_diff(diff, idx)
            .map_err(|e| GitError::Backend(e.to_string()))?
        {
            Some(patch) => {
                let (_ctx, adds, dels) = patch
                    .line_stats()
                    .map_err(|e| GitError::Backend(e.to_string()))?;
                (adds, dels)
            }
            None => (0, 0),
        };
        out.push(FileChange {
            path: p,
            status,
            additions,
            deletions,
        });
    }
    Ok(out)
}

/// 把一个 revision 字符串(commit SHA / 分支名 / 标签名)解析为它的 tree。
/// 用 revparse_single 兼容多种写法,再 peel 到 tree。
fn resolve_tree<'r>(repo: &'r git2::Repository, rev: &str) -> Result<git2::Tree<'r>, GitError> {
    let obj = repo
        .revparse_single(rev)
        .map_err(|e| GitError::Backend(format!("无法解析 '{rev}': {e}")))?;
    obj.peel_to_tree()
        .map_err(|e| GitError::Backend(e.to_string()))
}

/// 从一个 git2::Diff 里抽出指定文件的行级 FileDiff。
/// commit_file_diff(树↔树)与 working_diff(工作区/index)共用,保证渲染一致。
fn file_diff_from(diff: &git2::Diff, file: &str) -> Result<FileDiff, GitError> {
    let target = Path::new(file);
    let mut result = FileDiff {
        path: file.to_string(),
        is_binary: false,
        hunks: Vec::new(),
    };

    for (idx, delta) in diff.deltas().enumerate() {
        let p = delta.new_file().path().or_else(|| delta.old_file().path());
        if p != Some(target) {
            continue;
        }
        // Patch::from_diff 对二进制 delta 返回 None。
        match git2::Patch::from_diff(diff, idx).map_err(|e| GitError::Backend(e.to_string()))? {
            None => {
                result.is_binary = true;
            }
            Some(patch) => {
                for h in 0..patch.num_hunks() {
                    let (hunk, line_count) = patch
                        .hunk(h)
                        .map_err(|e| GitError::Backend(e.to_string()))?;
                    let header = String::from_utf8_lossy(hunk.header())
                        .trim_end()
                        .to_string();
                    let mut lines = Vec::with_capacity(line_count);
                    for l in 0..line_count {
                        let dl = patch
                            .line_in_hunk(h, l)
                            .map_err(|e| GitError::Backend(e.to_string()))?;
                        let kind = match dl.origin() {
                            '+' | '>' => DiffLineKind::Addition,
                            '-' | '<' => DiffLineKind::Deletion,
                            _ => DiffLineKind::Context,
                        };
                        // content 含行尾换行,去掉以便前端逐行渲染。
                        let content = String::from_utf8_lossy(dl.content())
                            .trim_end_matches(['\r', '\n'])
                            .to_string();
                        lines.push(DiffLine {
                            kind,
                            old_lineno: dl.old_lineno(),
                            new_lineno: dl.new_lineno(),
                            content,
                        });
                    }
                    result.hunks.push(Hunk { header, lines });
                }
            }
        }
        break; // 只处理第一个匹配文件
    }
    Ok(result)
}

/// 基于 git2(libgit2 绑定)的后端。阶段 0/1 用它实现读写。
/// 注意:git2 是同步阻塞的 —— 调用方必须在 spawn_blocking 里使用它!
#[derive(Default)]
pub struct Git2Backend;

impl Git2Backend {
    /// 为「行级暂存」重建一个只含选定行的 patch(应用到 index 即只暂存这些行)。
    /// `lines` 是该 hunk 内行下标(与 working_diff/FileDiff 的行顺序一致)。
    /// 规则:选中的 +/- 行保留;未选的「+」丢弃;未选的「-」转成上下文(保留)。
    pub fn build_line_stage_patch(
        &self,
        path: &Path,
        file: &str,
        hunk_index: usize,
        lines: &[usize],
    ) -> Result<String, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let mut opts = git2::DiffOptions::new();
        opts.pathspec(file);
        let diff = repo
            .diff_index_to_workdir(None, Some(&mut opts))
            .map_err(|e| GitError::Backend(e.to_string()))?;

        let target = Path::new(file);
        let idx = diff
            .deltas()
            .position(|d| d.new_file().path().or_else(|| d.old_file().path()) == Some(target))
            .ok_or_else(|| GitError::Backend("文件无未暂存改动".into()))?;
        let patch = git2::Patch::from_diff(&diff, idx)
            .map_err(|e| GitError::Backend(e.to_string()))?
            .ok_or_else(|| GitError::Backend("二进制文件不支持行级暂存".into()))?;
        if hunk_index >= patch.num_hunks() {
            return Err(GitError::Backend("hunk 越界".into()));
        }
        let (hunk, n) = patch
            .hunk(hunk_index)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        let old_start = hunk.old_start();

        let sel: std::collections::HashSet<usize> = lines.iter().copied().collect();
        let mut body = String::new();
        let (mut old_count, mut new_count) = (0u32, 0u32);
        let mut staged_any = false;
        for l in 0..n {
            let dl = patch
                .line_in_hunk(hunk_index, l)
                .map_err(|e| GitError::Backend(e.to_string()))?;
            let content = String::from_utf8_lossy(dl.content());
            let content = content.trim_end_matches(['\r', '\n']);
            match dl.origin() {
                '+' | '>' => {
                    if sel.contains(&l) {
                        body.push('+');
                        body.push_str(content);
                        body.push('\n');
                        new_count += 1;
                        staged_any = true;
                    } // 未选的新增 → 丢弃
                }
                '-' | '<' => {
                    if sel.contains(&l) {
                        body.push('-');
                        old_count += 1;
                        staged_any = true;
                    } else {
                        body.push(' '); // 未选的删除 → 保留为上下文
                        old_count += 1;
                        new_count += 1;
                    }
                    body.push_str(content);
                    body.push('\n');
                }
                _ => {
                    body.push(' ');
                    body.push_str(content);
                    body.push('\n');
                    old_count += 1;
                    new_count += 1;
                }
            }
        }
        if !staged_any {
            return Err(GitError::Backend("未选择任何可暂存的行".into()));
        }
        // newStart 与 oldStart 取一致:git apply 用 - 侧 + 上下文定位,+ 计数正确即可。
        let header = format!(
            "--- a/{file}\n+++ b/{file}\n@@ -{old_start},{old_count} +{old_start},{new_count} @@\n"
        );
        Ok(format!("{header}{body}"))
    }
}

impl GitBackend for Git2Backend {
    fn open(&self, path: &Path) -> Result<(), GitError> {
        git2::Repository::open(path)
            .map(|_| ())
            .map_err(|e| GitError::RepoNotFound(e.to_string()))
    }

    fn head_commit(&self, path: &Path) -> Result<Commit, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;

        let head = repo.head().map_err(|_| GitError::NoHead)?;
        let commit = head
            .peel_to_commit()
            .map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(build_commit(&commit))
    }

    fn status(&self, path: &Path) -> Result<WorkingTreeStatus, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let statuses = repo
            .statuses(None)
            .map_err(|e| GitError::Backend(e.to_string()))?;

        let mut entries = Vec::new();
        for entry in statuses.iter() {
            let s = entry.status();
            let path = entry.path().unwrap_or("").to_string();
            let mut push = |state, staged| {
                entries.push(FileEntry {
                    path: path.clone(),
                    state,
                    staged,
                });
            };

            // 冲突单独成一项(不拆暂存/未暂存)。
            if s.is_conflicted() {
                push(FileState::Conflicted, false);
                continue;
            }

            // 暂存侧:index 相对 HEAD 的改动。
            if s.is_index_new() {
                push(FileState::Added, true);
            } else if s.is_index_modified() {
                push(FileState::Modified, true);
            } else if s.is_index_deleted() {
                push(FileState::Deleted, true);
            } else if s.is_index_renamed() {
                push(FileState::Renamed, true);
            }

            // 未暂存侧:工作区相对 index 的改动。一个文件可同时出现在两侧
            // (部分暂存:暂存了一部分 hunk,工作区还有剩余改动)。
            if s.is_wt_new() {
                push(FileState::Untracked, false);
            } else if s.is_wt_modified() {
                push(FileState::Modified, false);
            } else if s.is_wt_deleted() {
                push(FileState::Deleted, false);
            } else if s.is_wt_renamed() {
                push(FileState::Renamed, false);
            }
        }
        Ok(WorkingTreeStatus { entries })
    }

    fn stage(&self, path: &Path, file: &Path) -> Result<(), GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let rel = to_repo_relative(&repo, file);
        let mut index = repo.index().map_err(|e| GitError::Backend(e.to_string()))?;
        // 已知限制(阶段 1):add_path 只覆盖修改/新增;暂存"已删除文件"需 remove_path,留待后续。
        index
            .add_path(&rel)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        index
            .write()
            .map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(())
    }

    fn unstage(&self, path: &Path, file: &Path) -> Result<(), GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
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
                index
                    .remove_path(&rel)
                    .map_err(|e| GitError::Backend(e.to_string()))?;
                index
                    .write()
                    .map_err(|e| GitError::Backend(e.to_string()))?;
            }
            Err(e) => return Err(GitError::Backend(e.to_string())),
        }
        Ok(())
    }

    fn log(
        &self,
        path: &Path,
        limit: usize,
        skip: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<Commit>, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;

        let Some(walk) = head_revwalk(&repo)? else {
            return Ok(Vec::new()); // 空仓库
        };

        // ⚠️ Revwalk 惰性:直接 skip/take,别先 collect 全部
        // 大仓库历史很长:每条检查取消信号,被取代的旧请求尽快放手。
        let mut out = Vec::new();
        for oid in walk.skip(skip).take(limit) {
            if cancelled() {
                return Err(GitError::Cancelled);
            }
            let oid = oid.map_err(|e| GitError::Backend(e.to_string()))?;
            let commit = repo
                .find_commit(oid)
                .map_err(|e| GitError::Backend(e.to_string()))?;
            out.push(build_commit(&commit));
        }
        Ok(out)
    }

    fn search_commits(
        &self,
        path: &Path,
        query: &str,
        limit: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<Commit>, GitError> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let Some(walk) = head_revwalk(&repo)? else {
            return Ok(Vec::new()); // 空仓库,无可搜
        };

        // 惰性遍历整段历史,逐条匹配后收集,够 limit 即停 —— 不预先 collect。
        // 大仓库可能遍历很久:每条都检查取消信号,用户改搜索词时尽快放手。
        let mut out = Vec::new();
        for oid in walk {
            if cancelled() {
                return Err(GitError::Cancelled);
            }
            let oid = oid.map_err(|e| GitError::Backend(e.to_string()))?;
            let commit = repo
                .find_commit(oid)
                .map_err(|e| GitError::Backend(e.to_string()))?;
            let c = build_commit(&commit);
            let hit = c.id.starts_with(&q)
                || c.summary.to_lowercase().contains(&q)
                || c.body.to_lowercase().contains(&q)
                || c.author.name.to_lowercase().contains(&q)
                || c.author.email.to_lowercase().contains(&q);
            if hit {
                out.push(c);
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }
    fn reflog(&self, path: &Path, limit: usize) -> Result<Vec<ReflogEntry>, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        // 刚 init、尚无任何 HEAD 移动的仓库没有 reflog → 当作空,不报错。
        let Ok(reflog) = repo.reflog("HEAD") else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for (i, e) in reflog.iter().enumerate().take(limit) {
            let new_oid = e.id_new().to_string();
            let committer = e.committer();
            out.push(ReflogEntry {
                index: i,
                selector: format!("HEAD@{{{i}}}"),
                new_short: new_oid.chars().take(7).collect(),
                new_oid,
                message: e.message().unwrap_or("").to_string(),
                committer: Signature {
                    name: committer.name().unwrap_or("").to_string(),
                    email: committer.email().unwrap_or("").to_string(),
                },
                timestamp: committer.when().seconds(),
            });
        }
        Ok(out)
    }
    fn commit_files(&self, path: &Path, commit_id: &str) -> Result<Vec<FileChange>, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let oid = git2::Oid::from_str(commit_id).map_err(|e| GitError::Backend(e.to_string()))?;
        let commit = repo
            .find_commit(oid)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        let new_tree = commit
            .tree()
            .map_err(|e| GitError::Backend(e.to_string()))?;
        // 坑1:首提交无父 → parent_tree 传 None(与空树 diff)。坑2:合并只跟第一个父。
        let parent_tree = if commit.parent_count() == 0 {
            None
        } else {
            Some(
                commit
                    .parent(0)
                    .map_err(|e| GitError::Backend(e.to_string()))?
                    .tree()
                    .map_err(|e| GitError::Backend(e.to_string()))?,
            )
        };
        let diff = repo
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&new_tree), None)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        diff_to_changes(&diff)
    }

    fn compare_files(
        &self,
        path: &Path,
        from: &str,
        to: &str,
    ) -> Result<Vec<FileChange>, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let from_tree = resolve_tree(&repo, from)?;
        let to_tree = resolve_tree(&repo, to)?;
        let diff = repo
            .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        diff_to_changes(&diff)
    }

    fn compare_file_diff(
        &self,
        path: &Path,
        from: &str,
        to: &str,
        file: &str,
    ) -> Result<FileDiff, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let from_tree = resolve_tree(&repo, from)?;
        let to_tree = resolve_tree(&repo, to)?;
        // pathspec 限定只 diff 这一个文件,避免整棵树 diff 的开销。
        let mut opts = git2::DiffOptions::new();
        opts.pathspec(file);
        let diff = repo
            .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), Some(&mut opts))
            .map_err(|e| GitError::Backend(e.to_string()))?;
        file_diff_from(&diff, file)
    }
    fn current_branch(&self, path: &Path) -> Result<Option<String>, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        match repo.head() {
            Ok(head) => Ok(head.shorthand().map(|s| s.to_string())),
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(None),
            Err(e) => Err(GitError::Backend(e.to_string())),
        }
    }

    fn branches(&self, path: &Path) -> Result<Vec<BranchInfo>, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let iter = repo
            .branches(Some(git2::BranchType::Local))
            .map_err(|e| GitError::Backend(e.to_string()))?;

        let mut out = Vec::new();
        for item in iter {
            let (branch, _) = item.map_err(|e| GitError::Backend(e.to_string()))?;
            // is_head 借用了 branch,要在 name() 之前取(name() 也借用)。
            let is_head = branch.is_head();
            if let Some(name) = branch
                .name()
                .map_err(|e| GitError::Backend(e.to_string()))?
            {
                out.push(BranchInfo {
                    name: name.to_string(),
                    is_head,
                });
            }
        }
        // 名字升序,UI 列表稳定。
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn refs(&self, path: &Path) -> Result<Vec<CommitRef>, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let mut out = Vec::new();

        // 本地分支 refs/heads/*
        let locals = repo
            .branches(Some(git2::BranchType::Local))
            .map_err(|e| GitError::Backend(e.to_string()))?;
        for item in locals {
            let (branch, _) = item.map_err(|e| GitError::Backend(e.to_string()))?;
            let target = branch.get().target();
            if let (Some(name), Some(oid)) = (
                branch
                    .name()
                    .map_err(|e| GitError::Backend(e.to_string()))?,
                target,
            ) {
                out.push(CommitRef {
                    name: name.to_string(),
                    kind: RefKind::LocalBranch,
                    target: oid.to_string(),
                });
            }
        }

        // 远程跟踪分支 refs/remotes/*(跳过 origin/HEAD 这类符号引用,target() 为 None 时自然跳过)
        let remotes = repo
            .branches(Some(git2::BranchType::Remote))
            .map_err(|e| GitError::Backend(e.to_string()))?;
        for item in remotes {
            let (branch, _) = item.map_err(|e| GitError::Backend(e.to_string()))?;
            let target = branch.get().target();
            if let (Some(name), Some(oid)) = (
                branch
                    .name()
                    .map_err(|e| GitError::Backend(e.to_string()))?,
                target,
            ) {
                // origin/HEAD 是符号引用,但保险起见也按名字过滤掉。
                if name.ends_with("/HEAD") {
                    continue;
                }
                out.push(CommitRef {
                    name: name.to_string(),
                    kind: RefKind::RemoteBranch,
                    target: oid.to_string(),
                });
            }
        }

        // 标签 refs/tags/*(轻量标签直接指向 commit;附注标签 peel 到 commit)。
        if let Ok(iter) = repo.references_glob("refs/tags/*") {
            for r in iter.flatten() {
                let Some(full) = r.name() else { continue };
                let name = full.strip_prefix("refs/tags/").unwrap_or(full).to_string();
                if let Ok(commit) = r.peel_to_commit() {
                    out.push(CommitRef {
                        name,
                        kind: RefKind::Tag,
                        target: commit.id().to_string(),
                    });
                }
            }
        }

        // HEAD:用当前分支短名;分离头则名为 "HEAD"。空仓库(未出生)跳过。
        match repo.head() {
            Ok(head) => {
                if let Some(oid) = head.target() {
                    out.push(CommitRef {
                        name: head.shorthand().unwrap_or("HEAD").to_string(),
                        kind: RefKind::Head,
                        target: oid.to_string(),
                    });
                }
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {}
            Err(e) => return Err(GitError::Backend(e.to_string())),
        }

        Ok(out)
    }

    fn create_tag(
        &self,
        path: &Path,
        name: &str,
        commit_id: &str,
        message: Option<&str>,
    ) -> Result<(), GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        // 同名已存在 → 友好错误(git2 force=false 也会报错,但这样 code 更精确)。
        if repo.refname_to_id(&format!("refs/tags/{name}")).is_ok() {
            return Err(GitError::TagAlreadyExists(name.to_string()));
        }
        let oid = git2::Oid::from_str(commit_id).map_err(|e| GitError::Backend(e.to_string()))?;
        let obj = repo
            .find_object(oid, None)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        match message {
            // 附注标签:带 tagger 身份与信息。
            Some(m) if !m.trim().is_empty() => {
                let sig = repo.signature().map_err(|_| GitError::EmptySignature)?;
                repo.tag(name, &obj, &sig, m, false)
                    .map_err(|e| GitError::Backend(e.to_string()))?;
            }
            // 轻量标签:仅一个指向 commit 的 ref。
            _ => {
                repo.tag_lightweight(name, &obj, false)
                    .map_err(|e| GitError::Backend(e.to_string()))?;
            }
        }
        Ok(())
    }

    fn delete_tag(&self, path: &Path, name: &str) -> Result<(), GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        repo.tag_delete(name)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(())
    }

    fn reset(&self, path: &Path, commit_id: &str, mode: ResetMode) -> Result<(), GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let oid = git2::Oid::from_str(commit_id).map_err(|e| GitError::Backend(e.to_string()))?;
        let obj = repo
            .find_object(oid, None)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        let kind = match mode {
            ResetMode::Soft => git2::ResetType::Soft,
            ResetMode::Mixed => git2::ResetType::Mixed,
            ResetMode::Hard => git2::ResetType::Hard,
        };
        repo.reset(&obj, kind, None)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(())
    }

    fn ahead_behind(&self, path: &Path) -> Result<Option<AheadBehind>, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;

        // 当前分支(分离头 / 空仓库 → None)。
        let head = match repo.head() {
            Ok(h) => h,
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Ok(None),
            Err(e) => return Err(GitError::Backend(e.to_string())),
        };
        if !head.is_branch() {
            return Ok(None); // 分离头无上游可比
        }
        let Some(local_oid) = head.target() else {
            return Ok(None);
        };
        let Some(branch_name) = head.shorthand() else {
            return Ok(None);
        };

        // 上游分支(未配置 → None)。
        let local = repo
            .find_branch(branch_name, git2::BranchType::Local)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        let upstream = match local.upstream() {
            Ok(u) => u,
            Err(_) => return Ok(None), // 没设上游
        };
        let Some(up_oid) = upstream.get().target() else {
            return Ok(None);
        };

        let (ahead, behind) = repo
            .graph_ahead_behind(local_oid, up_oid)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(Some(AheadBehind { ahead, behind }))
    }

    fn remotes(&self, path: &Path) -> Result<Vec<String>, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let arr = repo
            .remotes()
            .map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(arr.iter().flatten().map(|s| s.to_string()).collect())
    }

    fn sync_commits(&self, path: &Path) -> Result<SyncCommits, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;

        // 没有任何「实指向」的远程跟踪分支(全新仓库 / 从没 fetch/push 过)时无从对比,
        // 返回空集合 —— 否则会把整段历史都标成「未 push」,满屏空心圈,无意义。
        // origin/HEAD 是符号引用(target() 为 None),自然不计入。
        let has_remote_tracking = repo
            .branches(Some(git2::BranchType::Remote))
            .map_err(|e| GitError::Backend(e.to_string()))?
            .flatten()
            .any(|(b, _)| b.get().target().is_some());
        if !has_remote_tracking {
            return Ok(SyncCommits::default());
        }

        // revwalk:遍历 `include` 可达、`hide` 不可达的提交,收集 SHA。
        // 等价于 `git rev-list <include> --not <hide>`。
        let collect = |include: &str, hide: &str| -> Result<HashSet<String>, GitError> {
            let mut walk = repo
                .revwalk()
                .map_err(|e| GitError::Backend(e.to_string()))?;
            walk.push_glob(include)
                .map_err(|e| GitError::Backend(e.to_string()))?;
            walk.hide_glob(hide)
                .map_err(|e| GitError::Backend(e.to_string()))?;
            let mut set = HashSet::new();
            for oid in walk {
                let oid = oid.map_err(|e| GitError::Backend(e.to_string()))?;
                set.insert(oid.to_string());
            }
            Ok(set)
        };

        Ok(SyncCommits {
            // 本地分支独有 = 已 commit 未 push。
            outgoing: collect("refs/heads/*", "refs/remotes/*")?,
            // 远程跟踪分支独有 = 已 fetch 未 pull/merge。
            incoming: collect("refs/remotes/*", "refs/heads/*")?,
        })
    }

    fn blame(
        &self,
        path: &Path,
        file: &str,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<BlameLine>, GitError> {
        // blame_file 本身较重且不可中断;若到达阻塞线程时已被取代,先在这里放手。
        if cancelled() {
            return Err(GitError::Cancelled);
        }
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let blame = repo
            .blame_file(Path::new(file), None)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        let workdir = repo
            .workdir()
            .ok_or_else(|| GitError::Backend("裸仓库无工作区".into()))?;
        let bytes =
            std::fs::read(workdir.join(file)).map_err(|e| GitError::Backend(e.to_string()))?;
        let text = String::from_utf8_lossy(&bytes);

        let mut out = Vec::new();
        for (i, line) in text.lines().enumerate() {
            // 大文件逐行组装也可能不短:每 512 行检查一次取消。
            if i % 512 == 0 && cancelled() {
                return Err(GitError::Cancelled);
            }
            let lineno = i + 1;
            let (commit_id, short_id, author_name, timestamp) = match blame.get_line(lineno) {
                Some(h) => {
                    let oid = h.final_commit_id();
                    let sig = h.final_signature();
                    if oid.is_zero() {
                        (String::new(), String::new(), "未提交".to_string(), 0)
                    } else {
                        let id = oid.to_string();
                        let short = id.chars().take(7).collect();
                        (
                            id,
                            short,
                            sig.name().unwrap_or("").to_string(),
                            sig.when().seconds(),
                        )
                    }
                }
                None => (String::new(), String::new(), String::new(), 0),
            };
            out.push(BlameLine {
                line_no: lineno as u32,
                commit_id,
                short_id,
                author_name,
                timestamp,
                content: line.to_string(),
            });
        }
        Ok(out)
    }

    fn repo_state(&self, path: &Path) -> Result<RepoState, GitError> {
        use git2::RepositoryState as S;
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        Ok(match repo.state() {
            S::Clean => RepoState::Clean,
            S::Merge => RepoState::Merging,
            S::Rebase | S::RebaseInteractive | S::RebaseMerge => RepoState::Rebasing,
            S::CherryPick | S::CherryPickSequence => RepoState::CherryPicking,
            S::Revert | S::RevertSequence => RepoState::Reverting,
            _ => RepoState::Other,
        })
    }

    fn conflict_sides(&self, path: &Path, file: &str) -> Result<ConflictSides, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let index = repo.index().map_err(|e| GitError::Backend(e.to_string()))?;
        let conflicts = index
            .conflicts()
            .map_err(|e| GitError::Backend(e.to_string()))?;

        // git index 内部统一用正斜杠存路径;Windows 传进来的可能是反斜杠,先归一化。
        let want = file.replace('\\', "/").into_bytes();

        // 把一个 stage 条目的 blob 读成文本(二进制按 lossy 处理);缺失 → None。
        let read_blob = |entry: &Option<git2::IndexEntry>| -> Result<Option<String>, GitError> {
            match entry {
                Some(e) => {
                    let blob = repo
                        .find_blob(e.id)
                        .map_err(|e| GitError::Backend(e.to_string()))?;
                    Ok(Some(String::from_utf8_lossy(blob.content()).into_owned()))
                }
                None => Ok(None),
            }
        };

        for c in conflicts {
            let c = c.map_err(|e| GitError::Backend(e.to_string()))?;
            // 任一 stage 条目都带同一路径;取第一个存在的来匹配。
            let entry_path = c
                .our
                .as_ref()
                .or(c.their.as_ref())
                .or(c.ancestor.as_ref())
                .map(|e| e.path.clone());
            if entry_path.as_deref() == Some(want.as_slice()) {
                return Ok(ConflictSides {
                    base: read_blob(&c.ancestor)?,
                    ours: read_blob(&c.our)?,
                    theirs: read_blob(&c.their)?,
                });
            }
        }
        Err(GitError::Backend(format!("文件无冲突记录: {file}")))
    }

    fn set_upstream(&self, path: &Path, upstream: &str) -> Result<(), GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let head = repo.head().map_err(|_| GitError::NoHead)?;
        if !head.is_branch() {
            return Err(GitError::Backend("当前为分离头,无法设置上游".into()));
        }
        let name = head.shorthand().ok_or(GitError::NoHead)?.to_string();
        let mut branch = repo
            .find_branch(&name, git2::BranchType::Local)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        // upstream 形如 "origin/main";远程跟踪分支不存在时 git2 报错。
        branch
            .set_upstream(Some(upstream))
            .map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(())
    }

    fn checkout_branch(&self, path: &Path, name: &str) -> Result<(), GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let refname = format!("refs/heads/{name}");

        // 目标分支的提交对象(树);不存在 → 友好错误。
        let obj = repo
            .revparse_single(&refname)
            .map_err(|_| GitError::BranchNotFound(name.to_string()))?;

        // ⚠️ 显式 .safe():CheckoutBuilder 默认策略是 NONE(dry-run,不写盘)。
        // safe = 更新可安全覆盖的文件,遇到会丢失本地改动的冲突则报错,绝不强覆盖。
        let mut co = git2::build::CheckoutBuilder::new();
        co.safe();
        repo.checkout_tree(&obj, Some(&mut co)).map_err(|e| {
            // 只认 Conflict code(safe checkout 遇到会丢失本地改动时正是返回它);
            // 不要用 ErrorClass::Checkout 兜底 —— 那会把 IO/损坏等无关错误也误判成
            // 「工作区有改动」,给用户不可恢复却提示去暂存的误导。
            if e.code() == git2::ErrorCode::Conflict {
                GitError::CheckoutConflict
            } else {
                GitError::Backend(e.to_string())
            }
        })?;

        // 工作区/index 已就位后再移动 HEAD 指向该分支。
        repo.set_head(&refname)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(())
    }

    fn create_branch(&self, path: &Path, name: &str) -> Result<(), GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let target = repo
            .head()
            .map_err(|_| GitError::NoHead)?
            .peel_to_commit()
            .map_err(|e| GitError::Backend(e.to_string()))?;
        // force=false:同名已存在则报错,不覆盖。
        match repo.branch(name, &target, false) {
            Ok(_) => Ok(()),
            Err(e) if e.code() == git2::ErrorCode::Exists => {
                Err(GitError::BranchAlreadyExists(name.to_string()))
            }
            Err(e) => Err(GitError::Backend(e.to_string())),
        }
    }

    fn delete_branch(&self, path: &Path, name: &str) -> Result<(), GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        // 当前分支不可删。
        if let Ok(head) = repo.head()
            && head.shorthand() == Some(name)
        {
            return Err(GitError::CannotDeleteCurrentBranch);
        }
        let mut branch = repo
            .find_branch(name, git2::BranchType::Local)
            .map_err(|_| GitError::BranchNotFound(name.to_string()))?;
        branch
            .delete()
            .map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(())
    }

    fn branch_delete_impact(
        &self,
        path: &Path,
        name: &str,
    ) -> Result<BranchDeleteImpact, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let full = format!("refs/heads/{name}");
        let tip = repo
            .find_branch(name, git2::BranchType::Local)
            .map_err(|_| GitError::BranchNotFound(name.to_string()))?
            .get()
            .target()
            .ok_or_else(|| GitError::Backend("分支无目标提交".into()))?;

        // rev-list <name> --not <所有其它引用>:只在该分支、别处不可达的提交 = 删后会丢。
        let mut walk = repo
            .revwalk()
            .map_err(|e| GitError::Backend(e.to_string()))?;
        walk.push(tip)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        // 隐藏除「待删分支」外的每一个引用(本地/远程/标签),annotated tag 用 peel 解到 commit。
        for r in repo
            .references()
            .map_err(|e| GitError::Backend(e.to_string()))?
            .flatten()
        {
            if r.name() == Some(full.as_str()) {
                continue; // 别隐藏自己,否则全被抵消
            }
            if let Ok(commit) = r.peel_to_commit() {
                walk.hide(commit.id())
                    .map_err(|e| GitError::Backend(e.to_string()))?;
            }
        }
        // HEAD(可能是分离头,不在 references() 里)也要隐藏。
        if let Ok(head) = repo.head()
            && let Ok(commit) = head.peel_to_commit()
        {
            walk.hide(commit.id())
                .map_err(|e| GitError::Backend(e.to_string()))?;
        }

        let mut unmerged_commits = 0usize;
        let mut sample_summaries = Vec::new();
        for oid in walk {
            let oid = oid.map_err(|e| GitError::Backend(e.to_string()))?;
            unmerged_commits += 1;
            if sample_summaries.len() < 5
                && let Ok(commit) = repo.find_commit(oid)
            {
                sample_summaries.push(commit.summary().unwrap_or("").to_string());
            }
        }
        Ok(BranchDeleteImpact {
            unmerged_commits,
            sample_summaries,
        })
    }

    fn commit_file_diff(
        &self,
        path: &Path,
        commit_id: &str,
        file: &str,
    ) -> Result<FileDiff, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let oid = git2::Oid::from_str(commit_id).map_err(|e| GitError::Backend(e.to_string()))?;
        let commit = repo
            .find_commit(oid)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        let new_tree = commit
            .tree()
            .map_err(|e| GitError::Backend(e.to_string()))?;
        // 与 commit_files 一致:首提交无父→与空树 diff;合并只跟第一个父。
        let parent_tree = if commit.parent_count() == 0 {
            None
        } else {
            Some(
                commit
                    .parent(0)
                    .map_err(|e| GitError::Backend(e.to_string()))?
                    .tree()
                    .map_err(|e| GitError::Backend(e.to_string()))?,
            )
        };

        // pathspec 限定只 diff 这一个文件,避免整棵树 diff 的开销。
        let mut opts = git2::DiffOptions::new();
        opts.pathspec(file);
        let diff = repo
            .diff_tree_to_tree(parent_tree.as_ref(), Some(&new_tree), Some(&mut opts))
            .map_err(|e| GitError::Backend(e.to_string()))?;

        file_diff_from(&diff, file)
    }

    fn working_diff(&self, path: &Path, file: &str, staged: bool) -> Result<FileDiff, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let mut opts = git2::DiffOptions::new();
        opts.pathspec(file);

        let diff = if staged {
            // 已暂存:HEAD 树 ↔ index。空仓库(无 HEAD)→ 与空树比。
            let head_tree = match repo.head() {
                Ok(h) => Some(
                    h.peel_to_tree()
                        .map_err(|e| GitError::Backend(e.to_string()))?,
                ),
                Err(e) if e.code() == git2::ErrorCode::UnbornBranch => None,
                Err(e) => return Err(GitError::Backend(e.to_string())),
            };
            let index = repo.index().map_err(|e| GitError::Backend(e.to_string()))?;
            repo.diff_tree_to_index(head_tree.as_ref(), Some(&index), Some(&mut opts))
                .map_err(|e| GitError::Backend(e.to_string()))?
        } else {
            // 未暂存:index ↔ 工作区。含未跟踪文件(显示为新增,并带出内容)。
            opts.include_untracked(true)
                .recurse_untracked_dirs(true)
                .show_untracked_content(true);
            repo.diff_index_to_workdir(None, Some(&mut opts))
                .map_err(|e| GitError::Backend(e.to_string()))?
        };

        file_diff_from(&diff, file)
    }

    fn commit(&self, path: &Path, message: &str) -> Result<String, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;

        // 读 git config 的身份,未配置 → 友好错误
        let sig = repo.signature().map_err(|_| GitError::EmptySignature)?;

        let mut index = repo.index().map_err(|e| GitError::Backend(e.to_string()))?;
        let tree_oid = index
            .write_tree()
            .map_err(|e| GitError::Backend(e.to_string()))?;
        let tree = repo
            .find_tree(tree_oid)
            .map_err(|e| GitError::Backend(e.to_string()))?;

        match repo.head() {
            Ok(head_ref) => {
                let parent = head_ref
                    .peel_to_commit()
                    .map_err(|e| GitError::Backend(e.to_string()))?;
                // 没有任何改动(tree 与父提交相同)→ 无可提交
                if parent.tree_id() == tree_oid {
                    return Err(GitError::NothingToCommit);
                }
                let oid = repo
                    .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
                    .map_err(|e| GitError::Backend(e.to_string()))?;
                Ok(oid.to_string())
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
                // 首次提交:index 为空则无可提交,否则空 parents
                if index.is_empty() {
                    return Err(GitError::NothingToCommit);
                }
                let oid = repo
                    .commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
                    .map_err(|e| GitError::Backend(e.to_string()))?;
                Ok(oid.to_string())
            }
            Err(e) => Err(GitError::Backend(e.to_string())),
        }
    }

    fn amend_commit(&self, path: &Path, message: Option<&str>) -> Result<String, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        // 最近一次提交(空仓库无可修订)。
        let last = match repo.head() {
            Ok(h) => h
                .peel_to_commit()
                .map_err(|e| GitError::Backend(e.to_string()))?,
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Err(GitError::NoHead),
            Err(e) => return Err(GitError::Backend(e.to_string())),
        };
        // 用当前暂存区的树替换(允许只改信息:树与原树相同也放行)。
        let mut index = repo.index().map_err(|e| GitError::Backend(e.to_string()))?;
        let tree_oid = index
            .write_tree()
            .map_err(|e| GitError::Backend(e.to_string()))?;
        let tree = repo
            .find_tree(tree_oid)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        // message=None 保留原信息;committer 用 None 保留原 committer(简化)。
        let oid = last
            .amend(Some("HEAD"), None, None, None, message, Some(&tree))
            .map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(oid.to_string())
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
        let backend = Git2Backend;

        backend.stage(&repo, Path::new("a.txt")).unwrap();

        let status = backend.status(&repo).unwrap();
        let entry = status.entries.iter().find(|e| e.path == "a.txt").unwrap();
        assert!(entry.staged, "stage 后应标记 staged");
        assert_eq!(entry.state, FileState::Added);
    }

    #[test]
    fn stage_accepts_absolute_path() {
        // spec 第 7.1:适配器层兜底把绝对路径剥成仓库根相对路径。
        let (_tmp, repo) = init_repo();
        write(&repo, "a.txt", "hello");
        let backend = Git2Backend;

        let abs = repo.join("a.txt");
        assert!(abs.is_absolute());
        backend.stage(&repo, &abs).unwrap();

        let status = backend.status(&repo).unwrap();
        let entry = status.entries.iter().find(|e| e.path == "a.txt").unwrap();
        assert!(entry.staged, "传绝对路径也应能正确暂存");
    }

    #[test]
    fn unstage_with_head_reverts_to_committed() {
        let (_tmp, repo) = init_repo();
        let backend = Git2Backend;
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
        let backend = Git2Backend;
        // 全新仓库,没有任何提交(unborn HEAD)
        write(&repo, "a.txt", "hello");
        backend.stage(&repo, Path::new("a.txt")).unwrap();
        backend.unstage(&repo, Path::new("a.txt")).unwrap();

        let status = backend.status(&repo).unwrap();
        let entry = status.entries.iter().find(|e| e.path == "a.txt").unwrap();
        assert!(!entry.staged, "无 HEAD 时取消暂存应把条目从 index 移除");
        assert_eq!(entry.state, FileState::Untracked);
    }

    #[test]
    fn initial_commit_succeeds_and_status_clean() {
        let (_tmp, repo) = init_repo();
        let backend = Git2Backend;
        write(&repo, "a.txt", "hello");
        backend.stage(&repo, Path::new("a.txt")).unwrap();

        let sha = backend.commit(&repo, "init").unwrap();
        assert_eq!(sha.len(), 40, "返回完整 SHA");

        // 提交后工作区应干净
        let status = backend.status(&repo).unwrap();
        assert!(status.entries.is_empty(), "commit 后 status 应为空");

        // HEAD 现在存在且无父提交(首次提交)
        let g = git2::Repository::open(&repo).unwrap();
        let head = g.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.parent_count(), 0);
    }

    #[test]
    fn second_commit_has_parent() {
        let (_tmp, repo) = init_repo();
        let backend = Git2Backend;
        write(&repo, "a.txt", "v1");
        backend.stage(&repo, Path::new("a.txt")).unwrap();
        backend.commit(&repo, "init").unwrap();

        write(&repo, "a.txt", "v2");
        backend.stage(&repo, Path::new("a.txt")).unwrap();
        backend.commit(&repo, "second").unwrap();

        let g = git2::Repository::open(&repo).unwrap();
        let head = g.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.parent_count(), 1);
    }

    #[test]
    fn commit_nothing_staged_errors() {
        let (_tmp, repo) = init_repo();
        let backend = Git2Backend;
        // 全新仓库,index 为空
        let err = backend.commit(&repo, "empty").unwrap_err();
        assert!(matches!(err, GitError::NothingToCommit));
    }

    #[test]
    fn amend_changes_message_and_includes_staged() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        b.commit(&repo, "original").unwrap();

        // 仅改信息:不新增提交,summary 变化
        b.amend_commit(&repo, Some("amended msg")).unwrap();
        let log = b.log(&repo, 10, 0, &|| false).unwrap();
        assert_eq!(log.len(), 1, "amend 不应新增提交");
        assert_eq!(log[0].summary, "amended msg");

        // 暂存新文件后 amend(None 保留信息):新文件应进入该提交
        stage(&repo, "b.txt", "2");
        b.amend_commit(&repo, None).unwrap();
        let head = &b.log(&repo, 1, 0, &|| false).unwrap()[0];
        assert_eq!(head.summary, "amended msg", "None 应保留上次信息");
        let files = b.commit_files(&repo, &head.id).unwrap();
        assert!(
            files.iter().any(|f| f.path == "b.txt"),
            "amend 应纳入新暂存文件"
        );
    }

    #[test]
    fn amend_on_empty_repo_errors() {
        let (_tmp, repo) = init_repo();
        assert!(matches!(
            Git2Backend.amend_commit(&repo, Some("x")),
            Err(GitError::NoHead)
        ));
    }

    // ⚠️ 显式递增时间戳:避免同秒提交导致 Sort::TIME flaky。
    fn stage(repo_path: &Path, name: &str, contents: &str) {
        let repo = git2::Repository::open(repo_path).unwrap();
        std::fs::write(repo.workdir().unwrap().join(name), contents).unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(Path::new(name)).unwrap();
        idx.write().unwrap();
    }

    fn remove(repo_path: &Path, name: &str) {
        let repo = git2::Repository::open(repo_path).unwrap();
        std::fs::remove_file(repo.workdir().unwrap().join(name)).unwrap();
        let mut idx = repo.index().unwrap();
        idx.remove_path(Path::new(name)).unwrap();
        idx.write().unwrap();
    }

    /// 用显式时间戳把当前 index 提交,返回 SHA。
    fn commit_index(repo_path: &Path, msg: &str, secs: i64) -> String {
        let repo = git2::Repository::open(repo_path).unwrap();
        let mut idx = repo.index().unwrap();
        let tree_oid = idx.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = git2::Signature::new("Test", "t@e.local", &git2::Time::new(secs, 0)).unwrap();
        let parents: Vec<git2::Commit> = match repo.head() {
            Ok(h) => vec![h.peel_to_commit().unwrap()],
            Err(_) => vec![],
        };
        let prefs: Vec<&git2::Commit> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &prefs)
            .unwrap()
            .to_string()
    }

    #[test]
    fn log_returns_commits_time_descending() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);
        stage(&repo, "a.txt", "2");
        commit_index(&repo, "c2", 2000);
        stage(&repo, "b.txt", "x");
        commit_index(&repo, "c3", 3000);
        let log = b.log(&repo, 10, 0, &|| false).unwrap();
        let msgs: Vec<&str> = log.iter().map(|c| c.summary.as_str()).collect();
        assert_eq!(msgs, vec!["c3", "c2", "c1"]);
    }

    #[test]
    fn reflog_records_commits_newest_first() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        let c1 = commit_index(&repo, "c1", 1000);
        stage(&repo, "a.txt", "2");
        let c2 = commit_index(&repo, "c2", 2000);

        let entries = b.reflog(&repo, 10).unwrap();
        assert!(entries.len() >= 2, "两次提交应至少留两条 reflog");
        // 最近(c2)在前。
        assert_eq!(entries[0].selector, "HEAD@{0}");
        assert_eq!(entries[0].new_oid, c2);
        assert!(
            entries[0].message.contains("c2"),
            "首条信息应含提交摘要: {}",
            entries[0].message
        );
        // 找回:某条 reflog 指回 c1(它的 new_oid)。
        assert!(
            entries.iter().any(|e| e.new_oid == c1),
            "reflog 应保留 c1 的 SHA 以供找回"
        );
    }

    #[test]
    fn reflog_empty_repo_is_empty() {
        let (_tmp, repo) = init_repo();
        // 刚 init、无任何提交 → 无 HEAD reflog,应返回空而非报错。
        assert!(Git2Backend.reflog(&repo, 10).unwrap().is_empty());
    }

    /// 校验「撤销」分类器对**真实 git2 reflog 文案**成立——这是唯一靠肉眼看不出、
    /// 必须真机验的假设(reflog message 是否真的形如 "commit: ..." / "reset: ...")。
    #[test]
    fn real_reflog_messages_classify_for_undo() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        let c1 = commit_index(&repo, "c1", 1000);
        stage(&repo, "a.txt", "2");
        commit_index(&repo, "c2", 2000);

        // 顶项是真实的 commit reflog → 分类为「提交」。
        let top = &b.reflog(&repo, 1).unwrap()[0].message;
        assert_eq!(
            git_core::undoable_op_label(top),
            Some("提交"),
            "真实 commit reflog 文案应被识别为可撤销: {top}"
        );

        // 做一次真实 reset,顶项变成 reset reflog → 分类为「重置(reset)」。
        b.reset(&repo, &c1, ResetMode::Soft).unwrap();
        let top = &b.reflog(&repo, 1).unwrap()[0].message;
        assert_eq!(
            git_core::undoable_op_label(top),
            Some("重置(reset)"),
            "真实 reset reflog 文案应被识别为可撤销: {top}"
        );
    }

    #[test]
    fn log_paginates_lazily() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);
        stage(&repo, "a.txt", "2");
        commit_index(&repo, "c2", 2000);
        stage(&repo, "a.txt", "3");
        commit_index(&repo, "c3", 3000);
        let page = b.log(&repo, 1, 1, &|| false).unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].summary, "c2");
    }

    #[test]
    fn log_empty_repo_is_empty() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        assert!(b.log(&repo, 10, 0, &|| false).unwrap().is_empty());
    }

    // ===== Task 3: commit_files =====

    #[test]
    fn commit_files_initial_lists_added() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "l1\nl2\nl3\n");
        let sha = commit_index(&repo, "c1", 1000);
        let files = b.commit_files(&repo, &sha).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "a.txt");
        assert_eq!(files[0].status, FileState::Added);
        // 首提交三行全是新增,无删除
        assert_eq!(files[0].additions, 3);
        assert_eq!(files[0].deletions, 0);
    }

    #[test]
    fn commit_files_reports_add_and_delete_counts() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "a\nb\nc\n");
        commit_index(&repo, "c1", 1000);
        // 删掉 b、新增 d:1 删 1 增(c 行未变)
        stage(&repo, "a.txt", "a\nc\nd\n");
        let sha = commit_index(&repo, "c2", 2000);
        let files = b.commit_files(&repo, &sha).unwrap();
        let f = files.iter().find(|f| f.path == "a.txt").unwrap();
        assert_eq!(f.status, FileState::Modified);
        assert_eq!(f.additions, 1, "新增 d");
        assert_eq!(f.deletions, 1, "删除 b");
    }

    #[test]
    fn commit_files_modify_and_add() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "v1");
        commit_index(&repo, "c1", 1000);
        stage(&repo, "a.txt", "v2");
        stage(&repo, "b.txt", "x");
        let sha = commit_index(&repo, "c2", 2000);
        let files = b.commit_files(&repo, &sha).unwrap();
        let find = |p: &str| files.iter().find(|f| f.path == p).map(|f| f.status);
        assert_eq!(find("a.txt"), Some(FileState::Modified));
        assert_eq!(find("b.txt"), Some(FileState::Added));
    }

    #[test]
    fn commit_files_delete() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "v1");
        commit_index(&repo, "c1", 1000);
        remove(&repo, "a.txt");
        let sha = commit_index(&repo, "c2", 2000);
        let files = b.commit_files(&repo, &sha).unwrap();
        assert_eq!(
            files.iter().find(|f| f.path == "a.txt").unwrap().status,
            FileState::Deleted
        );
    }

    #[test]
    fn commit_files_rename_reported_as_delete_plus_add() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "same");
        commit_index(&repo, "c1", 1000);
        remove(&repo, "a.txt");
        stage(&repo, "c.txt", "same");
        let sha = commit_index(&repo, "c2", 2000);
        let files = b.commit_files(&repo, &sha).unwrap();
        let find = |p: &str| files.iter().find(|f| f.path == p).map(|f| f.status);
        assert_eq!(find("a.txt"), Some(FileState::Deleted));
        assert_eq!(find("c.txt"), Some(FileState::Added));
        assert!(files.iter().all(|f| f.status != FileState::Renamed));
    }

    // ===== Task 4: current_branch =====

    #[test]
    fn current_branch_some_after_commit() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "x");
        commit_index(&repo, "c1", 1000);
        let branch = b.current_branch(&repo).unwrap();
        assert!(branch.is_some());
        assert!(!branch.unwrap().is_empty());
    }

    #[test]
    fn current_branch_none_on_empty_repo() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        assert_eq!(b.current_branch(&repo).unwrap(), None);
    }

    // ===== 1b-2: commit_file_diff(行级 diff) =====

    #[test]
    fn commit_file_diff_initial_all_additions() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "l1\nl2\nl3\n");
        let sha = commit_index(&repo, "c1", 1000);

        let diff = b.commit_file_diff(&repo, &sha, "a.txt").unwrap();

        assert!(!diff.is_binary);
        assert_eq!(diff.path, "a.txt");
        assert!(!diff.hunks.is_empty(), "首提交新增文件应有 hunk");
        let lines: Vec<&DiffLine> = diff.hunks.iter().flat_map(|h| &h.lines).collect();
        // 全是新增行,内容覆盖三行
        assert!(lines.iter().all(|l| l.kind == DiffLineKind::Addition));
        assert!(lines.iter().any(|l| l.content.contains("l1")));
        assert!(lines.iter().any(|l| l.content.contains("l2")));
        assert!(lines.iter().any(|l| l.content.contains("l3")));
        // 新增行没有 old 行号,有 new 行号
        let first = lines[0];
        assert_eq!(first.old_lineno, None);
        assert_eq!(first.new_lineno, Some(1));
    }

    #[test]
    fn commit_file_diff_modify_has_add_and_delete() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "l1\nl2\nl3\n");
        commit_index(&repo, "c1", 1000);
        stage(&repo, "a.txt", "l1\nCHANGED\nl3\nl4\n");
        let sha = commit_index(&repo, "c2", 2000);

        let diff = b.commit_file_diff(&repo, &sha, "a.txt").unwrap();
        let lines: Vec<&DiffLine> = diff.hunks.iter().flat_map(|h| &h.lines).collect();

        // l2 被删除
        assert!(
            lines
                .iter()
                .any(|l| l.kind == DiffLineKind::Deletion && l.content.contains("l2")),
            "应有删除 l2 的行"
        );
        // CHANGED 与 l4 被新增
        assert!(
            lines
                .iter()
                .any(|l| l.kind == DiffLineKind::Addition && l.content.contains("CHANGED"))
        );
        assert!(
            lines
                .iter()
                .any(|l| l.kind == DiffLineKind::Addition && l.content.contains("l4"))
        );
        // hunk 头形如 @@ ... @@
        assert!(diff.hunks[0].header.starts_with("@@"));
    }

    #[test]
    fn commit_file_diff_unchanged_file_is_empty() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "same\n");
        stage(&repo, "b.txt", "x\n");
        commit_index(&repo, "c1", 1000);
        // c2 只改 b.txt,a.txt 不变
        stage(&repo, "b.txt", "y\n");
        let sha = commit_index(&repo, "c2", 2000);

        // 查 a.txt(本提交未改)→ 无 hunk
        let diff = b.commit_file_diff(&repo, &sha, "a.txt").unwrap();
        assert!(diff.hunks.is_empty(), "未改动的文件应无 hunk");
    }

    // ===== 2a: branches / checkout =====

    /// 在当前 HEAD 提交上建一条本地分支(不切过去)。
    fn make_branch(repo_path: &Path, name: &str) {
        let g = git2::Repository::open(repo_path).unwrap();
        let head = g.head().unwrap().peel_to_commit().unwrap();
        g.branch(name, &head, false).unwrap();
    }

    #[test]
    fn branches_lists_local_with_single_head() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);
        make_branch(&repo, "dev");
        make_branch(&repo, "feat/x");

        let list = b.branches(&repo).unwrap();
        let names: Vec<&str> = list.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"dev"));
        assert!(names.contains(&"feat/x"));
        // 升序排列
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        // 恰好一个当前分支
        assert_eq!(list.iter().filter(|x| x.is_head).count(), 1);
    }

    #[test]
    fn refs_reports_local_remote_and_head() {
        let (_tmp, repo) = init_repo();
        stage(&repo, "a.txt", "1");
        let sha = commit_index(&repo, "c1", 1000);

        let g = git2::Repository::open(&repo).unwrap();
        let commit = g.find_commit(git2::Oid::from_str(&sha).unwrap()).unwrap();
        // 本地分支 dev 指向同一提交
        g.branch("dev", &commit, false).unwrap();
        // 手动建一个远程跟踪 ref origin/main 指向同一提交
        g.reference("refs/remotes/origin/main", commit.id(), true, "arrange")
            .unwrap();
        // 轻量标签
        g.reference("refs/tags/v1.0", commit.id(), true, "arrange")
            .unwrap();

        let refs = Git2Backend.refs(&repo).unwrap();
        assert!(
            refs.iter()
                .any(|r| r.kind == RefKind::Tag && r.name == "v1.0"),
            "应含标签 v1.0"
        );
        assert!(
            refs.iter().any(|r| r.kind == RefKind::Head),
            "应含 HEAD 引用"
        );
        assert!(
            refs.iter()
                .any(|r| r.kind == RefKind::LocalBranch && r.name == "dev"),
            "应含本地分支 dev"
        );
        assert!(
            refs.iter()
                .any(|r| r.kind == RefKind::RemoteBranch && r.name == "origin/main"),
            "应含远程跟踪 origin/main"
        );
        // origin/HEAD 这类符号引用不应出现
        assert!(
            !refs.iter().any(|r| r.name.ends_with("/HEAD")),
            "不应包含 origin/HEAD 符号引用"
        );
        // 本测试所有引用都指向同一提交
        assert!(refs.iter().all(|r| r.target == sha), "所有引用应指向 c1");
    }

    #[test]
    fn status_splits_staged_and_unstaged_for_same_file() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "v1\n");
        commit_index(&repo, "c1", 1000);
        // 暂存 v2,再把工作区改成 v3(未暂存)→ 同一文件两侧都有改动
        stage(&repo, "a.txt", "v2\n");
        write(&repo, "a.txt", "v3\n");

        let st = b.status(&repo).unwrap();
        let mine: Vec<_> = st.entries.iter().filter(|e| e.path == "a.txt").collect();
        assert_eq!(mine.len(), 2, "部分暂存的文件应同时出现在暂存与未暂存两侧");
        assert!(mine.iter().any(|e| e.staged), "应有暂存侧条目");
        assert!(mine.iter().any(|e| !e.staged), "应有未暂存侧条目");
    }

    #[test]
    fn working_diff_unstaged_shows_workdir_change() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        // 提交一版,再改工作区但不暂存
        stage(&repo, "a.txt", "line1\n");
        commit_index(&repo, "c1", 1000);
        write(&repo, "a.txt", "line1\nline2\n");

        let diff = b.working_diff(&repo, "a.txt", false).unwrap();
        assert!(!diff.is_binary);
        let added: Vec<_> = diff
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| matches!(l.kind, DiffLineKind::Addition))
            .collect();
        assert!(
            added.iter().any(|l| l.content == "line2"),
            "未暂存 diff 应含新增的 line2"
        );
    }

    #[test]
    fn working_diff_unstaged_includes_untracked() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "x\n");
        commit_index(&repo, "c1", 1000);
        // 新建未跟踪文件
        write(&repo, "new.txt", "hello\n");

        let diff = b.working_diff(&repo, "new.txt", false).unwrap();
        assert!(
            diff.hunks
                .iter()
                .flat_map(|h| &h.lines)
                .any(|l| l.content == "hello"),
            "未跟踪文件也应能看到内容"
        );
    }

    #[test]
    fn working_diff_staged_shows_index_change() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "v1\n");
        commit_index(&repo, "c1", 1000);
        // 改并暂存
        stage(&repo, "a.txt", "v1\nv2\n");

        let diff = b.working_diff(&repo, "a.txt", true).unwrap();
        assert!(
            diff.hunks
                .iter()
                .flat_map(|h| &h.lines)
                .any(|l| matches!(l.kind, DiffLineKind::Addition) && l.content == "v2"),
            "已暂存 diff 应含 v2"
        );
        // 未暂存侧此时应无改动(都已进 index)
        let unstaged = b.working_diff(&repo, "a.txt", false).unwrap();
        assert!(unstaged.hunks.is_empty(), "改动已全部暂存,未暂存侧应为空");
    }

    #[test]
    fn ahead_behind_none_without_upstream() {
        let (_tmp, repo) = init_repo();
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);
        assert!(
            Git2Backend.ahead_behind(&repo).unwrap().is_none(),
            "没有上游应返回 None"
        );
    }

    #[test]
    fn ahead_behind_counts_against_upstream() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        let c1 = commit_index(&repo, "c1", 1000);

        // 配 origin 远程 + 在 C1 处建远程跟踪 ref,并把当前分支上游设过去。
        let g = git2::Repository::open(&repo).unwrap();
        g.remote("origin", "https://example.invalid/r.git").unwrap();
        let head_name = g.head().unwrap().shorthand().unwrap().to_string();
        g.reference(
            &format!("refs/remotes/origin/{head_name}"),
            git2::Oid::from_str(&c1).unwrap(),
            true,
            "arrange",
        )
        .unwrap();
        let mut local = g.find_branch(&head_name, git2::BranchType::Local).unwrap();
        local
            .set_upstream(Some(&format!("origin/{head_name}")))
            .unwrap();

        // 本地再提交一条 → 领先上游 1。
        stage(&repo, "a.txt", "2");
        commit_index(&repo, "c2", 2000);

        let ab = b.ahead_behind(&repo).unwrap().expect("配了上游应有结果");
        assert_eq!(ab.ahead, 1, "本地领先上游 1 个提交");
        assert_eq!(ab.behind, 0, "不落后");
    }

    #[test]
    fn set_upstream_then_ahead_behind_works() {
        let (_tmp, repo) = init_repo();
        stage(&repo, "a.txt", "1");
        let c1 = commit_index(&repo, "c1", 1000);
        let g = git2::Repository::open(&repo).unwrap();
        g.remote("origin", "https://example.invalid/r.git").unwrap();
        let head_name = g.head().unwrap().shorthand().unwrap().to_string();
        g.reference(
            &format!("refs/remotes/origin/{head_name}"),
            git2::Oid::from_str(&c1).unwrap(),
            true,
            "arrange",
        )
        .unwrap();

        Git2Backend
            .set_upstream(&repo, &format!("origin/{head_name}"))
            .unwrap();

        // 设上游后应能算出 ahead/behind(此处都在 c1 → 0/0)
        let ab = Git2Backend.ahead_behind(&repo).unwrap();
        assert!(ab.is_some(), "设上游后应能计算 ahead/behind");
        assert_eq!(ab.unwrap().ahead, 0);
    }

    #[test]
    fn search_commits_matches_message_author_sha() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "fix login bug", 1000);
        stage(&repo, "a.txt", "2");
        let c2 = commit_index(&repo, "add feature", 2000);

        let never = || false;
        // 按 message(大小写不敏感)
        let hits = b.search_commits(&repo, "LOGIN", 10, &never).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].summary, "fix login bug");
        // 按 SHA 前缀
        let by_sha = b.search_commits(&repo, &c2[..6], 10, &never).unwrap();
        assert_eq!(by_sha.len(), 1);
        assert_eq!(by_sha[0].id, c2);
        // 按作者(init_repo 配的是 Test)
        let by_author = b.search_commits(&repo, "test", 10, &never).unwrap();
        assert_eq!(by_author.len(), 2, "两条都由 Test 提交");
        // 空 query → 空
        assert!(
            b.search_commits(&repo, "  ", 10, &never)
                .unwrap()
                .is_empty()
        );
        // 无匹配 → 空
        assert!(
            b.search_commits(&repo, "zzz-nope", 10, &never)
                .unwrap()
                .is_empty()
        );
        // 取消信号 → Cancelled
        let always = || true;
        assert!(matches!(
            b.search_commits(&repo, "login", 10, &always),
            Err(GitError::Cancelled)
        ));
    }

    #[test]
    fn create_and_delete_tag() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        let c1 = commit_index(&repo, "c1", 1000);

        // 轻量标签
        b.create_tag(&repo, "v1.0", &c1, None).unwrap();
        // 附注标签(带信息)
        b.create_tag(&repo, "v1.1", &c1, Some("first release"))
            .unwrap();
        // refs() 应能看到两个 tag,都指向 c1
        let tags: Vec<_> = b
            .refs(&repo)
            .unwrap()
            .into_iter()
            .filter(|r| r.kind == RefKind::Tag)
            .collect();
        assert_eq!(tags.len(), 2);
        assert!(tags.iter().all(|t| t.target == c1));
        // 同名再建 → TagAlreadyExists
        assert!(matches!(
            b.create_tag(&repo, "v1.0", &c1, None),
            Err(GitError::TagAlreadyExists(_))
        ));
        // 删除
        b.delete_tag(&repo, "v1.0").unwrap();
        let left: Vec<_> = b
            .refs(&repo)
            .unwrap()
            .into_iter()
            .filter(|r| r.kind == RefKind::Tag)
            .collect();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].name, "v1.1");
    }

    #[test]
    fn reset_moves_head_and_handles_worktree() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        let c1 = commit_index(&repo, "c1", 1000);
        stage(&repo, "a.txt", "2");
        let c2 = commit_index(&repo, "c2", 2000);

        // soft reset 到 c1:HEAD 回到 c1,但工作区仍是 "2"。
        b.reset(&repo, &c1, ResetMode::Soft).unwrap();
        assert_eq!(
            b.log(&repo, 10, 0, &|| false).unwrap().len(),
            1,
            "soft 后只剩 c1 可达"
        );
        assert_eq!(std::fs::read_to_string(repo.join("a.txt")).unwrap(), "2");

        // 前进回 c2(hard),再 hard reset 到 c1:工作区也被重置成 "1"。
        b.reset(&repo, &c2, ResetMode::Hard).unwrap();
        assert_eq!(std::fs::read_to_string(repo.join("a.txt")).unwrap(), "2");
        b.reset(&repo, &c1, ResetMode::Hard).unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.join("a.txt")).unwrap(),
            "1",
            "hard reset 应丢弃工作区改动,回到 c1 内容"
        );
    }

    #[test]
    fn compare_files_and_diff_between_branches() {
        let repo = tempfile::tempdir().unwrap();
        let g = |args: &[&str]| {
            std::process::Command::new("git")
                .current_dir(repo.path())
                .args(args)
                .output()
                .unwrap();
        };
        g(&["init", "-b", "main", "."]);
        g(&["config", "user.email", "t@e"]);
        g(&["config", "user.name", "t"]);
        std::fs::write(repo.path().join("a.txt"), "base\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-m", "c1"]);
        // feat 分支:改 a.txt + 新增 b.txt
        g(&["checkout", "-b", "feat"]);
        std::fs::write(repo.path().join("a.txt"), "changed\n").unwrap();
        std::fs::write(repo.path().join("b.txt"), "new\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-m", "c2"]);

        let b = Git2Backend;
        // main → feat:a 改了、b 新增
        let files = b.compare_files(repo.path(), "main", "feat").unwrap();
        assert_eq!(files.len(), 2);
        let a = files.iter().find(|f| f.path == "a.txt").unwrap();
        assert_eq!(a.status, FileState::Modified);
        let bf = files.iter().find(|f| f.path == "b.txt").unwrap();
        assert_eq!(bf.status, FileState::Added);
        // 单文件行级 diff(main→feat 的 a.txt:base→changed)
        let diff = b
            .compare_file_diff(repo.path(), "main", "feat", "a.txt")
            .unwrap();
        assert_eq!(diff.path, "a.txt");
        assert!(
            diff.hunks
                .iter()
                .any(|h| h.lines.iter().any(|l| l.content == "changed"))
        );
        // 同一 revision 比较 → 无改动
        assert!(
            b.compare_files(repo.path(), "main", "main")
                .unwrap()
                .is_empty()
        );
        // 无法解析的 revision → 错误
        assert!(b.compare_files(repo.path(), "nope-xyz", "main").is_err());
    }

    #[test]
    fn sync_commits_empty_without_remote_tracking() {
        let (_tmp, repo) = init_repo();
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);
        let sync = Git2Backend.sync_commits(&repo).unwrap();
        assert!(sync.outgoing.is_empty(), "无远程跟踪引用 → 不标未push");
        assert!(sync.incoming.is_empty(), "无远程跟踪引用 → 不标未pull");
    }

    #[test]
    fn sync_commits_splits_outgoing_and_incoming() {
        let (_tmp, repo) = init_repo();
        stage(&repo, "a.txt", "1");
        let c1 = commit_index(&repo, "c1", 1000);
        stage(&repo, "a.txt", "2");
        let c2 = commit_index(&repo, "c2", 2000);

        let g = git2::Repository::open(&repo).unwrap();
        let head_name = g.head().unwrap().shorthand().unwrap().to_string();
        // 远程跟踪 origin/<head> 停在 c1 → 本地的 c2 是「未 push」。
        g.reference(
            &format!("refs/remotes/origin/{head_name}"),
            git2::Oid::from_str(&c1).unwrap(),
            true,
            "arrange",
        )
        .unwrap();
        // 在 c1 上再造一条只存在于远程的提交(不更新 HEAD) → 「未 pull」。
        let c1_commit = g.find_commit(git2::Oid::from_str(&c1).unwrap()).unwrap();
        let sig = git2::Signature::new("R", "r@e.local", &git2::Time::new(1500, 0)).unwrap();
        let incoming = g
            .commit(
                None,
                &sig,
                &sig,
                "remote-only",
                &c1_commit.tree().unwrap(),
                &[&c1_commit],
            )
            .unwrap()
            .to_string();
        g.reference(
            "refs/remotes/origin/feature",
            git2::Oid::from_str(&incoming).unwrap(),
            true,
            "arrange",
        )
        .unwrap();

        let sync = Git2Backend.sync_commits(&repo).unwrap();
        assert!(sync.outgoing.contains(&c2), "c2 本地独有 → 未 push");
        assert!(!sync.outgoing.contains(&c1), "c1 远程也有 → 不算未 push");
        assert!(sync.incoming.contains(&incoming), "远程独有提交 → 未 pull");
        assert!(!sync.incoming.contains(&c2), "c2 不应算未 pull");
    }

    #[test]
    fn blame_attributes_lines_to_commits() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        // c1:两行
        stage(&repo, "f.txt", "alpha\nbeta\n");
        let c1 = commit_index(&repo, "c1", 1000);
        // c2:追加第三行
        stage(&repo, "f.txt", "alpha\nbeta\ngamma\n");
        let c2 = commit_index(&repo, "c2", 2000);

        let lines = b.blame(&repo, "f.txt", &|| false).unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].content, "alpha");
        assert_eq!(lines[0].commit_id, c1, "前两行归 c1");
        assert_eq!(lines[1].commit_id, c1);
        assert_eq!(lines[2].commit_id, c2, "第三行归 c2");
        assert_eq!(lines[2].content, "gamma");
        assert_eq!(lines[0].short_id, c1.chars().take(7).collect::<String>());
    }

    #[test]
    fn log_honors_cancellation() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "f.txt", "x\n");
        commit_index(&repo, "c1", 1000);
        // cancelled 恒真 → 遍历第一条就中断,不白烧 CPU。
        let err = b.log(&repo, 10, 0, &|| true).unwrap_err();
        assert!(matches!(err, GitError::Cancelled));
        // 不取消时正常返回。
        assert_eq!(b.log(&repo, 10, 0, &|| false).unwrap().len(), 1);
    }

    #[test]
    fn blame_honors_cancellation() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "f.txt", "x\n");
        commit_index(&repo, "c1", 1000);
        // 入口即检查:被取代时连昂贵的 blame_file 都不跑。
        let err = b.blame(&repo, "f.txt", &|| true).unwrap_err();
        assert!(matches!(err, GitError::Cancelled));
        assert_eq!(b.blame(&repo, "f.txt", &|| false).unwrap().len(), 1);
    }

    #[test]
    fn repo_state_clean_then_merging_on_conflict() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        assert_eq!(b.repo_state(&repo).unwrap(), RepoState::Clean);

        // 用 CLI 造一个合并冲突
        let g = |args: &[&str]| {
            std::process::Command::new("git")
                .current_dir(&repo)
                .args(args)
                .output()
                .unwrap();
        };
        write(&repo, "f.txt", "base\n");
        g(&["add", "."]);
        g(&["commit", "-m", "c1"]);
        g(&["checkout", "-b", "other"]);
        write(&repo, "f.txt", "other\n");
        g(&["commit", "-am", "cO"]);
        g(&["checkout", "-"]); // 回到初始分支
        write(&repo, "f.txt", "main\n");
        g(&["commit", "-am", "cM"]);
        g(&["merge", "other"]); // 同一行分叉 → 冲突,进入合并中

        assert_eq!(b.repo_state(&repo).unwrap(), RepoState::Merging);
    }

    #[test]
    fn conflict_sides_returns_three_ways() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        let g = |args: &[&str]| {
            std::process::Command::new("git")
                .current_dir(&repo)
                .args(args)
                .output()
                .unwrap();
        };
        write(&repo, "f.txt", "base\n");
        g(&["add", "."]);
        g(&["commit", "-m", "c1"]);
        g(&["checkout", "-b", "other"]);
        write(&repo, "f.txt", "theirs\n");
        g(&["commit", "-am", "cO"]);
        g(&["checkout", "-"]); // 回到初始分支
        write(&repo, "f.txt", "ours\n");
        g(&["commit", "-am", "cM"]);
        g(&["merge", "other"]); // 同行分叉 → 冲突,进入合并中

        let sides = b.conflict_sides(&repo, "f.txt").unwrap();
        assert_eq!(sides.base.as_deref(), Some("base\n"));
        assert_eq!(sides.ours.as_deref(), Some("ours\n"));
        assert_eq!(sides.theirs.as_deref(), Some("theirs\n"));
    }

    #[test]
    fn remotes_lists_configured_remotes() {
        let (_tmp, repo) = init_repo();
        let g = git2::Repository::open(&repo).unwrap();
        g.remote("origin", "https://example.invalid/a.git").unwrap();
        g.remote("upstream", "https://example.invalid/b.git")
            .unwrap();

        let mut names = Git2Backend.remotes(&repo).unwrap();
        names.sort();
        assert_eq!(names, vec!["origin".to_string(), "upstream".to_string()]);
    }

    #[test]
    fn checkout_switches_head() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);
        make_branch(&repo, "dev");

        b.checkout_branch(&repo, "dev").unwrap();
        assert_eq!(b.current_branch(&repo).unwrap(), Some("dev".into()));
    }

    #[test]
    fn checkout_missing_branch_errors() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);

        let err = b.checkout_branch(&repo, "does-not-exist").unwrap_err();
        assert!(matches!(err, GitError::BranchNotFound(_)));
    }

    #[test]
    fn checkout_with_dirty_conflict_errors() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "base\n");
        commit_index(&repo, "c1", 1000);
        let main = b.current_branch(&repo).unwrap().unwrap();

        // dev 上把 a.txt 改成不同内容并提交
        make_branch(&repo, "dev");
        b.checkout_branch(&repo, "dev").unwrap();
        stage(&repo, "a.txt", "dev-version\n");
        commit_index(&repo, "c2", 2000);

        // 回到主分支(a.txt 恢复 base),再制造未提交的本地改动
        b.checkout_branch(&repo, &main).unwrap();
        std::fs::write(repo.join("a.txt"), "dirty-uncommitted\n").unwrap();

        // 切到 dev 会覆盖未提交改动 → 安全 checkout 拒绝
        let err = b.checkout_branch(&repo, "dev").unwrap_err();
        assert!(
            matches!(err, GitError::CheckoutConflict),
            "脏工作区切分支应报 CheckoutConflict,实际: {err:?}"
        );
    }

    #[test]
    fn create_branch_then_listable() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);

        b.create_branch(&repo, "feature/new").unwrap();
        let names: Vec<String> = b
            .branches(&repo)
            .unwrap()
            .into_iter()
            .map(|x| x.name)
            .collect();
        assert!(names.contains(&"feature/new".to_string()));
        // 新建不切换:当前分支不变
        assert_ne!(b.current_branch(&repo).unwrap(), Some("feature/new".into()));
    }

    #[test]
    fn create_branch_duplicate_errors() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);

        b.create_branch(&repo, "dup").unwrap();
        let err = b.create_branch(&repo, "dup").unwrap_err();
        assert!(matches!(err, GitError::BranchAlreadyExists(_)));
    }

    #[test]
    fn delete_branch_removes_it() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);
        b.create_branch(&repo, "tmp").unwrap();

        b.delete_branch(&repo, "tmp").unwrap();
        let names: Vec<String> = b
            .branches(&repo)
            .unwrap()
            .into_iter()
            .map(|x| x.name)
            .collect();
        assert!(!names.contains(&"tmp".to_string()));
    }

    #[test]
    fn branch_delete_impact_counts_only_orphaned_commits() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);
        let base = b.current_branch(&repo).unwrap().unwrap(); // 默认分支名(master/main 不定)

        // feat 上多两个提交 c2、c3(仅 feat 可达)。
        b.create_branch(&repo, "feat").unwrap();
        b.checkout_branch(&repo, "feat").unwrap();
        stage(&repo, "a.txt", "2");
        commit_index(&repo, "c2", 2000);
        stage(&repo, "a.txt", "3");
        commit_index(&repo, "c3", 3000);
        // 回基线分支(删别的分支时 HEAD 必然在别处;且不能删当前分支)。
        b.checkout_branch(&repo, &base).unwrap();

        let impact = b.branch_delete_impact(&repo, "feat").unwrap();
        assert_eq!(impact.unmerged_commits, 2, "c2/c3 只在 feat 上,删后会丢");
        // 摘要按新→旧。
        assert_eq!(impact.sample_summaries, vec!["c3", "c2"]);

        // 在 main 当前位置开个分支:它没有独有提交 → 删除安全(0)。
        b.create_branch(&repo, "atmain").unwrap();
        let safe = b.branch_delete_impact(&repo, "atmain").unwrap();
        assert_eq!(safe.unmerged_commits, 0, "已并入别处的分支删除无损失");
        assert!(safe.sample_summaries.is_empty());
    }

    #[test]
    fn branch_delete_impact_missing_branch_errors() {
        let (_tmp, repo) = init_repo();
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);
        let err = Git2Backend.branch_delete_impact(&repo, "nope").unwrap_err();
        assert!(matches!(err, GitError::BranchNotFound(_)));
    }

    #[test]
    fn delete_current_branch_errors() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);
        let main = b.current_branch(&repo).unwrap().unwrap();

        let err = b.delete_branch(&repo, &main).unwrap_err();
        assert!(matches!(err, GitError::CannotDeleteCurrentBranch));
    }

    #[test]
    fn delete_missing_branch_errors() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1");
        commit_index(&repo, "c1", 1000);

        let err = b.delete_branch(&repo, "ghost").unwrap_err();
        assert!(matches!(err, GitError::BranchNotFound(_)));
    }
}
