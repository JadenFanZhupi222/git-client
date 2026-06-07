use git_core::GitError;
use git_core::model::{FetchOutcome, PullOutcome, PushOutcome, RepoState, StashEntry};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// 解析一行 `git stash list` 输出,如 "stash@{0}: WIP on main: ...."。
fn parse_stash_line(line: &str) -> Option<StashEntry> {
    let (refpart, msg) = line.split_once(": ")?; // 只切第一个 ": ",message 保留其余冒号
    let index = refpart.split_once('{')?.1.trim_end_matches('}').parse().ok()?;
    Some(StashEntry { index, message: msg.to_string() })
}

/// 从完整 unified diff 文本里抽出第 `index` 个 hunk,拼上文件头形成可单独 apply 的 patch。
/// 文件头 = 第一个 `@@` 之前的所有行(diff --git / index / --- / +++)。
fn extract_hunk_patch(diff: &str, index: usize) -> Option<String> {
    let mut header = String::new();
    let mut hunks: Vec<String> = Vec::new();
    let mut cur: Option<String> = None;
    let mut in_hunks = false;
    for line in diff.lines() {
        if line.starts_with("@@") {
            in_hunks = true;
            if let Some(c) = cur.take() {
                hunks.push(c);
            }
            cur = Some(format!("{line}\n"));
        } else if in_hunks {
            if let Some(c) = cur.as_mut() {
                c.push_str(line);
                c.push('\n');
            }
        } else {
            header.push_str(line);
            header.push('\n');
        }
    }
    if let Some(c) = cur.take() {
        hunks.push(c);
    }
    let hunk = hunks.get(index)?;
    Some(format!("{header}{hunk}"))
}

/// 把 spawn 子进程的 io 错误映射成领域错误:git 不在 PATH → GitCliNotFound。
fn spawn_err(e: std::io::Error) -> GitError {
    if e.kind() == std::io::ErrorKind::NotFound {
        GitError::GitCliNotFound
    } else {
        GitError::Backend(e.to_string())
    }
}

/// 数 git 输出里有几个文件冲突(以 "CONFLICT" 开头的行)。
/// merge/rebase/cherry-pick/revert/pull/stash 冲突提示共用,给前端更友好的文件数。
fn count_conflicts(stdout: &str) -> usize {
    stdout
        .lines()
        .filter(|l| l.trim_start().starts_with("CONFLICT"))
        .count()
}

/// git push 的人类摘要:push 把结果写 stderr,优先取它,空则回落 stdout。
fn push_summary(stdout: &str, stderr: &str) -> String {
    let s = stderr.trim();
    if s.is_empty() {
        stdout.trim().to_string()
    } else {
        s.to_string()
    }
}

/// 把 push 失败的合并输出(小写)归类成精确错误。
fn classify_push_error(combined: &str, stderr: &str) -> GitError {
    let has = |s: &str| combined.contains(s);
    if has("non-fast-forward")
        || has("fetch first")
        || has("updates were rejected")
        || has("[rejected]")
    {
        GitError::PushRejected
    } else if has("authentication failed")
        || has("could not read username")
        || has("permission denied")
    {
        GitError::AuthFailed
    } else if has("could not resolve host") || has("unable to access") || has("timed out") {
        GitError::NetworkError
    } else if has("does not appear to be a git")
        || has("no configured push destination")
        || has("no such remote")
    {
        GitError::NoRemote
    } else {
        GitError::Backend(stderr.trim().to_string())
    }
}

/// 调用系统 git CLI 的后端,专管网络/复杂流程(凭据交给 git 的凭据助手)。
/// ⚠️ 子进程是阻塞的 —— 调用方必须在 spawn_blocking 里使用它。
#[derive(Default)]
pub struct CliBackend;

impl CliBackend {
    /// 执行 `git -C <repo> fetch --prune [remote]`。
    pub fn fetch(&self, repo: &Path, remote: Option<&str>) -> Result<FetchOutcome, GitError> {
        // 先确认仓库配了远程 —— 否则 `git fetch` 可能静默 no-op(exit 0、无输出),
        // 用户点了没反应很困惑。这一步顺带充当「git 是否安装」的探测。
        let remotes = Command::new("git")
            .arg("-C")
            .arg(repo)
            .arg("remote")
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    GitError::GitCliNotFound
                } else {
                    GitError::Backend(e.to_string())
                }
            })?;
        if remotes.status.success()
            && String::from_utf8_lossy(&remotes.stdout).trim().is_empty()
        {
            return Err(GitError::NoRemote);
        }

        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo).arg("fetch").arg("--prune");
        if let Some(r) = remote {
            cmd.arg(r);
        }

        let output = cmd.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GitError::GitCliNotFound
            } else {
                GitError::Backend(e.to_string())
            }
        })?;

        // git fetch 把进度/更新写到 stderr。
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            let summary = stderr.trim();
            return Ok(FetchOutcome {
                remote: remote.unwrap_or("").to_string(),
                summary: if summary.is_empty() {
                    "已是最新".to_string()
                } else {
                    summary.to_string()
                },
            });
        }

        // 非零退出:按 stderr 关键词归类成精确错误。
        let lower = stderr.to_lowercase();
        let has = |s: &str| lower.contains(s);
        let err = if has("authentication failed")
            || has("could not read username")
            || has("permission denied")
        {
            GitError::AuthFailed
        } else if has("could not resolve host") || has("unable to access") || has("timed out") {
            GitError::NetworkError
        } else if has("no remote repository") || has("does not appear to be a git") {
            GitError::NoRemote
        } else {
            GitError::Backend(stderr.trim().to_string())
        };
        Err(err)
    }

    /// 执行 `git -C <repo> pull [--rebase|--no-rebase] [remote]`。
    /// 会改动工作区与当前分支。冲突 → MergeConflict;无上游 → NoUpstream。
    /// rebase 冲突会停在 rebase 中途(留冲突标记),解决/中止 UI 待阶段 4。
    pub fn pull(
        &self,
        repo: &Path,
        remote: Option<&str>,
        rebase: bool,
    ) -> Result<PullOutcome, GitError> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo).arg("pull");
        // 显式指定模式,避免被用户全局 pull.rebase 配置左右。
        cmd.arg(if rebase { "--rebase" } else { "--no-rebase" });
        if let Some(r) = remote {
            cmd.arg(r);
        }

        let output = cmd.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GitError::GitCliNotFound
            } else {
                GitError::Backend(e.to_string())
            }
        })?;

        // pull 的结果走 stdout(Already up to date. / Fast-forward / Merge made…),
        // 进度与错误走 stderr;冲突两边都可能有,合起来判断。
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            let summary = stdout.trim();
            return Ok(PullOutcome {
                summary: if summary.is_empty() {
                    stderr.trim().to_string()
                } else {
                    summary.to_string()
                },
            });
        }

        let combined = format!("{stdout}\n{stderr}").to_lowercase();
        let has = |s: &str| combined.contains(s);
        let err = if has("conflict") || has("automatic merge failed") || has("could not apply") {
            GitError::MergeConflict { files: count_conflicts(&stdout) }
        } else if has("no tracking information") || has("no upstream") {
            GitError::NoUpstream
        } else if has("authentication failed")
            || has("could not read username")
            || has("permission denied")
        {
            GitError::AuthFailed
        } else if has("could not resolve host") || has("unable to access") || has("timed out") {
            GitError::NetworkError
        } else {
            GitError::Backend(stderr.trim().to_string())
        };
        Err(err)
    }

    /// 执行 `git -C <repo> push [remote]`,把当前分支推到远程。
    /// 当前分支无上游时自动 `-u` 建立跟踪后重试一次;
    /// 被拒(non-fast-forward)→ PushRejected。
    pub fn push(&self, repo: &Path, remote: Option<&str>) -> Result<PushOutcome, GitError> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo).arg("push");
        if let Some(r) = remote {
            cmd.arg(r);
        }

        let output = cmd.output().map_err(spawn_err)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            return Ok(PushOutcome {
                summary: push_summary(&stdout, &stderr),
                set_upstream: false,
            });
        }

        // 当前分支无上游 → 自动建立跟踪后重试(对应「点一下就推上去」的预期)。
        let combined = format!("{stdout}\n{stderr}").to_lowercase();
        if combined.contains("has no upstream")
            || combined.contains("no upstream branch")
            || combined.contains("set-upstream")
        {
            return self.push_set_upstream(repo, remote);
        }

        Err(classify_push_error(&combined, &stderr))
    }

    /// 首次 push:`git push -u <remote> <当前分支>`,推送并建立上游跟踪。
    fn push_set_upstream(
        &self,
        repo: &Path,
        remote: Option<&str>,
    ) -> Result<PushOutcome, GitError> {
        // 取当前分支短名;分离头(HEAD)无法自动建上游。
        let head = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .map_err(spawn_err)?;
        let branch = String::from_utf8_lossy(&head.stdout).trim().to_string();
        if branch.is_empty() || branch == "HEAD" {
            return Err(GitError::NoUpstream);
        }
        let remote = remote.unwrap_or("origin");

        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["push", "-u", remote, &branch])
            .output()
            .map_err(spawn_err)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            return Ok(PushOutcome {
                summary: push_summary(&stdout, &stderr),
                set_upstream: true,
            });
        }
        let combined = format!("{stdout}\n{stderr}").to_lowercase();
        Err(classify_push_error(&combined, &stderr))
    }

    /// 暂存某文件第 `hunk_index` 个未暂存 hunk:取 `git diff` 抽出该块,`git apply --cached`。
    pub fn stage_hunk(&self, repo: &Path, file: &str, hunk_index: usize) -> Result<(), GitError> {
        let diff = self.diff_text(repo, file, false)?;
        let patch = extract_hunk_patch(&diff, hunk_index)
            .ok_or_else(|| GitError::Backend(format!("找不到第 {hunk_index} 个改动块")))?;
        apply_cached(repo, &patch, false)
    }

    /// 把一段(已构造好的)patch 应用到 index(`git apply --cached`)。供行级暂存用。
    pub fn apply_cached_patch(&self, repo: &Path, patch: &str) -> Result<(), GitError> {
        apply_cached(repo, patch, false)
    }

    /// 取消暂存某文件第 `hunk_index` 个已暂存 hunk:取 `git diff --cached` 抽出该块,反向 apply。
    pub fn unstage_hunk(&self, repo: &Path, file: &str, hunk_index: usize) -> Result<(), GitError> {
        let diff = self.diff_text(repo, file, true)?;
        let patch = extract_hunk_patch(&diff, hunk_index)
            .ok_or_else(|| GitError::Backend(format!("找不到第 {hunk_index} 个改动块")))?;
        apply_cached(repo, &patch, true)
    }

    /// `git -C repo diff [--cached] -- file` 的文本输出。
    fn diff_text(&self, repo: &Path, file: &str, staged: bool) -> Result<String, GitError> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo).arg("diff");
        if staged {
            cmd.arg("--cached");
        }
        cmd.arg("--").arg(file);
        let out = cmd.output().map_err(spawn_err)?;
        if !out.status.success() {
            return Err(GitError::Backend(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    /// `git stash list` → StashEntry 列表(stash@{0} 在前)。
    pub fn stash_list(&self, repo: &Path) -> Result<Vec<StashEntry>, GitError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["stash", "list"])
            .output()
            .map_err(spawn_err)?;
        if !out.status.success() {
            return Err(GitError::Backend(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(parse_stash_line)
            .collect())
    }

    /// `git stash push [-m msg]`。无改动时 git 不报错、只打印提示 → 转 NothingToStash。
    pub fn stash_save(&self, repo: &Path, message: Option<&str>) -> Result<(), GitError> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo).args(["stash", "push"]);
        if let Some(m) = message.filter(|m| !m.trim().is_empty()) {
            cmd.arg("-m").arg(m);
        }
        let out = cmd.output().map_err(spawn_err)?;
        if !out.status.success() {
            return Err(GitError::Backend(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        if String::from_utf8_lossy(&out.stdout).contains("No local changes to save") {
            return Err(GitError::NothingToStash);
        }
        Ok(())
    }

    pub fn stash_apply(&self, repo: &Path, index: usize) -> Result<(), GitError> {
        self.run_stash_mutation(repo, "apply", index)
    }
    pub fn stash_pop(&self, repo: &Path, index: usize) -> Result<(), GitError> {
        self.run_stash_mutation(repo, "pop", index)
    }
    pub fn stash_drop(&self, repo: &Path, index: usize) -> Result<(), GitError> {
        self.run_stash_mutation(repo, "drop", index)
    }

    /// 整文件采用我方,并 add 标记已解决。
    pub fn resolve_ours(&self, repo: &Path, file: &str) -> Result<(), GitError> {
        self.resolve_side(repo, file, "--ours")
    }
    /// 整文件采用对方,并 add 标记已解决。
    pub fn resolve_theirs(&self, repo: &Path, file: &str) -> Result<(), GitError> {
        self.resolve_side(repo, file, "--theirs")
    }

    fn resolve_side(&self, repo: &Path, file: &str, side: &str) -> Result<(), GitError> {
        let co = Command::new("git")
            .arg("-C")
            .arg(repo)
            .arg("checkout")
            .arg(side)
            .arg("--")
            .arg(file)
            .output()
            .map_err(spawn_err)?;
        if !co.status.success() {
            return Err(GitError::Backend(
                String::from_utf8_lossy(&co.stderr).trim().to_string(),
            ));
        }
        let add = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["add", "--", file])
            .output()
            .map_err(spawn_err)?;
        if !add.status.success() {
            return Err(GitError::Backend(
                String::from_utf8_lossy(&add.stderr).trim().to_string(),
            ));
        }
        Ok(())
    }

    /// 继续进行中的操作(按状态分派)。仍有冲突 → MergeConflict。
    pub fn continue_op(&self, repo: &Path, state: RepoState) -> Result<(), GitError> {
        let args: &[&str] = match state {
            RepoState::Merging => &["commit", "--no-edit"],
            RepoState::Rebasing => &["rebase", "--continue"],
            RepoState::CherryPicking => &["cherry-pick", "--continue"],
            RepoState::Reverting => &["revert", "--continue"],
            RepoState::Clean | RepoState::Other => {
                return Err(GitError::Backend("没有进行中的操作可继续".into()));
            }
        };
        self.run_op(repo, args)
    }

    /// 中止进行中的操作(按状态分派)。
    pub fn abort_op(&self, repo: &Path, state: RepoState) -> Result<(), GitError> {
        let args: &[&str] = match state {
            RepoState::Merging => &["merge", "--abort"],
            RepoState::Rebasing => &["rebase", "--abort"],
            RepoState::CherryPicking => &["cherry-pick", "--abort"],
            RepoState::Reverting => &["revert", "--abort"],
            RepoState::Clean | RepoState::Other => {
                return Err(GitError::Backend("没有进行中的操作可中止".into()));
            }
        };
        self.run_op(repo, args)
    }

    /// 把某提交拣选到当前分支。冲突 → MergeConflict(进入 cherry-pick 中)。
    pub fn cherry_pick(&self, repo: &Path, commit_id: &str) -> Result<(), GitError> {
        self.run_op(repo, &["cherry-pick", commit_id])
    }

    /// 回滚某提交(生成抵消其改动的新提交)。冲突 → MergeConflict(进入 reverting 中)。
    /// 用 `--no-edit` 跳过提交信息编辑器。
    pub fn revert(&self, repo: &Path, commit_id: &str) -> Result<(), GitError> {
        self.run_op(repo, &["revert", "--no-edit", commit_id])
    }

    /// 跑 continue/abort/cherry-pick 命令;GIT_EDITOR=true 防止弹编辑器卡住,冲突归 MergeConflict。
    fn run_op(&self, repo: &Path, args: &[&str]) -> Result<(), GitError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .env("GIT_EDITOR", "true")
            .env("GIT_SEQUENCE_EDITOR", "true")
            .output()
            .map_err(spawn_err)?;
        if out.status.success() {
            return Ok(());
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let combined = format!("{stdout}\n{stderr}").to_lowercase();
        if combined.contains("conflict") || combined.contains("unmerged") || combined.contains("needs merge") {
            return Err(GitError::MergeConflict { files: count_conflicts(&stdout) });
        }
        Err(GitError::Backend(stderr.trim().to_string()))
    }

    /// apply/pop/drop 共用:`git stash <sub> stash@{index}`;apply/pop 冲突 → MergeConflict。
    fn run_stash_mutation(&self, repo: &Path, sub: &str, index: usize) -> Result<(), GitError> {
        let spec = format!("stash@{{{index}}}");
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .arg("stash")
            .arg(sub)
            .arg(&spec)
            .output()
            .map_err(spawn_err)?;
        if out.status.success() {
            return Ok(());
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let combined = format!("{stdout}\n{stderr}").to_lowercase();
        if combined.contains("conflict") {
            return Err(GitError::MergeConflict { files: count_conflicts(&stdout) });
        }
        Err(GitError::Backend(stderr.trim().to_string()))
    }
}

/// 把一段 patch 通过 stdin 喂给 `git apply --cached`(reverse 时加 --reverse)。
fn apply_cached(repo: &Path, patch: &str, reverse: bool) -> Result<(), GitError> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).arg("apply").arg("--cached");
    if reverse {
        cmd.arg("--reverse");
    }
    cmd.arg("-") // 从 stdin 读 patch
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(spawn_err)?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| GitError::Backend("无法写入 git apply stdin".into()))?;
        stdin
            .write_all(patch.as_bytes())
            .map_err(|e| GitError::Backend(e.to_string()))?;
    } // stdin 在此 drop → 关闭,git 收到 EOF
    let out = child
        .wait_with_output()
        .map_err(|e| GitError::Backend(e.to_string()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(GitError::Backend(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 在某目录里跑 git(arrange 用)。被测的是 CliBackend.fetch。
    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "arrange git 失败: {args:?}");
    }

    /// 用 git CLI 读某 ref 的 SHA(避免引入 git2 依赖,保持本模块 feature 无关)。
    fn rev_parse(repo: &Path, spec: &str) -> String {
        let out = Command::new("git")
            .current_dir(repo)
            .args(["rev-parse", spec])
            .output()
            .unwrap();
        assert!(out.status.success(), "rev-parse {spec} 失败");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn fetch_advances_remote_tracking_ref() {
        // 1) bare 仓库当“远程”(本地目录,无网络)
        let remote = tempfile::tempdir().unwrap();
        git(remote.path(), &["init", "--bare", "-b", "main", "."]);
        let remote_url = remote.path().to_str().unwrap();

        // 2) A 克隆远程,提交 c1 并 push → 远程 @ c1
        let a = tempfile::tempdir().unwrap();
        git(a.path(), &["clone", remote_url, "."]);
        git(a.path(), &["config", "user.email", "t@e"]);
        git(a.path(), &["config", "user.name", "t"]);
        std::fs::write(a.path().join("f.txt"), "v1").unwrap();
        git(a.path(), &["add", "."]);
        git(a.path(), &["commit", "-m", "c1"]);
        git(a.path(), &["push", "origin", "main"]);

        // 3) B 克隆远程(此时 origin/main @ c1)
        let b = tempfile::tempdir().unwrap();
        git(b.path(), &["clone", remote_url, "."]);
        let before = rev_parse(b.path(), "origin/main");

        // 4) A 再提交 c2 并 push → 远程前进,B 仍停在 c1
        std::fs::write(a.path().join("f.txt"), "v2").unwrap();
        git(a.path(), &["commit", "-am", "c2"]);
        git(a.path(), &["push", "origin", "main"]);

        // 5) 被测:在 B 上 fetch
        let outcome = CliBackend.fetch(b.path(), None).unwrap();
        let after = rev_parse(b.path(), "origin/main");

        assert_ne!(before, after, "fetch 后 origin/main 应指向新提交");
        assert_eq!(after, rev_parse(a.path(), "HEAD"), "应与远程最新一致");
        let _ = outcome; // summary 内容不强断言(git 版本相关)
    }

    #[test]
    fn fetch_without_remote_errors() {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        let err = CliBackend.fetch(repo.path(), None).unwrap_err();
        assert!(
            matches!(err, GitError::NoRemote),
            "无远程时应报 NoRemote,实际: {err:?}"
        );
    }

    /// clone 一个配好身份的工作仓库。
    fn clone_with_identity(remote_url: &str, dir: &Path) {
        git(dir, &["clone", remote_url, "."]);
        git(dir, &["config", "user.email", "t@e"]);
        git(dir, &["config", "user.name", "t"]);
        // 固定 push.default,避免被测机器的全局配置影响「无上游」判定。
        git(dir, &["config", "push.default", "simple"]);
    }

    #[test]
    fn pull_fast_forwards_local_branch() {
        let remote = tempfile::tempdir().unwrap();
        git(remote.path(), &["init", "--bare", "-b", "main", "."]);
        let url = remote.path().to_str().unwrap();

        // A 建立 c1 并 push
        let a = tempfile::tempdir().unwrap();
        clone_with_identity(url, a.path());
        std::fs::write(a.path().join("f.txt"), "v1").unwrap();
        git(a.path(), &["add", "."]);
        git(a.path(), &["commit", "-m", "c1"]);
        git(a.path(), &["push", "origin", "main"]);

        // B clone(@ c1),A 再推 c2
        let b = tempfile::tempdir().unwrap();
        clone_with_identity(url, b.path());
        std::fs::write(a.path().join("f.txt"), "v2").unwrap();
        git(a.path(), &["commit", "-am", "c2"]);
        git(a.path(), &["push", "origin", "main"]);

        // B pull → 快进到 c2
        CliBackend.pull(b.path(), None, false).unwrap();
        assert_eq!(
            rev_parse(b.path(), "HEAD"),
            rev_parse(a.path(), "HEAD"),
            "pull 后 B 的 HEAD 应快进到远程最新"
        );
        assert_eq!(std::fs::read_to_string(b.path().join("f.txt")).unwrap(), "v2");
    }

    #[test]
    fn pull_with_divergent_change_reports_conflict() {
        let remote = tempfile::tempdir().unwrap();
        git(remote.path(), &["init", "--bare", "-b", "main", "."]);
        let url = remote.path().to_str().unwrap();

        let a = tempfile::tempdir().unwrap();
        clone_with_identity(url, a.path());
        std::fs::write(a.path().join("f.txt"), "base\n").unwrap();
        git(a.path(), &["add", "."]);
        git(a.path(), &["commit", "-m", "c1"]);
        git(a.path(), &["push", "origin", "main"]);

        // B clone,然后两边对同一文件做不同改动
        let b = tempfile::tempdir().unwrap();
        clone_with_identity(url, b.path());

        std::fs::write(a.path().join("f.txt"), "A-side\n").unwrap();
        git(a.path(), &["commit", "-am", "cA"]);
        git(a.path(), &["push", "origin", "main"]);

        std::fs::write(b.path().join("f.txt"), "B-side\n").unwrap();
        git(b.path(), &["commit", "-am", "cB"]);

        // B pull → fetch cA 后与本地 cB 合并,同一文件冲突
        let err = CliBackend.pull(b.path(), None, false).unwrap_err();
        assert!(
            matches!(err, GitError::MergeConflict { .. }),
            "分叉改动 pull 应报 MergeConflict,实际: {err:?}"
        );
    }

    #[test]
    fn pull_rebase_replays_local_commit_linearly() {
        let remote = tempfile::tempdir().unwrap();
        git(remote.path(), &["init", "--bare", "-b", "main", "."]);
        let url = remote.path().to_str().unwrap();

        let a = tempfile::tempdir().unwrap();
        clone_with_identity(url, a.path());
        std::fs::write(a.path().join("f.txt"), "base\n").unwrap();
        git(a.path(), &["add", "."]);
        git(a.path(), &["commit", "-m", "c1"]);
        git(a.path(), &["push", "origin", "main"]);

        let b = tempfile::tempdir().unwrap();
        clone_with_identity(url, b.path());

        // A 推进远程
        std::fs::write(a.path().join("f.txt"), "from-A\n").unwrap();
        git(a.path(), &["commit", "-am", "cA"]);
        git(a.path(), &["push", "origin", "main"]);

        // B 在另一文件提交(不冲突)
        std::fs::write(b.path().join("g.txt"), "from-B\n").unwrap();
        git(b.path(), &["add", "."]);
        git(b.path(), &["commit", "-m", "cB"]);

        // B rebase pull → cB 重放到 cA 之上,线性无 merge
        CliBackend.pull(b.path(), None, true).unwrap();
        // trim_end 容忍 Windows autocrlf 把 \n 换成 \r\n。
        assert_eq!(std::fs::read_to_string(b.path().join("f.txt")).unwrap().trim_end(), "from-A");
        assert_eq!(std::fs::read_to_string(b.path().join("g.txt")).unwrap().trim_end(), "from-B");
        let merges = Command::new("git")
            .current_dir(b.path())
            .args(["rev-list", "--count", "--merges", "HEAD"])
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&merges.stdout).trim(),
            "0",
            "rebase pull 后不应有 merge 提交"
        );
    }

    #[test]
    fn push_uploads_commits_to_remote() {
        let remote = tempfile::tempdir().unwrap();
        git(remote.path(), &["init", "--bare", "-b", "main", "."]);
        let url = remote.path().to_str().unwrap();

        // A 建立上游(push -u),再多提交一条
        let a = tempfile::tempdir().unwrap();
        clone_with_identity(url, a.path());
        std::fs::write(a.path().join("f.txt"), "v1").unwrap();
        git(a.path(), &["add", "."]);
        git(a.path(), &["commit", "-m", "c1"]);
        git(a.path(), &["push", "-u", "origin", "main"]);

        std::fs::write(a.path().join("f.txt"), "v2").unwrap();
        git(a.path(), &["commit", "-am", "c2"]);

        // 被测:已有上游,普通 push
        let outcome = CliBackend.push(a.path(), None).unwrap();
        assert!(!outcome.set_upstream, "已有上游不应再标记 set_upstream");
        assert_eq!(
            rev_parse(remote.path(), "main"),
            rev_parse(a.path(), "HEAD"),
            "push 后远程 main 应与本地 HEAD 一致"
        );
    }

    #[test]
    fn push_sets_upstream_on_first_push() {
        let remote = tempfile::tempdir().unwrap();
        git(remote.path(), &["init", "--bare", "-b", "main", "."]);
        let url = remote.path().to_str().unwrap();

        // 用 init + remote add(不 clone):本地 main 不会自动配置上游,
        // 这样才能真正触发「无上游」分支。(clone 空仓库会自动建跟踪。)
        let a = tempfile::tempdir().unwrap();
        git(a.path(), &["init", "-b", "main", "."]);
        git(a.path(), &["config", "user.email", "t@e"]);
        git(a.path(), &["config", "user.name", "t"]);
        git(a.path(), &["config", "push.default", "simple"]);
        git(a.path(), &["remote", "add", "origin", url]);
        std::fs::write(a.path().join("f.txt"), "v1").unwrap();
        git(a.path(), &["add", "."]);
        git(a.path(), &["commit", "-m", "c1"]);

        // 被测:无上游 → 自动 -u
        let outcome = CliBackend.push(a.path(), None).unwrap();
        assert!(outcome.set_upstream, "首次 push 应自动建立上游");
        assert_eq!(
            rev_parse(remote.path(), "main"),
            rev_parse(a.path(), "HEAD"),
            "首次 push 后远程应有该提交"
        );
        // 上游确实建立了
        let up = Command::new("git")
            .current_dir(a.path())
            .args(["rev-parse", "--abbrev-ref", "main@{u}"])
            .output()
            .unwrap();
        assert!(up.status.success(), "应已配置上游");
        assert_eq!(
            String::from_utf8_lossy(&up.stdout).trim(),
            "origin/main",
            "上游应为 origin/main"
        );
    }

    #[test]
    fn push_rejected_when_remote_ahead() {
        let remote = tempfile::tempdir().unwrap();
        git(remote.path(), &["init", "--bare", "-b", "main", "."]);
        let url = remote.path().to_str().unwrap();

        // A 建立 c1 并推
        let a = tempfile::tempdir().unwrap();
        clone_with_identity(url, a.path());
        std::fs::write(a.path().join("f.txt"), "v1").unwrap();
        git(a.path(), &["add", "."]);
        git(a.path(), &["commit", "-m", "c1"]);
        git(a.path(), &["push", "-u", "origin", "main"]);

        // B clone(@ c1,带上游),A 推进远程到 c2
        let b = tempfile::tempdir().unwrap();
        clone_with_identity(url, b.path());
        std::fs::write(a.path().join("f.txt"), "v2").unwrap();
        git(a.path(), &["commit", "-am", "c2"]);
        git(a.path(), &["push", "origin", "main"]);

        // B 在本地 c1 上另提交 cB,直接 push → 落后于远程,被拒
        std::fs::write(b.path().join("f.txt"), "B-side").unwrap();
        git(b.path(), &["commit", "-am", "cB"]);
        let err = CliBackend.push(b.path(), None).unwrap_err();
        assert!(
            matches!(err, GitError::PushRejected),
            "落后远程的 push 应报 PushRejected,实际: {err:?}"
        );
    }

    /// 跑 git diff [--cached] 取文本(断言用)。
    fn run_diff(repo: &Path, cached: bool) -> String {
        let mut args = vec!["diff"];
        if cached {
            args.push("--cached");
        }
        let out = Command::new("git")
            .current_dir(repo)
            .args(&args)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// 建一个 10 行文件并提交,再改首尾两行 → 形成两个分离 hunk。返回仓库 tempdir。
    fn repo_with_two_hunks() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        git(repo.path(), &["config", "user.email", "t@e"]);
        git(repo.path(), &["config", "user.name", "t"]);
        let base: String = (1..=10).map(|i| format!("line{i}\n")).collect();
        std::fs::write(repo.path().join("f.txt"), &base).unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        // FIRST/LAST 互不为子串,避免断言时 "+LINE1" 误命中 "+LINE10"。
        let modified = base.replace("line1\n", "FIRST\n").replace("line10\n", "LAST\n");
        std::fs::write(repo.path().join("f.txt"), &modified).unwrap();
        repo
    }

    #[test]
    fn stage_hunk_stages_only_that_block() {
        let repo = repo_with_two_hunks();
        // 暂存第 0 个 hunk(首行改动)
        CliBackend.stage_hunk(repo.path(), "f.txt", 0).unwrap();

        let staged = run_diff(repo.path(), true);
        let unstaged = run_diff(repo.path(), false);
        assert!(
            staged.contains("FIRST") && !staged.contains("LAST"),
            "已暂存应只含第一个块,实际:\n{staged}"
        );
        assert!(
            unstaged.contains("LAST") && !unstaged.contains("FIRST"),
            "未暂存应只剩第二个块,实际:\n{unstaged}"
        );
    }

    /// 建一个有一次提交、且工作区有改动的仓库。
    fn repo_with_dirty_worktree() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        git(repo.path(), &["config", "user.email", "t@e"]);
        git(repo.path(), &["config", "user.name", "t"]);
        std::fs::write(repo.path().join("f.txt"), "base\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        std::fs::write(repo.path().join("f.txt"), "changed\n").unwrap();
        repo
    }

    /// 造一个处于合并冲突中的仓库(f.txt 冲突)。
    fn repo_in_merge_conflict() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        git(repo.path(), &["config", "user.email", "t@e"]);
        git(repo.path(), &["config", "user.name", "t"]);
        std::fs::write(repo.path().join("f.txt"), "base\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        git(repo.path(), &["checkout", "-b", "other"]);
        std::fs::write(repo.path().join("f.txt"), "other\n").unwrap();
        git(repo.path(), &["commit", "-am", "cO"]);
        git(repo.path(), &["checkout", "main"]);
        std::fs::write(repo.path().join("f.txt"), "main\n").unwrap();
        git(repo.path(), &["commit", "-am", "cM"]);
        // merge 会因冲突返回非零退出 —— 不能用断言成功的 git() helper。
        let _ = Command::new("git")
            .current_dir(repo.path())
            .args(["merge", "other"])
            .output()
            .unwrap();
        repo
    }

    fn porcelain(repo: &Path) -> String {
        let out = Command::new("git")
            .current_dir(repo)
            .args(["status", "--porcelain"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn cherry_pick_applies_commit() {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        git(repo.path(), &["config", "user.email", "t@e"]);
        git(repo.path(), &["config", "user.name", "t"]);
        std::fs::write(repo.path().join("a.txt"), "base\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        // feature 上加 g.txt
        git(repo.path(), &["checkout", "-b", "feature"]);
        std::fs::write(repo.path().join("g.txt"), "from feature\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c2"]);
        let sha = rev_parse(repo.path(), "HEAD");
        // 回 main 拣选 c2
        git(repo.path(), &["checkout", "main"]);
        CliBackend.cherry_pick(repo.path(), &sha).unwrap();
        assert!(repo.path().join("g.txt").exists(), "拣选后 main 应有 g.txt");
    }

    #[test]
    fn cherry_pick_conflict_reports_mergeconflict() {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        git(repo.path(), &["config", "user.email", "t@e"]);
        git(repo.path(), &["config", "user.name", "t"]);
        std::fs::write(repo.path().join("f.txt"), "base\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        git(repo.path(), &["checkout", "-b", "feature"]);
        std::fs::write(repo.path().join("f.txt"), "feature\n").unwrap();
        git(repo.path(), &["commit", "-am", "cF"]);
        let sha = rev_parse(repo.path(), "HEAD");
        git(repo.path(), &["checkout", "main"]);
        std::fs::write(repo.path().join("f.txt"), "main\n").unwrap();
        git(repo.path(), &["commit", "-am", "cM"]);
        // 拣选 feature 的提交 → 同行冲突
        let err = CliBackend.cherry_pick(repo.path(), &sha).unwrap_err();
        assert!(matches!(err, GitError::MergeConflict { .. }), "实际: {err:?}");
    }

    #[test]
    fn revert_undoes_commit() {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        git(repo.path(), &["config", "user.email", "t@e"]);
        git(repo.path(), &["config", "user.name", "t"]);
        std::fs::write(repo.path().join("a.txt"), "base\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        // c2 加 g.txt
        std::fs::write(repo.path().join("g.txt"), "added\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c2"]);
        let sha = rev_parse(repo.path(), "HEAD");
        // 回滚 c2 → g.txt 应消失,且生成新提交
        CliBackend.revert(repo.path(), &sha).unwrap();
        assert!(!repo.path().join("g.txt").exists(), "回滚后 g.txt 应被移除");
        let log = Command::new("git").current_dir(repo.path())
            .args(["log", "--oneline"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&log.stdout).lines().count(), 3, "应多出一条回滚提交");
    }

    #[test]
    fn revert_conflict_reports_mergeconflict() {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        git(repo.path(), &["config", "user.email", "t@e"]);
        git(repo.path(), &["config", "user.name", "t"]);
        std::fs::write(repo.path().join("f.txt"), "v1\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        let c1 = rev_parse(repo.path(), "HEAD");
        // c2 改同一行;回滚 c1 会与 c2 的改动冲突
        std::fs::write(repo.path().join("f.txt"), "v2\n").unwrap();
        git(repo.path(), &["commit", "-am", "c2"]);
        let err = CliBackend.revert(repo.path(), &c1).unwrap_err();
        assert!(matches!(err, GitError::MergeConflict { .. }), "实际: {err:?}");
    }

    #[test]
    fn resolve_theirs_then_continue_completes_merge() {
        let repo = repo_in_merge_conflict();
        CliBackend.resolve_theirs(repo.path(), "f.txt").unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.path().join("f.txt")).unwrap().trim_end(),
            "other",
            "采用对方后内容应为 other"
        );
        CliBackend.continue_op(repo.path(), RepoState::Merging).unwrap();
        assert!(porcelain(repo.path()).is_empty(), "合并完成后工作区应干净");
    }

    #[test]
    fn resolve_ours_keeps_our_side() {
        let repo = repo_in_merge_conflict();
        CliBackend.resolve_ours(repo.path(), "f.txt").unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.path().join("f.txt")).unwrap().trim_end(),
            "main",
            "采用我方后内容应为 main"
        );
    }

    #[test]
    fn continue_with_unresolved_conflict_errors() {
        let repo = repo_in_merge_conflict();
        let err = CliBackend.continue_op(repo.path(), RepoState::Merging).unwrap_err();
        assert!(matches!(err, GitError::MergeConflict { .. }), "实际: {err:?}");
    }

    #[test]
    fn abort_merge_restores_clean() {
        let repo = repo_in_merge_conflict();
        CliBackend.abort_op(repo.path(), RepoState::Merging).unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.path().join("f.txt")).unwrap().trim_end(),
            "main",
            "中止合并应回到我方版本"
        );
        assert!(porcelain(repo.path()).is_empty(), "中止后工作区应干净");
    }

    #[test]
    fn stash_save_list_pop_roundtrip() {
        let repo = repo_with_dirty_worktree();
        // 贮藏 → 工作区恢复干净,列表有一条
        CliBackend.stash_save(repo.path(), Some("wip")).unwrap();
        assert_eq!(std::fs::read_to_string(repo.path().join("f.txt")).unwrap().trim_end(), "base");
        let list = CliBackend.stash_list(repo.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].index, 0);
        assert!(list[0].message.contains("wip"), "应含自定义信息,实际: {}", list[0].message);

        // 弹出 → 改动回来,列表清空
        CliBackend.stash_pop(repo.path(), 0).unwrap();
        assert_eq!(std::fs::read_to_string(repo.path().join("f.txt")).unwrap().trim_end(), "changed");
        assert!(CliBackend.stash_list(repo.path()).unwrap().is_empty());
    }

    #[test]
    fn stash_save_without_changes_errors() {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        git(repo.path(), &["config", "user.email", "t@e"]);
        git(repo.path(), &["config", "user.name", "t"]);
        std::fs::write(repo.path().join("f.txt"), "x\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        // 工作区干净
        let err = CliBackend.stash_save(repo.path(), None).unwrap_err();
        assert!(matches!(err, GitError::NothingToStash), "实际: {err:?}");
    }

    #[test]
    fn stash_drop_removes_entry() {
        let repo = repo_with_dirty_worktree();
        CliBackend.stash_save(repo.path(), None).unwrap();
        assert_eq!(CliBackend.stash_list(repo.path()).unwrap().len(), 1);
        CliBackend.stash_drop(repo.path(), 0).unwrap();
        assert!(CliBackend.stash_list(repo.path()).unwrap().is_empty());
    }

    #[test]
    fn unstage_hunk_reverts_only_that_block() {
        let repo = repo_with_two_hunks();
        // 先把两处都暂存
        git(repo.path(), &["add", "f.txt"]);
        // 取消暂存第 0 个 hunk(首行改动)
        CliBackend.unstage_hunk(repo.path(), "f.txt", 0).unwrap();

        let staged = run_diff(repo.path(), true);
        let unstaged = run_diff(repo.path(), false);
        assert!(
            staged.contains("LAST") && !staged.contains("FIRST"),
            "已暂存应只剩第二个块,实际:\n{staged}"
        );
        assert!(
            unstaged.contains("FIRST"),
            "撤回的第一个块应回到未暂存,实际:\n{unstaged}"
        );
    }
}
