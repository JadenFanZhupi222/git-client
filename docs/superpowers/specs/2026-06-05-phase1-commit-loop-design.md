# 阶段 1 设计:核心提交回路(Commit Loop)

> 状态:已与用户确认,待实现
> 日期:2026-06-05
> 前置:阶段 0 已贯通"前端→Tauri→app-service→git-engine→git2"读 HEAD 全链路
> 路线图:见 ARCHITECTURE.md 第 13 部分(本 spec 是其阶段 1 的第一个聚焦切片)

---

## 1. 目标与范围

实现最小可用的**提交回路**:用户看到工作区改动 → 文件级暂存/取消暂存 → 写提交信息 → 提交。

**本次包含:**
- `status`(已在 Git2Backend 实现,本次只需接出来)
- 文件级 `stage` / `unstage`
- `commit`
- 前端:文件列表(已暂存 / 未暂存两区)+ 提交框

**明确不包含(拆到后续切片):**
- commit log 列表、单文件 diff → 阶段 1b
- 文件系统监听(notify/debounce/gitignore)→ 阶段 1c。本次刷新策略 = **操作后主动重拉 status + 手动刷新按钮**
- 行级暂存、index/worktree 双状态精确模型 → 阶段 2

### 已确认的关键决策

| 决策 | 选择 | 理由 |
|---|---|---|
| 架构推进力度 | 保持无状态 RepoService,trait 方法收 `&Path` 每次重开仓库 | 学习点集中,阶段 1 最快跑通;actor 留到阶段 2/3 |
| 文件状态模型 | 简单版 `FileEntry { path, state, staged }`,一文件一状态 | 模型/UI 简单;index/worktree 双状态留到阶段 2 |
| 刷新策略 | 操作后主动刷新 + 手动按钮,无文件监听 | 监听器复杂度高,延后 |
| 前端状态 | 普通 React hooks(useState + refreshStatus),不引库 | 开发者是 React 老手,YAGNI |

---

## 2. 数据流

每个操作都走同一条链路,git 操作一律在 `spawn_blocking` 里执行(项目铁律):

```
前端按钮 → ipc.ts → Tauri 命令(async + spawn_blocking) → RepoService → GitBackend trait → Git2Backend(git2)
                                                                                              ↓
前端渲染 ← DTO(serde) ← IpcError/DTO ←─────────────────────────────────────────── 领域模型/GitError
```

---

## 3. 分层改动

### 3.1 git-core(领域层)

**trait `GitBackend` 加 3 个方法**(`status` 已存在,不动):

```rust
/// 文件级暂存:把工作区某文件的当前内容加入 index。
fn stage(&self, repo: &Path, file: &Path) -> Result<(), GitError>;

/// 取消暂存:把某文件从 index 撤回(分有无 HEAD 两种语义,见 3.2)。
fn unstage(&self, repo: &Path, file: &Path) -> Result<(), GitError>;

/// 提交 index 内容,返回新 commit 的完整 SHA。
fn commit(&self, repo: &Path, message: &str) -> Result<String, GitError>;
```

**error.rs 加两个变体**(给前端清晰错误码):

```rust
#[error("没有已暂存的改动可提交")]
NothingToCommit,

#[error("git 身份未配置,请先设置 user.name / user.email")]
EmptySignature,
```

领域模型 `WorkingTreeStatus` / `FileEntry` / `FileState` 已存在,本阶段不改。

### 3.2 git-engine(适配器层)— Git2Backend 实现

**`stage`** — 文件级(非行级):
```
开 repo → index = repo.index() → index.add_path(file) → index.write()
```

**`unstage`** — ⚠️ 必须区分有无 HEAD:
- **有 HEAD**(仓库已有提交):`repo.reset_default(Some(&head_commit_obj), [file])`,把该文件的 index 条目重置回 HEAD 版本。
- **无 HEAD**(空仓库 / 首次提交前,reset 没有目标可重置):`index.remove_path(file)` + `index.write()`,直接把条目从 index 删除。
- 判断方式:`repo.head()` 是否 Err / `repo.head_detached()` / 检查 unborn 分支。

**`commit`** — ⚠️ 首次提交传空 parents:
```
index = repo.index()
tree_oid = index.write_tree();  tree = repo.find_tree(tree_oid)
sig = repo.signature()?     // 读 git config 的 user.name/email,失败 → EmptySignature
若有 HEAD:  parents = &[&head_commit];  目标 = Some("HEAD")
若无 HEAD:  parents = &[];              目标 = Some("HEAD")  // 首次提交
new_oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, parents)
返回 new_oid.to_string()
```

**`FakeBackend` 改造** — 从无状态死数据升级为**带内部状态**,供 TDD 断言:
```rust
pub struct FakeBackend {
    // ⚠️ 必须用 Mutex 不能用 RefCell:
    // GitBackend 要求 Send + Sync,FakeBackend 会作为 Arc<dyn GitBackend>
    // 丢进 spawn_blocking 跨线程使用。RefCell 是 !Sync,编译都过不了。
    // Mutex 提供跨线程内部可变性,满足 Sync。
    staged: Mutex<Vec<PathBuf>>,
    commits: Mutex<Vec<String>>,   // 记录提交信息,供断言
    // 可选:预置的 status 返回值
}
```
stage/unstage/commit 改 `staged`/`commits` 记录;commit 返回一个确定的假 SHA。

### 3.3 ipc-types(契约层)— 加 status 的 DTO

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntryDto {
    pub path: String,
    pub state: String,   // "modified" | "added" | "deleted" | "untracked" | "conflicted"
    pub staged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusDto {
    pub entries: Vec<FileEntryDto>,
}
```
`FileState` → 字符串的映射放在 `From<WorkingTreeStatus> for StatusDto`(或 service 层)。

### 3.4 app-service(应用层)— RepoService 加 4 个方法

保持无状态,薄转发 + 领域→DTO 映射:
```rust
fn status(&self, repo_path: &Path) -> Result<StatusDto, GitError>;
fn stage(&self, repo_path: &Path, file: &Path) -> Result<(), GitError>;
fn unstage(&self, repo_path: &Path, file: &Path) -> Result<(), GitError>;
fn commit(&self, repo_path: &Path, message: &str) -> Result<String, GitError>;
```
**空提交信息在本层校验**:`message.trim().is_empty()` → 直接返回错误(不下探 backend)。

### 3.5 src-tauri(外壳)— 加 4 个命令

复用阶段 0 模式,全部 `async` + `spawn_blocking` + `to_ipc` 翻译,注册进 `invoke_handler`:
```
get_status(repo_path)
stage_file(repo_path, file_path)
unstage_file(repo_path, file_path)
commit(repo_path, message)  -> 返回新 SHA
```

### 3.6 前端(React)

- **`ipc.ts`** 加 4 个函数 + 类型(`StatusDto`/`FileEntryDto`),组件不直接 `invoke`。
- **UI**:选完仓库展示状态,**两区:已暂存 / 未暂存**,每行 = 文件名 + 状态徽章 + [暂存]/[取消暂存];底部提交框 = textarea + [提交](无暂存项或空信息时禁用)。
- 每次 stage/unstage/commit 后调用 `refreshStatus()` 重拉;提交成功 → 清空输入 + 刷新 + 提示新 SHA。
- 状态:普通 hooks(`useState` + `refreshStatus`),不引库。

---

## 4. 测试策略(TDD —— 先写测试再实现)

| 层 | 测什么 | 怎么测 |
|---|---|---|
| app-service | stage/unstage/commit 转发正确 + DTO 映射 + **空信息被拦截** | 注入改造后的 FakeBackend(Mutex 记录调用),毫秒级断言 |
| git-engine | 真实 git2 行为 | **tempfile** 建临时真仓库 |

**git-engine 集成测试用例(至少):**
1. 新建临时仓库 + 一个改动文件 → `status` 显示该文件未暂存。
2. `stage` 后 → `status` 显示已暂存。
3. `unstage`(有 HEAD 场景)后 → 回到未暂存。
4. **空仓库 unstage 场景**:无 HEAD 时 stage 一个新文件再 unstage → 条目从 index 移除,不报错。
5. **首次提交**:空仓库 stage + `commit` → 返回有效 SHA、HEAD 指向它(空 parents 路径)。
6. **后续提交**:在已有提交基础上再 commit → parents 含上一提交。
7. ✅ **commit 后 `status` 应为干净**(entries 为空)。

---

## 5. 这一阶段会学到的 Rust 概念

- 给 trait 加方法并跨两个后端实现
- git2 的 index / tree / signature / commit API,以及 unborn-HEAD 边界
- **内部可变性 `Mutex`**,以及 `Send + Sync` 为何排除 `RefCell`(编译期线程安全)
- **`tempfile` 集成测试**模式
- serde 枚举 → 字符串序列化给前端
- 更多 `spawn_blocking` 命令接线

---

## 6. 验收标准

- [ ] `cargo test -p app-service`(FakeBackend)+ `cargo test -p git-engine`(tempfile)全绿,覆盖第 4 节用例
- [ ] `cargo build`(全工作区,含 app 外壳)通过
- [ ] `pnpm tauri dev`:选仓库 → 看到分区文件列表 → 暂存/取消暂存 → 写信息提交 → 列表刷新为干净、显示新 SHA
- [ ] 边界:空仓库首次提交可成功;空提交信息被拦并提示;未配 git 身份时给友好错误

---

## 7. 实现约束(补充)

1. **路径契约 = 仓库根相对路径,贯穿全链路。**
   - `status` 经 git2 `entry.path()` 返回的就是仓库根相对路径;前端把它原样传回给 stage/unstage。
   - 领域层 / DTO / 命令层都按"相对路径"约定,不做转换。
   - **兜底转换只在 git2_backend 适配器层**:若传入的是绝对路径,用 `repo.workdir()` 剥掉前缀再交给 `add_path`/`remove_path`;这样 git2 "要相对路径"这个泄漏细节被锁在适配器内,不污染上层。

2. **无 HEAD 用精确的 unborn 检测。**
   - 用 `repo.head_unborn()`(返回 bool),或捕获 `repo.head()` 的 `git2::ErrorCode::UnbornBranch`。
   - **不要**笼统拿 `head()` 是否 Err 当"无 HEAD"——它可能因别的原因报错,会误判。

3. **暂存"被删除的文件" = 阶段 1 已知限制。**
   - `git add` 一个已删除文件实为暂存删除,需 `index.remove_path()` 而非 `add_path()`。
   - 阶段 1 的 `stage` 只用 `add_path`,**只保证修改/新增文件**;暂存删除留到后续切片。UI 可暂不暴露删除文件的暂存按钮,或点击时给"暂未支持"提示。

4. **前端命令统一用 `pnpm`**(非 npm):`pnpm tauri dev` / `pnpm build` 等。
