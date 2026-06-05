# 阶段 1b-1 设计:历史列表 + 提交改动文件(文件级)

> 状态:待用户确认
> 日期:2026-06-05
> 前置:阶段 1(提交回路)已合并入 main
> 路线图:ARCHITECTURE.md 阶段 1 的"commit log + diff 查看"切片之一。1b 拆成 **1b-1(本 spec:log 列表 + 提交改动文件,文件级)** 和 **1b-2(行级 diff 渲染)**。

---

## 1. 范围

**1b-1 做:**
- `log`:commit 历史列表(暗色 + 提交轨视觉),分页("加载更多")。
- `commit_files`:选中一个提交,列出它**改动了哪些文件**,只到**文件级状态(A/M/D)**,**不解析行**。
- 前端:引入 **Tailwind v4**;顶部「更改 / 历史」标签页;历史视图三栏壳(提交列表 | 改动文件 | diff 占位)。

**明确不做(留 1b-2):**
- 行级 diff(`Patch::from_diff` 解析 hunk/行)——三栏的右列在 1b-1 是占位("选中文件后将在 1b-2 显示 diff")。
- 工作区(更改 tab)的文件 diff —— 延后。
- 语法高亮、commit 图谱多 lane 连线(阶段 3)。

### 已确认决策
| 项 | 选择 |
|---|---|
| 视觉风格 | 暗色 + 提交轨 + 分支徽章(JetBrains/GitKraken 风) |
| 布局 | 三栏:提交列表 \| 改动文件 \| diff(本期占位) |
| 样式方案 | Tailwind v4 |
| diff 高亮 | 不做 |
| commit_files 粒度 | 仅文件级 A/M/D,不解析行(行级全在 1b-2) |

---

## 2. 后端(Rust)

### 2.1 git-core 领域层

**trait `GitBackend` 加 2 个方法**:
```rust
/// 提交历史,按时间倒序(新→旧)。limit/skip 用于分页。
fn log(&self, repo: &Path, limit: usize, skip: usize) -> Result<Vec<Commit>, GitError>;

/// 某提交相对其(第一个)父提交改动了哪些文件,只到文件级状态。
fn commit_files(&self, repo: &Path, commit_id: &str) -> Result<Vec<FileChange>, GitError>;

/// 当前 HEAD 指向的分支短名(如 "main")。分离头/空仓库返回 None。
fn current_branch(&self, repo: &Path) -> Result<Option<String>, GitError>;
```
`Commit` 模型已存在(复用)。**新增模型** `model/diff.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub status: FileState,   // 复用已有枚举:Added/Modified/Deleted/Renamed
}
```
(`FileState` 已有六变体,commit diff 在 1b-1 实际只会产出 Added/Modified/Deleted —— 重命名不检测,见 2.2 末尾说明。)

### 2.2 git-engine Git2Backend 实现

**`log`** —— ⚠️ 坑 3(Revwalk 配置)+ 坑 4(惰性分页):
```
let mut walk = repo.revwalk()?;
walk.set_sorting(git2::Sort::TIME)?;     // 坑3:按提交时间排序
match walk.push_head() {                  // 坑3:从 HEAD 起步
    Ok(()) => {}
    Err(e) if e.code() == git2::ErrorCode::UnbornBranch => return Ok(vec![]), // 空仓库→空历史
    Err(e) => return Err(Backend(e)),
}
// 坑4:Revwalk 是惰性迭代器,直接 skip/take,别先 collect 全部再切片(大仓库会爆)
walk.skip(skip).take(limit)
    .map(|oid| {
        let oid = oid.map_err(Backend)?;
        let c = repo.find_commit(oid).map_err(Backend)?;
        Ok(build_commit(&c))   // 复用阶段 0 的 Commit 构造逻辑(id/short_id/summary/body/author/timestamp/parents)
    })
    .collect::<Result<Vec<_>, _>>()
```
> `build_commit` 把现有 `head_commit` 里构造 `Commit` 的逻辑抽成一个私有函数复用,避免重复。

**`commit_files`** —— ⚠️ 坑 1(首提交无父)+ 坑 2(合并只跟第一个父):
```
let oid = git2::Oid::from_str(commit_id).map_err(Backend)?;
let commit = repo.find_commit(oid).map_err(Backend)?;
let new_tree = commit.tree().map_err(Backend)?;

let parent_tree = if commit.parent_count() == 0 {
    None                                   // 坑1:首次提交无父 → 和空树 diff(None 即空)
} else {
    Some(commit.parent(0)?.tree()?)        // 坑2:合并提交只跟第一个父 diff(简化处理,写明)
};

let diff = repo
    .diff_tree_to_tree(parent_tree.as_ref(), Some(&new_tree), None)
    .map_err(Backend)?;

let mut out = Vec::new();
for delta in diff.deltas() {
    let status = match delta.status() {
        git2::Delta::Added | git2::Delta::Copied => FileState::Added,
        git2::Delta::Deleted => FileState::Deleted,
        git2::Delta::Renamed => FileState::Renamed, // 1b-1 不开重命名检测,此分支当前不命中(保留以备 1b-x)
        git2::Delta::Modified | git2::Delta::Typechange => FileState::Modified,
        _ => continue,                     // Unmodified/Ignored/Untracked 等在 commit diff 不该出现,跳过
    };
    // 删除的文件 new_file().path() 为 None → 回退 old_file()
    let path = delta.new_file().path()
        .or_else(|| delta.old_file().path())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    out.push(FileChange { path, status });
}
Ok(out)
```
> ⚠️ **重命名暂不检测(YAGNI)**:`diff_tree_to_tree` 默认**不**做相似度检测,一次重命名会被报成 **Deleted(旧名)+ Added(新名)** 两条。1b-1 接受这个行为、不调 `Diff::find_similar`;上面 `Delta::Renamed` 分支当前不会命中(保留无害)。真正的重命名检测留到以后切片。

**`current_branch`** —— 读真实分支名,别写死:
```
match repo.head() {
    Ok(head) => Ok(head.shorthand().map(|s| s.to_string())),  // 如 "main";分离头时 shorthand 可能为 None
    Err(e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(None), // 空仓库无分支
    Err(e) => Err(Backend(e)),
}
```

### 2.3 FakeBackend
加 `log`/`commit_files` 的桩 + 可预置返回值(Mutex,供 app-service TDD):
```rust
canned_log: Mutex<Vec<Commit>>,
canned_commit_files: Mutex<Vec<FileChange>>,
canned_branch: Mutex<Option<String>>,
// + with_log(...) / with_commit_files(...) / with_branch(...) 构造器或 setter
```

---

## 3. 契约 / 命令

### 3.1 ipc-types DTO
`CommitDto` 已存在(复用,log 返回 `Vec<CommitDto>`)。新增:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeDto {
    pub path: String,
    pub status: String,   // added | modified | deleted | renamed
}
```
`From<FileChange> for FileChangeDto`(枚举转小写字符串,复用阶段 1 的映射风格)。

### 3.2 Tauri 命令(全 spawn_blocking + to_ipc)
```
get_log(repo_path: String, limit: usize, skip: usize) -> Vec<CommitDto>
get_commit_files(repo_path: String, commit_id: String) -> Vec<FileChangeDto>
get_current_branch(repo_path: String) -> Option<String>
```

---

## 4. 前端

### 4.1 Tailwind v4 接入
- 装 `tailwindcss` + `@tailwindcss/vite`(pnpm)。
- `vite.config.ts` 加 `@tailwindcss/vite` 插件。
- 新建 `src/index.css` 含 `@import "tailwindcss";`,在 `main.tsx` import。
- 暗色 token 用 Tailwind 任意值(如 `bg-[#0d1117]`)或在 CSS 里定义 `@theme` 变量。

### 4.2 组件(新)
- `TabBar`:`更改` | `历史` 两标签,受控切换。
- `HistoryView`:三栏壳(flex)。**左** `CommitList`,**中** `CommitFileList`,**右** diff 占位区(灰字"选中文件后在 1b-2 显示 diff")。
- `CommitList`:每行 = 圆点+连线轨 + 消息 + 短哈希(等宽)+ 相对时间;HEAD 那行(skip=0 时的首条)带 `HEAD → <branch>` 徽章,**分支名来自 `getCurrentBranch` 的真实值**(如 "main"),不写死;branch 为 null(分离头/空仓库)时只显示 `HEAD`;选中高亮;底部"加载更多"按钮。
- `CommitFileList`:选中提交的改动文件,每行 = 状态色标(A 绿/M 蓝/D 红)+ 路径(等宽)。
- 现有「更改」视图(status/stage/commit)迁进 `更改` 标签,功能不变,顺手用 Tailwind 重写样式与历史视图统一(不改逻辑)。

### 4.3 ipc.ts
加 `getLog(repoPath, limit, skip)`、`getCommitFiles(repoPath, commitId)`、`getCurrentBranch(repoPath)` + 类型。

### 4.4 状态(hooks)
`activeTab`、`commits`(累加分页)、`logSkip`、`selectedCommit`、`commitFiles`。选中提交 → 拉 `getCommitFiles`。

---

## 5. 数据流
```
历史 tab → getLog(limit,skip) → get_log 命令(spawn_blocking) → RepoService.log → Git2Backend.log(Revwalk)
选中提交 → getCommitFiles(id) → get_commit_files(spawn_blocking) → RepoService.commit_files → Git2Backend(diff_tree_to_tree)
```

---

## 6. 测试(TDD)

**git-engine(tempfile 真仓库):**
> ⚠️ **fixture 必须显式控制提交时间戳递增**:不要用 `repo.signature()`(取系统当前时间,连续提交可能落在同一秒 → `Sort::TIME` 顺序 flaky)。测试用一个 `commit_at(repo, msg, secs)` 辅助函数,用 `git2::Signature::new(name, email, &git2::Time::new(secs, 0))` 给每个提交显式、递增的时间(如 1000/2000/3000),保证排序确定。

1. `log` 顺序:用 `commit_at` 建 3 个提交(t=1000/2000/3000)→ `log(10,0)` 返回 3 个,**时间倒序**(t=3000 在前),按 summary 断言。
2. `log` 分页:`log(1,1)` 只返回第 2 新的提交(验证惰性 skip/take)。
3. `log` 空仓库:unborn HEAD → 返回**空 vec**(坑3 的 UnbornBranch 分支)。
4. `commit_files` 首提交:初始提交加 a.txt → 返回 `[a.txt: Added]`(坑1,和空树 diff)。
5. `commit_files` 改+增:第二提交改 a.txt、加 b.txt → `[a.txt: Modified, b.txt: Added]`。
6. `commit_files` 删:某提交删 a.txt → 含 `[a.txt: Deleted]`(验证 new_file None 回退 old_file)。
7. `commit_files` 重命名按删+增报告:某提交把 a.txt 重命名为 c.txt → 断言结果是 `[a.txt: Deleted, c.txt: Added]`(**不是** Renamed),固化"1b-1 不检测重命名"的行为。
8. `current_branch`:有提交的仓库 → 返回 `Some("main")`(或 init 时的默认分支名,按 fixture 实际);空仓库 → `None`。
9. (可选)合并提交:构造一个 merge,断言只反映与第一个父的差异(坑2),或仅在 spec/代码注释写明简化、暂不测。

**app-service(FakeBackend):** `log`/`commit_files` 的 DTO 映射(`FileState`→字符串、`Commit`→`CommitDto`)+ 调用转发。

---

## 7. 这阶段会学到的 Rust
- `Revwalk` 提交遍历 + `Sort::TIME` + **惰性迭代器 skip/take**(为什么不能 collect 全部)
- git2 `diff_tree_to_tree` + `Diff::deltas` + `Delta` 枚举映射到自有 `FileState`
- 首提交/合并提交的 tree diff 边界(空树、第一父简化)
- 把 head_commit 的 Commit 构造抽成可复用私有函数(DRY)

---

## 8. 验收标准
- [ ] `cargo test`(git-engine + app-service)覆盖第 6 节用例,全绿
- [ ] `cargo build` 全工作区通过;clippy/fmt 干净
- [ ] `pnpm tsc --noEmit && pnpm build` 通过
- [ ] `pnpm tauri dev`:切到「历史」→ 暗色提交轨列表 → "加载更多"可翻页 → 点提交 → 中栏显示其改动文件(A/M/D 色标)→ 右栏占位提示 1b-2;「更改」标签原功能不变
- [ ] 边界:空仓库历史为空不报错;首次提交的改动文件正确显示

---

## 9. git2 坑(实现务必照做)
1. **首次提交无父** → `diff_tree_to_tree(None, Some(&tree), _)` 和空树 diff。
2. **合并提交只跟第一个父 diff**(`parent(0)`),简化处理,代码注释写明。
3. **Revwalk** 必须 `set_sorting(Sort::TIME)` **在** `push_head()` **之前**(set_sorting 会重置遍历,顺序调换则排序失效);unborn HEAD 时 `push_head` 报 `UnbornBranch`,返回空历史。
4. **分页**用 Revwalk 惰性迭代器的 `.skip(skip).take(limit)`,**不要**先 `collect` 全部再切片(大仓库性能灾难)。
5. **重命名不检测**:`diff_tree_to_tree` 默认不做相似度检测,重命名报成 Deleted+Added;1b-1 接受此行为,不调 `find_similar`(YAGNI),`Delta::Renamed` 分支当前不命中。
6. **测试时间戳**:log 排序测试的 fixture 用显式递增时间戳(`Signature::new` + `Time`),避免同秒提交导致 `Sort::TIME` flaky。
7. **分支名读真实值**:HEAD 徽章经 `current_branch`(`repo.head()?.shorthand()`)取真实分支名,不写死 "main"。
