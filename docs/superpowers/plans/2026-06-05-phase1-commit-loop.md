# 阶段 1「核心提交回路」实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现文件级 status/stage/unstage/commit 的提交回路,前端可选仓库、暂存改动、写信息提交。

**Architecture:** 沿用阶段 0 的六边形分层,RepoService 保持无状态(trait 方法收 `&Path`,每次重开仓库)。git2 的相对路径要求、unborn-HEAD 等泄漏细节锁在 git2_backend 适配器层。FakeBackend 用 `Mutex` 记录调用供 app-service 层毫秒级 TDD;git-engine 用 tempfile 临时真仓库做集成测试。

**Tech Stack:** Rust(git2 0.18、edition 2024)、Tauri 2、React/TS、pnpm。

**Spec:** `docs/superpowers/specs/2026-06-05-phase1-commit-loop-design.md`

---

## 文件结构(改动地图)

| 文件 | 动作 | 职责 |
|---|---|---|
| `crates/git-core/src/error.rs` | 改 | 加 `NothingToCommit` / `EmptyCommitMessage` / `EmptySignature` |
| `crates/git-core/src/backend.rs` | 改 | trait 加 `stage` / `unstage` / `commit` |
| `crates/git-engine/Cargo.toml` | 改 | 加 `tempfile` dev-dependency |
| `crates/git-engine/src/git2_backend.rs` | 改 | 实现 3 方法 + 路径相对化 helper + 集成测试 |
| `crates/git-engine/src/fake.rs` | 改 | Mutex 状态改造 + 访问器 + 单测 |
| `crates/ipc-types/src/lib.rs` | 改 | `StatusDto` / `FileEntryDto` + From 映射 |
| `crates/app-service/src/lib.rs` | 改 | RepoService 加 4 方法 + 单测 |
| `app/src-tauri/src/lib.rs` | 改 | 4 个命令 + to_ipc 新错误码 + 注册 |
| `app/src/ipc.ts` | 改 | 4 个 IPC 封装 + 类型 |
| `app/src/App.tsx` | 改 | 文件列表两区 + 提交框 + 刷新 |

> **Rust 概念铺垫**(给 Rust 初学者):
> - **内部可变性 / `Mutex`**:Rust 默认不可变,`&self` 方法不能改字段。`Mutex<T>` 让你在持有 `&self` 时也能安全改内部值(加锁)。FakeBackend 必须用 `Mutex` 而非 `RefCell`,因为 `GitBackend: Send + Sync`,而 `RefCell` 是 `!Sync`(不能跨线程共享)——编译器会拦住 RefCell。
> - **集成测试**:放在 `#[cfg(test)] mod tests` 里,用 `tempfile::tempdir()` 建临时目录跑真 git2 操作,测完自动删。
> - **git2 index/tree**:`index` 是暂存区,`add_path` 写入、`write_tree` 把 index 固化成 tree、`commit` 把 tree 挂到 HEAD。

---

## Task 1: 脚手架 —— trait 方法 + 错误变体 + 桩实现(保持全工作区可编译)

> Rust 不能只加 trait 方法而不实现(两个后端都得有)。本任务先加"表面",用 `todo!()` 占位让工作区编译通过,后续任务再 TDD 填真实现。

**Files:**
- Modify: `crates/git-core/src/error.rs`
- Modify: `crates/git-core/src/backend.rs`
- Modify: `crates/git-engine/src/git2_backend.rs`
- Modify: `crates/git-engine/src/fake.rs`
- Modify: `app/src-tauri/src/lib.rs`

- [ ] **Step 1: error.rs 加三个变体**

在 `crates/git-core/src/error.rs` 的 `Backend(String)` 之前插入:

```rust
    #[error("没有已暂存的改动可提交")]
    NothingToCommit,

    #[error("提交信息不能为空")]
    EmptyCommitMessage,

    #[error("git 身份未配置,请先设置 user.name / user.email")]
    EmptySignature,
```

- [ ] **Step 2: backend.rs trait 加三个方法**

在 `crates/git-core/src/backend.rs` 的 `status` 方法后(`}` 之前)加:

```rust
    /// 文件级暂存:把工作区某文件当前内容加入 index。路径为仓库根相对路径。
    fn stage(&self, repo: &Path, file: &Path) -> Result<(), GitError>;

    /// 取消暂存:把某文件从 index 撤回(有/无 HEAD 语义不同,见适配器实现)。
    fn unstage(&self, repo: &Path, file: &Path) -> Result<(), GitError>;

    /// 提交 index 内容,返回新 commit 的完整 SHA。
    fn commit(&self, repo: &Path, message: &str) -> Result<String, GitError>;
```

- [ ] **Step 3: Git2Backend 加桩实现**

在 `crates/git-engine/src/git2_backend.rs` 的 `impl GitBackend for Git2Backend` 块内、`status` 方法后加:

```rust
    fn stage(&self, _path: &Path, _file: &Path) -> Result<(), GitError> {
        todo!("Task 4")
    }

    fn unstage(&self, _path: &Path, _file: &Path) -> Result<(), GitError> {
        todo!("Task 5")
    }

    fn commit(&self, _path: &Path, _message: &str) -> Result<String, GitError> {
        todo!("Task 6")
    }
```

- [ ] **Step 4: FakeBackend 加桩实现**

在 `crates/git-engine/src/fake.rs` 的 `impl GitBackend for FakeBackend` 块内、`status` 方法后加:

```rust
    fn stage(&self, _path: &Path, _file: &Path) -> Result<(), GitError> {
        Ok(())
    }

    fn unstage(&self, _path: &Path, _file: &Path) -> Result<(), GitError> {
        Ok(())
    }

    fn commit(&self, _path: &Path, _message: &str) -> Result<String, GitError> {
        Ok("fake000000000000000000000000000000000000".to_string())
    }
```

- [ ] **Step 5: src-tauri 的 to_ipc 加新错误码分支**

在 `app/src-tauri/src/lib.rs` 的 `to_ipc` 函数 `match &e` 里,`Backend(_)` 之前加三行:

```rust
        NothingToCommit    => ("NOTHING_TO_COMMIT", false),
        EmptyCommitMessage => ("EMPTY_COMMIT_MESSAGE", false),
        EmptySignature     => ("EMPTY_SIGNATURE", false),
```

- [ ] **Step 6: 编译验证**

Run: `cargo build`
Expected: 通过(`todo!()` 满足类型,运行才 panic)。

- [ ] **Step 7: Commit**

```bash
git add crates/git-core/src/error.rs crates/git-core/src/backend.rs crates/git-engine/src/git2_backend.rs crates/git-engine/src/fake.rs app/src-tauri/src/lib.rs
git commit -m "feat(phase1): 脚手架 trait stage/unstage/commit + 错误变体"
```

---

## Task 2: Git2Backend `stage`(tempfile 集成测试)

**Files:**
- Modify: `crates/git-engine/Cargo.toml`
- Modify: `crates/git-engine/src/git2_backend.rs`

- [ ] **Step 1: 加 tempfile dev-dependency**

在 `crates/git-engine/Cargo.toml` 末尾加:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: 写失败测试**

在 `crates/git-engine/src/git2_backend.rs` 末尾加测试模块(本文件已是 `#[cfg(feature = "git2-backend")]`,测试随默认特性运行):

```rust
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
        let backend = Git2Backend::default();

        backend.stage(&repo, Path::new("a.txt")).unwrap();

        let status = backend.status(&repo).unwrap();
        let entry = status.entries.iter().find(|e| e.path == "a.txt").unwrap();
        assert!(entry.staged, "stage 后应标记 staged");
        assert_eq!(entry.state, FileState::Added);
    }
}
```

- [ ] **Step 3: 运行测试,确认失败**

Run: `cargo test -p git-engine stage_marks_file_staged`
Expected: FAIL —— panic `not yet implemented: Task 4`。

- [ ] **Step 4: 实现 stage + 路径相对化 helper**

在 `crates/git-engine/src/git2_backend.rs` 顶部(`impl` 之前)加 helper:

```rust
/// git2 的 add_path/remove_path 要求"仓库根相对路径"。
/// 若传入绝对路径,用 workdir 前缀剥成相对路径;否则原样返回。
/// 这个泄漏细节锁在适配器层,不污染上层。
fn to_repo_relative(repo: &git2::Repository, file: &Path) -> std::path::PathBuf {
    if file.is_absolute() {
        if let Some(wd) = repo.workdir() {
            if let Ok(stripped) = file.strip_prefix(wd) {
                return stripped.to_path_buf();
            }
        }
    }
    file.to_path_buf()
}
```

把 Task 1 的 `stage` 桩替换为:

```rust
    fn stage(&self, path: &Path, file: &Path) -> Result<(), GitError> {
        let repo = git2::Repository::open(path)
            .map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let rel = to_repo_relative(&repo, file);
        let mut index = repo.index().map_err(|e| GitError::Backend(e.to_string()))?;
        // 已知限制(阶段 1):add_path 只覆盖修改/新增;暂存"已删除文件"需 remove_path,留待后续。
        index.add_path(&rel).map_err(|e| GitError::Backend(e.to_string()))?;
        index.write().map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(())
    }
```

- [ ] **Step 5: 运行测试,确认通过**

Run: `cargo test -p git-engine stage_marks_file_staged`
Expected: PASS。

- [ ] **Step 6: Commit**

```bash
git add crates/git-engine/Cargo.toml crates/git-engine/src/git2_backend.rs
git commit -m "feat(phase1): Git2Backend.stage(add_path) + 路径相对化 + 集成测试"
```

---

## Task 3: Git2Backend `unstage`(有 HEAD / 无 HEAD 两种语义)

**Files:**
- Modify: `crates/git-engine/src/git2_backend.rs`

- [ ] **Step 1: 写两个失败测试**

在上面的 `mod tests` 内加:

```rust
    #[test]
    fn unstage_with_head_reverts_to_committed() {
        let (_tmp, repo) = init_repo();
        let backend = Git2Backend::default();
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
        let backend = Git2Backend::default();
        // 全新仓库,没有任何提交(unborn HEAD)
        write(&repo, "a.txt", "hello");
        backend.stage(&repo, Path::new("a.txt")).unwrap();
        backend.unstage(&repo, Path::new("a.txt")).unwrap();

        let status = backend.status(&repo).unwrap();
        let entry = status.entries.iter().find(|e| e.path == "a.txt").unwrap();
        assert!(!entry.staged, "无 HEAD 时取消暂存应把条目从 index 移除");
        assert_eq!(entry.state, FileState::Untracked);
    }
```

- [ ] **Step 2: 运行,确认失败**

Run: `cargo test -p git-engine unstage`
Expected: FAIL —— panic `not yet implemented: Task 5`。

- [ ] **Step 3: 实现 unstage**

把 Task 1 的 `unstage` 桩替换为:

```rust
    fn unstage(&self, path: &Path, file: &Path) -> Result<(), GitError> {
        let repo = git2::Repository::open(path)
            .map_err(|e| GitError::RepoNotFound(e.to_string()))?;
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
                index.remove_path(&rel).map_err(|e| GitError::Backend(e.to_string()))?;
                index.write().map_err(|e| GitError::Backend(e.to_string()))?;
            }
            Err(e) => return Err(GitError::Backend(e.to_string())),
        }
        Ok(())
    }
```

- [ ] **Step 4: 运行,确认通过**

Run: `cargo test -p git-engine unstage`
Expected: PASS(两个用例)。

- [ ] **Step 5: Commit**

```bash
git add crates/git-engine/src/git2_backend.rs
git commit -m "feat(phase1): Git2Backend.unstage 区分有无 HEAD(reset_default / remove_path)"
```

---

## Task 4: Git2Backend `commit`(首次空 parents / 后续传 parent / NothingToCommit / 提交后干净)

**Files:**
- Modify: `crates/git-engine/src/git2_backend.rs`

- [ ] **Step 1: 写失败测试**

在 `mod tests` 内加:

```rust
    #[test]
    fn initial_commit_succeeds_and_status_clean() {
        let (_tmp, repo) = init_repo();
        let backend = Git2Backend::default();
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
        let backend = Git2Backend::default();
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
        let backend = Git2Backend::default();
        // 全新仓库,index 为空
        let err = backend.commit(&repo, "empty").unwrap_err();
        assert!(matches!(err, GitError::NothingToCommit));
    }
```

- [ ] **Step 2: 运行,确认失败**

Run: `cargo test -p git-engine commit`
Expected: FAIL —— panic `not yet implemented: Task 6`。

- [ ] **Step 3: 实现 commit**

把 Task 1 的 `commit` 桩替换为:

```rust
    fn commit(&self, path: &Path, message: &str) -> Result<String, GitError> {
        let repo = git2::Repository::open(path)
            .map_err(|e| GitError::RepoNotFound(e.to_string()))?;

        // 读 git config 的身份,未配置 → 友好错误
        let sig = repo.signature().map_err(|_| GitError::EmptySignature)?;

        let mut index = repo.index().map_err(|e| GitError::Backend(e.to_string()))?;
        let tree_oid = index.write_tree().map_err(|e| GitError::Backend(e.to_string()))?;
        let tree = repo.find_tree(tree_oid).map_err(|e| GitError::Backend(e.to_string()))?;

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
```

- [ ] **Step 4: 运行全部 git-engine 测试,确认通过**

Run: `cargo test -p git-engine`
Expected: PASS(stage / unstage×2 / commit×3 全绿)。

- [ ] **Step 5: Commit**

```bash
git add crates/git-engine/src/git2_backend.rs
git commit -m "feat(phase1): Git2Backend.commit 首次空parents/后续带parent + NothingToCommit"
```

---

## Task 5: FakeBackend 改造(Mutex 状态 + 访问器)

**Files:**
- Modify: `crates/git-engine/src/fake.rs`

- [ ] **Step 1: 写失败测试**

把 `crates/git-engine/src/fake.rs` 末尾(`impl` 块后)加测试:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_stage_and_commit() {
        let fb = FakeBackend::default();
        fb.stage(Path::new("/r"), Path::new("a.txt")).unwrap();
        assert_eq!(fb.staged_files(), vec![std::path::PathBuf::from("a.txt")]);

        let sha = fb.commit(Path::new("/r"), "msg").unwrap();
        assert!(!sha.is_empty());
        assert_eq!(fb.commit_messages(), vec!["msg".to_string()]);
    }
}
```

- [ ] **Step 2: 运行,确认失败**

Run: `cargo test -p git-engine --no-default-features records_stage_and_commit`
Expected: FAIL —— 编译错误 `no method named staged_files`。

- [ ] **Step 3: 改造 FakeBackend 结构与实现**

替换 `crates/git-engine/src/fake.rs` 顶部的 `use` 和 struct(保留 `head_commit` 行为不变):

把开头改为:

```rust
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use git_core::{GitBackend, GitError};
use git_core::model::{Commit, Signature, WorkingTreeStatus, FileEntry};

/// 测试/演示用的假后端。阶段 1 起带内部状态,记录被 stage/commit 的调用供断言。
/// ⚠️ 用 Mutex 而非 RefCell:GitBackend 要求 Send + Sync,RefCell 是 !Sync 编译不过。
/// Mutex 提供"跨线程安全的内部可变性"。
#[derive(Default)]
pub struct FakeBackend {
    canned_status: Mutex<Vec<FileEntry>>,
    staged: Mutex<Vec<PathBuf>>,
    unstaged: Mutex<Vec<PathBuf>>,
    commits: Mutex<Vec<String>>,
}

impl FakeBackend {
    /// 预置一份 status 返回值,供 app-service 的 DTO 映射测试。
    pub fn with_status(entries: Vec<FileEntry>) -> Self {
        let fb = Self::default();
        *fb.canned_status.lock().unwrap() = entries;
        fb
    }
    pub fn staged_files(&self) -> Vec<PathBuf> { self.staged.lock().unwrap().clone() }
    pub fn unstaged_files(&self) -> Vec<PathBuf> { self.unstaged.lock().unwrap().clone() }
    pub fn commit_messages(&self) -> Vec<String> { self.commits.lock().unwrap().clone() }
}
```

把 `impl GitBackend for FakeBackend` 内的方法替换为:

```rust
impl GitBackend for FakeBackend {
    fn open(&self, _path: &Path) -> Result<(), GitError> {
        Ok(())
    }

    fn head_commit(&self, _path: &Path) -> Result<Commit, GitError> {
        Ok(Commit {
            id: "0123456789abcdef0123456789abcdef01234567".into(),
            short_id: "0123456".into(),
            summary: "这是来自 FakeBackend 的假提交".into(),
            body: String::new(),
            author: Signature { name: "测试者".into(), email: "test@example.com".into() },
            timestamp: 1_700_000_000,
            parents: vec![],
        })
    }

    fn status(&self, _path: &Path) -> Result<WorkingTreeStatus, GitError> {
        Ok(WorkingTreeStatus { entries: self.canned_status.lock().unwrap().clone() })
    }

    fn stage(&self, _path: &Path, file: &Path) -> Result<(), GitError> {
        self.staged.lock().unwrap().push(file.to_path_buf());
        Ok(())
    }

    fn unstage(&self, _path: &Path, file: &Path) -> Result<(), GitError> {
        self.unstaged.lock().unwrap().push(file.to_path_buf());
        Ok(())
    }

    fn commit(&self, _path: &Path, message: &str) -> Result<String, GitError> {
        self.commits.lock().unwrap().push(message.to_string());
        Ok("fake000000000000000000000000000000000000".to_string())
    }
}
```

> 注:原来 `use` 里若没有 `FileEntry`,本步已补上;`Signature` 仍需保留。

- [ ] **Step 4: 运行,确认通过**

Run: `cargo test -p git-engine --no-default-features records_stage_and_commit`
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/git-engine/src/fake.rs
git commit -m "feat(phase1): FakeBackend 改用 Mutex 记录调用 + with_status/访问器"
```

---

## Task 6: ipc-types 加 StatusDto / FileEntryDto + 映射

**Files:**
- Modify: `crates/ipc-types/src/lib.rs`

- [ ] **Step 1: 写失败测试**

在 `crates/ipc-types/src/lib.rs` 末尾加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use git_core::model::{WorkingTreeStatus, FileEntry, FileState};

    #[test]
    fn maps_status_to_dto_with_string_state() {
        let st = WorkingTreeStatus {
            entries: vec![
                FileEntry { path: "a.txt".into(), state: FileState::Modified, staged: false },
                FileEntry { path: "b.txt".into(), state: FileState::Added, staged: true },
            ],
        };
        let dto = StatusDto::from(st);
        assert_eq!(dto.entries.len(), 2);
        assert_eq!(dto.entries[0].state, "modified");
        assert_eq!(dto.entries[0].staged, false);
        assert_eq!(dto.entries[1].state, "added");
    }
}
```

- [ ] **Step 2: 运行,确认失败**

Run: `cargo test -p ipc-types maps_status_to_dto`
Expected: FAIL —— `cannot find type StatusDto`。

- [ ] **Step 3: 加 DTO 与映射**

在 `crates/ipc-types/src/lib.rs` 顶部 `use` 处补导入(已 `use git_core::model::Commit;`,改为):

```rust
use git_core::model::{Commit, WorkingTreeStatus, FileEntry, FileState};
```

在文件末尾(`IpcError` 之后)加:

```rust
/// 工作区单个文件状态的 DTO。state 用字符串,前端直接渲染徽章。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntryDto {
    pub path: String,
    pub state: String,   // modified | added | deleted | renamed | untracked | conflicted
    pub staged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusDto {
    pub entries: Vec<FileEntryDto>,
}

impl From<FileEntry> for FileEntryDto {
    fn from(e: FileEntry) -> Self {
        let state = match e.state {
            FileState::Added => "added",
            FileState::Modified => "modified",
            FileState::Deleted => "deleted",
            FileState::Renamed => "renamed",
            FileState::Untracked => "untracked",
            FileState::Conflicted => "conflicted",
        };
        FileEntryDto { path: e.path, state: state.to_string(), staged: e.staged }
    }
}

impl From<WorkingTreeStatus> for StatusDto {
    fn from(s: WorkingTreeStatus) -> Self {
        StatusDto { entries: s.entries.into_iter().map(FileEntryDto::from).collect() }
    }
}
```

- [ ] **Step 4: 运行,确认通过**

Run: `cargo test -p ipc-types`
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/ipc-types/src/lib.rs
git commit -m "feat(phase1): ipc-types 加 StatusDto/FileEntryDto + FileState→字符串映射"
```

---

## Task 7: app-service RepoService 加 4 方法(FakeBackend 单测)

**Files:**
- Modify: `crates/app-service/src/lib.rs`

- [ ] **Step 1: 写失败测试**

在 `crates/app-service/src/lib.rs` 的 `mod tests` 内(已有 `head_commit_via_fake_backend`)追加:

```rust
    use git_core::model::{FileEntry, FileState};

    #[test]
    fn status_maps_to_dto() {
        let fb = FakeBackend::with_status(vec![
            FileEntry { path: "a.txt".into(), state: FileState::Modified, staged: false },
        ]);
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
```

- [ ] **Step 2: 运行,确认失败**

Run: `cargo test -p app-service --no-default-features status_maps_to_dto`
Expected: FAIL —— `no method named status` on RepoService。

- [ ] **Step 3: 实现 4 个方法**

在 `crates/app-service/src/lib.rs` 顶部 `use` 补:

```rust
use ipc_types::StatusDto;
```

在 `impl RepoService` 内(`head_commit` 方法之后)加:

```rust
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
```

- [ ] **Step 4: 运行,确认通过**

Run: `cargo test -p app-service --no-default-features`
Expected: PASS(含原有 head_commit 测试)。

- [ ] **Step 5: Commit**

```bash
git add crates/app-service/src/lib.rs
git commit -m "feat(phase1): RepoService.status/stage/unstage/commit + 空信息拦截 + 测试"
```

---

## Task 8: src-tauri 4 个命令 + 注册

**Files:**
- Modify: `app/src-tauri/src/lib.rs`

- [ ] **Step 1: 加 DRY 的 join 错误 helper + 4 个命令**

在 `app/src-tauri/src/lib.rs` 顶部 `use` 补:

```rust
use ipc_types::StatusDto;
```

在 `get_head_commit` 命令之后加(helper 把 spawn_blocking 的 JoinError 统一转 IpcError,避免每个命令重复):

```rust
/// spawn_blocking 自身失败(线程 panic)→ 统一转可识别错误,绝不让进程崩。
fn join_panic(e: tokio::task::JoinError) -> IpcError {
    IpcError {
        code: "TASK_PANIC".into(),
        message: format!("后台任务异常: {e}"),
        recoverable: true,
    }
}

#[tauri::command]
async fn get_status(repo_path: String) -> Result<StatusDto, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend::default()));
        service.status(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn stage_file(repo_path: String, file_path: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend::default()));
        service.stage(&PathBuf::from(repo_path), &PathBuf::from(file_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn unstage_file(repo_path: String, file_path: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend::default()));
        service.unstage(&PathBuf::from(repo_path), &PathBuf::from(file_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn commit(repo_path: String, message: String) -> Result<String, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend::default()));
        service.commit(&PathBuf::from(repo_path), &message)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}
```

- [ ] **Step 2: 注册命令**

把 `invoke_handler` 那行改为:

```rust
        .invoke_handler(tauri::generate_handler![
            get_head_commit,
            get_status,
            stage_file,
            unstage_file,
            commit
        ])
```

- [ ] **Step 3: 编译验证**

Run: `cargo build -p app`
Expected: 通过。

- [ ] **Step 4: Commit**

```bash
git add app/src-tauri/src/lib.rs
git commit -m "feat(phase1): Tauri 命令 get_status/stage_file/unstage_file/commit + 注册"
```

---

## Task 9: 前端 ipc.ts 封装

**Files:**
- Modify: `app/src/ipc.ts`

- [ ] **Step 1: 加类型与函数**

在 `app/src/ipc.ts` 末尾加:

```ts
export interface FileEntryDto {
  path: string;
  state: string; // modified | added | deleted | renamed | untracked | conflicted
  staged: boolean;
}

export interface StatusDto {
  entries: FileEntryDto[];
}

export async function getStatus(repoPath: string): Promise<StatusDto> {
  return await invoke<StatusDto>("get_status", { repoPath });
}

export async function stageFile(repoPath: string, filePath: string): Promise<void> {
  await invoke("stage_file", { repoPath, filePath });
}

export async function unstageFile(repoPath: string, filePath: string): Promise<void> {
  await invoke("unstage_file", { repoPath, filePath });
}

export async function commit(repoPath: string, message: string): Promise<string> {
  return await invoke<string>("commit", { repoPath, message });
}
```

- [ ] **Step 2: 类型检查**

Run: `cd app && pnpm tsc --noEmit`
Expected: 退出码 0。

- [ ] **Step 3: Commit**

```bash
git add app/src/ipc.ts
git commit -m "feat(phase1): 前端 ipc 封装 getStatus/stageFile/unstageFile/commit"
```

---

## Task 10: 前端 UI —— 文件两区 + 提交框 + 刷新

**Files:**
- Modify: `app/src/App.tsx`

- [ ] **Step 1: 替换 App.tsx**

把 `app/src/App.tsx` 整体替换为:

```tsx
// app/src/App.tsx
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  getStatus,
  stageFile,
  unstageFile,
  commit,
  type StatusDto,
  type FileEntryDto,
  type IpcError,
} from "./ipc";

export default function App() {
  const [repo, setRepo] = useState<string | null>(null);
  const [status, setStatus] = useState<StatusDto | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refreshStatus(repoPath: string) {
    setError(null);
    try {
      setStatus(await getStatus(repoPath));
    } catch (e) {
      setError((e as IpcError).message ?? String(e));
    }
  }

  async function pickRepo() {
    const dir = await open({ directory: true, title: "选择一个 git 仓库" });
    if (typeof dir !== "string") return;
    setRepo(dir);
    setInfo(null);
    await refreshStatus(dir);
  }

  async function run(action: () => Promise<void>) {
    if (!repo) return;
    setBusy(true);
    setError(null);
    try {
      await action();
      await refreshStatus(repo);
    } catch (e) {
      setError((e as IpcError).message ?? String(e));
    } finally {
      setBusy(false);
    }
  }

  const staged = status?.entries.filter((e) => e.staged) ?? [];
  const unstaged = status?.entries.filter((e) => !e.staged) ?? [];

  function Row({ entry, staged }: { entry: FileEntryDto; staged: boolean }) {
    return (
      <li style={{ display: "flex", alignItems: "center", gap: 8, padding: "2px 0" }}>
        <span style={{ fontSize: 12, color: "#888", width: 84 }}>{entry.state}</span>
        <span style={{ flex: 1, fontFamily: "monospace" }}>{entry.path}</span>
        <button
          disabled={busy}
          onClick={() =>
            run(() =>
              staged ? unstageFile(repo!, entry.path) : stageFile(repo!, entry.path)
            )
          }
        >
          {staged ? "取消暂存" : "暂存"}
        </button>
      </li>
    );
  }

  async function doCommit() {
    if (!repo) return;
    setBusy(true);
    setError(null);
    try {
      const sha = await commit(repo, message);
      setInfo(`已提交 ${sha.slice(0, 7)}`);
      setMessage("");
      await refreshStatus(repo);
    } catch (e) {
      setError((e as IpcError).message ?? String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main style={{ fontFamily: "system-ui", padding: 24, maxWidth: 720 }}>
      <h1>Git 客户端 · 阶段 1</h1>
      <button onClick={pickRepo} disabled={busy}>选择仓库</button>
      {repo && <span style={{ marginLeft: 12, color: "#666" }}>{repo}</span>}

      {error && <p style={{ color: "crimson" }}>错误:{error}</p>}
      {info && <p style={{ color: "green" }}>{info}</p>}

      {repo && (
        <>
          <button onClick={() => refreshStatus(repo)} disabled={busy} style={{ marginTop: 12 }}>
            刷新
          </button>

          <h3 style={{ marginBottom: 4 }}>已暂存 ({staged.length})</h3>
          <ul style={{ listStyle: "none", padding: 0 }}>
            {staged.map((e) => <Row key={e.path} entry={e} staged />)}
            {staged.length === 0 && <li style={{ color: "#aaa" }}>(空)</li>}
          </ul>

          <h3 style={{ marginBottom: 4 }}>未暂存 ({unstaged.length})</h3>
          <ul style={{ listStyle: "none", padding: 0 }}>
            {unstaged.map((e) => <Row key={e.path} entry={e} staged={false} />)}
            {unstaged.length === 0 && <li style={{ color: "#aaa" }}>(空)</li>}
          </ul>

          <h3 style={{ marginBottom: 4 }}>提交</h3>
          <textarea
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            placeholder="提交信息"
            rows={3}
            style={{ width: "100%", fontFamily: "inherit" }}
          />
          <button
            onClick={doCommit}
            disabled={busy || staged.length === 0 || message.trim() === ""}
            style={{ marginTop: 8 }}
          >
            提交 {staged.length} 个改动
          </button>
        </>
      )}
    </main>
  );
}
```

- [ ] **Step 2: 类型检查 + 构建**

Run: `cd app && pnpm tsc --noEmit && pnpm build`
Expected: 退出码 0,vite 构建成功。

- [ ] **Step 3: Commit**

```bash
git add app/src/App.tsx
git commit -m "feat(phase1): 前端文件列表两区 + 提交框 + 操作后刷新"
```

---

## Task 11: 全量验收

**Files:** 无改动,仅验证。

- [ ] **Step 1: 全工作区测试**

Run: `cargo test`
Expected: git-engine / ipc-types / app-service 全绿。

- [ ] **Step 2: 全工作区构建(含外壳)**

Run: `cargo build`
Expected: 通过。

- [ ] **Step 3: 前端检查**

Run: `cd app && pnpm tsc --noEmit && pnpm build`
Expected: 退出码 0。

- [ ] **Step 4: 手动冒烟(用户本机,需图形窗口)**

Run: `cd app && pnpm tauri dev`
验证:选仓库 → 看到分区文件列表 → 暂存/取消暂存 → 写信息提交 → 列表刷新为干净、显示新 SHA。
边界:空仓库首次提交可成功;空提交信息时"提交"按钮禁用;未配 git 身份时显示友好错误(EMPTY_SIGNATURE)。

- [ ] **Step 5: 标记阶段完成(可选,对照 spec 第 6 节验收勾选)**

---

## 自查记录(写计划时已核对)

- **Spec 覆盖**:status(已有→接出 Task 7/8/9/10)、stage(Task 2)、unstage 有无 HEAD(Task 3)、commit 首次/后续/NothingToCommit(Task 4)、FakeBackend Mutex(Task 5)、DTO(Task 6)、空信息拦截(Task 7)、命令(Task 8)、前端(Task 9/10)、7 个集成用例(Task 2-4 覆盖含"commit 后 status 干净")。
- **git2 API 已按 0.18.3 源码核对**:`head()` + `ErrorCode::UnbornBranch`(git2 0.18 无 `head_unborn()`,用错误码精确判断)、`reset_default(Some(&Object), [&str])`、`commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&parent])`、`index.add_path/remove_path/write/write_tree/is_empty`。
- **类型一致性**:`StatusDto`/`FileEntryDto`/`getStatus`/`stageFile`/`unstageFile`/`commit` 前后端命名一致;命令参数 `repoPath`/`filePath`/`message` 与 Rust 端 `repo_path`/`file_path`/`message`(Tauri 自动驼峰)对应。
- **已知限制**:暂存"已删除文件"阶段 1 不支持(stage 只用 add_path),已在代码注释与 spec 标注。
