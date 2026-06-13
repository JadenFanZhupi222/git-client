# 交接文档(随仓库走,换机器拉分支后看这个)

> 这份文件在 git 仓库里,会随 push/pull 跟到新机器。记录当前进度、铁律、下一步。
> 配套必读:`CLAUDE.md`(铁律)、`ARCHITECTURE.md`(架构)、`README.md`(启动)。
> 最近更新:2026-06-13(M6.1 并排 diff 体验补强完成:折叠未改区 + DiffView 拍平+虚拟化 + 并排纵横联动)。

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
  - ✅ **M5.3 文件历史**(已合 main):`git log --follow -- <file>` 走 **CliBackend**(git2 原生不支持 --follow,
    跟随重命名);纯函数 `parse_log_records` 按 0x1f/0x1e 分隔健壮解析(含换行 body 不错位)。
    复用 `Commit`/`CommitDto`,**无新增 DTO**。竖切:trait `file_history`(默认 Unsupported)→ cli_backend +
    tempfile 测试 → composite 路由 cli → RepoService/RepoContext(不缓存)→ `file_history` 命令 →
    ipc `getFileHistory` / `useFileHistory`(limit 200)→ `FileHistoryPanel`(双栏 overlay:提交列表 + 该文件
    在选中提交的 diff,复用 DiffView/M5.2)+ CommitFileList 行悬浮 HistoryIcon 入口(`onFileHistory`)。
    spec 见 `docs/superpowers/specs/2026-06-12-file-history-design.md`。
    ⚠️ 已知小限制:跨重命名的旧提交里,右侧 diff 用「当前文件名」查 → 改名前那条可能显示无改动(列表本身正确);
    真机视觉验收待做(tsc+build+test 已过)。
  - ✅ **M5.4 行历史**(已合 main):`git log -L<start>,<end>:<file>` 走 **CliBackend**(git2 无 -L)。
    新增 `LineHistoryEntry{commit,diff}` 模型 + DTO(commit 列表每条带「仅范围 hunk」的 diff)。
    纯函数 `parse_unified_diff`(unified diff 文本 → FileDiff,@@ 头初始化行号、context/+/- 各自递增,
    可复用)+ `parse_line_log`(0x1e 切块、0x1f 切元数据)。format 用 `%x1e` 起头(源码里不出现,
    避 marker 撞 diff 内容)。竖切:core 模型+trait → cli_backend 两纯函数+方法+tempfile/纯函数测试 →
    composite → ipc-types DTO → RepoService/RepoContext(不缓存)→ `line_history` 命令 →
    ipc `getLineHistory`/`useLineHistory` → `LineHistoryPanel`(双栏:提交列表 + entry.diff 直接渲染)+
    **BlameView 选行入口**(点选单行 / shift-点扩范围 → 工具栏「第 a–b 行历史」)。
    spec 见 `docs/superpowers/specs/2026-06-12-line-history-design.md`。⚠️ 真机视觉验收待做。
  - ✅ **M5.5 pickaxe**(已合 main):`git log -S<q>`(出现次数变化)/`-G<q>`(改动行匹配正则)走
    **CliBackend**,复用 M5.3 `LOG_FORMAT`+`parse_log_records`,返回 `Vec<Commit>`(与 search 同形)。
    **接进 HistoryView 现有搜索栏**:加 `searchMode`(信息/内容/正则)三态——「信息」=旧 search_commits、
    「内容」=pickaxe -S、「正则」=pickaxe -G;三者结果都喂同一个 SearchList,点结果走现有 MidColumn 看 diff,
    零新结果 UI。竖切:trait→cli_backend(+tempfile 测 -S 引入/删除、-G 正则)→composite→
    RepoService/RepoContext(不缓存)→`pickaxe` 命令→ipc/usePickaxe→HistoryView 模式切换。
    spec 见 `docs/superpowers/specs/2026-06-12-pickaxe-design.md`。
  - ✅ **图片 diff**(已合 main):`FileDiff` 加 `is_image`/`old_image`/`new_image`(`ImageData{mime,base64}`);
    git2_backend `file_diff_from`(单一构建点)检测图片扩展名(png/jpg/gif/webp/bmp/ico/avif;SVG 仍走文本),
    按 delta 新旧 blob oid 取字节 base64;未暂存改动新一侧 oid 为零时退回读工作区文件;8MB 上限。
    数据随现有 commit/working/compare diff 命令流出(**无新命令/DTO 命令**)。DiffView `is_image` 分支(在
    `is_binary` 前)并排两栏(旧|新,新增只显新、删除只显旧),`.checkerboard` 棋盘格衬透明 + 显尺寸/体积。
    base64 crate 跟 `git2-backend` feature。spec 无(实现直接,设计写在提交信息)。
  - **M5「更深的 diff 与历史」全部完成**(5.1 词级 / 5.2 并排 / 5.3 文件历史 / 5.4 行历史 / 5.5 pickaxe / 图片 diff)。
- **当前里程碑:M6 · Polish & Harden(打磨与硬化)** —— 不堆新功能,还 M5 留的债。
  **完整切片见 `docs/superpowers/plans/2026-06-12-m6-polish-harden.md`**(M6.1 并排同步滚动+折叠+虚拟化 /
  M6.2 图片去 base64+对比模式 / M6.3 新 CLI 读路径接缓存取消 / M6.4 specta 自动类型 / M6.5 M5 测试补齐 /
  M6.6 面板键盘 a11y)。原 roadmap「M6 协作/PR」顺延为 M7。
  - ✅ **M6.1 并排 diff 体验补强**(已合 main,纯前端,三刀):
    新增纯函数库 `app/src/lib/diffRows.ts`(+ vitest):`collapseContext` 折叠未改区
    (改动块上下各留 ctx=3 行,隐藏 ≥ minFold=2 才折,带原始 li)、`buildSbsRows`(从
    DiffView 迁来,吃 LineRef[] 保留 li)、`buildDiffRows` 把整个 FileDiff 拍平成一维渲染项
    (hunk头/折叠条/统一行/并排配对行)、`maxContentCols`/`maxSideCols` 算横滚体宽。
    DiffView 改用单个虚拟化纵向滚动容器(`useVirtualizer`,固定行高 ROW_H=20「估计=实际」),
    近 2 万行也只挂可视窗口;并排从「两列各自横滚」改成**单容器配对行 PairRow/HalfCell**
    (左半定宽、右半 flex-1 吸余量)→ 纵横联动(同步滚动)。折叠条点开就地展开(expanded 集合,
    切文件清空,单向展开)。StageHeader 适配固定行高(去 sticky/ml-auto,按钮就近左排)。
    行级暂存键(hi:li)/词级高亮全部不变。**后端零改动**;tsc + vitest(58)+ build 全绿。
    ⚠️ **真机视觉验收待做**:大 diff 滚动是否 60fps 无错位、并排纵横联动、折叠条展开手感、
    短 diff 是否铺满视口、行级暂存在并排里照常。
  - 下一刀建议 **M6.3**(CLI 读路径 file_history/line_history/pickaxe 接 LRU 缓存 + 取消,
    后端小刀、低风险、立刻和 M1 基建一致),再 M6.2(图片去 base64,破坏性)/ M6.6 / M6.4。
- ⚠️ **全 M5 + 图片 diff + M6.1 真机视觉验收仍待做**(自动门 test/clippy/fmt/tsc/build 全过;
  M5 已 push origin,**M6.1 已合 main 但尚未 push**)。
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
