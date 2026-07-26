# 交接文档(随仓库走,换机器拉分支后看这个)

> 这份文件在 git 仓库里,会随 push/pull 跟到新机器。记录当前进度、铁律、下一步。
> 配套必读:`CLAUDE.md`(铁律)、`ARCHITECTURE.md`(架构)、`README.md`(启动)。
> 最近更新:2026-07-27(**0.1.4 发布候选加固**:严格发布预检、CSP、跨平台 CI、核心桌面 E2E、依赖边界、GitLab MR i18n、前端拆包与体积门禁)。
> 前次:2026-06-13(M6 · Polish & Harden 全部完成:M6.1 并排虚拟化/M6.2 图片去 base64+对比/M6.3 CLI 读缓存/M6.4 ts-rs 自动类型/M6.6 面板 a11y)。

## 当前状态
- 阶段 0/1/2/3 全部完成,**阶段 4 核心(交互式 rebase)已落地**。
- Linux/macOS/Windows CI 已覆盖 fmt、Clippy、Rust/前端测试、构建、依赖边界和真实桌面 E2E。
- E2E 不再只测启动:会初始化临时仓库、写文件、经 UI 暂存/提交,并在历史页断言提交标题。
- `app-v*` 标签发布已 fail-closed;缺 updater、Windows 签名或 macOS 签名/公证输入时不会创建 prerelease。
- 正式前端已启用 CSP,首入口 JS 预算 500,000 bytes;WDIO 桥和 fixture 命令只在 `e2e` feature/config 中存在。
- 当前 Git 同步状态不要写死在本文;换机器或交接前以 `git status -sb`、`git log --oneline --decorate -10` 为准。

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
- **M6 · Polish & Harden 全部完成**(M6.1/M6.2/M6.3/M6.4/M6.6 均已合 main;M6.5 测试随各刀写入)。
  **切片见 `docs/superpowers/plans/2026-06-12-m6-polish-harden.md`**。原 roadmap「M6 协作/PR」顺延为 **M7**。
  - ✅ **M6.2 图片去 base64 + 对比模式**(已合 main,竖切):`ImageData{base64}` → `ImageRef{mime,oid}`
    (oid 空=工作区文件);trait `read_blob` + git2 实现(超 8MB 拒);`read_image` 命令把字节以
    `tauri::ipc::Response`(ArrayBuffer)直传,不再 base64-in-JSON。app-service `read_image_bytes`
    (oid 读 blob / 空走 `safe_join` 防越权读工作区)+ 纯函数 safe_join 测试。前端 `useImageUrl`
    钩子取字节转 Blob URL(revoke 清理);两版都在给「并排/滑块(clip-path)/洋葱皮(opacity)」模式
    (localStorage)。DiffView 加 repo 入参(5 处透传)。base64 依赖移除。
    ⚠️ 真机验收待做(图片加载、滑块/洋葱皮、工作区未暂存图)。**SVG 文本/预览切换顺延**(小后续)。
  - ✅ **M6.4 ts-rs 自动生成 DTO 类型**(已合 main):31 个 DTO 加 `#[derive(TS)]`,
    `app/src/bindings/*.ts` 自动生成(+手写 `index.ts` barrel);`ipc.ts` 删手写 interface 改 re-export。
    **改 DTO 后必须重跑 `cargo test -p ipc-types` 刷新 bindings**(它就是生成器)。i64 字段标
    `#[ts(type="number")]`、`emphasis` 标 `#[ts(optional)]`。注:RefDto.kind/GraphRowDto.sync/
    SubmoduleInfoDto.status 由字面量联合变成 `string`(生成器不导出 Rust String 的字面量约束)。
  - ✅ **M6.6 面板键盘 a11y**(已合 main,纯前端):`listNav.ts` 加 `useModalListNav`(Esc 关 /
    ↑↓jk/gG 移动 / Tab 焦点陷阱 / 开关焦点还原,导航键 stopPropagation 不扰背景列表);
    file/line history 面板改下标驱动选中 + role=dialog/aria-modal + 选中行 scrollIntoView。
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
  - ✅ **M6.3 CLI 读路径接缓存**(已合 main,后端小刀):`file_history`/`line_history`/`pickaxe`
    此前直透后端绕过缓存,现接 `RepoCache` 三个 ref 域 LRU(键分别 (file,limit) /
    (file,start,end) / (query,regex,limit)),与 log/blame 一致。`invalidate(GitRef)` 清这三者,
    WorkingTree 不动(工作区改动不改提交历史)。FakeBackend 补这三方法 + 调用计数,
    app-service 加 3 个命中/失效测试。**零行为变化**;cargo test/clippy/fmt 全绿。
    取消(子进程可杀)按 plan「按需」顺延——前端已 keepPreviousData,缓存命中即跳过重跑。
- ✅ **Tier 0 硬门槛补全(2026-06-17,均已合 main 待 push,见 `2026-06-17-path-to-number-one.md`)**:
  三刀竖切补齐「任何排得上号的客户端都默认有」的洞:
  - **远程管理 add/remove/rename**:git2 实现(remote/remote_delete/remote_rename)+ `RemoteInfo{name,url}`
    模型 / DTO;错误 `RemoteAlreadyExists`/`RemoteNotFound`/`InvalidRemoteName`。入口 = 「更多」菜单
    + 命令面板「管理远程」→ `RemoteManager` 模态(列表/改名/删除二次确认/新增)。`useRemoteList` 按需拉。
  - **独立 merge**:`merge_branch` 走 **CliBackend**(`git merge --no-edit`,跑 hooks+签名);`MergeOutcome{summary,fast_forward}`;
    冲突 → MergeConflict 落 merging 态复用冲突 UI。入口 = BranchSwitcher 每个非当前分支悬浮「合并到当前分支」(MergeIcon);
    记入撤销时间线(Restore)。
  - **clone + init**(最大的洞,onboarding 正门):**不走 RepoContext**(仓库刚诞生)——命令从
    `RepoRegistry.backend_arc()` 取后端、spawn_blocking 里建临时 RepoService。`git init`(尊重 init.defaultBranch)
    / `git clone` 均走 CLI;纯函数 `derive_repo_name`(app-service)推导目录名,克隆进 parent/<名>、返回仓库根路径。
    错误 `DestinationNotEmpty`/`InvalidUrl`。入口 = 启动屏 EmptyState「克隆仓库/新建仓库」+ `CloneDialog` 模态
    + 命令面板;成功后自动打开新仓库。⚠️ **trait 方法命名 `clone_repo` 而非 `clone`** —— 否则与
    `Clone::clone` 在 `Arc<dyn GitBackend>` 上撞名,所有 `.clone(url,dst)` 调用点会误解析成 Arc 的 0 参 clone。
  - ⚠️ 真机视觉验收待做:远程管理面板、merge 冲突跳转、clone 进度/认证失败提示、init 后空仓库视图。
  - ✅ **diff / 冲突编辑器语法高亮**(2026-07-05,纯前端):`app/src/lib/syntax.ts` 做路径语言识别 + 轻量单行 tokenization,
    `DiffView` 在统一/并排两种视图里复用同一套 token 渲染,保留词级 emphasis 与行级暂存;`ConflictEditor`
    通过 `cmSyntax.ts` 转 CodeMirror decoration,三栏共享语法高亮。无 Rust / IPC / DTO 变更。
  - ✅ **协作 token 弹窗 i18n 切片**(2026-07-05,纯前端):GitHub/GitLab token 设置弹窗的状态、按钮、
    placeholder、成功 toast 全部接入 `useT()` 和 `collabToken.*` 字典键;补 `TokenDialogs.test.tsx`
    锁定中英文切换行为。PR/MR 大面板仍有散落文案,后续按面板继续切。
  - ✅ **协作创建 PR/MR 弹窗 i18n 切片**(2026-07-05,纯前端):GitHub 创建 PR / GitLab 创建 MR 弹窗
    的标题、字段、草稿、缺远程提示、token/cancel/create 按钮和 toast 接入 `collabCreate.*`;
    原有 API payload 测试保留,新增中英文可见文案断言。PR/MR 列表详情面板仍待继续切。
  - ✅ **GitHub PR 面板外壳 i18n 切片**(2026-07-06,纯前端):`GithubPrPanel` 的 dialog 名称、标题、
    远程/分支 fallback、缺远程/缺分支错误、loading/empty 状态、列表打开/详情按钮、底部 token/refresh/close
    接入 `githubPr.*`;详情区指标、check runs、comments、merge/comment 控件仍留给后续切片。
  - ✅ **GitHub PR 详情区 i18n 切片**(2026-07-06,纯前端):`GithubPrPanel` 详情体里的指标 label、
    计数单位、Check runs/Recent comments/Review threads 标题、merge method/options、merge/comment 按钮、
    评论输入 label/placeholder 接入 `githubPrDetail.*`;API 状态、CI 名称、用户、日期、toast、阻塞原因仍保持原样。
  - ✅ **GitLab MR 面板外壳 i18n 切片**(2026-07-06,纯前端):`GitlabMrPanel` 的 dialog 名称、标题、
    远程/分支 fallback、缺远程/缺分支错误、loading/empty 状态、列表打开/详情按钮、底部 token/refresh/close
    接入 `gitlabMr.*`;详情区 approvals、notes、discussions、pipeline jobs、merge/comment 控件仍留给后续切片。
  - **Tier 0 自动化门禁已补齐**:跨平台 CI、严格标签发布预检、双架构 macOS、CSP、
    依赖边界、核心桌面 E2E 和包体预算均已落地。真正公开发布仍需发布负责人配置
    Windows/macOS/updater 密钥与生产 endpoint,并完成各架构安装验收。
- **下一里程碑:M7 · 协作/PR**(原 roadmap M6,见 `2026-06-08-world-class-roadmap.md`)。
- ⚠️ **真机视觉/交互验收欠账**(自动门 test/clippy/fmt/tsc/build 全过):M5 各刀、图片 diff、
  **M6.1**(大 diff 虚拟化滚动/并排联动/折叠)、**M6.2**(图片字节流加载/滑块/洋葱皮)、
  **M6.6**(面板键盘)都需真机过一遍。M6.3/M6.4 是后端/类型纯逻辑,无需真机视觉验收。
  Git 同步与分支状态以命令实时检查为准,不要继续维护易过期的 ahead/push 文本快照。
- 零散续做:worktree 切换/新建(M4.5 只做展示);log 里 ctrl-多选两提交→比较。
- 工程收尾:真机验收交互式 rebase(尤其中途冲突的继续/中止、大仓库 cp/exec 路径);
  为 updater 配生产公钥/endpoint,由发布负责人注入签名/公证 secrets,并在 Windows/Linux/Intel Mac/Apple Silicon 安装验收。
- 已知小项:composite 40+ 透传样板(Rust 固有税,可选 delegate crate)。

## 验证命令
- 后端:`cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all --check`
- 边界:`powershell -NoProfile -File scripts/check-dependency-boundaries.ps1`
- 前端:`pnpm -C app test`、`pnpm -C app build`
- 发布门:`node --test scripts/release-preflight.test.mjs`、`node --test scripts/check-bundle-size.test.mjs`、
  `pnpm -C app release:check -- --allow-unsigned`
- 桌面 E2E:`pnpm -C app e2e:ci`
- 真机开发:`pnpm -C app tauri dev`

## superpowers 产物
`docs/superpowers/plans/` 下有各功能的 spec/plan(remote-fetch、interactive-rebase、post-sync-marks-roadmap 等)。

## 新机器准备(换电脑后)
1. 装 Rust(rustup,≥1.85)、Node(LTS)、pnpm、git(必须在 PATH)、Tauri 系统依赖(见 README 第 1.3)。
2. `git pull` 拿到 main 最新。
3. `cargo test --workspace` + `cd app && pnpm install`(首次)验证环境。
