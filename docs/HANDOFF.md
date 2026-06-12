# 交接文档(随仓库走,换机器拉分支后看这个)

> 这份文件在 git 仓库里,会随 push/pull 跟到新机器。记录当前进度、铁律、下一步。
> 配套必读:`CLAUDE.md`(铁律)、`ARCHITECTURE.md`(架构)、`README.md`(启动)。
> 最近更新:2026-06-12(M5.2 并排 diff 完成:DiffView 加统一/并排切换,复用 M5.1 词级段)。

## 当前状态
- 阶段 0/1/2/3 全部完成,**阶段 4 核心(交互式 rebase)已落地**。
- 后端 `cargo test --workspace` 全绿、`cargo clippy --workspace` 零警告、`cargo fmt --check` 干净。
- 前端 `npx tsc --noEmit` 干净、`npm run build` 通过。
- ⚠️ **main 领先 origin 若干 commit**;push 由用户手动做(铁律:代码 push 到 origin 必须用户发话)。
  换机器前请先在旧机 push,新机器再 pull。

## 竖切模式(每个 git 功能都这么走)
git-core trait(+默认方法) → git2_backend / cli_backend / composite(+tempfile 测试) → fake.rs
→ ipc-types DTO → app-service 用例(+FakeBackend 测试) → src-tauri 命令(spawn_blocking + to_ipc)
→ ipc.ts → queries.ts(TanStack Query) → UI。

## 铁律 / 约定(违反会出问题)
- git 操作一律 `spawn_blocking`;上层只依赖 `GitBackend` trait;库用 thiserror,跨 IPC 转结构化 `IpcError`(带 code)。命令层永不 panic(用 `join_panic` 捕获)。
- **`to_ipc`(app/src-tauri/src/lib.rs)是穷尽 match**:新增 `GitError` 变体必须同步加 arm,否则编译不过。
- 网络/复杂流程(fetch/pull/push/stash/cherry-pick/revert/交互式 rebase)走 `CliBackend`(shell out git,**要求系统装了 git 且在 PATH**);本地读写走 git2;生产用 `CompositeBackend` 路由。
- **Windows 必须**:`crates/git-engine/build.rs` 补链 advapi32(否则 libgit2-sys 测试 LNK2019)。
- 前端:颜色/字体只用 `app/src/index.css` 的 `@theme` token(bg-canvas/text-fg/border-line/text-success/text-accent…),**禁硬编码 hex**;图标用 `app/src/components/icons.tsx` 内联 SVG;读数据走 `app/src/lib/queries.ts` 的 hooks;反馈走 `useToast()`;loading 用圆环 SpinnerIcon + 顶部进度条,别全屏遮罩。
- **包管理用 pnpm**(`pnpm --dir app ...`)。⚠️ 别用 npm install(会崩 pnpm 结构的 node_modules)。注:跑命令脚本里历史上用过 `npm run build`/`npx tsc`,那是调用脚本/二进制(OK);装依赖才必须 pnpm。
- 工作流:每功能开 `feat/xxx` 分支 → `git merge --no-ff` 回 main → 删分支;提交信息中文 + `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>` 尾注。
- `app/src-tauri/Cargo.toml` 偶显 modified 只是 LF→CRLF 噪音,别提交。
- ⚠️ origin(github)上已有些早先推的 feat 分支;新建同名本地分支删除时可能要 `git branch -D`。给新分支起名避开。

## 已完成功能(可在 app 里用)
- 阶段 1:status/stage/unstage/commit、提交历史、行级 diff、文件监听自动刷新、行级/hunk 暂存。
- 阶段 2:分支列表/切换/新建/删除、fetch/pull(merge|rebase)/push、远程选择器、ahead/behind 角标(SyncBadge)、set-upstream。
- 阶段 3:提交图谱(自研 lane)、图谱装饰(作者名 + 分支/远程/tag 徽章)、stash、冲突解决(三栏 CodeMirror 合并编辑器)、cherry-pick、blame。
- 近期(2026-06-07):
  - **未push/未pull 图谱标记**:节点空心环 + 行首色条(未push绿/未pull蓝);`SyncCommits`/`sync_commits`/`GraphRowDto.sync`。
  - **revert**、**日志搜索**(可取消,代次计数)、**tag 创建/删除**、**reset**(soft/mixed/hard)、**两提交/分支 diff 比较**(「比较」tab + remote/tag 选择器)、**amend**(修订上次提交)。
  - **交互式 rebase**:全程非交互驱动 CLI——todo 经 `GIT_SEQUENCE_EDITOR=cp <todo>` 注入、改信息用 todo 里 `exec git commit --amend -F <msg>` 行、`GIT_EDITOR=true` 兜底、冲突复用 ConflictBanner。UI = 历史页提交右键/详情头「变基」→ RebaseEditor 弹层(↑↓ 调序 + pick/reword/squash/fixup/drop)。**Windows cp/exec 已在本机 tempfile 测试验证。**
  - **提交右键上下文菜单**(`CommitContextMenu`):图谱行/搜索行右键 → Cherry-pick/Revert/从此交互变基/Reset 到此(子菜单)/打标签/复制 SHA。这是 per-commit 操作的主入口;新加单提交动作优先进这里。
- M4 Correct(扛真实世界 git,详见 `docs/superpowers/plans/2026-06-08-world-class-roadmap.md`):
- **M4 · Correct 全部完成**(均已合 main,详见 `docs/superpowers/plans/2026-06-08-world-class-roadmap.md`):
  - M4.1 超大文件/二进制防卡、M4.2 提交签名徽章、M4.3 提交走 CLI(尊重 hooks+签名)、
    M4.4 子模块感知、M4.5 worktree 列表(只读)、M4.6 LFS 指针感知 + 稀疏检出感知。
  - **LFS 指针**:`file_diff_from`(commit/working/compare diff 的单一构建点)末尾检测 LFS 指针,
    标 `FileDiff.is_lfs_pointer`/`lfs_size` 并清 hunks;DiffView 显占位,不把指针文本当内容 diff。
  - **动态标签套路**:子模块/工作树/稀疏检出三个标签「按仓库特性按需出现」(useSubmodules/
    useWorktrees/useSparseCheckout 驱动 TabBar 显隐 + 切到不再可用的标签时退回「更改」)。
    新增此类标签照此办理:加 use*hook → App 算 has* → TabBar 加 prop+push → 渲染分支 + 复位 effect。

## 下一步候选(按价值/风险)
- **M5 · 更深的 diff 与历史(进行中)**:
  - ✅ **M5.1 词级 diff**(已合 main):`file_diff_from` 末尾跑纯函数 `annotate_word_level`,
    对配对的删/增行用 `similar::from_words` 算行内段,标到 `DiffLine.emphasis`(`Vec<Seg{text,changed}>`,
    传切好的段非字节偏移);相似度 <0.25 视为整行重写不标。DiffView 逐段渲染,changed 段底色深一档。
    spec/plan 见 `docs/superpowers/specs|plans/2026-06-09-word-level-diff*`。
  - ✅ **M5.2 并排 diff**(已合 main):DiffView 内加「统一/并排」切换条,偏好存 localStorage
    (仿 theme.ts),三处调用(ChangesView/HistoryView/ComparePanel)全受益。**纯前端零 Rust 改动**。
    `buildSbsRows` 把 hunk 扁平行配对成左右行(context 两侧同行;连续删块 del[i] 配连续增块 add[i],
    口径同 M5.1 annotate_word_level,多余行对侧留空占位);列优先布局,左右两列各自横滚、行数配平等高对齐;
    抽出共享 `LineContent` 复用 M5.1 emphasis 段;行级暂存在并排里照常(选中集仍按 hi:li 键)。
    spec 见 `docs/superpowers/specs/2026-06-12-side-by-side-diff-design.md`。⚠️ 真机视觉验收待做(tsc+build 已过)。
  - **下一刀 M5.3 文件历史**(`git log -- <path>`:某文件的提交历史列表)。
  - 再后:M5.4 行历史(`log -L`)、M5.5 pickaxe(`-S`/`-G`)、图片 diff。
- 零散续做:worktree 切换/新建(M4.5 只做展示);log 里 ctrl-多选两提交→比较。
- 工程收尾:真机验收交互式 rebase(尤其中途冲突的继续/中止、大仓库 cp/exec 路径);CI 加 `fmt --check` + `clippy -D warnings` 卡口(属 infra,之前没动 .github);push 到 origin。
- 已知小项:composite 40+ 透传样板(Rust 固有税,可选 delegate crate)。

## 验证命令
- 后端:`cargo test --workspace`、`cargo clippy --workspace`、`cargo fmt --check`、`cargo check -p app`
- 前端(在 app/ 下或 `--prefix app`):`npx tsc -p tsconfig.json --noEmit`、`npm run build`
- 真机:`cd app && npm run tauri dev`

## superpowers 产物
`docs/superpowers/plans/` 下有各功能的 spec/plan(remote-fetch、interactive-rebase、post-sync-marks-roadmap 等)。

## 新机器准备(换电脑后)
1. 装 Rust(rustup,≥1.85)、Node(LTS)、pnpm、git(必须在 PATH)、Tauri 系统依赖(见 README 第 1.3)。
2. `git pull` 拿到 main 最新。
3. `cargo test --workspace` + `cd app && pnpm install`(首次)验证环境。
