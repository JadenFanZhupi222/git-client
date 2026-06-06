# 阶段 2d-1 远程基础设施 + fetch · 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让用户能从默认远程 fetch(更新远程跟踪分支),并一次性建立 CLI 后端 + CompositeBackend 这套远程操作地基。

**Architecture:** 六边形分层。`GitBackend` trait 加 `fetch` 默认方法(默认返回 Unsupported);新增 `CliBackend`(spawn `git fetch`,复用系统凭据助手)与 `CompositeBackend`(既有方法委托 git2、fetch 委托 cli);命令层改用 CompositeBackend。fetch 后由现有文件监听(refs 变化→"ref"事件)自动刷新 UI。

**Tech Stack:** Rust(git2 / std::process::Command / thiserror / tempfile)、Tauri 2.x、React + TypeScript。

**对应 spec:** `docs/superpowers/specs/2026-06-06-remote-fetch-design.md`

---

## 文件结构

| 文件 | 职责 | 新建/改 |
|---|---|---|
| `crates/git-core/src/model/remote.rs` | `FetchOutcome` 领域模型 | 新建 |
| `crates/git-core/src/model/mod.rs` | 导出 FetchOutcome | 改 |
| `crates/git-core/src/error.rs` | 5 个新错误变体 | 改 |
| `crates/git-core/src/backend.rs` | trait `fetch` 默认方法 | 改 |
| `crates/git-engine/src/cli_backend.rs` | `CliBackend::fetch`(shell out)+ 测试 | 新建 |
| `crates/git-engine/src/composite.rs` | `CompositeBackend` 委托 + 测试 | 新建 |
| `crates/git-engine/src/fake.rs` | FakeBackend 实现 fetch | 改 |
| `crates/git-engine/src/lib.rs` | 导出新模块 | 改 |
| `crates/ipc-types/src/lib.rs` | `FetchResultDto` + From | 改 |
| `crates/app-service/src/lib.rs` | `fetch` 用例 + 测试 | 改 |
| `app/src-tauri/src/lib.rs` | `fetch` 命令 + to_ipc arm + 改用 CompositeBackend | 改 |
| `app/src/ipc.ts` | `FetchResultDto` + `fetch()` | 改 |
| `app/src/components/icons.tsx` | `FetchIcon` | 改 |
| `app/src/App.tsx` | 顶栏 Fetch 按钮 + 状态/错误 | 改 |

---

## Task 1: git-core —— FetchOutcome 模型 + 错误变体 + trait 默认方法

**Files:**
- Create: `crates/git-core/src/model/remote.rs`
- Modify: `crates/git-core/src/model/mod.rs`
- Modify: `crates/git-core/src/error.rs`
- Modify: `crates/git-core/src/backend.rs`

- [ ] **Step 1: 写 FetchOutcome 模型**

`crates/git-core/src/model/remote.rs`:
```rust
use serde::{Deserialize, Serialize};

/// 一次 fetch 的结果(MVP:不解析结构化更新明细)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FetchOutcome {
    /// 实际 fetch 的远程名;用默认远程时为空串。
    pub remote: String,
    /// git 的人类输出(stderr 那几行);为空表示「已是最新」。
    pub summary: String,
}
```

- [ ] **Step 2: 在 model/mod.rs 导出**

`crates/git-core/src/model/mod.rs` —— 加 `mod remote;` 与 `pub use remote::FetchOutcome;`(放在 branch 行附近,保持字母序):
```rust
mod branch;
mod commit;
mod diff;
mod remote;
mod status;

pub use branch::BranchInfo;
pub use commit::{Commit, Signature};
pub use diff::{DiffLine, DiffLineKind, FileChange, FileDiff, Hunk};
pub use remote::FetchOutcome;
pub use status::{FileEntry, FileState, WorkingTreeStatus};
```

- [ ] **Step 3: 加 5 个错误变体**

`crates/git-core/src/error.rs` —— 在 `CheckoutConflict` 之后、`Backend` 之前插入:
```rust
    #[error("未找到 git 命令,请确认已安装 git 并在 PATH 中")]
    GitCliNotFound,

    #[error("认证失败,请检查凭据或 SSH key")]
    AuthFailed,

    #[error("网络错误,无法访问远程")]
    NetworkError,

    #[error("没有配置远程仓库")]
    NoRemote,

    #[error("该后端不支持此操作")]
    Unsupported,
```

- [ ] **Step 4: trait 加 fetch 默认方法**

`crates/git-core/src/backend.rs` —— 顶部 use 增加 `FetchOutcome`:
```rust
use crate::model::{BranchInfo, Commit, FetchOutcome, FileChange, FileDiff, WorkingTreeStatus};
```
在 trait 末尾 `delete_branch` 之后加:
```rust
    /// 从远程拉取更新(更新远程跟踪分支,不改工作区/当前分支)。
    /// remote = None 时用 git 默认远程(通常当前分支的 upstream / origin)。
    /// 默认实现返回 Unsupported —— 不做网络的后端无需覆盖。
    fn fetch(&self, _repo: &Path, _remote: Option<&str>) -> Result<FetchOutcome, GitError> {
        Err(GitError::Unsupported)
    }
```

- [ ] **Step 5: 编译 git-core**

Run: `cargo build -p git-core`
Expected: 编译通过(此时 git-engine/app-service 尚未实现 fetch,但因为是默认方法,不会破坏既有 impl)。

- [ ] **Step 6: Commit**

```bash
git add crates/git-core/
git commit -m "feat(core): FetchOutcome 模型 + 远程错误变体 + trait fetch 默认方法"
```

---

## Task 2: git-engine —— CliBackend::fetch + 本地远程测试

**Files:**
- Create: `crates/git-engine/src/cli_backend.rs`
- Modify: `crates/git-engine/src/lib.rs`

- [ ] **Step 1: 写 CliBackend 实现**

`crates/git-engine/src/cli_backend.rs`:
```rust
use git_core::GitError;
use git_core::model::FetchOutcome;
use std::path::Path;
use std::process::Command;

/// 调用系统 git CLI 的后端,专管网络/复杂流程(凭据交给 git 的凭据助手)。
/// ⚠️ 子进程是阻塞的 —— 调用方必须在 spawn_blocking 里使用它。
#[derive(Default)]
pub struct CliBackend;

impl CliBackend {
    /// 执行 `git -C <repo> fetch --prune [remote]`。
    pub fn fetch(&self, repo: &Path, remote: Option<&str>) -> Result<FetchOutcome, GitError> {
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
}
```

- [ ] **Step 2: 在 lib.rs 导出(暂不导出 composite)**

`crates/git-engine/src/lib.rs` —— 加模块声明与导出(与现有 `pub mod` / `pub use` 风格一致):
```rust
pub mod cli_backend;
pub use cli_backend::CliBackend;
```

- [ ] **Step 3: 写失败测试(主测试 + 错误路径)**

在 `crates/git-engine/src/cli_backend.rs` 末尾追加:
```rust
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

    fn rev_parse(repo: &Path, spec: &str) -> String {
        let r = git2::Repository::open(repo).unwrap();
        r.revparse_single(spec).unwrap().id().to_string()
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
}
```

- [ ] **Step 4: 运行测试确认失败再通过**

Run: `cargo test -p git-engine cli_backend`
Expected: 先确认能编译并运行;两个测试 PASS(`fetch_advances_remote_tracking_ref`、`fetch_without_remote_errors`)。若 `fetch_without_remote_errors` 因 git 文案差异未命中 NoRemote,检查实际 stderr 调整关键词(应含 "no remote repository")。

- [ ] **Step 5: Commit**

```bash
git add crates/git-engine/src/cli_backend.rs crates/git-engine/src/lib.rs
git commit -m "feat(engine): CliBackend.fetch —— shell out git fetch + 本地远程测试"
```

---

## Task 3: git-engine —— CompositeBackend 委托

**Files:**
- Create: `crates/git-engine/src/composite.rs`
- Modify: `crates/git-engine/src/lib.rs`

- [ ] **Step 1: 写 CompositeBackend(全方法委托 + fetch 走 cli)**

`crates/git-engine/src/composite.rs`:
```rust
use crate::cli_backend::CliBackend;
use crate::git2_backend::Git2Backend;
use git_core::model::{
    BranchInfo, Commit, FetchOutcome, FileChange, FileDiff, WorkingTreeStatus,
};
use git_core::{GitBackend, GitError};
use std::path::Path;

/// 组合后端:对外是一个 GitBackend,内部按操作路由。
/// 既有(读 + 本地写)方法走 git2;网络方法(fetch)走 CLI。
#[derive(Default)]
pub struct CompositeBackend {
    git2: Git2Backend,
    cli: CliBackend,
}

impl GitBackend for CompositeBackend {
    fn open(&self, repo: &Path) -> Result<(), GitError> {
        self.git2.open(repo)
    }
    fn head_commit(&self, repo: &Path) -> Result<Commit, GitError> {
        self.git2.head_commit(repo)
    }
    fn status(&self, repo: &Path) -> Result<WorkingTreeStatus, GitError> {
        self.git2.status(repo)
    }
    fn stage(&self, repo: &Path, file: &Path) -> Result<(), GitError> {
        self.git2.stage(repo, file)
    }
    fn unstage(&self, repo: &Path, file: &Path) -> Result<(), GitError> {
        self.git2.unstage(repo, file)
    }
    fn commit(&self, repo: &Path, message: &str) -> Result<String, GitError> {
        self.git2.commit(repo, message)
    }
    fn log(&self, repo: &Path, limit: usize, skip: usize) -> Result<Vec<Commit>, GitError> {
        self.git2.log(repo, limit, skip)
    }
    fn commit_files(&self, repo: &Path, commit_id: &str) -> Result<Vec<FileChange>, GitError> {
        self.git2.commit_files(repo, commit_id)
    }
    fn commit_file_diff(
        &self,
        repo: &Path,
        commit_id: &str,
        file: &str,
    ) -> Result<FileDiff, GitError> {
        self.git2.commit_file_diff(repo, commit_id, file)
    }
    fn current_branch(&self, repo: &Path) -> Result<Option<String>, GitError> {
        self.git2.current_branch(repo)
    }
    fn branches(&self, repo: &Path) -> Result<Vec<BranchInfo>, GitError> {
        self.git2.branches(repo)
    }
    fn checkout_branch(&self, repo: &Path, name: &str) -> Result<(), GitError> {
        self.git2.checkout_branch(repo, name)
    }
    fn create_branch(&self, repo: &Path, name: &str) -> Result<(), GitError> {
        self.git2.create_branch(repo, name)
    }
    fn delete_branch(&self, repo: &Path, name: &str) -> Result<(), GitError> {
        self.git2.delete_branch(repo, name)
    }

    // 网络操作走 CLI 后端。
    fn fetch(&self, repo: &Path, remote: Option<&str>) -> Result<FetchOutcome, GitError> {
        self.cli.fetch(repo, remote)
    }
}
```

- [ ] **Step 2: 在 lib.rs 导出**

`crates/git-engine/src/lib.rs` —— 加:
```rust
pub mod composite;
pub use composite::CompositeBackend;
```

- [ ] **Step 3: 写委托测试**

在 `crates/git-engine/src/composite.rs` 末尾追加:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        Command::new("git").current_dir(dir).args(args).output().unwrap();
    }

    #[test]
    fn delegates_branches_to_git2() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-b", "main", "."]);
        git(dir.path(), &["config", "user.email", "t@e"]);
        git(dir.path(), &["config", "user.name", "t"]);
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        git(dir.path(), &["add", "."]);
        git(dir.path(), &["commit", "-m", "c1"]);

        let composite = CompositeBackend::default();
        let git2 = Git2Backend;
        let via_composite: Vec<String> =
            composite.branches(dir.path()).unwrap().into_iter().map(|b| b.name).collect();
        let via_git2: Vec<String> =
            git2.branches(dir.path()).unwrap().into_iter().map(|b| b.name).collect();
        assert_eq!(via_composite, via_git2, "composite 应把 branches 透传给 git2");
        assert!(via_composite.contains(&"main".to_string()));
    }
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test -p git-engine composite`
Expected: `delegates_branches_to_git2` PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/git-engine/src/composite.rs crates/git-engine/src/lib.rs
git commit -m "feat(engine): CompositeBackend —— 既有方法委托 git2、fetch 走 cli"
```

---

## Task 4: git-engine —— FakeBackend 实现 fetch

**Files:**
- Modify: `crates/git-engine/src/fake.rs`

- [ ] **Step 1: 给 FakeBackend 加 canned outcome + 计数**

`crates/git-engine/src/fake.rs` —— 顶部 use 增加 `FetchOutcome`:
```rust
use git_core::model::{
    BranchInfo, Commit, FetchOutcome, FileChange, FileDiff, FileEntry, Signature, WorkingTreeStatus,
};
```
在结构体字段区(`deleted` 之后)加:
```rust
    canned_fetch: Mutex<Option<FetchOutcome>>,
    fetch_calls: Mutex<u32>,
```
在 `impl FakeBackend`(`deleted_branches` 之后)加:
```rust
    pub fn with_fetch(self, outcome: FetchOutcome) -> Self {
        *self.canned_fetch.lock().unwrap() = Some(outcome);
        self
    }
    pub fn fetch_call_count(&self) -> u32 {
        *self.fetch_calls.lock().unwrap()
    }
```

- [ ] **Step 2: 实现 trait 的 fetch**

在 `impl GitBackend for FakeBackend`(`delete_branch` 之后)加:
```rust
    fn fetch(&self, _repo: &Path, remote: Option<&str>) -> Result<FetchOutcome, GitError> {
        *self.fetch_calls.lock().unwrap() += 1;
        Ok(self.canned_fetch.lock().unwrap().clone().unwrap_or(FetchOutcome {
            remote: remote.unwrap_or("").to_string(),
            summary: "已是最新".to_string(),
        }))
    }
```

- [ ] **Step 3: 编译 + 跑 git-engine 全测**

Run: `cargo test -p git-engine`
Expected: 全部 PASS(含既有 30 + 新增 cli/composite 测试)。

- [ ] **Step 4: Commit**

```bash
git add crates/git-engine/src/fake.rs
git commit -m "test(engine): FakeBackend 实现 fetch(canned + 计数)"
```

---

## Task 5: ipc-types —— FetchResultDto

**Files:**
- Modify: `crates/ipc-types/src/lib.rs`

- [ ] **Step 1: 加 DTO + From 映射**

`crates/ipc-types/src/lib.rs` —— 顶部 use 增加 `FetchOutcome`:
```rust
use git_core::model::{
    BranchInfo, Commit, DiffLine, DiffLineKind, FetchOutcome, FileChange, FileDiff, FileEntry,
    FileState, Hunk, WorkingTreeStatus,
};
```
在 `BranchDto` 定义之后加:
```rust
/// 一次 fetch 的结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResultDto {
    pub remote: String,
    pub summary: String,
}

impl From<FetchOutcome> for FetchResultDto {
    fn from(o: FetchOutcome) -> Self {
        FetchResultDto {
            remote: o.remote,
            summary: o.summary,
        }
    }
}
```

- [ ] **Step 2: 编译**

Run: `cargo build -p ipc-types`
Expected: 通过。

- [ ] **Step 3: Commit**

```bash
git add crates/ipc-types/
git commit -m "feat(ipc-types): FetchResultDto"
```

---

## Task 6: app-service —— fetch 用例 + 测试

**Files:**
- Modify: `crates/app-service/src/lib.rs`

- [ ] **Step 1: 加用例方法**

`crates/app-service/src/lib.rs` —— 顶部 use 增加 `FetchResultDto`:
```rust
use ipc_types::{BranchDto, CommitDto, FetchResultDto, FileChangeDto, FileDiffDto, GraphRowDto, StatusDto};
```
在 `delete_branch` 用例之后加:
```rust
    /// 用例:从远程 fetch。remote=None 用默认远程。
    pub fn fetch(&self, repo_path: &Path, remote: Option<&str>) -> Result<FetchResultDto, GitError> {
        let outcome = self.backend.fetch(repo_path, remote)?;
        Ok(FetchResultDto::from(outcome))
    }
```

- [ ] **Step 2: 写测试**

在 `crates/app-service/src/lib.rs` 的 `#[cfg(test)] mod tests` 内(`delete_branch_forwards` 之后)加:
```rust
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
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p app-service`
Expected: 全部 PASS(含既有 30 + 2 个新增)。

- [ ] **Step 4: Commit**

```bash
git add crates/app-service/
git commit -m "feat(app-service): fetch 用例 + FakeBackend 测试"
```

---

## Task 7: src-tauri —— fetch 命令 + to_ipc + 改用 CompositeBackend

**Files:**
- Modify: `app/src-tauri/src/lib.rs`

- [ ] **Step 1: 改 import,加 DTO + 后端**

`app/src-tauri/src/lib.rs` 顶部:
```rust
use git_engine::CompositeBackend; // 生产后端:git2 + cli 组合
use ipc_types::{
    BranchDto, CommitDto, FetchResultDto, FileChangeDto, FileDiffDto, GraphRowDto, IpcError,
    StatusDto,
};
```
(删掉原 `use git_engine::Git2Backend;`,见 Step 3 把所有用到处替换。)

- [ ] **Step 2: to_ipc 加 5 个 arm**

在 `to_ipc` 的 match 里、`CheckoutConflict` 之后、`Backend` 之前加:
```rust
        GitCliNotFound => ("GIT_CLI_NOT_FOUND", false),
        AuthFailed => ("AUTH_FAILED", true),
        NetworkError => ("NETWORK_ERROR", true),
        NoRemote => ("NO_REMOTE", false),
        Unsupported => ("UNSUPPORTED", false),
```

- [ ] **Step 3: 全局把 Git2Backend 换成 CompositeBackend**

把文件内所有 `RepoService::new(Arc::new(Git2Backend))` 替换为 `RepoService::new(Arc::new(CompositeBackend::default()))`(共约 13 处:get_head_commit / get_status / stage_file / unstage_file / commit / get_log / get_commit_files / get_commit_file_diff / get_commit_graph / get_current_branch / list_branches / checkout_branch / create_branch / delete_branch)。
> 用编辑器全局替换:`Arc::new(Git2Backend)` → `Arc::new(CompositeBackend::default())`。

- [ ] **Step 4: 加 fetch 命令**

在 `delete_branch` 命令之后、`watch_repo` 之前加:
```rust
#[tauri::command]
async fn fetch(repo_path: String, remote: Option<String>) -> Result<FetchResultDto, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.fetch(&PathBuf::from(repo_path), remote.as_deref())
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}
```

- [ ] **Step 5: 注册命令**

在 `invoke_handler` 的 `generate_handler![...]` 里、`delete_branch,` 之后加一行 `fetch,`。

- [ ] **Step 6: 编译整个 workspace(含 Tauri 壳)**

Run: `cargo check -p app`
Expected: 通过,无 warning。若有 `unused import Git2Backend` 警告说明 Step 3 有遗漏,补替换。

- [ ] **Step 7: Commit**

```bash
git add app/src-tauri/
git commit -m "feat(tauri): fetch 命令 + 错误码 + 命令层改用 CompositeBackend"
```

---

## Task 8: 前端 ipc 封装

**Files:**
- Modify: `app/src/ipc.ts`

- [ ] **Step 1: 加类型 + 封装**

`app/src/ipc.ts` —— 在 `deleteBranch` 之后加:
```typescript
// ── 远程(阶段 2d-1) ──
export interface FetchResultDto {
  remote: string;
  summary: string;
}

/** 从默认远程 fetch(remote 省略 = git 默认远程)。 */
export async function fetchRemote(repoPath: string, remote?: string): Promise<FetchResultDto> {
  return await invoke<FetchResultDto>("fetch", { repoPath, remote: remote ?? null });
}
```

- [ ] **Step 2: typecheck**

Run: `npx --prefix d:/Codes/git-client/app tsc -p d:/Codes/git-client/app/tsconfig.json --noEmit`
Expected: 无错误。

- [ ] **Step 3: Commit**

```bash
git add app/src/ipc.ts
git commit -m "feat(ui): fetchRemote ipc 封装"
```

---

## Task 9: UI —— 顶栏 Fetch 按钮

**Files:**
- Modify: `app/src/components/icons.tsx`
- Modify: `app/src/App.tsx`

- [ ] **Step 1: 加 FetchIcon(向下箭头入托盘,git fetch 习惯图标)**

`app/src/components/icons.tsx` —— 在 `MoonIcon` 之后加:
```tsx
export const FetchIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base} {...p}>
    <path d="M8 2v7M5 6.5 8 9.5l3-3M3 12.5h10" />
  </svg>
);
```

- [ ] **Step 2: App.tsx 加 fetch 状态 + 处理函数**

`app/src/App.tsx` —— import 增加 `FetchIcon`:
```tsx
import { FolderIcon, SunIcon, MoonIcon, FetchIcon } from "./components/icons";
```
增加 ipc import:
```tsx
import { getCurrentBranch, watchRepo, onRepoChanged, fetchRemote, type IpcError } from "./ipc";
```
在 `const [theme, ...]` 之后加状态:
```tsx
  const [fetching, setFetching] = useState(false);
  const [fetchMsg, setFetchMsg] = useState<string | null>(null);
  const [fetchErr, setFetchErr] = useState<string | null>(null);
```
在 `toggleTheme` 之后加:
```tsx
  async function doFetch() {
    if (!repo) return;
    setFetching(true);
    setFetchMsg(null);
    setFetchErr(null);
    try {
      const r = await fetchRemote(repo);
      // refs 变化会触发文件监听 → 各视图自动重载;这里只反馈结果。
      setFetchMsg(r.summary === "已是最新" ? "已是最新" : `已 fetch ${r.remote || "远程"}`);
      setTimeout(() => setFetchMsg(null), 4000);
    } catch (e) {
      setFetchErr((e as IpcError).message ?? String(e));
    } finally {
      setFetching(false);
    }
  }
```

- [ ] **Step 3: 顶栏渲染 Fetch 按钮 + 状态**

`app/src/App.tsx` —— 在顶栏右侧 `<div className="ml-auto flex items-center gap-2">` 内、仓库名 span 之前(即 repo 区域)插入(仅 repo 存在时显示):
```tsx
          {repo && (
            <div className="flex items-center gap-1.5">
              <button
                onClick={doFetch}
                disabled={fetching}
                title="Fetch(从远程拉取更新)"
                className="flex items-center gap-1.5 rounded-md border border-line-strong bg-elevated px-2.5 py-1 text-xs text-fg transition-colors hover:bg-overlay hover:border-fg-subtle disabled:opacity-50"
              >
                <FetchIcon width={13} height={13} className={fetching ? "animate-spin" : ""} />
                {fetching ? "Fetch…" : "Fetch"}
              </button>
              {fetchErr ? (
                <span className="max-w-[16rem] truncate text-xs text-danger" title={fetchErr}>
                  {fetchErr}
                </span>
              ) : fetchMsg ? (
                <span className="text-xs text-success">{fetchMsg}</span>
              ) : null}
            </div>
          )}
```

- [ ] **Step 4: typecheck + 构建**

Run: `npx --prefix d:/Codes/git-client/app tsc -p d:/Codes/git-client/app/tsconfig.json --noEmit`
Expected: 无错误(注意 `fetchErr` 用到了 `IpcError` 类型,已在 Step 2 import)。
Run: `npm --prefix d:/Codes/git-client/app run build`
Expected: build 成功。

- [ ] **Step 5: Commit**

```bash
git add app/src/components/icons.tsx app/src/App.tsx
git commit -m "feat(ui): 顶栏 Fetch 按钮 + 结果/错误反馈"
```

---

## Task 10: 收尾验证

- [ ] **Step 1: 全工作区测试**

Run: `cargo test --workspace`
Expected: 全部 PASS,无 FAILED。

- [ ] **Step 2: 无警告检查**

Run: `cargo build --workspace`
Expected: 无 warning。

- [ ] **Step 3: 前端构建**

Run: `npm --prefix d:/Codes/git-client/app run build`
Expected: 成功。

- [ ] **Step 4: 更新进度记忆**

更新 `roadmap-progress` 记忆:阶段 2 加一条 `2d-1 远程基础设施 + fetch:✅`,并记下引入了 CliBackend + CompositeBackend(命令层已切换)。

- [ ] **Step 5: 人工验收提示(交给用户)**

提示用户在 `npm run tauri dev` 里点 Fetch 验真实远程(公开/私有/断网三种)。

---

## 自检备注

- **Spec 覆盖**:CliBackend(§3.2)→T2;CompositeBackend(§3.3)→T3;trait 默认方法(§3.1)→T1;FetchOutcome/DTO(§4.1)→T1/T5;远程选择 None(§4.2)→贯穿;数据流(§4.3)→T6/T7/T8/T9;UI(§5)→T9;错误归类(§6)→T1/T2/T7;测试(§7)→T2/T3/T4/T6。UI 自动刷新依赖现有 watcher(refs→"ref"),无需改动,已在计划说明。
- **类型一致性**:`FetchOutcome{remote,summary}`、`FetchResultDto{remote,summary}`、`fetch(repo, Option<&str>)`、命令参数 `{repoPath, remote}`、前端 `fetchRemote(repoPath, remote?)` 全程一致。
- **packed-refs 边界**:极少数情况下 fetch 只更新 packed-refs(不写 loose ref),此时 watcher 不触发刷新;MVP 接受,用户可手动刷新或重开视图。push 切片再统一处理 ahead/behind 时一并优化。
