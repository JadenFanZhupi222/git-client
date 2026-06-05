# 阶段 1b-1「历史列表 + 提交改动文件」实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 加 commit log 历史列表 + 选中提交看其改动文件(文件级 A/M/D),暗色三栏 UI(Tailwind v4),diff 列占位留 1b-2。

**Architecture:** 沿用无状态 RepoService + spawn_blocking。git-engine 用 Revwalk 取 log、diff_tree_to_tree 取改动文件;git2 边界(首提交空树/合并取第一父/Sort::TIME 在 push_head 前/惰性分页/不检测重命名)锁在适配器层。前端引入 Tailwind v4,App 拆成「更改 / 历史」标签页。

**Tech Stack:** Rust(git2 0.18、edition 2024)、Tauri 2、React/TS、Tailwind v4、pnpm。

**Spec:** `docs/superpowers/specs/2026-06-05-phase1b-1-history-list-design.md`

---

## 文件结构

| 文件 | 动作 | 职责 |
|---|---|---|
| `crates/git-core/src/model/diff.rs` | 创建 | `FileChange` 模型 |
| `crates/git-core/src/model/mod.rs` | 改 | 导出 `FileChange` |
| `crates/git-core/src/backend.rs` | 改 | trait 加 `log`/`commit_files`/`current_branch` |
| `crates/git-engine/src/git2_backend.rs` | 改 | 实现三方法 + `build_commit` 抽取 + 测试 |
| `crates/git-engine/src/fake.rs` | 改 | canned 字段 + setter + 实现三方法 |
| `crates/ipc-types/src/lib.rs` | 改 | `FileChangeDto` + From 映射 |
| `crates/app-service/src/lib.rs` | 改 | RepoService 三方法 + 测试 |
| `app/src-tauri/src/lib.rs` | 改 | 三命令 + 注册 |
| `app/package.json` `vite.config.ts` `src/index.css` `src/main.tsx` | 改 | Tailwind v4 接入 |
| `app/src/lib/ipc.ts`(现 `src/ipc.ts`) | 改 | 三个 IPC 封装 + 类型 |
| `app/src/lib/time.ts` | 创建 | 相对时间格式化 |
| `app/src/components/TabBar.tsx` | 创建 | 标签切换 |
| `app/src/views/ChangesView.tsx` | 创建 | 现有 更改 UI 迁入(逻辑不变) |
| `app/src/views/HistoryView.tsx` | 创建 | 三栏壳 |
| `app/src/components/CommitList.tsx` `CommitFileList.tsx` | 创建 | 提交轨列表 / 改动文件列表 |
| `app/src/App.tsx` | 改 | 外壳:仓库选择 + TabBar + 视图切换 |

> **Rust 概念铺垫**:
> - **Revwalk**:提交历史遍历器。`set_sorting(Sort::TIME)` 设成按时间排序,**必须在 `push_head()` 之前**(set_sorting 会重置遍历)。它实现了 `Iterator<Item=Result<Oid>>`,所以 `.skip(skip).take(limit)` 是**惰性**的——只遍历需要的那部分,大仓库不会爆。
> - **diff_tree_to_tree(old, new, opts)**:比较两棵 tree。首次提交无父 → old 传 `None`(和空树比)。合并提交有多个父 → 只跟 `parent(0)` 比(简化)。
> - **Delta**:每个改动文件的状态枚举。默认**不检测重命名**,改名会报成 Deleted+Added。

---

## Task 1: 脚手架(模型 + trait + 桩,保持可编译)

**Files:**
- Create: `crates/git-core/src/model/diff.rs`
- Modify: `crates/git-core/src/model/mod.rs`, `crates/git-core/src/backend.rs`, `crates/git-engine/src/git2_backend.rs`, `crates/git-engine/src/fake.rs`

- [ ] **Step 1: 新建 FileChange 模型**

创建 `crates/git-core/src/model/diff.rs`:
```rust
use serde::{Deserialize, Serialize};
use crate::model::FileState;

/// 一个提交里改动的单个文件(文件级,不含行)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub status: FileState,
}
```

- [ ] **Step 2: 导出**

`crates/git-core/src/model/mod.rs` 加:
```rust
mod diff;
pub use diff::FileChange;
```

- [ ] **Step 3: trait 加三方法**

`crates/git-core/src/backend.rs`：顶部 `use` 改为带 `FileChange`：
```rust
use crate::model::{Commit, WorkingTreeStatus, FileChange};
```
trait 内 `commit` 等方法之后加:
```rust
    /// 提交历史,时间倒序(新→旧)。limit/skip 分页。
    fn log(&self, repo: &Path, limit: usize, skip: usize) -> Result<Vec<Commit>, GitError>;

    /// 某提交相对第一个父的改动文件(文件级)。
    fn commit_files(&self, repo: &Path, commit_id: &str) -> Result<Vec<FileChange>, GitError>;

    /// 当前 HEAD 分支短名(如 "main");分离头/空仓库为 None。
    fn current_branch(&self, repo: &Path) -> Result<Option<String>, GitError>;
```

- [ ] **Step 4: Git2Backend 桩**

`crates/git-engine/src/git2_backend.rs` 顶部 `use` 引入 `FileChange`(改成):
```rust
use git_core::model::{Commit, FileChange, FileState, Signature, WorkingTreeStatus};
```
`impl GitBackend for Git2Backend` 末尾加:
```rust
    fn log(&self, _path: &Path, _limit: usize, _skip: usize) -> Result<Vec<Commit>, GitError> {
        todo!("Task 2")
    }
    fn commit_files(&self, _path: &Path, _commit_id: &str) -> Result<Vec<FileChange>, GitError> {
        todo!("Task 3")
    }
    fn current_branch(&self, _path: &Path) -> Result<Option<String>, GitError> {
        todo!("Task 4")
    }
```

- [ ] **Step 5: FakeBackend 桩**

`crates/git-engine/src/fake.rs` 顶部 `use git_core::model::{...}` 加 `FileChange`。`impl GitBackend for FakeBackend` 末尾加:
```rust
    fn log(&self, _path: &Path, _limit: usize, _skip: usize) -> Result<Vec<Commit>, GitError> {
        Ok(Vec::new())
    }
    fn commit_files(&self, _path: &Path, _commit_id: &str) -> Result<Vec<FileChange>, GitError> {
        Ok(Vec::new())
    }
    fn current_branch(&self, _path: &Path) -> Result<Option<String>, GitError> {
        Ok(None)
    }
```

- [ ] **Step 6: 编译**

Run: `cargo build`
Expected: 通过。

- [ ] **Step 7: Commit**
```bash
git add crates/git-core crates/git-engine/src/git2_backend.rs crates/git-engine/src/fake.rs
git commit -m "feat(1b-1): 脚手架 FileChange 模型 + trait log/commit_files/current_branch"
```

---

## Task 2: Git2Backend.log + build_commit 抽取(tempfile 测试)

**Files:** Modify `crates/git-engine/src/git2_backend.rs`

- [ ] **Step 1: 写测试模块的 fixture 辅助 + log 测试**

在 `crates/git-engine/src/git2_backend.rs` 末尾已有的 `#[cfg(test)] mod tests` 里(若该模块已存在,追加;函数 `init_repo`/`write` 可能已存在阶段 1 的版本——本任务新增**不冲突的**辅助 `stage`/`remove`/`commit_index`,并新增 log 测试)。追加:
```rust
    // ⚠️ 显式递增时间戳:不用系统时间,避免同秒提交导致 Sort::TIME flaky。
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

    /// 用显式时间戳把当前 index 提交。返回新 commit 的 SHA。
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
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &prefs).unwrap().to_string()
    }

    #[test]
    fn log_returns_commits_time_descending() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1"); commit_index(&repo, "c1", 1000);
        stage(&repo, "a.txt", "2"); commit_index(&repo, "c2", 2000);
        stage(&repo, "b.txt", "x"); commit_index(&repo, "c3", 3000);

        let log = b.log(&repo, 10, 0).unwrap();
        let msgs: Vec<&str> = log.iter().map(|c| c.summary.as_str()).collect();
        assert_eq!(msgs, vec!["c3", "c2", "c1"]); // 新→旧
    }

    #[test]
    fn log_paginates_lazily() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "1"); commit_index(&repo, "c1", 1000);
        stage(&repo, "a.txt", "2"); commit_index(&repo, "c2", 2000);
        stage(&repo, "a.txt", "3"); commit_index(&repo, "c3", 3000);

        let page = b.log(&repo, 1, 1).unwrap(); // 跳过 1 取 1 → 第二新
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].summary, "c2");
    }

    #[test]
    fn log_empty_repo_is_empty() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        assert!(b.log(&repo, 10, 0).unwrap().is_empty());
    }
```
> 注:`init_repo` 来自阶段 1 的测试(建临时仓库 + 配身份)。若现有 `init_repo` 已 `cfg.set_str("user.name"...)`,保留即可;本任务的提交用显式 `Signature` 不依赖它。

- [ ] **Step 2: 运行,确认失败**

Run: `cargo test -p git-engine log_`
Expected: FAIL（`todo!("Task 2")` panic）。

- [ ] **Step 3: 抽取 build_commit + 实现 log**

在 `git2_backend.rs` 顶部(`impl` 之前、`to_repo_relative` 附近)加私有函数:
```rust
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
```
把现有 `head_commit` 里构造 `Commit { ... }` 的整段替换为复用:
```rust
        let commit = head
            .peel_to_commit()
            .map_err(|e| GitError::Backend(e.to_string()))?;
        Ok(build_commit(&commit))
```
把 `log` 的 `todo!()` 替换为:
```rust
    fn log(&self, path: &Path, limit: usize, skip: usize) -> Result<Vec<Commit>, GitError> {
        let repo = git2::Repository::open(path)
            .map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let mut walk = repo.revwalk().map_err(|e| GitError::Backend(e.to_string()))?;
        // ⚠️ set_sorting 必须在 push_head 之前(set_sorting 会重置遍历)
        walk.set_sorting(git2::Sort::TIME)
            .map_err(|e| GitError::Backend(e.to_string()))?;
        match walk.push_head() {
            Ok(()) => {}
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Ok(Vec::new()),
            Err(e) => return Err(GitError::Backend(e.to_string())),
        }
        // ⚠️ Revwalk 是惰性迭代器:直接 skip/take,别先 collect 全部
        let mut out = Vec::new();
        for oid in walk.skip(skip).take(limit) {
            let oid = oid.map_err(|e| GitError::Backend(e.to_string()))?;
            let commit = repo.find_commit(oid).map_err(|e| GitError::Backend(e.to_string()))?;
            out.push(build_commit(&commit));
        }
        Ok(out)
    }
```

- [ ] **Step 4: 运行,确认通过**

Run: `cargo test -p git-engine log_`
Expected: PASS（3 个）。再 `cargo test -p git-engine` 确认 head_commit 等旧测试不破。

- [ ] **Step 5: Commit**
```bash
git add crates/git-engine/src/git2_backend.rs
git commit -m "feat(1b-1): Git2Backend.log(Revwalk+Sort::TIME+惰性分页) + build_commit 抽取"
```

---

## Task 3: Git2Backend.commit_files(tempfile 测试)

**Files:** Modify `crates/git-engine/src/git2_backend.rs`

- [ ] **Step 1: 写测试**

在 `mod tests` 内加:
```rust
    use git_core::model::FileState;

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
        stage(&repo, "a.txt", "v1"); commit_index(&repo, "c1", 1000);
        stage(&repo, "a.txt", "v2"); stage(&repo, "b.txt", "x");
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
        stage(&repo, "a.txt", "v1"); commit_index(&repo, "c1", 1000);
        remove(&repo, "a.txt");
        let sha = commit_index(&repo, "c2", 2000);
        let files = b.commit_files(&repo, &sha).unwrap();
        assert_eq!(files.iter().find(|f| f.path == "a.txt").unwrap().status, FileState::Deleted);
    }

    #[test]
    fn commit_files_rename_reported_as_delete_plus_add() {
        // 1b-1 不检测重命名:改名报成 Deleted(旧)+ Added(新)
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "same"); commit_index(&repo, "c1", 1000);
        remove(&repo, "a.txt"); stage(&repo, "c.txt", "same");
        let sha = commit_index(&repo, "c2", 2000);
        let files = b.commit_files(&repo, &sha).unwrap();
        let find = |p: &str| files.iter().find(|f| f.path == p).map(|f| f.status);
        assert_eq!(find("a.txt"), Some(FileState::Deleted));
        assert_eq!(find("c.txt"), Some(FileState::Added));
        assert!(files.iter().all(|f| f.status != FileState::Renamed));
    }
```

- [ ] **Step 2: 运行,确认失败**

Run: `cargo test -p git-engine commit_files_`
Expected: FAIL（todo! panic）。

- [ ] **Step 3: 实现 commit_files**

把 `commit_files` 的 `todo!()` 替换为:
```rust
    fn commit_files(&self, path: &Path, commit_id: &str) -> Result<Vec<FileChange>, GitError> {
        let repo = git2::Repository::open(path)
            .map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        let oid = git2::Oid::from_str(commit_id).map_err(|e| GitError::Backend(e.to_string()))?;
        let commit = repo.find_commit(oid).map_err(|e| GitError::Backend(e.to_string()))?;
        let new_tree = commit.tree().map_err(|e| GitError::Backend(e.to_string()))?;
        // ⚠️ 坑1:首提交无父 → None(和空树 diff)。坑2:合并只跟第一个父。
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
                // ⚠️ 坑5:不开重命名检测,此分支当前不命中(保留无害)
                git2::Delta::Renamed => FileState::Renamed,
                git2::Delta::Modified | git2::Delta::Typechange => FileState::Modified,
                _ => continue,
            };
            // 删除文件 new_file().path() 为 None → 回退 old_file()
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
```

- [ ] **Step 4: 运行,确认通过**

Run: `cargo test -p git-engine commit_files_`
Expected: PASS（4 个）。

- [ ] **Step 5: Commit**
```bash
git add crates/git-engine/src/git2_backend.rs
git commit -m "feat(1b-1): Git2Backend.commit_files(diff_tree_to_tree,文件级,不检测重命名)"
```

---

## Task 4: Git2Backend.current_branch

**Files:** Modify `crates/git-engine/src/git2_backend.rs`

- [ ] **Step 1: 写测试**
```rust
    #[test]
    fn current_branch_some_after_commit() {
        let (_tmp, repo) = init_repo();
        let b = Git2Backend;
        stage(&repo, "a.txt", "x"); commit_index(&repo, "c1", 1000);
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
```

- [ ] **Step 2: 运行,确认失败**

Run: `cargo test -p git-engine current_branch`
Expected: FAIL。

- [ ] **Step 3: 实现**
```rust
    fn current_branch(&self, path: &Path) -> Result<Option<String>, GitError> {
        let repo = git2::Repository::open(path)
            .map_err(|e| GitError::RepoNotFound(e.to_string()))?;
        match repo.head() {
            Ok(head) => Ok(head.shorthand().map(|s| s.to_string())),
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(None),
            Err(e) => Err(GitError::Backend(e.to_string())),
        }
    }
```

- [ ] **Step 4: 运行,确认通过 + 全 git-engine 测试**

Run: `cargo test -p git-engine`
Expected: PASS（log/commit_files/current_branch + 阶段 1 旧测试全绿）。

- [ ] **Step 5: Commit**
```bash
git add crates/git-engine/src/git2_backend.rs
git commit -m "feat(1b-1): Git2Backend.current_branch(repo.head.shorthand)"
```

---

## Task 5: FakeBackend canned 实现

**Files:** Modify `crates/git-engine/src/fake.rs`

- [ ] **Step 1: 写测试**

在 `fake.rs` 的 `mod tests` 内加:
```rust
    use git_core::model::{Commit, FileChange, FileState, Signature};

    #[test]
    fn fake_returns_canned_log_and_files() {
        let commit = Commit {
            id: "x".into(), short_id: "x".into(), summary: "s".into(), body: "".into(),
            author: Signature { name: "n".into(), email: "e".into() }, timestamp: 1, parents: vec![],
        };
        let fb = FakeBackend::default()
            .with_log(vec![commit])
            .with_commit_files(vec![FileChange { path: "a".into(), status: FileState::Added }])
            .with_branch(Some("main".into()));
        assert_eq!(fb.log(Path::new("/r"), 10, 0).unwrap().len(), 1);
        assert_eq!(fb.commit_files(Path::new("/r"), "x").unwrap()[0].path, "a");
        assert_eq!(fb.current_branch(Path::new("/r")).unwrap(), Some("main".into()));
    }
```

- [ ] **Step 2: 运行,确认失败**

Run: `cargo test -p git-engine --no-default-features fake_returns_canned`
Expected: FAIL（`no method named with_log`）。

- [ ] **Step 3: 加字段 + setter + 实现**

`fake.rs` 顶部 `use git_core::model::{...}` 加 `FileChange`。结构体加字段:
```rust
    canned_log: Mutex<Vec<Commit>>,
    canned_commit_files: Mutex<Vec<FileChange>>,
    canned_branch: Mutex<Option<String>>,
```
`impl FakeBackend` 加链式 setter:
```rust
    pub fn with_log(self, commits: Vec<Commit>) -> Self {
        *self.canned_log.lock().unwrap() = commits; self
    }
    pub fn with_commit_files(self, files: Vec<FileChange>) -> Self {
        *self.canned_commit_files.lock().unwrap() = files; self
    }
    pub fn with_branch(self, branch: Option<String>) -> Self {
        *self.canned_branch.lock().unwrap() = branch; self
    }
```
把 Task 1 的三个桩替换为:
```rust
    fn log(&self, _path: &Path, _limit: usize, _skip: usize) -> Result<Vec<Commit>, GitError> {
        Ok(self.canned_log.lock().unwrap().clone())
    }
    fn commit_files(&self, _path: &Path, _commit_id: &str) -> Result<Vec<FileChange>, GitError> {
        Ok(self.canned_commit_files.lock().unwrap().clone())
    }
    fn current_branch(&self, _path: &Path) -> Result<Option<String>, GitError> {
        Ok(self.canned_branch.lock().unwrap().clone())
    }
```

- [ ] **Step 4: 运行,确认通过**

Run: `cargo test -p git-engine --no-default-features`
Expected: PASS。

- [ ] **Step 5: Commit**
```bash
git add crates/git-engine/src/fake.rs
git commit -m "feat(1b-1): FakeBackend canned log/commit_files/branch + setter"
```

---

## Task 6: ipc-types FileChangeDto

**Files:** Modify `crates/ipc-types/src/lib.rs`

- [ ] **Step 1: 写测试**

`mod tests` 内加:
```rust
    use git_core::model::{FileChange, FileState};

    #[test]
    fn maps_file_change_to_dto() {
        let dto = FileChangeDto::from(FileChange { path: "a.rs".into(), status: FileState::Deleted });
        assert_eq!(dto.path, "a.rs");
        assert_eq!(dto.status, "deleted");
    }
```

- [ ] **Step 2: 运行,确认失败**

Run: `cargo test -p ipc-types maps_file_change`
Expected: FAIL（`cannot find type FileChangeDto`）。

- [ ] **Step 3: 实现**

顶部 `use` 加 `FileChange`(改 `use git_core::model::{Commit, WorkingTreeStatus, FileEntry, FileState, FileChange};`)。文件末尾(tests 前)加:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeDto {
    pub path: String,
    pub status: String, // added | modified | deleted | renamed
}

impl From<FileChange> for FileChangeDto {
    fn from(c: FileChange) -> Self {
        let status = match c.status {
            FileState::Added => "added",
            FileState::Modified => "modified",
            FileState::Deleted => "deleted",
            FileState::Renamed => "renamed",
            FileState::Untracked => "untracked",
            FileState::Conflicted => "conflicted",
        };
        FileChangeDto { path: c.path, status: status.to_string() }
    }
}
```

- [ ] **Step 4: 运行,确认通过**

Run: `cargo test -p ipc-types`
Expected: PASS。

- [ ] **Step 5: Commit**
```bash
git add crates/ipc-types/src/lib.rs
git commit -m "feat(1b-1): ipc-types FileChangeDto + From 映射"
```

---

## Task 7: app-service RepoService 三方法

**Files:** Modify `crates/app-service/src/lib.rs`

- [ ] **Step 1: 写测试**

`mod tests` 内加:
```rust
    use git_core::model::{Commit, FileChange, FileState, Signature};

    fn fake_commit(summary: &str) -> Commit {
        Commit { id: "i".into(), short_id: "i".into(), summary: summary.into(), body: "".into(),
            author: Signature { name: "n".into(), email: "e".into() }, timestamp: 1, parents: vec![] }
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
    fn commit_files_maps_dto() {
        let fb = FakeBackend::default()
            .with_commit_files(vec![FileChange { path: "a".into(), status: FileState::Modified }]);
        let svc = RepoService::new(Arc::new(fb));
        let dtos = svc.commit_files(Path::new("/r"), "x").unwrap();
        assert_eq!(dtos[0].status, "modified");
    }

    #[test]
    fn current_branch_forwards() {
        let fb = FakeBackend::default().with_branch(Some("main".into()));
        let svc = RepoService::new(Arc::new(fb));
        assert_eq!(svc.current_branch(Path::new("/r")).unwrap(), Some("main".into()));
    }
```

- [ ] **Step 2: 运行,确认失败**

Run: `cargo test -p app-service --no-default-features log_returns_commit_dtos`
Expected: FAIL（`no method named log`）。

- [ ] **Step 3: 实现**

顶部 `use ipc_types::{...}` 加 `CommitDto, FileChangeDto`(现有 `StatusDto` 也保留)。`impl RepoService` 内加:
```rust
    pub fn log(&self, repo_path: &Path, limit: usize, skip: usize) -> Result<Vec<CommitDto>, GitError> {
        let commits = self.backend.log(repo_path, limit, skip)?;
        Ok(commits.into_iter().map(CommitDto::from).collect())
    }

    pub fn commit_files(&self, repo_path: &Path, commit_id: &str) -> Result<Vec<FileChangeDto>, GitError> {
        let files = self.backend.commit_files(repo_path, commit_id)?;
        Ok(files.into_iter().map(FileChangeDto::from).collect())
    }

    pub fn current_branch(&self, repo_path: &Path) -> Result<Option<String>, GitError> {
        self.backend.current_branch(repo_path)
    }
```
> `CommitDto: From<Commit>` 已存在(阶段 0)。

- [ ] **Step 4: 运行,确认通过**

Run: `cargo test -p app-service --no-default-features`
Expected: PASS（含旧测试）。

- [ ] **Step 5: Commit**
```bash
git add crates/app-service/src/lib.rs
git commit -m "feat(1b-1): RepoService.log/commit_files/current_branch + 测试"
```

---

## Task 8: src-tauri 三命令

**Files:** Modify `app/src-tauri/src/lib.rs`

- [ ] **Step 1: 加命令**

顶部 `use ipc_types::{...}` 加 `FileChangeDto`。在现有命令后加:
```rust
#[tauri::command]
async fn get_log(repo_path: String, limit: usize, skip: usize) -> Result<Vec<CommitDto>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend::default()));
        service.log(&PathBuf::from(repo_path), limit, skip)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_commit_files(repo_path: String, commit_id: String) -> Result<Vec<FileChangeDto>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend::default()));
        service.commit_files(&PathBuf::from(repo_path), &commit_id)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_current_branch(repo_path: String) -> Result<Option<String>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend::default()));
        service.current_branch(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}
```

- [ ] **Step 2: 注册**

`generate_handler!` 里加 `get_log, get_commit_files, get_current_branch`。

- [ ] **Step 3: 编译**

Run: `cargo build -p app`
Expected: 通过。

- [ ] **Step 4: Commit**
```bash
git add app/src-tauri/src/lib.rs
git commit -m "feat(1b-1): Tauri 命令 get_log/get_commit_files/get_current_branch"
```

---

## Task 9: 前端 Tailwind v4 接入

**Files:** Modify `app/package.json` (via pnpm), `app/vite.config.ts`, create `app/src/index.css`, modify `app/src/main.tsx`

- [ ] **Step 1: 安装**

Run: `cd app && pnpm add -D tailwindcss @tailwindcss/vite`

- [ ] **Step 2: vite 插件**

`app/vite.config.ts`:import `tailwindcss from "@tailwindcss/vite"`,加进 `plugins: [react(), tailwindcss()]`(保留现有 react 插件)。

- [ ] **Step 3: CSS 入口**

创建 `app/src/index.css`:
```css
@import "tailwindcss";
```

- [ ] **Step 4: main.tsx import**

`app/src/main.tsx` 顶部加 `import "./index.css";`。

- [ ] **Step 5: 冒烟验证 Tailwind 生效**

临时在 `App.tsx` 根节点加一个 `className="text-red-500"` 试,`cd app && pnpm build`,预期构建成功(确认 Tailwind 接入无误后移除该临时 class)。

- [ ] **Step 6: Commit**
```bash
git add app/package.json app/pnpm-lock.yaml app/vite.config.ts app/src/index.css app/src/main.tsx
git commit -m "chore(1b-1): 接入 Tailwind v4(@tailwindcss/vite)"
```

---

## Task 10: 前端 ipc.ts + 相对时间

**Files:** Modify `app/src/ipc.ts`, create `app/src/lib/time.ts`

- [ ] **Step 1: ipc.ts 加类型与函数**

`app/src/ipc.ts` 末尾加:
```ts
export interface FileChangeDto {
  path: string;
  status: string; // added | modified | deleted | renamed
}

export async function getLog(repoPath: string, limit: number, skip: number): Promise<CommitDto[]> {
  return await invoke<CommitDto[]>("get_log", { repoPath, limit, skip });
}

export async function getCommitFiles(repoPath: string, commitId: string): Promise<FileChangeDto[]> {
  return await invoke<FileChangeDto[]>("get_commit_files", { repoPath, commitId });
}

export async function getCurrentBranch(repoPath: string): Promise<string | null> {
  return await invoke<string | null>("get_current_branch", { repoPath });
}
```

- [ ] **Step 2: 相对时间 helper**

创建 `app/src/lib/time.ts`:
```ts
/** Unix 秒 → 中文相对时间("刚刚"/"3 分钟前"/"2 小时前"/"5 天前"/日期)。 */
export function formatRelative(unixSeconds: number): string {
  const diff = Date.now() / 1000 - unixSeconds;
  if (diff < 60) return "刚刚";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)} 天前`;
  return new Date(unixSeconds * 1000).toLocaleDateString();
}
```

- [ ] **Step 3: 类型检查**

Run: `cd app && pnpm tsc --noEmit`
Expected: 退出码 0。

- [ ] **Step 4: Commit**
```bash
git add app/src/ipc.ts app/src/lib/time.ts
git commit -m "feat(1b-1): 前端 ipc getLog/getCommitFiles/getCurrentBranch + 相对时间"
```

---

## Task 11: 前端 TabBar + App 外壳 + ChangesView 迁移

**Files:** create `app/src/components/TabBar.tsx`, `app/src/views/ChangesView.tsx`; modify `app/src/App.tsx`

- [ ] **Step 1: TabBar**

创建 `app/src/components/TabBar.tsx`:
```tsx
export type Tab = "changes" | "history";

export function TabBar({ active, onChange }: { active: Tab; onChange: (t: Tab) => void }) {
  const item = (t: Tab, label: string) =>
    `px-4 py-2 text-sm cursor-pointer ${
      active === t ? "text-[#e6edf3] border-b-2 border-[#3b82f6] font-semibold" : "text-[#8b949e]"
    }`;
  return (
    <div className="flex gap-1 border-b border-[#21262d] px-2">
      <div className={item("changes", "")} onClick={() => onChange("changes")}>更改</div>
      <div className={item("history", "")} onClick={() => onChange("history")}>历史</div>
    </div>
  );
}
```

- [ ] **Step 2: ChangesView(把现有 App 的 status/stage/commit 逻辑搬过来,逻辑不变,Tailwind 重写样式)**

创建 `app/src/views/ChangesView.tsx`:
```tsx
import { useEffect, useState } from "react";
import { getStatus, stageFile, unstageFile, commit, type StatusDto, type FileEntryDto, type IpcError } from "../ipc";

export function ChangesView({ repo }: { repo: string }) {
  const [status, setStatus] = useState<StatusDto | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    setError(null);
    try { setStatus(await getStatus(repo)); }
    catch (e) { setError((e as IpcError).message ?? String(e)); }
  }
  useEffect(() => { refresh(); /* eslint-disable-next-line */ }, [repo]);

  async function run(action: () => Promise<void>) {
    setBusy(true); setError(null);
    try { await action(); await refresh(); }
    catch (e) { setError((e as IpcError).message ?? String(e)); }
    finally { setBusy(false); }
  }

  async function doCommit() {
    setBusy(true); setError(null);
    try {
      const sha = await commit(repo, message);
      setInfo(`已提交 ${sha.slice(0, 7)}`); setMessage(""); await refresh();
    } catch (e) { setError((e as IpcError).message ?? String(e)); }
    finally { setBusy(false); }
  }

  const staged = status?.entries.filter((e) => e.staged) ?? [];
  const unstaged = status?.entries.filter((e) => !e.staged) ?? [];

  const Row = ({ entry, isStaged }: { entry: FileEntryDto; isStaged: boolean }) => (
    <li className="flex items-center gap-2 px-3 py-1 hover:bg-[#161b22]">
      <span className="text-xs text-[#8b949e] w-20">{entry.state}</span>
      <span className="flex-1 font-mono text-sm text-[#c9d1d9] truncate">{entry.path}</span>
      <button className="text-xs text-[#58a6ff] disabled:opacity-40" disabled={busy}
        onClick={() => run(() => (isStaged ? unstageFile(repo, entry.path) : stageFile(repo, entry.path)))}>
        {isStaged ? "取消暂存" : "暂存"}
      </button>
    </li>
  );

  return (
    <div className="p-4 max-w-2xl">
      <button className="text-sm text-[#58a6ff]" disabled={busy} onClick={refresh}>刷新</button>
      {error && <p className="text-[#f85149] mt-2">错误:{error}</p>}
      {info && <p className="text-[#3fb950] mt-2">{info}</p>}

      <h3 className="text-[#e6edf3] font-semibold mt-4 mb-1">已暂存 ({staged.length})</h3>
      <ul>{staged.map((e) => <Row key={e.path} entry={e} isStaged />)}{staged.length === 0 && <li className="text-[#6e7681] px-3">(空)</li>}</ul>

      <h3 className="text-[#e6edf3] font-semibold mt-4 mb-1">未暂存 ({unstaged.length})</h3>
      <ul>{unstaged.map((e) => <Row key={e.path} entry={e} isStaged={false} />)}{unstaged.length === 0 && <li className="text-[#6e7681] px-3">(空)</li>}</ul>

      <h3 className="text-[#e6edf3] font-semibold mt-4 mb-1">提交</h3>
      <textarea className="w-full bg-[#0d1117] border border-[#21262d] rounded text-[#e6edf3] p-2 text-sm font-mono"
        rows={3} placeholder="提交信息" value={message} onChange={(e) => setMessage(e.target.value)} />
      <button className="mt-2 px-3 py-1.5 text-sm rounded bg-[#238636] text-white disabled:opacity-40"
        disabled={busy || staged.length === 0 || message.trim() === ""} onClick={doCommit}>
        提交 {staged.length} 个改动
      </button>
    </div>
  );
}
```

- [ ] **Step 3: App 外壳(仓库选择 + TabBar + 视图切换)**

把 `app/src/App.tsx` 整体替换为:
```tsx
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { TabBar, type Tab } from "./components/TabBar";
import { ChangesView } from "./views/ChangesView";
import { HistoryView } from "./views/HistoryView";

export default function App() {
  const [repo, setRepo] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("changes");

  async function pickRepo() {
    const dir = await open({ directory: true, title: "选择一个 git 仓库" });
    if (typeof dir === "string") setRepo(dir);
  }

  return (
    <main className="min-h-screen bg-[#0d1117] text-[#e6edf3] font-sans">
      <header className="flex items-center gap-3 px-4 py-3 border-b border-[#21262d]">
        <h1 className="text-lg font-bold">Git 客户端</h1>
        <button className="text-sm px-2 py-1 rounded bg-[#21262d] text-[#c9d1d9]" onClick={pickRepo}>选择仓库</button>
        {repo && <span className="text-xs text-[#8b949e] font-mono truncate">{repo}</span>}
      </header>
      {repo && <TabBar active={tab} onChange={setTab} />}
      {repo
        ? (tab === "changes" ? <ChangesView repo={repo} /> : <HistoryView repo={repo} />)
        : <p className="p-6 text-[#8b949e]">选择一个本地 git 仓库开始。</p>}
    </main>
  );
}
```
> 注:此步引用了下个任务才创建的 `HistoryView`。**本任务先创建一个最小占位 `HistoryView`** 让编译通过:创建 `app/src/views/HistoryView.tsx`:
```tsx
export function HistoryView({ repo: _repo }: { repo: string }) {
  return <p className="p-6 text-[#8b949e]">历史视图(Task 12 实现)</p>;
}
```

- [ ] **Step 4: 类型检查 + 构建**

Run: `cd app && pnpm tsc --noEmit && pnpm build`
Expected: 退出码 0。

- [ ] **Step 5: Commit**
```bash
git add app/src/App.tsx app/src/components/TabBar.tsx app/src/views/ChangesView.tsx app/src/views/HistoryView.tsx
git commit -m "feat(1b-1): TabBar + App 外壳 + ChangesView 迁移(Tailwind)"
```

---

## Task 12: 前端 HistoryView 三栏 + CommitList + CommitFileList

**Files:** create `app/src/components/CommitList.tsx`, `app/src/components/CommitFileList.tsx`; replace `app/src/views/HistoryView.tsx`

- [ ] **Step 1: CommitList(提交轨 + HEAD 徽章 + 加载更多)**

创建 `app/src/components/CommitList.tsx`:
```tsx
import { type CommitDto } from "../ipc";
import { formatRelative } from "../lib/time";

export function CommitList({
  commits, branch, selectedId, onSelect, onLoadMore, loading,
}: {
  commits: CommitDto[]; branch: string | null; selectedId: string | null;
  onSelect: (c: CommitDto) => void; onLoadMore: () => void; loading: boolean;
}) {
  return (
    <div className="overflow-y-auto">
      {commits.map((c, i) => (
        <div key={c.id} onClick={() => onSelect(c)}
          className={`flex gap-2 px-3 py-2 cursor-pointer ${selectedId === c.id ? "bg-[#161b22] border-l-2 border-[#3b82f6]" : "border-l-2 border-transparent hover:bg-[#161b22]"}`}>
          <div className="flex flex-col items-center w-3 pt-1">
            <div className={`w-2.5 h-2.5 rounded-full ${i === 0 ? "bg-[#3b82f6]" : "bg-[#8b949e]"}`} />
            <div className="flex-1 w-px bg-[#30363d] mt-1" />
          </div>
          <div className="min-w-0 flex-1">
            {i === 0 && (
              <span className="inline-block text-[10px] text-[#3fb950] bg-[#10311a] border border-[#1f6f33] rounded-full px-1.5 mb-1">
                HEAD{branch ? ` → ${branch}` : ""}
              </span>
            )}
            <div className="text-sm text-[#e6edf3] truncate">{c.summary}</div>
            <div className="text-[11px] text-[#8b949e] font-mono">{c.short_id} · {formatRelative(c.timestamp)}</div>
          </div>
        </div>
      ))}
      <button className="w-full py-2 text-xs text-[#58a6ff] disabled:opacity-40" onClick={onLoadMore} disabled={loading}>
        {loading ? "加载中…" : "加载更多"}
      </button>
    </div>
  );
}
```

- [ ] **Step 2: CommitFileList(状态色标)**

创建 `app/src/components/CommitFileList.tsx`:
```tsx
import { type FileChangeDto } from "../ipc";

const COLOR: Record<string, string> = {
  added: "text-[#3fb950]", modified: "text-[#58a6ff]", deleted: "text-[#f85149]", renamed: "text-[#d29922]",
};
const LETTER: Record<string, string> = { added: "A", modified: "M", deleted: "D", renamed: "R" };

export function CommitFileList({
  files, selected, onSelect,
}: { files: FileChangeDto[]; selected: string | null; onSelect: (path: string) => void }) {
  if (files.length === 0) return <div className="p-3 text-xs text-[#6e7681]">无改动文件</div>;
  return (
    <div className="overflow-y-auto">
      {files.map((f) => (
        <div key={f.path} onClick={() => onSelect(f.path)}
          className={`flex items-center gap-2 px-3 py-1.5 cursor-pointer font-mono text-sm ${selected === f.path ? "bg-[#161b22]" : "hover:bg-[#161b22]"}`}>
          <span className={`w-4 ${COLOR[f.status] ?? "text-[#8b949e]"}`}>{LETTER[f.status] ?? "?"}</span>
          <span className="truncate text-[#c9d1d9]">{f.path}</span>
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 3: HistoryView(三栏壳 + 数据)**

替换 `app/src/views/HistoryView.tsx`:
```tsx
import { useEffect, useState } from "react";
import { getLog, getCommitFiles, getCurrentBranch, type CommitDto, type FileChangeDto, type IpcError } from "../ipc";
import { CommitList } from "../components/CommitList";
import { CommitFileList } from "../components/CommitFileList";

const PAGE = 50;

export function HistoryView({ repo }: { repo: string }) {
  const [commits, setCommits] = useState<CommitDto[]>([]);
  const [branch, setBranch] = useState<string | null>(null);
  const [selected, setSelected] = useState<CommitDto | null>(null);
  const [files, setFiles] = useState<FileChangeDto[]>([]);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function loadPage(skip: number) {
    setLoading(true); setError(null);
    try {
      const page = await getLog(repo, PAGE, skip);
      setCommits((prev) => (skip === 0 ? page : [...prev, ...page]));
    } catch (e) { setError((e as IpcError).message ?? String(e)); }
    finally { setLoading(false); }
  }

  useEffect(() => {
    setCommits([]); setSelected(null); setFiles([]); setSelectedFile(null);
    loadPage(0);
    getCurrentBranch(repo).then(setBranch).catch(() => setBranch(null));
    // eslint-disable-next-line
  }, [repo]);

  async function selectCommit(c: CommitDto) {
    setSelected(c); setSelectedFile(null); setFiles([]);
    try { setFiles(await getCommitFiles(repo, c.id)); }
    catch (e) { setError((e as IpcError).message ?? String(e)); }
  }

  return (
    <div className="flex h-[calc(100vh-6.5rem)]">
      <aside className="w-72 border-r border-[#21262d] overflow-hidden flex flex-col">
        {error && <p className="text-[#f85149] text-xs p-2">{error}</p>}
        <CommitList commits={commits} branch={branch} selectedId={selected?.id ?? null}
          onSelect={selectCommit} onLoadMore={() => loadPage(commits.length)} loading={loading} />
      </aside>
      <div className="w-64 border-r border-[#21262d] overflow-hidden flex flex-col">
        <div className="px-3 py-2 text-[11px] uppercase tracking-wide text-[#8b949e] border-b border-[#21262d]">改动文件</div>
        {selected
          ? <CommitFileList files={files} selected={selectedFile} onSelect={setSelectedFile} />
          : <div className="p-3 text-xs text-[#6e7681]">选择一个提交</div>}
      </div>
      <main className="flex-1 overflow-auto p-4">
        <div className="text-[11px] uppercase tracking-wide text-[#8b949e] mb-2">Diff</div>
        <div className="text-sm text-[#6e7681]">
          {selectedFile ? `${selectedFile} 的行级 diff 将在 1b-2 显示` : "选择一个文件查看 diff(1b-2)"}
        </div>
      </main>
    </div>
  );
}
```

- [ ] **Step 4: 类型检查 + 构建**

Run: `cd app && pnpm tsc --noEmit && pnpm build`
Expected: 退出码 0。

- [ ] **Step 5: Commit**
```bash
git add app/src/components/CommitList.tsx app/src/components/CommitFileList.tsx app/src/views/HistoryView.tsx
git commit -m "feat(1b-1): HistoryView 三栏 + CommitList(提交轨/HEAD徽章) + CommitFileList"
```

---

## Task 13: 全量验收(控制器执行)

- [ ] **Step 1: 后端测试** — Run: `cargo test` — 覆盖 spec 第 6 节用例全绿。
- [ ] **Step 2: 构建 + lint** — Run: `cargo build && cargo clippy --workspace --all-targets`(0 warning)`&& cargo fmt --check`。
- [ ] **Step 3: 前端** — Run: `cd app && pnpm tsc --noEmit && pnpm build`。
- [ ] **Step 4: 手动冒烟(用户本机)** — `cd app && pnpm tauri dev`:切「历史」→ 暗色提交轨列表 → "加载更多" → 点提交看改动文件(A/M/D 色标)→ 右栏占位;「更改」标签原功能正常。空仓库历史为空不报错。

---

## 自查记录(写计划时已核对)
- **Spec 覆盖**:log(T2)、commit_files(T3)、current_branch(T4)、FakeBackend canned(T5)、DTO(T6)、app-service(T7)、命令(T8)、Tailwind(T9)、ipc/time(T10)、TabBar/ChangesView 迁移(T11)、HistoryView 三栏(T12)。9 个测试用例:log 顺序/分页/空(T2)、commit_files 首/改增/删/重命名(T3)、current_branch 有/无(T4)。
- **七个 git2 坑**:①T3 parent None ②T3 parent(0) ③T2 set_sorting 在 push_head 前 ④T2 skip/take 惰性 ⑤T3 Renamed 不命中 + T3 重命名测试 ⑥T2 commit_index 显式 Time ⑦T4 shorthand。
- **类型一致**:`FileChange`/`FileChangeDto`/`getLog`/`getCommitFiles`/`getCurrentBranch`/`CommitDto` 前后端命名一致;命令参数 `repoPath`/`commitId`/`limit`/`skip` 与 Rust `repo_path`/`commit_id`(Tauri 驼峰)对应;`current_branch` 返回 `Option<String>`↔`string | null`。
- **已知限制**:重命名报删+增(已固化测试);diff 列占位留 1b-2。
