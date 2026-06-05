# 生产级 Rust Git 客户端 · 完整架构设计

> 目标:一款达到 JetBrains 内置 Git 水平的桌面客户端。
> 前提:你是 React/Next 前端,Rust 零基础。
> 原则:按"长期维护、多人协作、上线给真实用户"的标准来,不走捷径。

---

## 第 0 部分:动手前必须先建立的 Rust 心智模型

你不需要先学完 Rust 才能看懂这份架构,但下面 6 个概念是**整套设计的地基**。每一个我都说清楚"它是什么"和"它如何影响架构决策",这样你后面看代码不会懵。

### 0.1 所有权(Ownership)—— 为什么 Rust 没有 GC 还安全

每个值都有唯一的"主人"。主人离开作用域,值自动销毁。把值"给"别人(move)后,你自己就不能再用它了。要么把所有权交出去,要么"借"出去(引用 `&`)。

**对架构的影响**:数据在层与层之间传递时,你必须想清楚"是移交所有权,还是借引用"。跨线程共享的数据需要特殊包装(见 0.4),这直接决定了状态管理怎么写。

### 0.2 Result 与 Option —— 没有异常,没有 null

- `Option<T>`:要么 `Some(值)`,要么 `None`。代替了 null。
- `Result<T, E>`:要么 `Ok(值)`,要么 `Err(错误)`。代替了 try/catch 异常。

`?` 运算符:遇到 `Err` 就提前返回这个错误,否则取出 `Ok` 里的值继续。

```rust
fn load_repo(path: &str) -> Result<Repository, GitError> {
    let repo = gix::open(path)?;   // 失败就直接 return Err
    Ok(repo)
}
```

**对架构的影响**:错误是类型系统的一等公民。我们整个错误处理体系(第 5 部分)就是围绕"如何设计 E 这个类型、如何让它跨进程边界传给前端"展开的。生产代码里**几乎不允许 panic(崩溃)**,所有可能失败的地方都用 Result。

### 0.3 trait —— Rust 的"接口"

`trait` 类似 TypeScript 的 `interface`。定义一组行为,不同类型去实现它。

```rust
trait GitBackend {
    fn status(&self, repo: &Path) -> Result<Status, GitError>;
    fn log(&self, repo: &Path, limit: usize) -> Result<Vec<Commit>, GitError>;
}
```

**对架构的影响**:这是整个项目最重要的抽象手段。我们会用 trait 把"git 怎么实现"和"业务怎么用"彻底解耦——上层只认 `GitBackend` 这个接口,底下可以是 gix、git2、或调命令行,甚至测试时换成假实现。

### 0.4 Arc / Mutex / RwLock —— 跨线程共享数据

- `Arc<T>`:原子引用计数,允许多个线程**共享**同一份数据的只读访问。
- `Mutex<T>` / `RwLock<T>`:加锁后才能**修改**共享数据。RwLock 允许多读单写。
- 常见组合:`Arc<RwLock<AppState>>` = 多线程共享、可读可写的全局状态。

**对架构的影响**:桌面 git 客户端是重并发的(UI 线程、git 计算线程、文件监听线程同时跑)。如何共享状态、用锁还是用消息传递(actor 模型),是第 4 部分的核心议题。**这是 Rust 桌面应用最容易写崩的地方。**

### 0.5 同步阻塞 vs 异步(async)—— 本项目最大的坑

- **异步(async/await + tokio)**:适合 IO 等待(网络、等响应),一个线程能处理很多任务。Tauri 的命令默认跑在异步运行时上。
- **同步阻塞**:函数执行时会"占住"当前线程直到完成。

**致命陷阱**:`git2` 是同步阻塞的,`gix` 大量操作也是 CPU/IO 密集的同步调用。如果你在 Tauri 的 async 命令里**直接**调这些阻塞操作,会卡死整个异步运行时,UI 直接冻结。

**对架构的影响**:我们必须建立一条铁律——所有 git 操作都丢到专门的阻塞线程池执行(`spawn_blocking` 或 rayon)。这条规则贯穿第 4 部分,违反它你的客户端在大仓库下必然卡顿。

### 0.6 Cargo workspace —— 大项目不是单个 crate

Rust 的编译单元叫 crate(类似一个 npm 包)。一个严肃项目应该拆成**多个 crate 组成的 workspace**(类似前端的 monorepo + 多 package)。好处:编译更快(改一个 crate 不用全编)、强制清晰的依赖边界、各层可独立测试。

**对架构的影响**:第 2 部分的整个物理结构就是 workspace 划分。

---

## 第 1 部分:架构哲学 —— 分层 + 六边形

我们采用**六边形架构(端口与适配器)** 的思路。一句话:**核心业务逻辑不依赖任何外部实现细节**(不依赖 git2、不依赖 Tauri、不依赖文件系统),外部实现通过 trait(端口)接入。

```
┌─────────────────────────────────────────────────────┐
│                  UI (React/TypeScript)                │  ← 展示层
└───────────────────────┬─────────────────────────────┘
                        │  IPC(Tauri command / event)
┌───────────────────────┴─────────────────────────────┐
│              src-tauri(进程外壳,极薄)               │  ← 适配器
└───────────────────────┬─────────────────────────────┘
                        │
┌───────────────────────┴─────────────────────────────┐
│            app-service(用例编排 / 状态 / 任务)       │  ← 应用层
└───────────────────────┬─────────────────────────────┘
                        │  依赖 trait,不依赖具体实现
┌───────────────────────┴─────────────────────────────┐
│        git-core(领域模型 + GitBackend trait)        │  ← 领域层(纯净)
└───────────────────────┬─────────────────────────────┘
                        │  实现 trait
┌───────────────────────┴─────────────────────────────┐
│   git-engine(gix / git2 / CLI 三套后端实现)        │  ← 适配器
└──────────────────────────────────────────────────────┘
```

**为什么这么分?** 三个实际收益:

1. **可测试**:`git-core` 是纯逻辑,不碰真实仓库,可以用假后端跑单元测试,毫秒级。
2. **可替换**:今天 gix 不支持交互式 rebase,你用 CLI 后端;明天 gix 支持了,只改 `git-engine`,上层一行不动。
3. **可并行开发**:多人时,做 UI 的人对着 `ipc-types` 的契约写,不用等后端;做后端的人对着 trait 写。

---

## 第 2 部分:项目物理结构(Cargo Workspace)

```
git-client/
├── Cargo.toml                  # workspace 根:声明所有成员 crate + 统一依赖版本
├── rust-toolchain.toml         # 锁定 Rust 版本,保证团队/CI 一致
├── .cargo/config.toml          # 编译配置(如更快的 linker)
│
├── crates/
│   ├── git-core/               # 【领域层】模型 + trait,零外部 git 依赖
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── model/          # Commit, Branch, Status, Diff, Hunk...
│   │       ├── backend.rs      # GitBackend trait 定义(端口)
│   │       └── error.rs        # 领域错误类型
│   │
│   ├── git-engine/             # 【适配器】trait 的具体实现
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── gix_backend.rs   # 用 gix 实现读路径
│   │       ├── git2_backend.rs  # 用 git2 实现常规写
│   │       ├── cli_backend.rs   # 调 git CLI 实现复杂流程
│   │       └── composite.rs     # 组合后端:按操作路由到不同实现
│   │
│   ├── app-service/            # 【应用层】用例、状态、任务调度、缓存
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── repo_actor.rs    # 每个仓库一个 actor(见第 4 部分)
│   │       ├── jobs.rs          # 后台任务系统(可取消)
│   │       ├── cache.rs         # diff/log/graph 缓存
│   │       ├── watcher.rs       # 文件系统监听
│   │       └── graph_layout.rs  # commit 图谱布局算法
│   │
│   └── ipc-types/              # 【契约】前后端共享的数据类型
│       ├── Cargo.toml          #  用 specta/ts-rs 自动生成 TS 类型
│       └── src/lib.rs
│
├── src-tauri/                  # 【外壳】Tauri 应用,极薄,只做 IPC 桥接
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs
│       ├── commands.rs         # #[tauri::command] 全部在这,薄薄一层
│       └── state.rs            # 全局应用状态注入
│
├── ui/                         # 【前端】React + Vite
│   ├── package.json
│   └── src/
│       ├── bindings/           # 自动生成的 TS 类型(来自 ipc-types)
│       ├── ipc/                # 封装 invoke 调用 + 事件订阅
│       ├── features/           # 按功能切分:log/diff/status/branches...
│       └── components/
│
├── xtask/                      # 自定义构建脚本(代码生成、发布流程)
└── .github/workflows/          # CI/CD
```

**依赖方向铁律(必须严格遵守)**:

```
src-tauri ──> app-service ──> git-core <── git-engine
   │                              ▲
   └──> ipc-types <───────────────┘
```

- `git-core` 不依赖任何人(最纯)。
- `git-engine` 只依赖 `git-core`(去实现它的 trait)。
- `app-service` 依赖 `git-core`(用 trait),通过依赖注入拿到 `git-engine` 的实现。
- `src-tauri` 是最外壳,谁都能依赖,但谁都不依赖它。
- **绝对禁止**反向依赖(比如 git-core 依赖 git-engine),否则六边形架构就破了。

---

## 第 3 部分:各层详解

### 3.1 git-core(领域层)—— 整个项目的心脏

这一层定义"在我们的世界里,git 是什么样子",完全用我们自己的类型,不暴露任何 gix/git2 的类型给上层。

**领域模型示例**(`model/commit.rs`):

```rust
use serde::{Serialize, Deserialize};

/// 我们自己定义的 Commit,而不是直接用 gix::Commit。
/// 这样上层和前端永远只认这个类型,底层换实现都不影响。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    pub id: String,              // 完整 SHA
    pub short_id: String,        // 短 SHA
    pub summary: String,         // 提交信息首行
    pub body: String,            // 提交信息正文
    pub author: Signature,
    pub committer: Signature,
    pub timestamp: i64,          // Unix 时间戳
    pub parents: Vec<String>,    // 父提交(合并提交有多个)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub name: String,
    pub email: String,
}
```

**端口定义**(`backend.rs`)—— 这是上层唯一认识的接口:

```rust
use crate::model::*;
use crate::error::GitError;
use std::path::Path;

/// 所有 git 后端必须实现这个 trait。
/// 上层(app-service)只依赖这个 trait,不知道底下是 gix 还是 CLI。
pub trait GitBackend: Send + Sync {
    fn open(&self, path: &Path) -> Result<RepoHandle, GitError>;

    fn status(&self, repo: &RepoHandle) -> Result<WorkingTreeStatus, GitError>;

    fn log(
        &self,
        repo: &RepoHandle,
        opts: &LogOptions,
    ) -> Result<Vec<Commit>, GitError>;

    fn diff_file(
        &self,
        repo: &RepoHandle,
        path: &Path,
        staged: bool,
    ) -> Result<FileDiff, GitError>;

    fn stage(&self, repo: &RepoHandle, path: &Path) -> Result<(), GitError>;
    fn commit(&self, repo: &RepoHandle, msg: &str) -> Result<String, GitError>;

    // ... branch / checkout / push / fetch / rebase 等
}
```

> 注意 trait 上的 `Send + Sync`:这是 Rust 在告诉编译器"这个东西可以安全地在线程间传递和共享"。因为我们要把后端放进多线程环境,这个约束是必须的(回顾 0.4)。

**为什么花力气做这层映射?** 短期看是多写代码,长期看:
- gix 的 API 还在演进,直接用它,它一改你全线崩。有了这层隔离,影响范围被锁死在 `git-engine`。
- 前端只需要稳定的 `Commit` 结构,不该关心底层 git 库的内存布局。

### 3.2 git-engine(适配器层)—— 三套后端 + 智能路由

回顾上一轮我们的结论:**没有任何单一 git 库能优雅地吃下所有操作**。所以这层是三套实现的组合。

```rust
/// 组合后端:对外是一个 GitBackend,内部按操作类型路由。
pub struct CompositeBackend {
    gix: GixBackend,     // 读:log / status / diff / blame —— 最快
    git2: Git2Backend,   // 常规写:stage / commit / branch
    cli: CliBackend,     // 复杂流程:交互式 rebase / push 认证 / hooks
}

impl GitBackend for CompositeBackend {
    fn log(&self, repo: &RepoHandle, opts: &LogOptions) -> Result<Vec<Commit>, GitError> {
        // log 走 gix,因为遍历性能碾压
        self.gix.log(repo, opts)
    }

    fn commit(&self, repo: &RepoHandle, msg: &str) -> Result<String, GitError> {
        // 普通提交走 git2(API 成熟稳定)
        self.git2.commit(repo, msg)
    }

    fn interactive_rebase(&self, repo: &RepoHandle, plan: &RebasePlan) -> Result<(), GitError> {
        // 交互式 rebase 直接驱动 CLI,别硬抗
        self.cli.interactive_rebase(repo, plan)
    }
    // ...
}
```

**CLI 后端的关键细节**(决定稳定性):

- 调用 `git status --porcelain=v2 -z`:`-z` 用 `\0` 分隔,避免文件名里有空格/换行导致解析错乱。
- 解析输出时**永远假设它会出错**,做防御性处理。
- 复杂操作(push)的认证、凭据,交给 git 自己的凭据助手处理,你别自己存密码。

### 3.3 app-service(应用层)—— 用例、状态、并发

这是把"领域能力"组织成"产品功能"的地方,也是并发的主战场。详见第 4 部分。它负责:
- 编排用例(一次"刷新"可能要并发跑 status + log + branch 三个查询)。
- 持有运行时状态(当前打开的仓库、缓存、文件监听器)。
- 调度后台任务并支持取消。

### 3.4 ipc-types(契约层)—— 前后端类型安全的关键

千万级项目里,前后端类型不一致是 bug 的重灾区。我们用 `specta` + `tauri-specta`(或 `ts-rs`)**从 Rust 结构体自动生成 TypeScript 类型**。

```rust
use serde::{Serialize, Deserialize};
use specta::Type;

#[derive(Serialize, Deserialize, Type)]
pub struct CommitDto {
    pub id: String,
    pub summary: String,
    pub author_name: String,
    pub timestamp: i64,
}
```

构建时自动生成 `ui/src/bindings/index.ts`,前端直接 import。**后端改了字段,前端编译就报错**——这正是你要的安全网。

### 3.5 src-tauri(外壳)—— 越薄越好

命令层不写业务逻辑,只做三件事:接收参数 → 调 app-service → 返回结果。

```rust
#[tauri::command]
async fn get_log(
    state: tauri::State<'_, AppHandle>,
    repo_id: String,
    limit: usize,
) -> Result<Vec<CommitDto>, IpcError> {
    // 关键:git 是阻塞操作,丢到阻塞线程池,绝不阻塞 async 运行时
    let service = state.service.clone();
    tokio::task::spawn_blocking(move || {
        service.get_log(&repo_id, limit)
    })
    .await
    .map_err(IpcError::from)?   // 处理任务 panic
    .map_err(IpcError::from)    // 处理业务错误
}
```

### 3.6 ui(前端)—— 你的主场

唯一要新建的纪律:**所有 IPC 调用封装在 `ui/src/ipc/` 一层**,组件不直接调 `invoke`。这样后端契约变了,只改一处。状态用你熟的(Zustand/TanStack Query 都行),git 数据天然适合用 query + 失效重取的模型。

---

## 第 4 部分:并发模型 —— 本项目的成败手(重点)

这是 Rust 桌面 git 客户端最难、也最能拉开质量差距的部分。我推荐 **Actor 模型**,而不是到处加锁。

### 4.1 为什么不用 `Arc<RwLock<AppState>>` 满天飞

新手最容易写成:全局一个大锁,谁要读写状态都来抢锁。后果:
- 一个慢操作持锁,所有人卡住(锁竞争)。
- 容易写出死锁(两个线程互相等对方的锁)。
- 代码到处是 `.lock().unwrap()`,可读性差且每个 unwrap 都是潜在 panic 点。

### 4.2 Actor 模型:每个仓库一个"管家"

核心思想:**一份状态只由一个任务(actor)独占拥有,别人想动它,发消息**。没有共享锁,因为根本没有共享。

```rust
/// 发给仓库 actor 的命令
enum RepoCommand {
    GetStatus { reply: oneshot::Sender<Result<Status, GitError>> },
    GetLog { limit: usize, reply: oneshot::Sender<Result<Vec<Commit>, GitError>> },
    Stage { path: PathBuf, reply: oneshot::Sender<Result<(), GitError>> },
    Refresh,
    Shutdown,
}

/// actor 本体:独占持有仓库状态,串行处理消息
struct RepoActor {
    backend: Arc<dyn GitBackend>,
    handle: RepoHandle,
    cache: RepoCache,
    rx: mpsc::Receiver<RepoCommand>,
}

impl RepoActor {
    async fn run(mut self) {
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                RepoCommand::GetLog { limit, reply } => {
                    // 重活丢去阻塞线程池(铁律!),拿到结果通过 reply 通道送回
                    let backend = self.backend.clone();
                    let handle = self.handle.clone();
                    let res = tokio::task::spawn_blocking(move || {
                        backend.log(&handle, &LogOptions { limit, ..Default::default() })
                    }).await.unwrap();
                    let _ = reply.send(res);
                }
                RepoCommand::Shutdown => break,
                // ...
            }
        }
    }
}
```

**对外的句柄**(命令层拿这个,干净):

```rust
#[derive(Clone)]
pub struct RepoActorHandle {
    tx: mpsc::Sender<RepoCommand>,
}

impl RepoActorHandle {
    pub async fn get_log(&self, limit: usize) -> Result<Vec<Commit>, GitError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx.send(RepoCommand::GetLog { limit, reply: reply_tx }).await?;
        reply_rx.await?   // 等 actor 把结果送回来
    }
}
```

收益:状态无锁、不会死锁、每个仓库的操作天然串行化(避免竞态),而 CPU 重活仍然并行(因为丢去了 spawn_blocking)。

### 4.3 三条必须刻进 DNA 的并发铁律

1. **任何 git 操作都不在 async 上下文里直接跑**,一律 `spawn_blocking` 或 rayon。
2. **状态独占,通过消息通信**(actor),而不是共享加锁。
3. **所有耗时操作可取消**(下一节)。

### 4.4 取消(Cancellation)—— JetBrains 级流畅的秘密

用户在加载一个大 log 时突然切到另一个分支,旧的查询必须能**立刻被取消**,否则白白占用 CPU、结果回来还可能覆盖新数据。

```rust
use tokio_util::sync::CancellationToken;

// 每个可取消任务持有一个 token
let token = CancellationToken::new();

// 在长循环里定期检查
for commit in commit_iter {
    if token.is_cancelled() {
        return Err(GitError::Cancelled);
    }
    // ... 处理
}

// 用户切走时:
token.cancel();
```

log 遍历、diff 计算、blame、图谱布局——这些都要支持取消。这是"感觉很跟手"和"一卡一卡"的分水岭。

---

## 第 5 部分:错误处理体系

生产代码的标准:**库 crate 用类型化错误(`thiserror`),应用入口用 `anyhow`,跨 IPC 边界转成结构化错误码**。

**库内(git-core / git-engine)** —— 精确的类型化错误:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("仓库未找到: {0}")]
    RepoNotFound(String),

    #[error("合并冲突,涉及 {count} 个文件")]
    MergeConflict { count: usize },

    #[error("操作被取消")]
    Cancelled,

    #[error("底层 git 错误: {0}")]
    Backend(#[from] anyhow::Error),
}
```

**跨 IPC 边界** —— 转成前端能消费的结构(带错误码 + 用户友好信息):

```rust
#[derive(Serialize, Type)]
pub struct IpcError {
    pub code: String,        // "MERGE_CONFLICT" —— 前端用它做逻辑分支
    pub message: String,     // 给用户看的话
    pub recoverable: bool,   // 前端决定要不要提供重试按钮
}

impl From<GitError> for IpcError {
    fn from(e: GitError) -> Self {
        match e {
            GitError::MergeConflict { count } => IpcError {
                code: "MERGE_CONFLICT".into(),
                message: format!("有 {count} 个文件存在冲突,需要手动解决", ),
                recoverable: true,
            },
            // ...
        }
    }
}
```

**铁律**:命令层**永不 panic**。每个 `unwrap()` 在生产代码里都是定时炸弹。spawn_blocking 里万一 panic 了,也要在 `.await` 后捕获并转成错误返回,不能让整个应用挂掉。

---

## 第 6 部分:状态、缓存与失效

### 6.1 缓存什么

| 数据 | 计算成本 | 失效条件 |
|---|---|---|
| commit log | 高(大仓库遍历) | 有新提交 / fetch / 切分支 |
| 单文件 diff | 中 | 该文件工作区变化 / 暂存状态变化 |
| 图谱布局 | 高(图算法) | log 变化 |
| blame | 很高 | 该文件提交历史变化 |

### 6.2 失效由两个源驱动

1. **文件系统监听**(见第 7 部分)→ 工作区变了 → 失效 status 和相关 diff。
2. **我们自己执行的 git 写操作** → 主动失效对应缓存。

缓存放在每个仓库的 actor 内部(因为 actor 独占状态,缓存读写天然无锁安全)。用 `lru` crate 控制内存上限,避免大仓库把内存吃爆。

---

## 第 7 部分:文件系统监听

用 `notify` crate 监听工作区,实现"文件一改,状态自动刷新",这是流畅感的来源之一。但有几个生产级细节新手必踩:

1. **必须 debounce**:保存一次文件可能触发多个事件,IDE 批量改文件会刷屏。攒一小段时间(如 200ms)合并成一次刷新。
2. **区分 `.git` 内部变化 vs 工作区变化**:`.git/index` 变了说明有 git 操作,`.git/HEAD` 变了说明切了分支,工作树文件变了说明要刷 status——三者触发的失效范围不同。
3. **尊重 `.gitignore`**:`node_modules`、`target` 这种目录的海量变化必须忽略,否则监听器自己就成了性能灾难。用 `ignore` crate 复用 git 的忽略规则。
4. **大仓库的监听成本**:递归监听几十万文件本身有开销,要测。

---

## 第 8 部分:可观测性(Observability)

用户报 bug 时你不可能去他机器上调试,所以日志是生产应用的生命线。用 `tracing`(不是简单的 println):

```rust
use tracing::{info, instrument};

#[instrument(skip(self))]   // 自动记录函数进入/退出 + 参数 + 耗时
pub fn get_log(&self, repo_id: &str, limit: usize) -> Result<Vec<Commit>, GitError> {
    info!(repo_id, limit, "开始加载 log");
    // ...
}
```

配套:
- `tracing-subscriber` 把日志写到文件(用户能打包发给你)。
- 用 span 串起一次完整操作的耗时,方便定位"为什么这次刷新慢"。
- 生产构建里日志级别可配置,默认 info,出问题让用户开 debug。

---

## 第 9 部分:测试策略

| 层 | 测什么 | 怎么测 |
|---|---|---|
| git-core | 纯逻辑(图谱布局算法、diff 解析) | 普通单元测试,毫秒级 |
| git-engine | 后端实现是否符合 trait 契约 | 用 `tempfile` 建临时真实仓库,跑真 git 操作验证 |
| app-service | 用例编排、缓存失效、取消 | 注入**假后端**(mock 实现 GitBackend),不碰真仓库 |
| 端到端 | 关键用户流程 | Tauri 的 WebDriver 测试 |

**测试夹具(fixture)模式**:写一个辅助函数,用代码生成一个有特定历史结构的临时仓库,这样图谱布局这种算法可以针对各种刁钻的分支拓扑做断言。这是保证图谱质量的关键。

假后端示例(为什么 trait 抽象值这么多钱):

```rust
struct FakeBackend { commits: Vec<Commit> }

impl GitBackend for FakeBackend {
    fn log(&self, _: &RepoHandle, _: &LogOptions) -> Result<Vec<Commit>, GitError> {
        Ok(self.commits.clone())   // 不碰磁盘,测试飞快且确定
    }
    // ...
}
```

---

## 第 10 部分:配置与持久化

- 用 `directories` crate 拿到跨平台的标准目录(macOS/Windows/Linux 各不同),别硬编码路径。
- 存什么:最近打开的仓库、窗口大小位置、用户偏好、各仓库的 UI 状态。
- 格式:配置用 TOML 或 JSON;如果要存结构化的本地数据(如缓存索引),可考虑 SQLite(`rusqlite`)。
- **迁移**:配置结构会演进,第一天就给配置文件加 `version` 字段,预留升级迁移逻辑。

---

## 第 11 部分:构建、CI/CD 与发布

千万级项目这部分不能是事后补的:

1. **CI 矩阵**:macOS(Intel + Apple Silicon)、Windows、Linux 三平台都要在 CI 上编译 + 测试。
2. **代码签名**:macOS 要 Apple 公证(notarization),Windows 要签名证书,否则用户下载会被系统拦截报毒。
3. **自动更新**:Tauri 自带 updater 插件,配好更新服务器,用户无感升级。这是桌面产品的刚需。
4. **xtask 模式**:把代码生成(TS 类型)、打包、发布等自定义流程写成 Rust 的 `xtask`,而不是一堆零散 shell 脚本。
5. **编译加速**:大 workspace 编译慢,配置更快的 linker(如 `mold`/`lld`),CI 上做依赖缓存。

---

## 第 12 部分:一次完整请求的生命周期(把所有层串起来)

以"用户点击某文件查看 diff"为例,看数据怎么流过每一层:

```
1. [UI] 用户点文件
   → ui/src/ipc/diff.ts 调 invoke('get_file_diff', { repoId, path })

2. [src-tauri] commands.rs 的 get_file_diff 命令被触发
   → 拿到 repo 对应的 RepoActorHandle
   → 调 handle.get_diff(path)(异步,但内部会丢阻塞线程池)

3. [app-service] RepoActor 收到 GetDiff 消息
   → 先查缓存,命中直接返回
   → 未命中:spawn_blocking 里调 backend.diff_file(...)
   → 持有 CancellationToken,用户切走可取消

4. [git-engine] CompositeBackend 路由到 gix_backend.diff_file
   → gix 计算 diff,再用 similar crate 做行内字符级 diff

5. [git-core] 结果映射成我们自己的 FileDiff 领域模型

6. 原路返回:FileDiff → DTO(serde 序列化)→ 前端

7. [UI] 收到强类型的 FileDiff(类型来自自动生成的 bindings)
   → 渲染三栏 diff + 语法高亮 + 行级 staging 按钮
```

每一步职责单一、边界清晰——这就是分层架构的价值。

---

## 第 13 部分:实施路线图(分阶段,先能跑再变强)

**阶段 0 · 地基(1-2 周)**
- 搭好 workspace 骨架、依赖方向、CI、tracing、错误体系。
- 跑通最小链路:Tauri 命令 → app-service → gix 读个 HEAD → 返回前端显示。
- *验证:架构能跑通,spawn_blocking 边界正确,UI 不冻结。*

**阶段 1 · 核心读写(MVP)**
- status + 文件级 stage/unstage + commit。
- commit log(先线性列表)+ 单文件 diff 查看。
- 文件系统监听 + 自动刷新。
- *目标:能完成最基础的日常提交流程。*

**阶段 2 · 日常可用**
- branch / checkout / 行级 staging。
- push / pull / fetch + 远程管理 + 凭据。
- *到这一步就已经是相当大的工程量,且基本能替代日常使用了。*

**阶段 3 · 拉开差距的高级特性**
- commit 图谱可视化(自研 lane assignment 算法)。
- 三栏冲突合并 UI。
- blame、stash、cherry-pick。

**阶段 4 · JetBrains 级**
- 交互式 rebase(CLI 驱动)。
- changelist 抽象、行内字符级 diff 精修。
- 大型 monorepo 性能压测与优化。

---

## 第 14 部分:给 Rust 零基础的你 —— 学习路径

不要"先学完 Rust 再开工",而是**伴随阶段 0 学够用的部分**:

1. **第一周**:《The Rust Book》前 10 章(所有权、Result/Option、struct/enum、trait、错误处理)。这覆盖了你看懂本文档 80% 代码所需。
2. **结合阶段 0**:边搭骨架边查,重点吃透 `spawn_blocking`、`Arc`、channel(mpsc/oneshot)——这三个是本项目并发的命脉。
3. **遇到生命周期(lifetime,`'a` 那些标注)报错先别钻牛角尖**,很多时候 `clone()` 一下或调整数据所有权就过了,深入理解可以放后面。
4. **善用编译器**:Rust 编译器报错信息极其详细,常常直接告诉你怎么改。把"和编译器对话"当成学习方式。

---

## 关键决策速查表

| 维度 | 选择 | 理由 |
|---|---|---|
| 桌面框架 | Tauri 2.x | 体积小、内存低、前端用 React |
| git 读路径 | gix | 性能碾压,纯 Rust |
| git 常规写 | git2 | API 成熟稳定 |
| git 复杂流程 | CLI shell out | 行为与本机 git 一致,稳 |
| 并发模型 | Actor + spawn_blocking | 无锁、不死锁、重活并行 |
| 错误处理 | thiserror(库)+ 结构化 IPC error | 类型安全 + 前端可消费 |
| 类型同步 | specta/ts-rs 自动生成 | 前后端契约编译期保证 |
| 文件监听 | notify + ignore + debounce | 自动刷新且不被海量事件淹没 |
| 日志 | tracing | 生产可观测性 |
| 项目结构 | Cargo workspace 多 crate | 边界清晰、编译快、可测试 |

---

*这份架构的核心思想就一句话:用 trait 把"git 怎么实现"和"业务怎么用"彻底解耦,用 actor 把并发关进笼子,其余都是按这两条主线展开的工程纪律。*
