# 路线图:迈向"世界第一 Git 工具"(2026-06-08)

> 现状:日常功能已基本对齐 JetBrains 内置 Git(status/stage/commit、图谱、
> 文件/块/行级 diff、分支、push/pull/fetch、tag、reset、amend、cherry-pick、
> revert、stash、三栏冲突、任意两提交比较 + 就地比较、可取消搜索、交互式 rebase、
> reflog)。CI 卡口已立。
>
> 但"功能齐全"≠"世界第一"。下面定义目标、诚实评估架构债,并按**最重要优先**排序。

---

## 一、"世界第一"的定义:五根支柱

对标 JetBrains / GitKraken / Fork / lazygit / GitButler,真正让人换用并称之为最好的,是:

1. **Instant(快)** —— 任意仓库规模都不等待、不卡顿。10 万文件 monorepo、百万提交历史下依然跟手。**这是地基**,所有体验都建在它上面。
2. **Fearless(稳)** —— 永不丢工作。每个操作可撤销,状态永远清晰,破坏性操作有预览。
3. **Fluid(顺)** —— 键盘优先、命令面板、拖拽,比任何对手更少点击完成同一件事。
4. **Correct(对)** —— 扛得住真实世界 git:子模块、worktree、LFS、稀疏检出、提交签名、hooks、超大文件。
5. **Trustworthy(可信)** —— 有测试网、可观测、崩不了。生产级软件的底线。

排序原则:**地基 → 信任 → 顺手 → 正确性 → 协作**。先把不可动摇的底座打牢,再往上叠体验。

---

## 二、诚实的架构债(为什么 M1 排第一)

当前实现是"能跑通分层"的最简版,离世界级有明确差距:

| 债 | 现状 | 后果 |
|---|---|---|
| 无长驻状态 | 49/51 个命令每次 `RepoService::new(...)` + 重新 `open` 仓库 | 无法缓存,重复计算 |
| 无缓存 | 图谱/status/diff/blame 每次从头算 | 大仓库每次切换都卡 |
| 无取消(除搜索) | 切分支时旧 log 还在跑 | 浪费 CPU,结果回来还可能覆盖新数据 |
| 图谱非增量 | "加载更多"重算全部泳道 O(n) | 长历史分页越来越慢 |
| 无虚拟滚动 | 一次渲染所有行,DOM 无上限增长 | 长列表滚动卡顿 |
| 前端零测试 | 无 vitest/RTL | 重构易回归(比较页 bug 就是例证) |
| app-service 未 actor 化 | 见 ARCHITECTURE 第 4 部分蓝图 | "成败手"未落地 |

ARCHITECTURE.md 自己把 **actor + 缓存 + 取消** 称为本项目的"成败手"。这就是 M1。

---

## 三、里程碑(最重要优先)

### M1 · Instant —— 性能与响应的地基 【✅ 已完成】
> 为什么第一:所有功能都坐在它上面;是和世界级差距最大的维度;也是 Rust 概念最密集
> 的部分(Arc / channel / Mutex / lru / CancellationToken),贴合学习目标。
>
> 状态:六刀全部完成并 merge 入 main(未 push)。每刀竖切 + 测试兜底 + 全门绿。

- ✅ **M1.0 测试网先行**:接入 vitest + RTL + jsdom,12 测试(mergeModel / graphGeometry / 比较页默认值回归),CI 前端 job 跑 `pnpm test`。
- ✅ **M1.1 长驻仓库上下文**:`RepoRegistry`(Tauri `State`)+ `RepoContext`(repo_path→`Arc<RepoContext>`,共享后端建一次);49 命令路由到它,行为零变化。
- ✅ **M1.2 读缓存 + 失效**:`RepoContext` 内 `lru` 缓存 status/graph/log/diff/blame/refs/…;三类失效语义(不可变 SHA 寻址 / worktree 域 / ref 域,blame 双域),双源失效(自身写 `after_write` + 文件监听回调 `invalidate`)。
- ✅ **M1.3 全面取消**:`GitBackend::log/blame` 加 `cancelled` 闭包,git2 循环里 honor;`RepoContext` 每操作类代次计数,新请求取消旧。单文件 diff(很快)暂未纳入。
- ✅ **M1.4 虚拟滚动**:`CommitGraph` 用 `@tanstack/react-virtual` 只渲染可见行(+overscan)。
- ✅ **M1.5 增量泳道布局**:`graph.rs` 可续算(`LayoutState` + `layout_into`),`RepoContext` graph 缓存换 `GraphAccum` 累加器,加载更多 O(可见)、锁外可取消。

**成功标准**:在合成的大仓库(10 万提交 / 数万文件)上,首屏 < 1s,切 tab/切分支无可感卡顿,滚动 60fps,切换时旧任务即时取消。
> 注:合成大仓库压测基准尚未建(贯穿全程 Trustworthy 项),真机已用 git-client 自身仓库验证滚动/切换跟手。

### M2 · Fearless —— 永不丢工作的信任 【✅ 已完成】
> 为什么第二:在已有 reflog 之上很便宜,却带来巨大信任(GitButler 的核心卖点)。
> 状态:三刀全部完成并 merge 入 main(未 push)。每刀竖切 + 测试兜底 + 全门绿。

- ✅ **M2.1 多级 Undo/Redo**:操作时间线 + 光标(编辑器式),撤销/重做只移光标、不追加点,消除「撤销的撤销」乒乓;冷启动从 reflog `HEAD@{1}` bootstrap。纯逻辑 `UndoNav` 穷尽单测。
- ✅ **M2.2 破坏性操作统一确认 + 影响预览**:`ConfirmDialog` 收口 force push / hard reset / 删未合并分支 / 丢弃改动;删分支用 git2 revwalk 算「将丢弃 N 个孤儿提交」并列样本。
- ✅ **M2.3 操作日志面板**:本会话写操作时间线(复用 `UndoNav` + 时间戳),当前位置高亮,点任意项 reset 回跳(撤销/重做的泛化)。
- ✅ **撤销还原语义修复**(真机暴露):撤销按被撤操作分类选 `reset --soft`(提交,退暂存区留活)/ `--hard`(reset/cherry-pick/revert/merge/rebase/pull,忠实还原工作区、不留残渣);hard 前脏工作区拦截(`UncommittedChanges`),守住「永不丢工作」。

**成功标准**:任何破坏性操作后都能一键恢复到操作前状态;用户敢放心乱试。【达成:撤销/重做/操作日志回跳三路径均忠实还原,且脏工作区绝不被覆盖】

### M3 · Fluid —— 键盘优先的顺手 【主体完成】
- ✅ **M3.1 命令面板(⌘K)**:全局 ⌘K/Ctrl+K 开关;输入即模糊过滤;↑↓ 移动、回车执行、
  Esc 关闭。命令注册:视图切换、选/切仓库、主题、Fetch/Pull(合并|变基)/Push、
  撤销/重做、操作日志;不可用项灰显不可执行。纯逻辑(模糊匹配+排序)在 `app/src/lib/commands.ts`
  +12 vitest;UI 在 `app/src/components/CommandPalette.tsx`。纯前端,合入 main。
- ✅ **M3.2 列表键盘导航**:历史视图 j/k/↑↓/g/G 移动选中→驱动详情/diff;选中行自动滚进可视区。
  纯逻辑 `app/src/lib/listNav.ts`(navTarget,8 vitest)。
- ✅ **M3.2 续 聚焦面板模型 + 文件列表导航**:历史视图 commits|files 双面板,Tab/h・l/←→/Enter
  切焦点、点击也聚焦,聚焦面板显 accent 环,j/k 作用于聚焦面板。**待续:ChangesView 文件列表
  接入(那边有 stage/unstage,需空格等额外键)。**
- ✅ **M3.3 模糊跳转(分支)**:命令面板「跳转」子模式(统一 Entry + 通用 rankBy<T>),首个
  provider「跳转到分支…」模糊找分支→checkout。**待续:跳转到提交/文件(提交跳转需跨视图选中)。**
- **M3.4 拖拽**(较重,放后):拖提交到分支 = cherry-pick / reset;rebase 列表拖拽重排。
- ✅ **M3.4 拖拽(rebase 重排)**:RebaseEditor 提交列表拖拽重排(GripIcon 手柄 + 插入线 +
  纯函数 `moveItem<T>`,5 vitest)。**待续:拖提交到分支 = cherry-pick/reset(行内徽章小目标 +
  意图消歧,留作独立设计)。**

**成功标准**:常用流程(切分支、暂存提交、比较、cherry-pick)可全程不碰鼠标。
**进度**:命令面板 ⌘K、提交/文件列表 j/k 导航 + 聚焦面板、⌘K 跳转到分支、rebase 拖拽重排均已落地;
键盘流主体打通。剩零散续做(ChangesView 文件键盘化、跳转到提交/文件、拖提交到分支)。

### M4 · Correct —— 扛住真实世界 git 【当前支柱 · 计划 2026-06-08】
按「先防崩 → 再可信 → 后覆盖」排序,每刀仍走竖切 + feat 分支 + 全门绿。

- ✅ **M4.1 超大文件 / 二进制优雅处理**(已合 main):diff 用 `patch.line_stats` 算总行数,
  超 20000 行→`FileDiff.too_large`、不构建 Vec(真正卡点是前端渲染几万 DOM 行);DiffView 占位。
  blame 调 blame_file 前挡掉超大(>2MB→`FileTooLarge`)与二进制(前 8000 字节含 NUL→`BinaryFile`);
  BlameView 居中显友好错误。GitError +FileTooLarge/BinaryFile,to_ipc 同步。git-engine +2 tempfile 测试。
  注:`delta.new_file().size()` 在 tree-to-tree diff 返回 0(libgit2 不填),故用 line_stats 而非字节数。
- **M4.2 提交签名验证徽章**(高可见、对标 GitHub「Verified」)
  - 读签名状态:CLI `git log --format=%G?`(G=good/B=bad/U=unknown/N=none)或 `git verify-commit`;
    git2 读签名不便,走 CliBackend。模型 SignatureStatus → CommitDto/GraphRow 加字段 → 图谱行/提交详情显徽章。
  - 仅读、不改写,风险低。
- **M4.3 尊重 hooks 与签名的提交路径**(修正确性硬伤)
  - 现状:commit/amend 走 git2,**绕过 pre-commit/commit-msg hooks 与 commit.gpgsign 签名**。
  - 改:检测到仓库配了 hooks 或 `commit.gpgsign=true`(或用户开「签名提交」)时,提交走 CliBackend
    (`git commit`,原生跑 hooks + 签名);否则保持 git2 快路径。需谨慎,有真机验收清单。
- **M4.4 子模块感知**:读 `.gitmodules` + 子模块状态(未初始化/有改动/落后);UI 列出,支持 init/update(CLI)。
- **M4.5 worktree 列表**(niche):`git worktree list` 展示;切换/新建留后。
- **M4.6 LFS 感知 / 稀疏检出**(niche、检测优先):识别 LFS 指针文件(别把指针当内容 diff)、
  显示稀疏检出范围;完整管理留后。

**成功标准**:在带子模块/LFS/签名的真实大型仓库上不崩、不误判;签名提交与 hooks 正常生效。
**建议起点**:M4.1(防崩)→ M4.2(签名徽章)。

### M5 · 更深的 diff 与历史 【第五】
- 并排(side-by-side)diff、行内/词级 diff(`similar`)、图片/二进制 diff。
- 文件历史 / 行历史(`log -L`)、pickaxe 搜索(`-S`/`-G`,按代码内容找提交)。

### M6 · 协作 —— PR / 平台集成 【最后,面最大】
- GitHub/GitLab:查看/创建 PR、内联 CI 状态、评论与评审。

### 贯穿全程 · Trustworthy
- 前端 vitest + 关键路径 E2E(Tauri WebDriver);后端测试持续补;`tracing` 可观测;
  合成大仓库 fixture 用于性能基准与回归。

---

## 四、立即开始:M1.0 → M1.1

**先做 M1.0(测试网)**,半天,产出:
- `app/` 接入 vitest + @testing-library/react + jsdom;`pnpm test` 脚本;CI 前端 job 加跑 `pnpm test`。
- 首批测试:① 比较页默认两端=当前分支(锁住刚修的 bug);② graph lane 布局纯函数;③ mergeModel 纯函数。

**再做 M1.1(RepoActor 骨架)**:
- `app-service` 增 `RepoRegistry`(`Mutex<HashMap<repoPath, Arc<RepoContext>>>`)或每仓库 actor;`RepoContext` 持 `Arc<dyn GitBackend>` + 缓存位。
- `src-tauri` 用 `State` 持 registry;命令改为 `registry.get(repo)` 取上下文,不再每次 `new`。
- 铁律不变:git 操作仍走 `spawn_blocking`;上层只依赖 `GitBackend` trait;`to_ipc` 穷尽匹配。
- 行为零变化,纯架构,跑通后 M1.2 缓存才有落点。

每刀仍走竖切 + feat 分支 + `--no-ff` 合 main + CI 绿。

---

## 五、明确不做(避免散焦)
- 不在 M1 之前堆新功能(地基没好,加功能是欠债)。
- 协作/PR(M6)推后:面最大、且核心还没"无敌"前不值得分心。
- 不追求一次做完;每个里程碑内按竖切一刀刀来,每刀都能跑、能验。
