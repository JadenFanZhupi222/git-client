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

### M2 · Fearless —— 永不丢工作的信任 【次高】
> 为什么第二:在已有 reflog 之上很便宜,却带来巨大信任(GitButler 的核心卖点)。

- **撤销上一步操作**:基于 reflog / ORIG_HEAD 一键撤销刚才的 commit/reset/rebase/merge/cherry-pick(顶栏"撤销"+ toast 提供)。
- **破坏性操作预览/确认**:force push、hard reset、删未合并分支、丢弃改动 —— 统一二次确认 + "将影响 N 项"预览(部分已有,收口成一致体验)。
- **操作日志面板**:本工具做过的写操作时间线,可点回看 reflog 对应项。

**成功标准**:任何破坏性操作后都能一键恢复到操作前状态;用户敢放心乱试。

### M3 · Fluid —— 键盘优先的顺手 【第三】
- **命令面板(⌘K)**:所有动作可搜索、可键盘触发。
- **全局键盘导航**:列表 j/k、回车进入、各视图统一快捷键。
- **拖拽**:拖提交到分支 = cherry-pick / reset;rebase 列表拖拽重排。
- **模糊跳转**:分支/文件/提交统一 ⌘K 入口(现有 BranchSwitcher 升级)。

**成功标准**:常用流程(切分支、暂存提交、比较、cherry-pick)可全程不碰鼠标。

### M4 · Correct —— 扛住真实世界 git 【第四】
- 子模块、worktree、LFS 感知、稀疏检出、超大文件优雅处理。
- 提交签名(GPG/SSH)创建与验证徽章;尊重 hooks。

**成功标准**:在带子模块/LFS/签名的真实大型仓库上不崩、不误判。

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
