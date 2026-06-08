# 交接 · 当前进度与下一步(2026-06-08)

> 给接手的 agent:先读 `CLAUDE.md` + `ARCHITECTURE.md` + 本会话记忆(MEMORY.md），
> 再读本文件拿到「现在到哪了 / 接下来做什么 / 有哪些坑」。路线图全貌见
> `docs/superpowers/plans/2026-06-08-world-class-roadmap.md`。

## 现在的状态

- 分支 `main`,工作区干净。**main 领先 origin 若干 commit(M1 全部 + M2 全部),尚未 push**——
  push 由用户发话才做(铁律)。
- 五支柱路线图:**M1「Instant」性能地基 ✅ 全完成**、**M2「Fearless」永不丢工作 ✅ 全完成**。
- 当前支柱:**M3「Fluid」键盘优先的顺手**(进行中)。
  - ✅ **M3.1 命令面板(⌘K)**(已合 main):全局 ⌘K/Ctrl+K → 模糊搜索所有动作 → 键盘执行。
    纯逻辑 `app/src/lib/commands.ts`(fuzzyMatch + rankCommands,12 vitest);UI `CommandPalette.tsx`;
    App 顶栏加可发现的「⌘K」按钮。纯前端、后端零改动。
    **真机待验**:⌘K 打开/关闭、输入过滤高亮、↑↓/回车/Esc、各命令执行(切视图/Fetch/撤销等),
    不可用项(未开仓库时)灰显。需 `pnpm -C app tauri dev` 整段重启(无 Rust 改动,纯前端 HMR 即可)。
  - ✅ **M3.2 提交列表键盘导航**(已合 main):历史视图 j/k/↑↓/g/G 移动选中→驱动详情/diff;
    选中行自动滚进可视区;输入态与组合键不拦截。纯逻辑 `app/src/lib/listNav.ts`(navTarget + useListKeyboardNav,8 vitest)。
  - ✅ **M3.2 续 聚焦面板模型**(已合 main):历史视图 commits|files 双面板;Tab/h・l/←→/Enter 切焦点、
    点击也聚焦,聚焦面板显 accent inset 环,j/k 作用于聚焦面板;文件列表选中自动滚进可视区。
  - ✅ **M3.3 模糊跳转(分支)**(已合 main):命令面板「跳转」子模式——统一 Entry 抽象 + 通用 `rankBy<T>`;
    首个 provider「跳转到分支…」模糊找本地分支→checkout(标「当前」、不重复切);Esc/空输入 Backspace 返回命令模式。
  - ✅ **M3.4 拖拽(rebase 重排)**(已合 main):RebaseEditor 提交列表拖拽重排——GripIcon 手柄 +
    拖动变淡 + 目标行顶端 accent 插入线;纯函数 `moveItem<T>`(lib/listNav.ts,5 vitest),▲▼ 按钮也复用它。
  - ✅ **M3 收尾 更改视图键盘导航 + 空格暂存**(已合 main):「更改」视图三段(冲突/暂存/未暂存)扁平成
    一个 j/k 导航列表→驱动 diff,空格暂存/取消暂存选中文件(冲突除外),自动滚进可视区。
    至此键盘流覆盖历史 + 更改两个主视图。
  - **M3 完成**(命令面板 / j・k 导航 + 聚焦面板 / ⌘K 跳转分支 / rebase 拖拽 / 更改视图键盘化)。
    剩极零散续做(非阻塞):跳转到提交/文件(需跨视图选中)、拖提交到分支(意图消歧)。
  - **M4 · Correct(扛住真实世界 git)进行中**——计划见 world-class-roadmap.md「M4」段。
    - ✅ **M4.1 超大文件/二进制优雅处理**(已合 main):diff 超 20000 行→`FileDiff.too_large` 跳过逐行
      (line_stats 廉价判定;注意 git2 `delta.size()` 在 tree-to-tree 返回 0,不可用);blame 调
      blame_file 前挡超大(>2MB)/二进制(NUL),`GitError::FileTooLarge`/`BinaryFile`。DiffView/BlameView 占位。
    - ✅ **M4.2 提交签名验证徽章**(已合 main):CliBackend `git show -s --format=%G?<NUL>%GS`→
      `SignatureInfo`{status,signer};trait commit_signature(默认 Unsupported,Composite 委托 cli);
      CommitDetail 选中提交显徽章(绿已验证/黄已签名未验证/红无效),无签名不显。待续:图谱行批量徽章。
    - 下一刀:M4.3 尊重 hooks/签名的提交(commit 走 git2 绕过了 hooks+gpgsign,检测到配置时改走 CLI)。

## 已完成(本里程碑 M2「Fearless」)

三刀竖切,均 `feat 分支 → merge --no-ff 回 main → 删分支`,全门绿:

1. **M2.1 多级 Undo/Redo** —— 进程内「操作时间线 + 光标」的编辑器式撤销/重做,
   消除「撤销的撤销」乒乓。纯逻辑在 `crates/app-service/src/undo_nav.rs` 的 `UndoNav`。
2. **M2.2 破坏性操作统一确认 + 影响预览** —— `app/src/components/ConfirmDialog.tsx`
   收口 force push / hard reset / 删未合并分支 / 丢弃改动;删分支用 git2 revwalk
   算「将丢弃 N 个孤儿提交」(`branch_delete_impact`,git-core→git2→composite→fake→ipc 全竖通)。
3. **M2.3 操作日志面板** —— `app/src/components/OpLogPanel.tsx`,复用 UndoNav 时间线,
   点任意项 reset 回跳;顶栏 HistoryIcon 入口。

### ⚠️ 关键修复(真机暴露,记牢这条 git 语义)

撤销原本一律 `reset --soft`,但 **soft 只挪 HEAD、不碰工作区/暂存区**——撤销一次
`reset --hard` 后 HEAD 退回而暂存区仍停在 hard 的目标处,差异全变「已暂存」=凭空多文件
(cherry-pick/revert/merge/rebase 同病)。

**修法**:撤销模式跟着被撤操作的「还原语义」走(新增 `git_core::UndoKind{Uncommit,Restore}`):
- 提交 / amend = `Uncommit` → `reset --soft`(内容退暂存区,保留你的活)。
- reset / cherry-pick / revert / merge / rebase / pull = `Restore` → `reset --hard`(忠实还原完整工作区,不留残渣)。
- 操作日志跨多步跳转一律 hard。

**守「永不丢工作」**:`RepoContext::apply_nav` 在 hard 前查 status,脏就报
`GitError::UncommittedChanges`(可恢复,提示先提交/贮藏),绝不覆盖。

**铁律:`reset --soft` 永远修不回 `reset --hard` 改过的工作区——撤销破坏性操作必须用 hard。**

## 接下来要做(按优先级)

### 首选:M3「Fluid」键盘优先(进行中)
让常用流程全程不碰鼠标。仍按竖切、一次一小块:
- ✅ **命令面板(⌘K)**:已落地(见上)。新增顶层动作时往 `App.tsx` 的 `commands` 数组加一条即可
  (run 闭包就地建、拿最新 state;不可用给 `disabled`)。
- **下一刀 · 全局键盘导航**:提交列表 / 文件列表 j/k 移动、回车进入、各视图统一快捷键。
  建议抽一个 `useListKeyboardNav(items, onActivate)` hook,先接 HistoryView 的 CommitGraph/SearchList,
  再推广到 ChangesView 文件列表。注意与命令面板/输入框焦点互斥(输入态不抢 j/k)。
- **模糊跳转**:分支/文件/提交统一 ⌘K 入口(现有 `BranchSwitcher` 升级,或命令面板二级模式)。
- **拖拽**(较重,放后):拖提交到分支 = cherry-pick/reset;rebase 列表拖拽重排。
- 成功标准:切分支、暂存提交、比较、cherry-pick 可全程键盘完成。

### 可选:零散补完 M2(小而独立,想随手清掉)
- **force push**(带二次确认,复用 `ConfirmDialog`)——当前不存在该功能。
- **丢弃单文件改动**(带确认)——当前不存在。

### 延后项(非阻塞)
- `[[cleanup-split-tauri-shell]]`:拆 `app/src-tauri/src/lib.rs` 外壳(纯整理,不为行数单独重构)。
- 单文件 diff 取消(很快,M1.3 暂未纳入)。
- 合成大仓库性能基准 fixture(Trustworthy 贯穿项,未建)。
- UI 配色:用户曾反馈历史视图配色不够精美,调色未做。

## 必守的工程纪律(详见 CLAUDE.md / 记忆)

- 所有 git 操作走 `spawn_blocking`,绝不在 async 命令里直接调;命令层极薄
  (`registry.context()` → spawn_blocking → `ctx.X` → `map_err(to_ipc)`)。
- 上层只依赖 `GitBackend` trait;改 trait 要同步 4 个实现(composite/git2/cli/fake)。
- `to_ipc` 是穷尽 match:新增 `GitError` 变体必须加对应分支。
- 每功能开 feat 分支 → 完成 → `git merge --no-ff` 回 main → 删分支。**绝不擅自 push,绝不改全局 git 配置。**
- 前端只用 `@theme` token;按钮一律用封装的 `ui/Button`、`ui/IconButton`,缺样式就加到原语(别 className 覆盖)。
- 提交前全门:`cargo fmt --all --check` / `cargo clippy --workspace --all-targets -- -D warnings` /
  `cargo test --workspace` / `pnpm -C app test` / `pnpm -C app build`。
- 提交信息页脚:`Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`。
- 后端改动无法肉眼验时,诚实说明并给真机验收清单,等用户验过再合。

## 测试仓库

`/Users/jaden.fan/git-branch-lab`(本会话刚重置为干净基线,reflog 已清空):
- `main` @ 合并提交;`feature-merged`(已并入)、`feature-unmerged`(3 个未合并提交)、
  `wip-experiment`(2 个实验提交)、`at-main`。
- 本地 `user.email` 用个人邮箱(仅本地);专测删分支影响 / 撤销 / reset 还原。

## 已知坑

- libgit2 `Revwalk::push_head()` 对空仓库返回 `GenericError` 而非 `UnbornBranch`
  → 检测空仓库用 `repo.head()` 捕 `ErrorCode::UnbornBranch`。
- shell 是 **zsh**:未加引号的 `$var` 不做单词分割;数组用 `"${(@f)...}"`。
- 横向可滚的逐行视图,行背景靠「`min-w-max` 容器 + `min-w-full` 行」对齐,别用 per-row `w-max`。
- 交互式 rebase 的 squash 实现是 `fixup + git commit --amend -F msg`(只用你输入的信息,
  多条 squash 会丢原始信息,非原生拼接)——已有单测,但用户尚未本机 GUI 逐项点验。

## 提醒下一个 agent

- 接手第一步:`git log --oneline -10` 看最近进展,`git status` 确认干净。
- 改了新 Rust 命令 / DTO 后,真机要**整段重启** `pnpm -C app tauri dev`(HMR 不会重编 Rust)。
- 用户是 React/Next 出身、Rust 初学者,讲 Rust 概念时多解释;偏好被直接驱动 + 给推荐,少用选择题。
