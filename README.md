# Git 客户端 · 启动指南(阶段 0)

这个压缩包是**已经编译验证过的架构骨架**。你要做的是:装好环境 → 确认骨架能跑 → 加上 Tauri 外壳和前端 → 让一条命令(读 HEAD)贯通"前端 → Tauri → app-service → git-engine → git-core"全链路,亲眼看到结果。

> 已验证:`git-core` / `ipc-types` / `git-engine` / `app-service` 四个 crate 在本地编译通过,分层测试(注入 FakeBackend)通过。
> 你机器上要做的:装现代 Rust 后启用真实 git2 后端 + 加 Tauri 外壳(这部分需要你本机的图形/系统依赖,无法在别处替你编译)。

---

## 第 1 步:安装环境(必须,按你的操作系统来)

### 1.1 安装 Rust(用 rustup,不要用系统包管理器的旧版)

**macOS / Linux**,终端执行:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# 装完重开终端,或执行:
source "$HOME/.cargo/env"
```

**Windows**:去 https://rustup.rs 下载 `rustup-init.exe` 运行。

验证(应显示 1.85 以上):
```bash
rustc --version
cargo --version
```

> 为什么强调版本:现代 git 库(gix/git2 的依赖)需要 Rust 1.85+。系统自带的旧 Rust 会编译失败。

### 1.2 安装 Node.js(给前端用)

去 https://nodejs.org 装 LTS 版(20 以上),或用 nvm。验证:
```bash
node --version
npm --version
```

### 1.3 安装 Tauri 的系统依赖(平台相关,关键一步)

- **macOS**:装 Xcode Command Line Tools
  ```bash
  xcode-select --install
  ```
- **Windows**:装 **Microsoft C++ Build Tools**(含 MSVC)+ **WebView2 Runtime**(Win11 自带,Win10 可能要手动装)。
- **Linux(Debian/Ubuntu)**:
  ```bash
  sudo apt update
  sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
    libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```

> 官方完整清单(以最新为准):https://tauri.app/start/prerequisites/

---

## 第 2 步:确认骨架能编译 + 测试通过

解压本包,进入目录,先验证纯架构部分(用假后端,不依赖系统 git 库):
```bash
cd git-client
cargo test --no-default-features -p app-service
```

应看到:
```
test tests::head_commit_via_fake_backend ... ok
```

这一步通过,说明你的 Rust 环境 OK,且整套分层架构在你机器上成立。

接着验证**真实的 git2 后端**(现在你的 Rust 够新了,这步在本机能成功,沙箱里因 Rust 太旧不行):
```bash
cargo build
```
默认会启用 `git2-backend` 特性,编译真实的 `Git2Backend`。第一次会下载并编译 libgit2,稍慢,正常。

---

## 第 3 步:加上 Tauri 外壳 + 前端

外壳和前端用**官方脚手架**生成最稳(它会正确处理图标、权限、平台配置,这些手写极易出错),然后我们把它接到已有的 crate 上。

### 3.1 在 `git-client/` 目录内生成 Tauri + React 应用

```bash
cd git-client
npm create tauri-app@latest
```
交互式选择:
- 项目名:输入 `app`(会生成 `git-client/app/` 目录)
- 前端语言:**TypeScript / JavaScript**
- 包管理器:**npm**
- 前端框架:**React**
- 风格:**TypeScript**

生成后结构大致是 `git-client/app/`(里面有 `src/` 前端 和 `src-tauri/` Rust 外壳)。

### 3.2 把 Tauri 外壳纳入工作区

编辑根 `git-client/Cargo.toml`,在 `members` 里加上外壳路径:
```toml
[workspace]
resolver = "2"
members = [
    "crates/git-core",
    "crates/git-engine",
    "crates/app-service",
    "crates/ipc-types",
    "app/src-tauri",          # ← 新增这行
]
```

### 3.3 让外壳依赖我们的 app-service

编辑 `git-client/app/src-tauri/Cargo.toml`,在 `[dependencies]` 里加:
```toml
app-service = { path = "../../crates/app-service" }
ipc-types   = { path = "../../crates/ipc-types" }
git-engine  = { path = "../../crates/git-engine" }
```

### 3.4 接入命令(读 HEAD)—— 贯通全链路

把 `docs/src-tauri-lib.rs` 的内容**参照**着合并进 `git-client/app/src-tauri/src/lib.rs`(关键是注册 `get_head_commit` 命令 + 用 `spawn_blocking` 跑阻塞的 git 操作)。该文件里有详细注释。

### 3.5 接入前端

- 把 `docs/frontend-ipc.ts` 复制到 `git-client/app/src/ipc.ts`
- 用 `docs/frontend-App.tsx` 的内容替换 `git-client/app/src/App.tsx`

---

## 第 4 步:跑起来,看到结果

```bash
cd git-client/app
npm install
npm run tauri dev
```

第一次编译较久。窗口起来后:点按钮选一个**本地 git 仓库目录**,界面会显示该仓库 HEAD 的提交 SHA、提交信息和作者——这条数据正是从 `git2 → git-core → app-service → Tauri → 前端` 一路流过来的。

**看到它,就代表你的生产级架构地基已经跑通了。** 后面阶段 1(status / stage / commit)就是在这个骨架上往 trait 里加方法、往前端加界面而已。

---

## 目录速览

```
git-client/
├── Cargo.toml              # 工作区根
├── crates/
│   ├── git-core/           # 领域层:模型 + GitBackend trait + 错误(纯净)
│   ├── git-engine/         # 适配器:FakeBackend(已验证) + Git2Backend(本机启用)
│   ├── app-service/        # 应用层:RepoService,依赖注入 + 用例 + 测试
│   └── ipc-types/          # 前后端共享 DTO
├── docs/                   # 第 3 步要用的接线代码(复制/参照)
│   ├── src-tauri-lib.rs
│   ├── frontend-ipc.ts
│   └── frontend-App.tsx
└── app/                    # ← 第 3 步你用脚手架生成,不在本包内
```

---

## 常见卡点

- **`cargo build` 报 edition2024 / Rust 版本太旧**:你的 Rust 不是 rustup 装的最新版。执行 `rustup update`,确认 `rustc --version` ≥ 1.85。
- **Tauri `npm run tauri dev` 报缺少 webkit / 链接错误**:第 1.3 步的系统依赖没装全,回去补。
- **选了仓库却报 RepoNotFound**:确认选的是含 `.git` 的仓库根目录,且该仓库至少有一次提交。
- **UI 卡死**:检查 git 操作是不是没放进 `spawn_blocking`(见第 0 部分铁律)。

---

*下一步建议:先把这个跑通,再回来我们一起做阶段 1 —— 给 trait 加 `status` / `stage` / `commit`,前端加文件列表和提交框。一次加一小块,每步都能跑。*
