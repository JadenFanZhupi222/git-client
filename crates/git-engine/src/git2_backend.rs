use git_core::model::{
    AheadBehind, BranchInfo, Commit, CommitRef, DiffLine, DiffLineKind, FileChange, FileDiff,
    FileEntry, FileState, Hunk, RefKind, Signature, WorkingTreeStatus,
};
use git_core::{GitBackend, GitError};
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
                    let (hunk, line_count) =
                        patch.hunk(h).map_err(|e| GitError::Backend(e.to_string()))?;
                    let header = String::from_utf8_lossy(hunk.header()).trim_end().to_string();
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

    fn log(&self, path: &Path, limit: usize, skip: usize) -> Result<Vec<Commit>, GitError> {
        let repo =
            git2::Repository::open(path).map_err(|e| GitError::RepoNotFound(e.to_string()))?;

        // ⚠️ 空仓库(unborn HEAD)时 repo.head() 返回 UnbornBranch 错误。
        // 用 repo.head() 提前判断,避免 push_head 在不同 libgit2 版本上返回不一致的错误码。
        match repo.head() {
            Ok(_) => {} // 有 HEAD,继续走 revwalk
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Ok(Vec::new()),
            Err(e) => return Err(GitError::Backend(e.to_string())),
        }

        let mut walk = repo
            .revwalk()
            .map_err(|e| GitError::Backend(e.to_string()))?;
        // ⚠️ set_sorting 必须在 push_head 之前
        walk.set_sorting(git2::Sort::TIME)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        walk.push_head()
            .map_err(|e| GitError::Backend(e.to_string()))?;

        // ⚠️ Revwalk 惰性:直接 skip/take,别先 collect 全部
        let mut out = Vec::new();
        for oid in walk.skip(skip).take(limit) {
            let oid = oid.map_err(|e| GitError::Backend(e.to_string()))?;
            let commit = repo
                .find_commit(oid)
                .map_err(|e| GitError::Backend(e.to_string()))?;
            out.push(build_commit(&commit));
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
        let mut out = Vec::new();
        for delta in diff.deltas() {
            let status = match delta.status() {
                git2::Delta::Added | git2::Delta::Copied => FileState::Added,
                git2::Delta::Deleted => FileState::Deleted,
                // 坑5:不开重命名检测,此分支当前不命中(保留无害)
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
            out.push(FileChange { path: p, status });
        }
        Ok(out)
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
            if let Some(name) = branch.name().map_err(|e| GitError::Backend(e.to_string()))? {
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
                branch.name().map_err(|e| GitError::Backend(e.to_string()))?,
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
                branch.name().map_err(|e| GitError::Backend(e.to_string()))?,
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
                Ok(h) => Some(h.peel_to_tree().map_err(|e| GitError::Backend(e.to_string()))?),
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
        let log = b.log(&repo, 10, 0).unwrap();
        let msgs: Vec<&str> = log.iter().map(|c| c.summary.as_str()).collect();
        assert_eq!(msgs, vec!["c3", "c2", "c1"]);
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
        let page = b.log(&repo, 1, 1).unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].summary, "c2");
    }

    #[test]
    fn log_empty_repo_is_empty() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        assert!(b.log(&repo, 10, 0).unwrap().is_empty());
    }

    // ===== Task 3: commit_files =====

    #[test]
    fn commit_files_initial_lists_added() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "hi");
        let sha = commit_index(&repo, "c1", 1000);
        let files = b.commit_files(&repo, &sha).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "a.txt");
        assert_eq!(files[0].status, FileState::Added);
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

        let refs = Git2Backend.refs(&repo).unwrap();
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
        let mut local = g
            .find_branch(&head_name, git2::BranchType::Local)
            .unwrap();
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
        let names: Vec<String> = b.branches(&repo).unwrap().into_iter().map(|x| x.name).collect();
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
        let names: Vec<String> = b.branches(&repo).unwrap().into_iter().map(|x| x.name).collect();
        assert!(!names.contains(&"tmp".to_string()));
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
