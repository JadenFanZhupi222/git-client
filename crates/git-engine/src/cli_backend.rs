use git_core::GitError;
use git_core::model::{FetchOutcome, PullOutcome};
use std::path::Path;
use std::process::Command;

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

    /// 执行 `git -C <repo> pull [remote]`(默认 merge,不 rebase)。
    /// 会改动工作区与当前分支。冲突 → MergeConflict;无上游 → NoUpstream。
    pub fn pull(&self, repo: &Path, remote: Option<&str>) -> Result<PullOutcome, GitError> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo).arg("pull");
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
        let err = if has("conflict") || has("automatic merge failed") {
            // 数有几个文件冲突,给更友好的提示。
            let files = stdout
                .lines()
                .filter(|l| l.trim_start().starts_with("CONFLICT"))
                .count();
            GitError::MergeConflict { files }
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
        CliBackend.pull(b.path(), None).unwrap();
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
        let err = CliBackend.pull(b.path(), None).unwrap_err();
        assert!(
            matches!(err, GitError::MergeConflict { .. }),
            "分叉改动 pull 应报 MergeConflict,实际: {err:?}"
        );
    }
}
