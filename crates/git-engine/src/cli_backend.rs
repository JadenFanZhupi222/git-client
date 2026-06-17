use git_core::GitError;
use git_core::model::{
    Commit, DiffLine, DiffLineKind, FetchOutcome, FileDiff, Hunk, LineHistoryEntry, MergeOutcome,
    PullOutcome, PushOutcome, RebaseAction, RebaseStep, RepoState, Signature, SignatureInfo,
    SignatureStatus, StashEntry, SubmoduleInfo, SubmoduleStatus, WorktreeInfo,
};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// 进程内单调计数,给临时文件起唯一名(避免并发 rebase 撞名)。
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// 解析一行 `git stash list` 输出,如 "stash@{0}: WIP on main: ...."。
fn parse_stash_line(line: &str) -> Option<StashEntry> {
    let (refpart, msg) = line.split_once(": ")?; // 只切第一个 ": ",message 保留其余冒号
    let index = refpart
        .split_once('{')?
        .1
        .trim_end_matches('}')
        .parse()
        .ok()?;
    Some(StashEntry {
        index,
        message: msg.to_string(),
    })
}

/// 解析一行 `git submodule status` 输出,如:
///   " 1a2b3c... vendor/libfoo (heads/main)"  已同步
///   "-1a2b3c... vendor/libfoo"               未初始化
///   "+1a2b3c... vendor/libfoo (v1.0-2-gxx)"  未同步
///   "U1a2b3c... vendor/libfoo"               冲突
/// 返回 (状态, sha, 路径, 描述)。无法识别行首 → None(防御:跳过该行)。
fn parse_submodule_status_line(line: &str) -> Option<(SubmoduleStatus, String, String, String)> {
    let prefix = line.chars().next()?;
    let status = match prefix {
        '-' => SubmoduleStatus::Uninitialized,
        ' ' => SubmoduleStatus::UpToDate,
        '+' => SubmoduleStatus::Modified,
        'U' => SubmoduleStatus::Conflict,
        _ => return None,
    };
    // 行首是单字节 ascii,按字节切安全。
    let rest = line[1..].trim_start();
    let (sha, after) = rest.split_once(char::is_whitespace)?;
    let after = after.trim();
    // after = "路径" 或 "路径 (描述)";描述在末尾括号里,路径可能含空格,故从右侧 " (" 切。
    let (path, describe) = match (after.ends_with(')'), after.rfind(" (")) {
        (true, Some(idx)) => (
            after[..idx].to_string(),
            after[idx + 2..after.len() - 1].to_string(),
        ),
        _ => (after.to_string(), String::new()),
    };
    if path.is_empty() {
        return None;
    }
    Some((status, sha.to_string(), path, describe))
}

/// 读 `.gitmodules` 得到 子模块路径 → 远程 URL 的映射。文件不存在 / 读失败 → 空表
///(不是错误:子模块的 URL 只是附加信息)。
fn submodule_urls(repo: &Path) -> HashMap<String, String> {
    let gitmodules = repo.join(".gitmodules");
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("config")
        .arg("--file")
        .arg(&gitmodules)
        .args(["-z", "--list"])
        .output();
    let Ok(out) = out else {
        return HashMap::new();
    };
    if !out.status.success() {
        return HashMap::new();
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // `-z --list`:条目以 NUL 分隔,每条 "key\nvalue"。key 形如 submodule.<name>.path / .url。
    let mut name_path: HashMap<String, String> = HashMap::new();
    let mut name_url: HashMap<String, String> = HashMap::new();
    for entry in text.split('\0') {
        if entry.is_empty() {
            continue;
        }
        let Some((key, value)) = entry.split_once('\n') else {
            continue;
        };
        let Some(rest) = key.strip_prefix("submodule.") else {
            continue;
        };
        if let Some(name) = rest.strip_suffix(".path") {
            name_path.insert(name.to_string(), value.to_string());
        } else if let Some(name) = rest.strip_suffix(".url") {
            name_url.insert(name.to_string(), value.to_string());
        }
    }
    // 按 name 关联 path↔url,产出 path→url。
    name_path
        .into_iter()
        .filter_map(|(name, path)| name_url.get(&name).map(|url| (path, url.clone())))
        .collect()
}

/// 解析 `git worktree list --porcelain` 的一条记录(记录间空行分隔)。行形如:
///   worktree <绝对路径> / HEAD <sha> / branch refs/heads/<name> / detached / bare / locked。
/// `is_main` / `is_current` 由调用方填(本函数只解析记录自身字段)。无 `worktree` 行 → None。
fn parse_worktree_record(record: &str) -> Option<WorktreeInfo> {
    let mut wt = WorktreeInfo::default();
    let mut saw_path = false;
    for line in record.lines() {
        let line = line.trim_end();
        if let Some(p) = line.strip_prefix("worktree ") {
            wt.path = p.to_string();
            saw_path = true;
        } else if let Some(sha) = line.strip_prefix("HEAD ") {
            wt.head_sha = sha.to_string();
        } else if let Some(b) = line.strip_prefix("branch ") {
            wt.branch = b.strip_prefix("refs/heads/").unwrap_or(b).to_string();
        } else if line == "detached" {
            wt.detached = true;
        } else if line == "bare" {
            wt.bare = true;
        } else if line == "locked" || line.starts_with("locked ") {
            wt.locked = true;
        }
    }
    saw_path.then_some(wt)
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

/// `git log` 的机器可读 format:字段间用 0x1F(单元分隔)、提交间用 0x1E(记录分隔)。
/// 字段顺序:id / 父(空格分隔) / 作者名 / 作者邮箱 / 作者时间戳 / summary / body。
/// 用不可见分隔符而非换行,使含换行的 body 也不会错位。
const LOG_FORMAT: &str = "%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s%x1f%b%x1e";

/// 解析 `git log --format=LOG_FORMAT` 的 stdout 成 Commit 列表(时间倒序按 git 给的顺序)。
/// 纯函数、无 IO、永不失败:字段缺失取默认值,id 为空的记录跳过。
fn parse_log_records(stdout: &[u8]) -> Vec<Commit> {
    let text = String::from_utf8_lossy(stdout);
    let mut out = Vec::new();
    for record in text.split('\u{1e}') {
        // git 在每条记录后会带一个换行,trim 掉首尾空白(body 尾部空白也无所谓)。
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let mut f = record.split('\u{1f}');
        let id = f.next().unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        let parents = f.next().unwrap_or("");
        let name = f.next().unwrap_or("");
        let email = f.next().unwrap_or("");
        let timestamp = f.next().unwrap_or("").trim().parse().unwrap_or(0);
        let summary = f.next().unwrap_or("").to_string();
        let body = f.next().unwrap_or("").trim().to_string();
        out.push(Commit {
            short_id: id.chars().take(7).collect(),
            id,
            summary,
            body,
            author: Signature {
                name: name.to_string(),
                email: email.to_string(),
            },
            timestamp,
            parents: parents.split_whitespace().map(str::to_string).collect(),
        });
    }
    out
}

/// `git log -L` 的机器可读 format:以 0x1E(记录分隔)起头、字段间 0x1F(单元分隔)。
/// 0x1E 在源码/diff 内容里几乎不可能出现 → 据它切提交块,绕开 marker 撞 diff 行内容。
/// 字段:id / 父 / 作者名 / 邮箱 / 时间戳 / summary。**不含 body**(多行会破坏「首行=元数据」切分)。
const LINE_LOG_FORMAT: &str = "%x1e%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s";

/// 把一段 unified diff 文本解析成 FileDiff(只填 hunks,二进制/LFS 等标志为默认 false)。
/// 给 `git log -L` 的每条 diff 块用;纯函数、容错。`emphasis` 全 None(行历史不做词级)。
fn parse_unified_diff(text: &str) -> FileDiff {
    let mut diff = FileDiff::default();
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("@@") {
            // `@@ -a,b +c,d @@ 可选标题`:取 - 后的 a 作旧起点、+ 后的 c 作新起点。
            old_no = parse_hunk_start(rest, '-').unwrap_or(0);
            new_no = parse_hunk_start(rest, '+').unwrap_or(0);
            diff.hunks.push(Hunk {
                header: line.to_string(),
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = diff.hunks.last_mut() else {
            continue; // 首个 @@ 之前的 `diff --git` / `---` / `+++` 行,跳过
        };
        let mut chars = line.chars();
        let (kind, old_lineno, new_lineno) = match chars.next() {
            Some(' ') => {
                let l = (DiffLineKind::Context, Some(old_no), Some(new_no));
                old_no += 1;
                new_no += 1;
                l
            }
            Some('+') => {
                let l = (DiffLineKind::Addition, None, Some(new_no));
                new_no += 1;
                l
            }
            Some('-') => {
                let l = (DiffLineKind::Deletion, Some(old_no), None);
                old_no += 1;
                l
            }
            // `\ No newline at end of file` 等非内容行:跳过。
            _ => continue,
        };
        hunk.lines.push(DiffLine {
            kind,
            old_lineno,
            new_lineno,
            content: chars.as_str().to_string(),
            emphasis: None,
        });
    }
    diff
}

/// 从 `@@` 之后的串里取某侧(`-` 或 `+`)的起始行号:`-a,b` / `-a` → a。
fn parse_hunk_start(rest: &str, side: char) -> Option<u32> {
    let after = rest.split(side).nth(1)?;
    let digits: String = after
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// 解析 `git log -L --format=LINE_LOG_FORMAT` 的 stdout 成行历史条目。
/// 按 0x1E 切块;每块首行(到首个 \n)按 0x1F 切元数据建 Commit,其后是该提交的 diff 文本。
fn parse_line_log(stdout: &[u8]) -> Vec<LineHistoryEntry> {
    let text = String::from_utf8_lossy(stdout);
    let mut out = Vec::new();
    for block in text.split('\u{1e}') {
        if block.trim().is_empty() {
            continue;
        }
        let (meta, diff_text) = block.split_once('\n').unwrap_or((block, ""));
        let mut f = meta.split('\u{1f}');
        let id = f.next().unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        let parents = f.next().unwrap_or("");
        let name = f.next().unwrap_or("");
        let email = f.next().unwrap_or("");
        let timestamp = f.next().unwrap_or("").trim().parse().unwrap_or(0);
        let summary = f.next().unwrap_or("").to_string();
        out.push(LineHistoryEntry {
            commit: Commit {
                short_id: id.chars().take(7).collect(),
                id,
                summary,
                body: String::new(),
                author: Signature {
                    name: name.to_string(),
                    email: email.to_string(),
                },
                timestamp,
                parents: parents.split_whitespace().map(str::to_string).collect(),
            },
            diff: parse_unified_diff(diff_text),
        });
    }
    out
}

/// 读 HEAD 的完整 SHA(提交/修订成功后取返回值)。
fn head_sha(repo: &Path) -> Result<String, GitError> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(spawn_err)?;
    if !out.status.success() {
        return Err(GitError::Backend("提交后无法读取 HEAD".into()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 把 `git commit` 失败的输出归类成领域错误。hook 拦截/签名失败等落到 Backend(原文),
/// 让用户看到 git 给的真实原因(如 pre-commit hook 的报错)。
fn classify_commit_error(stdout: &[u8], stderr: &[u8]) -> GitError {
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    );
    let low = combined.to_lowercase();
    if low.contains("nothing to commit")
        || low.contains("no changes added")
        || low.contains("nothing added to commit")
    {
        return GitError::NothingToCommit;
    }
    if low.contains("please tell me who you are")
        || low.contains("empty ident")
        || low.contains("user.name")
        || low.contains("user.email")
    {
        return GitError::EmptySignature;
    }
    GitError::Backend(combined.trim().to_string())
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
    /// `git init <path>`:新建空仓库(尊重 init.defaultBranch)。
    /// 父目录不存在则先创建。已是仓库时 git init 幂等。
    pub fn init(&self, path: &Path) -> Result<(), GitError> {
        if let Err(e) = std::fs::create_dir_all(path) {
            return Err(GitError::Backend(format!("创建目录失败: {e}")));
        }
        let output = Command::new("git")
            .arg("init")
            .arg(path)
            .output()
            .map_err(spawn_err)?;
        if output.status.success() {
            return Ok(());
        }
        Err(GitError::Backend(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }

    /// `git clone <url> <dst>`:克隆远程仓库到 dst。dst 须不存在或为空目录。
    /// 认证/网络/无效地址映射成精确错误;凭据交给系统凭据助手。
    pub fn clone_repo(&self, url: &str, dst: &Path) -> Result<(), GitError> {
        let url = url.trim();
        if url.is_empty() {
            return Err(GitError::InvalidUrl);
        }
        // dst 已存在且非空 → 拒绝(git clone 也会拒,这里给精确错误)。
        if dst.exists() {
            let non_empty = std::fs::read_dir(dst)
                .map(|mut it| it.next().is_some())
                .unwrap_or(false);
            if non_empty {
                return Err(GitError::DestinationNotEmpty(dst.display().to_string()));
            }
        }
        let output = Command::new("git")
            .arg("clone")
            .arg(url)
            .arg(dst)
            .output()
            .map_err(spawn_err)?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let lower = stderr.to_lowercase();
        let has = |s: &str| lower.contains(s);
        let err = if has("authentication failed")
            || has("could not read username")
            || has("permission denied")
        {
            GitError::AuthFailed
        } else if has("could not resolve host") || has("unable to access") || has("timed out") {
            GitError::NetworkError
        } else if (has("repository") && has("not found")) || has("does not appear to be a git") {
            GitError::InvalidUrl
        } else if has("already exists and is not an empty") {
            GitError::DestinationNotEmpty(dst.display().to_string())
        } else {
            GitError::Backend(stderr.trim().to_string())
        };
        Err(err)
    }

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
        if remotes.status.success() && String::from_utf8_lossy(&remotes.stdout).trim().is_empty() {
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
            GitError::MergeConflict {
                files: count_conflicts(&stdout),
            }
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

    /// 把某分支合并进当前分支(`git merge --no-edit <name>`)。
    /// 走 CLI 才能跑 hooks + 按配置签名合并提交。冲突 → MergeConflict(进入 merging 中)。
    pub fn merge_branch(&self, repo: &Path, name: &str) -> Result<MergeOutcome, GitError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["merge", "--no-edit", name])
            // 冲突场景 git 可能打开编辑器写合并信息;兜底关掉。
            .env("GIT_EDITOR", "true")
            .output()
            .map_err(spawn_err)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            let summary = stdout.trim();
            let lower = summary.to_lowercase();
            return Ok(MergeOutcome {
                fast_forward: lower.contains("fast-forward"),
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
            GitError::MergeConflict {
                files: count_conflicts(&stdout),
            }
        } else if has("not something we can merge") || has("- not something") {
            // 给的名字不是有效分支/commit-ish。
            GitError::BranchNotFound(name.to_string())
        } else {
            GitError::Backend(stderr.trim().to_string())
        };
        Err(err)
    }

    /// 提交。走 `git commit -m`,**原生跑 pre-commit/commit-msg hooks、并按 commit.gpgsign 签名**
    /// ——这正是相比 git2 直接写提交所修正的正确性硬伤。失败按输出归类。
    pub fn commit(&self, repo: &Path, message: &str) -> Result<String, GitError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["commit", "-m", message])
            .output()
            .map_err(spawn_err)?;
        if out.status.success() {
            return head_sha(repo);
        }
        Err(classify_commit_error(&out.stdout, &out.stderr))
    }

    /// 修订最近一次提交。message=None → `--no-edit` 保留原信息。同样跑 hooks + 签名。
    pub fn amend_commit(&self, repo: &Path, message: Option<&str>) -> Result<String, GitError> {
        // 空仓库无可修订 → NoHead(与 git2 行为一致)。
        let head = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "--verify", "HEAD"])
            .output()
            .map_err(spawn_err)?;
        if !head.status.success() {
            return Err(GitError::NoHead);
        }
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo).args(["commit", "--amend"]);
        match message {
            Some(m) => {
                cmd.args(["-m", m]);
            }
            None => {
                cmd.arg("--no-edit");
            }
        }
        let out = cmd.output().map_err(spawn_err)?;
        if out.status.success() {
            return head_sha(repo);
        }
        Err(classify_commit_error(&out.stdout, &out.stderr))
    }

    /// 读某提交的签名状态:`git show -s --format=%G?<NUL>%GS`。
    /// `%G?` 是状态码,`%GS` 是签名者;用 NUL 分隔避免签名者名里的换行/空格干扰。
    /// 某文件的提交历史:`git log --follow -n<limit> --format=… -- <file>`。
    /// `--follow` 跟随重命名(只能配单个 pathspec,正合文件历史)。
    /// 文件无历史 / 不存在 → git 成功且输出为空 → 空 Vec。
    pub fn file_history(
        &self,
        repo: &Path,
        file: &str,
        limit: usize,
    ) -> Result<Vec<Commit>, GitError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args([
                "log",
                "--follow",
                &format!("-n{limit}"),
                &format!("--format={LOG_FORMAT}"),
                "--",
            ])
            .arg(file)
            .output()
            .map_err(spawn_err)?;
        if !out.status.success() {
            return Err(GitError::Backend(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        Ok(parse_log_records(&out.stdout))
    }

    /// pickaxe:按 diff 内容搜提交。`regex=false` → `git log -S<query>`(出现次数变化);
    /// `regex=true` → `git log -G<query>`(改动行匹配正则)。query 拼进同一个 arg(不经 shell,空格安全)。
    pub fn pickaxe(
        &self,
        repo: &Path,
        query: &str,
        regex: bool,
        limit: usize,
    ) -> Result<Vec<Commit>, GitError> {
        let needle = if regex {
            format!("-G{query}")
        } else {
            format!("-S{query}")
        };
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args([
                "log",
                &needle,
                &format!("-n{limit}"),
                &format!("--format={LOG_FORMAT}"),
            ])
            .output()
            .map_err(spawn_err)?;
        if !out.status.success() {
            return Err(GitError::Backend(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        Ok(parse_log_records(&out.stdout))
    }

    /// 某文件某几行的演变史:`git log -L<start>,<end>:<file> --format=…`。
    /// 每条带该提交对这几行的 diff（仅范围 hunk）。范围无历史 → 空 Vec。
    pub fn line_history(
        &self,
        repo: &Path,
        file: &str,
        start: u32,
        end: u32,
    ) -> Result<Vec<LineHistoryEntry>, GitError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args([
                "log",
                &format!("-L{start},{end}:{file}"),
                &format!("--format={LINE_LOG_FORMAT}"),
            ])
            .output()
            .map_err(spawn_err)?;
        if !out.status.success() {
            return Err(GitError::Backend(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        Ok(parse_line_log(&out.stdout))
    }

    pub fn commit_signature(
        &self,
        repo: &Path,
        commit_id: &str,
    ) -> Result<SignatureInfo, GitError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["show", "-s", "--format=%G?%x00%GS", commit_id])
            .output()
            .map_err(spawn_err)?;
        if !out.status.success() {
            return Err(GitError::Backend(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let line = stdout.lines().next().unwrap_or("");
        let mut parts = line.splitn(2, '\0');
        let code = parts.next().unwrap_or("").trim();
        let signer = parts.next().unwrap_or("").trim();
        let status = match code.chars().next() {
            Some('G') => SignatureStatus::Good,
            Some('B') => SignatureStatus::Bad,
            // U 未知有效性 / X 过期 / Y 密钥过期 / R 吊销 / E 无法校验 → 都归「有签名但未完全可信」
            Some('U') | Some('X') | Some('Y') | Some('R') | Some('E') => {
                SignatureStatus::Unverified
            }
            _ => SignatureStatus::None, // 'N' 或空
        };
        Ok(SignatureInfo {
            status,
            signer: if status == SignatureStatus::None {
                String::new()
            } else {
                signer.to_string()
            },
        })
    }

    /// 列出子模块:`git submodule status` 给状态 + sha + 路径,`.gitmodules` 给 URL。
    /// 无子模块 → 命令成功且输出为空 → 空 Vec。
    pub fn list_submodules(&self, repo: &Path) -> Result<Vec<SubmoduleInfo>, GitError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["submodule", "status"])
            .output()
            .map_err(spawn_err)?;
        if !out.status.success() {
            return Err(GitError::Backend(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        let urls = submodule_urls(repo);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut list = Vec::new();
        for line in stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Some((status, sha, path, describe)) = parse_submodule_status_line(line) {
                let url = urls.get(&path).cloned().unwrap_or_default();
                list.push(SubmoduleInfo {
                    path,
                    url,
                    head_sha: sha,
                    status,
                    describe,
                });
            }
        }
        Ok(list)
    }

    /// 初始化并更新某子模块到超级项目记录的提交。`--init` 让未初始化的也能一步 clone+checkout。
    /// 可能联网(clone),由上层 spawn_blocking 兜住,不阻塞 UI。
    pub fn update_submodule(&self, repo: &Path, path: &str) -> Result<(), GitError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["submodule", "update", "--init", "--"])
            .arg(path)
            .output()
            .map_err(spawn_err)?;
        if out.status.success() {
            return Ok(());
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(GitError::Backend(
            format!("{stdout}\n{stderr}").trim().to_string(),
        ))
    }

    /// 列出工作树:`git worktree list --porcelain`。第一条为主工作树;路径与打开仓库一致
    /// 的那条标 `is_current`(canonicalize 抹平 /tmp↔/private/tmp 等符号链接差异)。
    pub fn list_worktrees(&self, repo: &Path) -> Result<Vec<WorktreeInfo>, GitError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["worktree", "list", "--porcelain"])
            .output()
            .map_err(spawn_err)?;
        if !out.status.success() {
            return Err(GitError::Backend(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let current = std::fs::canonicalize(repo).ok();
        let mut list = Vec::new();
        for record in stdout.split("\n\n") {
            if record.trim().is_empty() {
                continue;
            }
            if let Some(mut wt) = parse_worktree_record(record) {
                wt.is_main = list.is_empty(); // 首条成功解析的记录 = 主工作树
                wt.is_current = match (&current, std::fs::canonicalize(&wt.path).ok()) {
                    (Some(cur), Some(p)) => *cur == p,
                    _ => false,
                };
                list.push(wt);
            }
        }
        Ok(list)
    }

    /// 稀疏检出范围:`git sparse-checkout list`。未开启稀疏检出时该命令非零退出
    /// (`this worktree is not sparse`)—— 这是普通仓库的常态,按「空范围」处理,不当错误。
    pub fn sparse_checkout_patterns(&self, repo: &Path) -> Result<Vec<String>, GitError> {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["sparse-checkout", "list"])
            .output()
            .map_err(spawn_err)?;
        if !out.status.success() {
            return Ok(Vec::new()); // 非稀疏仓库 → 空范围
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    /// 交互式 rebase(全程非交互)。base=最旧提交的父(None→--root);steps 为 oldest→newest。
    ///
    /// 实现:① 把生成的 todo 写临时文件,经 GIT_SEQUENCE_EDITOR=`cp <todo>` 注入;
    /// ② 改信息(reword/squash)用 todo 里的 `exec git commit --amend -F <msg>` 行,不弹编辑器;
    /// ③ GIT_EDITOR=true 兜底。冲突 → MergeConflict(进入 rebasing 中,复用 continue/abort)。
    pub fn interactive_rebase(
        &self,
        repo: &Path,
        base: Option<&str>,
        steps: &[RebaseStep],
    ) -> Result<(), GitError> {
        // 给每个需要新信息的步骤写一个 msg 临时文件,并生成 todo 行。
        let mut tmp_files: Vec<PathBuf> = Vec::new();
        let mut write_msg = |content: &str| -> Result<String, GitError> {
            let seq = TEMP_SEQ.fetch_add(1, Ordering::SeqCst);
            let p = std::env::temp_dir().join(format!(
                "git-client-rebase-msg-{}-{}.txt",
                std::process::id(),
                seq
            ));
            std::fs::write(&p, content).map_err(|e| GitError::Backend(e.to_string()))?;
            let fwd = p.to_string_lossy().replace('\\', "/");
            tmp_files.push(p);
            Ok(fwd)
        };

        let mut todo = String::new();
        for step in steps {
            match &step.action {
                RebaseAction::Pick => {
                    todo.push_str(&format!("pick {}\n", step.sha));
                }
                RebaseAction::Reword(msg) => {
                    let f = write_msg(msg)?;
                    todo.push_str(&format!("pick {}\n", step.sha));
                    todo.push_str(&format!("exec git commit --amend -F \"{f}\"\n"));
                }
                RebaseAction::Fixup => {
                    todo.push_str(&format!("fixup {}\n", step.sha));
                }
                RebaseAction::Squash(msg) => {
                    let f = write_msg(msg)?;
                    // fixup 并入前一个(丢本信息),再 exec 把合并后的信息设为我们准备的内容。
                    todo.push_str(&format!("fixup {}\n", step.sha));
                    todo.push_str(&format!("exec git commit --amend -F \"{f}\"\n"));
                }
                RebaseAction::Drop => { /* 不输出该行 = 丢弃 */ }
            }
        }

        // 写 todo 临时文件。
        let seq = TEMP_SEQ.fetch_add(1, Ordering::SeqCst);
        let todo_path = std::env::temp_dir().join(format!(
            "git-client-rebase-todo-{}-{}.txt",
            std::process::id(),
            seq
        ));
        std::fs::write(&todo_path, &todo).map_err(|e| GitError::Backend(e.to_string()))?;
        tmp_files.push(todo_path.clone());
        let todo_fwd = todo_path.to_string_lossy().replace('\\', "/");

        // GIT_SEQUENCE_EDITOR 把我们的 todo 覆盖进去;git 会在命令后追加 todo 文件路径,
        // 故等价于 `cp "<我们的 todo>" <git-rebase-todo>`(Git 自带 sh/cp)。
        let seq_editor = format!("cp \"{todo_fwd}\"");

        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo).arg("rebase").arg("-i");
        match base {
            Some(b) if !b.trim().is_empty() => {
                cmd.arg(b);
            }
            _ => {
                cmd.arg("--root");
            }
        }
        let out = cmd
            .env("GIT_SEQUENCE_EDITOR", &seq_editor)
            .env("GIT_EDITOR", "true")
            .output()
            .map_err(spawn_err);

        // 尽力清理临时文件(失败无害)。
        for p in &tmp_files {
            let _ = std::fs::remove_file(p);
        }

        let out = out?;
        if out.status.success() {
            return Ok(());
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let combined = format!("{stdout}\n{stderr}").to_lowercase();
        if combined.contains("conflict")
            || combined.contains("could not apply")
            || combined.contains("needs merge")
        {
            return Err(GitError::MergeConflict {
                files: count_conflicts(&stdout),
            });
        }
        Err(GitError::Backend(stderr.trim().to_string()))
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
        if combined.contains("conflict")
            || combined.contains("unmerged")
            || combined.contains("needs merge")
        {
            return Err(GitError::MergeConflict {
                files: count_conflicts(&stdout),
            });
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
            return Err(GitError::MergeConflict {
                files: count_conflicts(&stdout),
            });
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
    fn commit_signature_unsigned_is_none() {
        // 未签名提交:%G? = N → SignatureStatus::None,签名者空。
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        git(repo.path(), &["config", "user.email", "t@e"]);
        git(repo.path(), &["config", "user.name", "t"]);
        git(repo.path(), &["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.path().join("f.txt"), "x").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        let sha = rev_parse(repo.path(), "HEAD");

        let sig = CliBackend.commit_signature(repo.path(), &sha).unwrap();
        assert_eq!(sig.status, SignatureStatus::None);
        assert!(sig.signer.is_empty());
    }

    /// 建一个配好身份、关签名的空仓库(提交相关测试共用)。
    fn init_repo_for_commit() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        git(repo.path(), &["config", "user.email", "t@e"]);
        git(repo.path(), &["config", "user.name", "t"]);
        git(repo.path(), &["config", "commit.gpgsign", "false"]);
        repo
    }

    #[test]
    fn init_creates_repo() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("fresh");
        CliBackend.init(&target).unwrap();
        assert!(target.join(".git").exists(), "init 应建出 .git");
    }

    #[test]
    fn clone_from_local_source_succeeds() {
        // 源仓库(有一次提交)
        let src = init_repo_for_commit();
        std::fs::write(src.path().join("a.txt"), "x").unwrap();
        git(src.path(), &["add", "."]);
        git(src.path(), &["commit", "-m", "c1"]);

        let work = tempfile::tempdir().unwrap();
        let dst = work.path().join("cloned");
        // 本地路径当 URL,免网络。
        CliBackend
            .clone_repo(&src.path().to_string_lossy(), &dst)
            .unwrap();
        assert!(dst.join(".git").exists(), "clone 应建出工作区");
        assert!(dst.join("a.txt").exists(), "clone 应检出文件");
    }

    #[test]
    fn clone_into_non_empty_dst_rejected() {
        let work = tempfile::tempdir().unwrap();
        let dst = work.path().join("occupied");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(dst.join("keep.txt"), "x").unwrap();

        let err = CliBackend
            .clone_repo("https://example.com/x.git", &dst)
            .unwrap_err();
        assert!(
            matches!(err, GitError::DestinationNotEmpty(_)),
            "实际:{err:?}"
        );
    }

    #[test]
    fn clone_empty_url_is_invalid() {
        let work = tempfile::tempdir().unwrap();
        let err = CliBackend
            .clone_repo("  ", &work.path().join("x"))
            .unwrap_err();
        assert!(matches!(err, GitError::InvalidUrl), "实际:{err:?}");
    }

    #[test]
    fn merge_fast_forward_succeeds() {
        let repo = init_repo_for_commit();
        let p = repo.path();
        std::fs::write(p.join("a.txt"), "1\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c1"]);
        // feature 领先 main 一个提交;main 不动 → 合并应快进。
        git(p, &["checkout", "-b", "feature"]);
        std::fs::write(p.join("a.txt"), "1\n2\n").unwrap();
        git(p, &["commit", "-am", "c2"]);
        git(p, &["checkout", "main"]);

        let out = CliBackend.merge_branch(p, "feature").unwrap();
        assert!(out.fast_forward, "应为快进:{}", out.summary);
        // main 现已含 feature 的提交
        assert_eq!(rev_parse(p, "main"), rev_parse(p, "feature"));
    }

    #[test]
    fn merge_conflict_returns_merge_conflict() {
        let repo = init_repo_for_commit();
        let p = repo.path();
        std::fs::write(p.join("a.txt"), "base\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c1"]);
        // 两分支改同一行 → 必冲突。
        git(p, &["checkout", "-b", "feature"]);
        std::fs::write(p.join("a.txt"), "feature\n").unwrap();
        git(p, &["commit", "-am", "f"]);
        git(p, &["checkout", "main"]);
        std::fs::write(p.join("a.txt"), "main\n").unwrap();
        git(p, &["commit", "-am", "m"]);

        let err = CliBackend.merge_branch(p, "feature").unwrap_err();
        assert!(
            matches!(err, GitError::MergeConflict { files } if files >= 1),
            "应为冲突,实际:{err:?}"
        );
    }

    #[test]
    fn merge_unknown_branch_errors() {
        let repo = init_repo_for_commit();
        let p = repo.path();
        std::fs::write(p.join("a.txt"), "x\n").unwrap();
        git(p, &["add", "."]);
        git(p, &["commit", "-m", "c1"]);

        let err = CliBackend.merge_branch(p, "nope").unwrap_err();
        assert!(matches!(err, GitError::BranchNotFound(_)), "实际:{err:?}");
    }

    #[test]
    fn commit_creates_then_nothing_to_commit() {
        let repo = init_repo_for_commit();
        std::fs::write(repo.path().join("f.txt"), "x").unwrap();
        git(repo.path(), &["add", "."]);

        let sha = CliBackend.commit(repo.path(), "c1").unwrap();
        assert_eq!(sha.len(), 40, "应返回完整 SHA");
        assert_eq!(sha, rev_parse(repo.path(), "HEAD"));

        // 没有新改动 → NothingToCommit
        assert!(matches!(
            CliBackend.commit(repo.path(), "c2").unwrap_err(),
            GitError::NothingToCommit
        ));
    }

    #[test]
    fn amend_changes_message() {
        let repo = init_repo_for_commit();
        std::fs::write(repo.path().join("f.txt"), "x").unwrap();
        git(repo.path(), &["add", "."]);
        CliBackend.commit(repo.path(), "orig").unwrap();

        CliBackend
            .amend_commit(repo.path(), Some("amended"))
            .unwrap();
        let msg = {
            let out = Command::new("git")
                .current_dir(repo.path())
                .args(["log", "-1", "--format=%s"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(msg, "amended");
    }

    #[test]
    fn commit_respects_pre_commit_hook() {
        // M4.3 的核心:commit 走 CLI 后会跑 hooks——失败的 pre-commit 应拦下提交。
        let repo = init_repo_for_commit();
        let hook = repo.path().join(".git/hooks/pre-commit");
        std::fs::write(&hook, "#!/bin/sh\necho blocked-by-hook 1>&2\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(repo.path().join("f.txt"), "x").unwrap();
        git(repo.path(), &["add", "."]);

        let err = CliBackend
            .commit(repo.path(), "should-be-blocked")
            .unwrap_err();
        assert!(
            matches!(err, GitError::Backend(_)),
            "被 hook 拦截应为 Backend 错误"
        );
        // 提交未发生:HEAD 仍未生(空仓库)。
        let head = Command::new("git")
            .current_dir(repo.path())
            .args(["rev-parse", "--verify", "HEAD"])
            .output()
            .unwrap();
        assert!(!head.status.success(), "hook 拦截后不应产生提交");
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
        assert_eq!(
            std::fs::read_to_string(b.path().join("f.txt")).unwrap(),
            "v2"
        );
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
        assert_eq!(
            std::fs::read_to_string(b.path().join("f.txt"))
                .unwrap()
                .trim_end(),
            "from-A"
        );
        assert_eq!(
            std::fs::read_to_string(b.path().join("g.txt"))
                .unwrap()
                .trim_end(),
            "from-B"
        );
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
        let modified = base
            .replace("line1\n", "FIRST\n")
            .replace("line10\n", "LAST\n");
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
        assert!(
            matches!(err, GitError::MergeConflict { .. }),
            "实际: {err:?}"
        );
    }

    /// 建一个 3 提交、各改不同文件的仓库,返回 (tmp, c1, c2, c3 的 SHA)。
    fn repo_three_commits() -> (tempfile::TempDir, String, String, String) {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        git(repo.path(), &["config", "user.email", "t@e"]);
        git(repo.path(), &["config", "user.name", "t"]);
        std::fs::write(repo.path().join("a.txt"), "1\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        let c1 = rev_parse(repo.path(), "HEAD");
        std::fs::write(repo.path().join("b.txt"), "2\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c2"]);
        let c2 = rev_parse(repo.path(), "HEAD");
        std::fs::write(repo.path().join("c.txt"), "3\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c3"]);
        let c3 = rev_parse(repo.path(), "HEAD");
        (repo, c1, c2, c3)
    }

    fn log_subjects(repo: &Path) -> Vec<String> {
        let out = Command::new("git")
            .current_dir(repo)
            .args(["log", "--format=%s"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn interactive_rebase_reword_and_fixup() {
        let (repo, c1, c2, c3) = repo_three_commits();
        // base=c1;c2 改信息,c3 并入 c2(fixup)
        let steps = vec![
            RebaseStep {
                sha: c2.clone(),
                action: RebaseAction::Reword("c2 reworded".into()),
            },
            RebaseStep {
                sha: c3.clone(),
                action: RebaseAction::Fixup,
            },
        ];
        CliBackend
            .interactive_rebase(repo.path(), Some(&c1), &steps)
            .unwrap();
        // 结果:c1 + 1 个合并提交(信息 = c2 reworded),且含 b.txt 与 c.txt
        let subs = log_subjects(repo.path());
        assert_eq!(subs, vec!["c2 reworded".to_string(), "c1".to_string()]);
        assert!(repo.path().join("b.txt").exists());
        assert!(repo.path().join("c.txt").exists(), "fixup 应保留 c3 的改动");
    }

    #[test]
    fn interactive_rebase_drop_and_reorder() {
        let (repo, c1, c2, c3) = repo_three_commits();
        // base=c1;丢弃 c2、保留 c3 → 历史变 c1, c3(b.txt 消失)
        let steps = vec![
            RebaseStep {
                sha: c2.clone(),
                action: RebaseAction::Drop,
            },
            RebaseStep {
                sha: c3.clone(),
                action: RebaseAction::Pick,
            },
        ];
        CliBackend
            .interactive_rebase(repo.path(), Some(&c1), &steps)
            .unwrap();
        let subs = log_subjects(repo.path());
        assert_eq!(subs, vec!["c3".to_string(), "c1".to_string()]);
        assert!(
            !repo.path().join("b.txt").exists(),
            "c2 被 drop,b.txt 应消失"
        );
        assert!(repo.path().join("c.txt").exists());
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
        let log = Command::new("git")
            .current_dir(repo.path())
            .args(["log", "--oneline"])
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&log.stdout).lines().count(),
            3,
            "应多出一条回滚提交"
        );
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
        assert!(
            matches!(err, GitError::MergeConflict { .. }),
            "实际: {err:?}"
        );
    }

    #[test]
    fn resolve_theirs_then_continue_completes_merge() {
        let repo = repo_in_merge_conflict();
        CliBackend.resolve_theirs(repo.path(), "f.txt").unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.path().join("f.txt"))
                .unwrap()
                .trim_end(),
            "other",
            "采用对方后内容应为 other"
        );
        CliBackend
            .continue_op(repo.path(), RepoState::Merging)
            .unwrap();
        assert!(porcelain(repo.path()).is_empty(), "合并完成后工作区应干净");
    }

    #[test]
    fn resolve_ours_keeps_our_side() {
        let repo = repo_in_merge_conflict();
        CliBackend.resolve_ours(repo.path(), "f.txt").unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.path().join("f.txt"))
                .unwrap()
                .trim_end(),
            "main",
            "采用我方后内容应为 main"
        );
    }

    #[test]
    fn continue_with_unresolved_conflict_errors() {
        let repo = repo_in_merge_conflict();
        let err = CliBackend
            .continue_op(repo.path(), RepoState::Merging)
            .unwrap_err();
        assert!(
            matches!(err, GitError::MergeConflict { .. }),
            "实际: {err:?}"
        );
    }

    #[test]
    fn abort_merge_restores_clean() {
        let repo = repo_in_merge_conflict();
        CliBackend
            .abort_op(repo.path(), RepoState::Merging)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.path().join("f.txt"))
                .unwrap()
                .trim_end(),
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
        assert_eq!(
            std::fs::read_to_string(repo.path().join("f.txt"))
                .unwrap()
                .trim_end(),
            "base"
        );
        let list = CliBackend.stash_list(repo.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].index, 0);
        assert!(
            list[0].message.contains("wip"),
            "应含自定义信息,实际: {}",
            list[0].message
        );

        // 弹出 → 改动回来,列表清空
        CliBackend.stash_pop(repo.path(), 0).unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.path().join("f.txt"))
                .unwrap()
                .trim_end(),
            "changed"
        );
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

    // ---- 子模块(M4.4)----

    #[test]
    fn parse_submodule_status_line_variants() {
        // 各行首字符 → 状态;描述(末尾括号)正确剥离;路径含空格也能切对。
        let up = parse_submodule_status_line(
            " 1111111111111111111111111111111111111111 vendor/foo (heads/main)",
        )
        .unwrap();
        assert_eq!(up.0, SubmoduleStatus::UpToDate);
        assert_eq!(up.1, "1111111111111111111111111111111111111111");
        assert_eq!(up.2, "vendor/foo");
        assert_eq!(up.3, "heads/main");

        let uninit =
            parse_submodule_status_line("-2222222222222222222222222222222222222222 libs/bar")
                .unwrap();
        assert_eq!(uninit.0, SubmoduleStatus::Uninitialized);
        assert_eq!(uninit.2, "libs/bar");
        assert!(uninit.3.is_empty(), "无括号 → 描述为空");

        let modified = parse_submodule_status_line(
            "+3333333333333333333333333333333333333333 my sub dir (v1.0-2-gabc)",
        )
        .unwrap();
        assert_eq!(modified.0, SubmoduleStatus::Modified);
        assert_eq!(modified.2, "my sub dir", "路径含空格应原样保留");
        assert_eq!(modified.3, "v1.0-2-gabc");

        let conflict =
            parse_submodule_status_line("U4444444444444444444444444444444444444444 c").unwrap();
        assert_eq!(conflict.0, SubmoduleStatus::Conflict);

        // 无法识别的行首 → None(防御)。
        assert!(parse_submodule_status_line("garbage").is_none());
        assert!(parse_submodule_status_line("").is_none());
    }

    /// 建一个含一个子模块的超级项目,返回 (超级项目, 上游, 子模块路径名)。
    /// 用 file 协议加子模块,故需 `protocol.file.allow=always`(现代 git 默认禁)。
    fn super_with_submodule() -> (tempfile::TempDir, tempfile::TempDir) {
        // 上游(子模块来源):一次提交。
        let upstream = tempfile::tempdir().unwrap();
        git(upstream.path(), &["init", "-b", "main", "."]);
        git(upstream.path(), &["config", "user.email", "t@e"]);
        git(upstream.path(), &["config", "user.name", "t"]);
        std::fs::write(upstream.path().join("lib.txt"), "hello").unwrap();
        git(upstream.path(), &["add", "."]);
        git(upstream.path(), &["commit", "-m", "lib c1"]);

        // 超级项目:配好身份,加上游为子模块 sub,再提交。`-c protocol.file.allow=always`
        // 放开 file 协议(现代 git 默认禁子模块走 file://,且只认命令行 -c,不认 repo 配置)。
        let sup = tempfile::tempdir().unwrap();
        git(sup.path(), &["init", "-b", "main", "."]);
        git(sup.path(), &["config", "user.email", "t@e"]);
        git(sup.path(), &["config", "user.name", "t"]);
        let url = upstream.path().to_string_lossy().to_string();
        git(
            sup.path(),
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &url,
                "sub",
            ],
        );
        git(sup.path(), &["commit", "-m", "add sub"]);
        (sup, upstream)
    }

    #[test]
    fn list_submodules_reports_added_submodule() {
        let (sup, _upstream) = super_with_submodule();
        let subs = CliBackend.list_submodules(sup.path()).unwrap();
        assert_eq!(subs.len(), 1, "应识别到一个子模块");
        let s = &subs[0];
        assert_eq!(s.path, "sub");
        assert_eq!(
            s.status,
            SubmoduleStatus::UpToDate,
            "刚 add 即检出 → 已同步"
        );
        assert_eq!(s.head_sha.len(), 40);
        assert!(!s.url.is_empty(), "URL 应从 .gitmodules 读到");
    }

    #[test]
    fn list_submodules_empty_when_none() {
        let repo = init_repo_for_commit();
        std::fs::write(repo.path().join("f.txt"), "x").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        assert!(
            CliBackend.list_submodules(repo.path()).unwrap().is_empty(),
            "无子模块 → 空列表(非错误)"
        );
    }

    #[test]
    fn update_submodule_reinitializes_deinitialized() {
        let (sup, _upstream) = super_with_submodule();
        // 反初始化 → 状态变未初始化。
        git(sup.path(), &["submodule", "deinit", "-f", "sub"]);
        let before = CliBackend.list_submodules(sup.path()).unwrap();
        assert_eq!(before[0].status, SubmoduleStatus::Uninitialized);

        // update --init 重新检出 → 回到已同步。
        CliBackend.update_submodule(sup.path(), "sub").unwrap();
        let after = CliBackend.list_submodules(sup.path()).unwrap();
        assert_eq!(
            after[0].status,
            SubmoduleStatus::UpToDate,
            "update --init 后应已同步"
        );
    }

    // ---- 工作树(M4.5)----

    #[test]
    fn parse_worktree_record_variants() {
        let main = parse_worktree_record(
            "worktree /repo/main\nHEAD 1111111111111111111111111111111111111111\nbranch refs/heads/main",
        )
        .unwrap();
        assert_eq!(main.path, "/repo/main");
        assert_eq!(main.head_sha, "1111111111111111111111111111111111111111");
        assert_eq!(main.branch, "main", "应剥掉 refs/heads/ 前缀");
        assert!(!main.detached);

        let detached = parse_worktree_record(
            "worktree /repo/wt\nHEAD 2222222222222222222222222222222222222222\ndetached",
        )
        .unwrap();
        assert!(detached.detached);
        assert!(detached.branch.is_empty(), "分离头无分支");

        let locked = parse_worktree_record(
            "worktree /mnt/usb/wt\nHEAD 3333333333333333333333333333333333333333\nbranch refs/heads/x\nlocked on removable media",
        )
        .unwrap();
        assert!(locked.locked);
        assert_eq!(locked.branch, "x");

        // 无 worktree 行 → None。
        assert!(parse_worktree_record("HEAD abc\nbranch refs/heads/y").is_none());
    }

    #[test]
    fn list_worktrees_reports_main_and_linked() {
        let repo = tempfile::tempdir().unwrap();
        git(repo.path(), &["init", "-b", "main", "."]);
        git(repo.path(), &["config", "user.email", "t@e"]);
        git(repo.path(), &["config", "user.name", "t"]);
        std::fs::write(repo.path().join("a.txt"), "x").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);

        // 在仓库外加一个链接工作树(检出新分支 feature)。
        let linked_parent = tempfile::tempdir().unwrap();
        let linked = linked_parent.path().join("wt");
        git(
            repo.path(),
            &["worktree", "add", linked.to_str().unwrap(), "-b", "feature"],
        );

        let wts = CliBackend.list_worktrees(repo.path()).unwrap();
        assert_eq!(wts.len(), 2, "主 + 链接共两个工作树");

        let main = &wts[0];
        assert!(main.is_main, "第一条为主工作树");
        assert!(main.is_current, "打开的就是主工作树 → is_current");
        assert_eq!(main.branch, "main");

        let feat = wts.iter().find(|w| w.branch == "feature").unwrap();
        assert!(!feat.is_main);
        assert!(!feat.is_current);
    }

    #[test]
    fn list_worktrees_single_for_plain_repo() {
        let repo = init_repo_for_commit();
        std::fs::write(repo.path().join("f.txt"), "x").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        let wts = CliBackend.list_worktrees(repo.path()).unwrap();
        assert_eq!(wts.len(), 1, "普通仓库只有主工作树");
        assert!(wts[0].is_main && wts[0].is_current);
    }

    // ---- 稀疏检出(M4.6b)----

    #[test]
    fn sparse_checkout_empty_for_normal_repo() {
        let repo = init_repo_for_commit();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::write(repo.path().join("src/a.txt"), "a").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        // 未开启稀疏检出 → 空(不是错误)。
        assert!(
            CliBackend
                .sparse_checkout_patterns(repo.path())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn sparse_checkout_lists_patterns_when_enabled() {
        let repo = init_repo_for_commit();
        std::fs::create_dir_all(repo.path().join("src")).unwrap();
        std::fs::create_dir_all(repo.path().join("docs")).unwrap();
        std::fs::write(repo.path().join("src/a.txt"), "a").unwrap();
        std::fs::write(repo.path().join("docs/b.txt"), "b").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);
        // 开启稀疏检出,只留 src。
        git(repo.path(), &["sparse-checkout", "set", "src"]);

        let patterns = CliBackend.sparse_checkout_patterns(repo.path()).unwrap();
        assert!(!patterns.is_empty(), "稀疏检出开启后应列出范围");
        assert!(patterns.iter().any(|p| p.contains("src")));
    }

    #[test]
    fn file_history_returns_only_that_files_commits_newest_first() {
        let repo = init_repo_for_commit();
        // a.txt 改两次,中间夹一次只动 b.txt 的提交 → a 的历史应是 2 条、不含 b 的那次。
        std::fs::write(repo.path().join("a.txt"), "a1").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "a first"]);
        std::fs::write(repo.path().join("b.txt"), "b1").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "b unrelated"]);
        std::fs::write(repo.path().join("a.txt"), "a2").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "a second"]);

        let hist = CliBackend.file_history(repo.path(), "a.txt", 50).unwrap();
        assert_eq!(hist.len(), 2, "只应有动过 a.txt 的 2 次提交");
        assert_eq!(hist[0].summary, "a second", "时间倒序:最新在前");
        assert_eq!(hist[1].summary, "a first");
        assert_eq!(hist[0].id.len(), 40, "应是完整 SHA");
        assert!(!hist[0].parents.is_empty(), "第二次提交应有父");
    }

    #[test]
    fn file_history_follows_renames() {
        let repo = init_repo_for_commit();
        std::fs::write(repo.path().join("old.txt"), "v1").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "create old"]);
        git(repo.path(), &["mv", "old.txt", "new.txt"]);
        git(repo.path(), &["commit", "-m", "rename to new"]);

        // --follow:查 new.txt 应能看到改名前 old.txt 的那次提交。
        let hist = CliBackend.file_history(repo.path(), "new.txt", 50).unwrap();
        assert_eq!(hist.len(), 2, "--follow 应穿过重命名拿到改名前历史");
        assert_eq!(hist[0].summary, "rename to new");
        assert_eq!(hist[1].summary, "create old");
    }

    #[test]
    fn file_history_unknown_path_is_empty() {
        let repo = init_repo_for_commit();
        std::fs::write(repo.path().join("a.txt"), "a").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1"]);

        assert!(
            CliBackend
                .file_history(repo.path(), "does-not-exist.txt", 50)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn pickaxe_finds_commits_introducing_and_removing_a_string() {
        let repo = init_repo_for_commit();
        // c1 引入 needle_token,c2 删掉它,c3 无关。
        std::fs::write(repo.path().join("a.txt"), "x\nneedle_token\ny\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1 add needle"]);
        std::fs::write(repo.path().join("a.txt"), "x\ny\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c2 remove needle"]);
        std::fs::write(repo.path().join("a.txt"), "x\ny\nz\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c3 unrelated"]);

        // -S:出现次数变化的两次(引入 + 删除),新→旧。
        let hits = CliBackend
            .pickaxe(repo.path(), "needle_token", false, 50)
            .unwrap();
        let summaries: Vec<&str> = hits.iter().map(|c| c.summary.as_str()).collect();
        assert_eq!(summaries, vec!["c2 remove needle", "c1 add needle"]);

        // 不命中 → 空。
        assert!(
            CliBackend
                .pickaxe(repo.path(), "nonexistent_zzz", false, 50)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn pickaxe_regex_matches_changed_lines() {
        let repo = init_repo_for_commit();
        std::fs::write(repo.path().join("a.txt"), "fn alpha() {}\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1 add fn"]);
        std::fs::write(repo.path().join("a.txt"), "let beta = 1;\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c2 replace"]);

        // -G 正则:匹配以 fn 开头的改动行 → c1 和 c2 都动过这行(c2 删了它)。
        let hits = CliBackend.pickaxe(repo.path(), "^fn ", true, 50).unwrap();
        let summaries: Vec<&str> = hits.iter().map(|c| c.summary.as_str()).collect();
        assert!(summaries.contains(&"c1 add fn"), "应含引入 fn 行的提交");
    }

    #[test]
    fn parse_log_records_keeps_multiline_body_and_parents() {
        // 手工拼一条记录:body 含换行不能错位;父字段空格分割成多个。
        let p1 = "1111111111111111111111111111111111111111";
        let p2 = "2222222222222222222222222222222222222222";
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let raw = format!(
            "{id}\u{1f}{p1} {p2}\u{1f}Jane\u{1f}j@e\u{1f}1700000000\u{1f}subject line\u{1f}line one\nline two\u{1e}\n"
        );
        let v = parse_log_records(raw.as_bytes());
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, id);
        assert_eq!(v[0].short_id, "aaaaaaa");
        assert_eq!(v[0].summary, "subject line");
        assert_eq!(
            v[0].body, "line one\nline two",
            "body 的换行应保留、不被当记录分隔"
        );
        assert_eq!(v[0].author.name, "Jane");
        assert_eq!(v[0].timestamp, 1700000000);
        assert_eq!(v[0].parents, vec![p1.to_string(), p2.to_string()]);
    }

    #[test]
    fn line_history_tracks_a_line_range() {
        let repo = init_repo_for_commit();
        std::fs::write(repo.path().join("f.txt"), "line1\nline2\nline3\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c1 create"]);
        // 改第 2 行
        std::fs::write(repo.path().join("f.txt"), "line1\nLINE2 changed\nline3\n").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c2 change line2"]);
        // 只改第 3 行(不该出现在第 2 行的历史里)
        std::fs::write(
            repo.path().join("f.txt"),
            "line1\nLINE2 changed\nLINE3 changed\n",
        )
        .unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "c3 change line3"]);

        // 只查第 2 行 → 只有 c2(改它)和 c1(创建它)。
        let hist = CliBackend.line_history(repo.path(), "f.txt", 2, 2).unwrap();
        assert_eq!(hist.len(), 2, "第 2 行的历史应只有创建 + 改它那两次");
        assert_eq!(hist[0].commit.summary, "c2 change line2", "新→旧");
        assert_eq!(hist[1].commit.summary, "c1 create");

        // c2 那条应带一个 hunk:删 line2 / 增 LINE2 changed。
        let h = &hist[0].diff.hunks;
        assert!(!h.is_empty(), "应解析出范围 hunk");
        let contents: Vec<&str> = h[0].lines.iter().map(|l| l.content.as_str()).collect();
        assert!(contents.contains(&"line2"), "应含被删的旧行");
        assert!(contents.contains(&"LINE2 changed"), "应含新增的新行");
    }

    #[test]
    fn parse_unified_diff_numbers_lines_from_hunk_header() {
        // `@@ -2,2 +2,2 @@`:context 两侧行号都给;-/+ 各只给一侧并各自递增。
        let text =
            "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -2,2 +2,2 @@\n line_ctx\n-old3\n+new3\n";
        let d = parse_unified_diff(text);
        assert_eq!(d.hunks.len(), 1);
        let lines = &d.hunks[0].lines;
        assert_eq!(lines.len(), 3);
        // context 行:old=2 new=2
        assert_eq!(lines[0].kind, DiffLineKind::Context);
        assert_eq!(
            (lines[0].old_lineno, lines[0].new_lineno),
            (Some(2), Some(2))
        );
        assert_eq!(lines[0].content, "line_ctx");
        // 删除行:old=3 new=None
        assert_eq!(lines[1].kind, DiffLineKind::Deletion);
        assert_eq!((lines[1].old_lineno, lines[1].new_lineno), (Some(3), None));
        // 新增行:old=None new=3
        assert_eq!(lines[2].kind, DiffLineKind::Addition);
        assert_eq!((lines[2].old_lineno, lines[2].new_lineno), (None, Some(3)));
    }

    #[test]
    fn parse_unified_diff_handles_creation_dev_null() {
        // 文件创建:`--- /dev/null` + `@@ -0,0 +1,2 @@`,全是新增行,新行号从 1 起。
        let text = "--- /dev/null\n+++ b/f\n@@ -0,0 +1,2 @@\n+first\n+second\n";
        let d = parse_unified_diff(text);
        assert_eq!(d.hunks.len(), 1);
        let lines = &d.hunks[0].lines;
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].new_lineno, Some(1));
        assert_eq!(lines[1].new_lineno, Some(2));
        assert!(
            lines
                .iter()
                .all(|l| l.kind == DiffLineKind::Addition && l.old_lineno.is_none())
        );
    }
}
