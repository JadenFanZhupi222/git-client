# 设计:阶段 2d-1 · 远程基础设施 + fetch

> 状态:已通过设计评审,待写实现计划
> 日期:2026-06-06
> 范围:阶段 2「分支 / 远程」的第一个网络切片

## 1. 背景与目标

阶段 2a/2b 已完成本地分支的列表 / 切换 / 新建 / 删除,但客户端尚不能与远程同步。本切片填上「能 fetch」这一步,并**一次性敲定所有远程操作的地基**:如何 shell out 到 git CLI、凭据如何走、CLI 后端如何接进六边形架构。pull / push 作为后续切片复用这套地基。

**本切片交付**:用户能在 app 里点「Fetch」,从默认远程拉取更新(更新远程跟踪分支),不需要在应用内输入或存储任何凭据。

## 2. 关键决策(已与用户确认)

| 决策点 | 选择 | 理由 |
|---|---|---|
| 网络执行方式 | **调用 git CLI**(`std::process::Command` spawn `git fetch`) | 直接复用系统已配的 git 凭据助手(Windows: manager-core/wincred),应用零存储密码;行为与用户终端里的 git 完全一致。符合 ARCHITECTURE.md 决策。 |
| 凭据策略 | **交给 git 自身的凭据助手**,应用不碰 | 同上;避免自管密码的安全风险与 libgit2 凭据集成的复杂度。 |
| CLI 接入架构 | **CompositeBackend**(方案 A) | 贴合架构设想,既有方法委托 git2、网络方法走 cli;为 pull/push 铺路。 |
| 本切片范围 | **基础设施 + fetch**(只读) | 最小可跑;fetch 不改工作区,风险最低。pull/push 各自后续 spec。 |

## 3. 架构与新组件

依赖方向不变:`src-tauri → app-service → git-core ← git-engine`。

### 3.1 `GitBackend` trait 新增 `fetch`(带默认实现)

```rust
// git-core/src/backend.rs
/// 从远程拉取更新(更新远程跟踪分支,不改工作区/当前分支)。
/// remote = None 时用 git 的默认远程(通常当前分支的 upstream / origin)。
/// 默认实现返回 Unsupported —— 不做网络的后端(如 Git2Backend)无需覆盖。
fn fetch(&self, _repo: &Path, _remote: Option<&str>) -> Result<FetchOutcome, GitError> {
    Err(GitError::Unsupported)
}
```

**Rust 概念:trait 默认方法。** 类似接口里带默认实现的方法。这样:
- `fetch` 进入统一 trait → 上层 app-service 仍只依赖 `dyn GitBackend`,能透过 trait 对象调到它;
- `Git2Backend` 不必实现网络(它不该管网络),沿用默认体;
- 只有 `CompositeBackend`(委托 cli)和 `FakeBackend`(测试)覆盖它。

### 3.2 `CliBackend`(新)— `git-engine/src/cli_backend.rs`

不实现整个 trait,只是带方法的结构体:

```rust
pub struct CliBackend;

impl CliBackend {
    pub fn fetch(&self, repo: &Path, remote: Option<&str>) -> Result<FetchOutcome, GitError> {
        // git -C <repo> fetch [remote] --prune
        // 捕获 stdout/stderr;按退出码 + stderr 关键词归类错误。
    }
}
```

- 用 `std::process::Command`,**仍在 spawn_blocking 里调用**(子进程是阻塞的,铁律不变)。
- `--prune`:顺手清理远程已删除的跟踪分支,符合直觉。
- `-C <repo>`:在目标仓库目录下执行,不靠进程 cwd。

### 3.3 `CompositeBackend`(新)— `git-engine/src/composite.rs`

```rust
pub struct CompositeBackend {
    git2: Git2Backend,
    cli: CliBackend,
}

impl GitBackend for CompositeBackend {
    // 所有既有方法逐一委托给 self.git2.<method>(...)（~17 行样板)
    // fetch 覆盖默认实现,委托给 self.cli.fetch(...)
}
```

命令层(src-tauri)由 `RepoService::new(Arc::new(Git2Backend))` 改为 `RepoService::new(Arc::new(CompositeBackend::default()))`。

## 4. 领域模型与数据流

### 4.1 模型

```rust
// git-core/src/model/remote.rs
/// 一次 fetch 的结果(MVP:不解析结构化更新明细)。
pub struct FetchOutcome {
    pub remote: String,    // 实际 fetch 的远程名(无则空串/“default”)
    pub summary: String,   // git 的人类输出(stderr 那几行);为空表示“已是最新”
}
```

对应 `ipc-types::FetchResultDto { remote: String, summary: String }` + `From<FetchOutcome>`。

### 4.2 远程选择

MVP 不做选择器。`fetch(repo, remote)` 的 `remote` 由 UI 传 `None` → 执行 `git fetch`,由 git 自行选当前分支的远程(通常 origin)。列远程 / 多远程选择器留给 push 切片。

### 4.3 数据流(一次 Fetch)

```
UI「Fetch」按钮
  → ipc.fetch(repoPath)                         // app/src/ipc.ts
  → #[tauri::command] fetch (spawn_blocking)     // app/src-tauri/src/lib.rs
  → RepoService::fetch                           // app-service
  → CompositeBackend::fetch → CliBackend::fetch  // git-engine
  → spawn `git -C <repo> fetch --prune`
  → 解析退出码/stderr → FetchOutcome
  → FetchResultDto(serde)→ 前端
  → UI 显示 summary,并触发分支列表 + 历史重载
```

## 5. UI

- **顶栏右侧新增「Fetch」按钮**(在主题切换钮旁;fetch 是仓库级操作,适合全局位置)。
- 点击 → 按钮转圈(busy)→
  - 成功:按钮旁显示一行简短结果(remote + summary 或「已是最新」),数秒后淡出;并触发分支列表 / 历史重载,让新到的远程提交显形。
  - 失败:显示错误信息(持续到下次操作);认证类错误提示去检查凭据。
- 仅当已打开仓库时显示该按钮。
- **不做**:远程选择器、ahead/behind 角标、流式进度(后者留给 pull/push)。

## 6. 错误处理

`CliBackend` 把 git 退出码 + stderr 归类为精确的 `GitError`,经 `to_ipc` 映射成带 code 的 `IpcError`(前端可分支)。新增变体:

| 触发 | GitError | IpcError code | recoverable |
|---|---|---|---|
| spawn 失败(未装 git / 不在 PATH) | `GitCliNotFound` | `GIT_CLI_NOT_FOUND` | false |
| stderr 含 `Authentication failed` / `could not read Username` / `Permission denied (publickey)` | `AuthFailed` | `AUTH_FAILED` | true |
| stderr 含 `Could not resolve host` / `unable to access` / `timed out` | `NetworkError` | `NETWORK_ERROR` | true |
| stderr 含 `No remote repository` / `does not appear to be a git` | `NoRemote` | `NO_REMOTE` | false |
| 其余非零退出 | `Backend(stderr)` | `BACKEND` | true |
| trait 默认体被调用 | `Unsupported` | `UNSUPPORTED` | false |

> 注:`to_ipc` 是穷尽匹配,新增 GitError 变体必须同步加 arm,否则编译不过(这是设计上的安全网)。

## 7. 测试策略

诀窍:**git 把本地文件路径当作合法「远程」**,用本地 bare 仓库冒充远程 → 全程无网络、无凭据、CI 可确定性运行。被测对象只有 `CliBackend::fetch`(它确实 spawn git、解析输出),只是去掉了网络与认证两个不确定因素。

### 7.1 主测试:fetch 推进远程跟踪分支(`git-engine`,真 git CLI,本地)

1. `git init --bare` 建「远程」R(本地目录)。
2. clone A;在 A 提交 c1 并 push → R @ c1。
3. clone B(此时 B 的 `origin/main` @ c1)。
4. A 再提交 c2 并 push → R @ c2,B 仍停在 c1。
5. **在 B 上调 `CliBackend.fetch(B, None)`** → 断言 `origin/main` 从 c1 变为 c2(用 git2 `rev_parse` 读 SHA)。

arrange 阶段用 git CLI 建仓 / clone / push 图省事;被测的是第 5 步。

### 7.2 错误路径(`git-engine`)

- 无远程的仓库 `fetch(None)` → 断言 `GitError::NoRemote`(验证 stderr 关键词归类命中)。
- 认证失败类无法离线稳定复现,靠关键词匹配 + 第 9 节人工验收覆盖。

### 7.3 其余层(沿用现有套路)

- **CompositeBackend 委托**:`CompositeBackend.branches()` 在临时仓库上结果与 `Git2Backend.branches()` 一致,证明既有方法透传。
- **FakeBackend**:`.fetch` 返回 canned `FetchOutcome` 并记录调用(像现有 `checked_out_branches()`)。
- **app-service**:注入 FakeBackend,断言 `service.fetch` 转发 + `FetchOutcome → FetchResultDto` 映射正确。

### 7.4 前提

CLI 测试要求机器上装有 git 并在 PATH —— 这本就是 fetch 功能的硬依赖(开发机 / CI 均满足)。

## 8. 各层改动清单

- `git-core`:`model/remote.rs`(FetchOutcome)+ mod 导出;`backend.rs` 加 `fetch` 默认方法;`error.rs` 加 5 个变体(GitCliNotFound / AuthFailed / NetworkError / NoRemote / Unsupported)。
- `git-engine`:`cli_backend.rs`(新)、`composite.rs`(新)、`lib.rs` 导出;`fake.rs` 实现 `fetch` + 记录。
- `ipc-types`:`FetchResultDto` + `From<FetchOutcome>`。
- `app-service`:`fetch` 用例 + FakeBackend 测试。
- `src-tauri`:`fetch` 命令(spawn_blocking)、注册、`to_ipc` 新 arm;命令构造 `CompositeBackend`。
- `app/src`:`ipc.ts` 加 `FetchResultDto` + `fetch()`;顶栏 Fetch 按钮 + 状态/错误展示;成功后触发刷新。

## 9. 验收(人工,实现后)

在跑起来的 app 里对真实远程仓库点 Fetch:
- 公开仓库 → 成功,summary 合理;
- 私有仓库(已配凭据助手)→ 成功,无应用内密码提示;
- 断网 → NetworkError 友好提示。

## 10. 明确不做(后续切片)

pull(merge/rebase)、push、remote add/remove/rename、远程选择器、ahead/behind 角标、流式进度。
