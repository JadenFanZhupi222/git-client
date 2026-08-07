---
target: 当前 History UI 重新评分
total_score: 32
p0_count: 0
p1_count: 1
timestamp: 2026-08-07T09-51-54Z
slug: app-src-views-historyview-tsx
---
## Design Health Score

| # | 启发式原则 | 评分 | 主要问题 |
|---|---|---:|---|
| 1 | 系统状态可见性 | 3/4 | 加载、选中、搜索和 Toast 清晰，但 Diff/文件错误被统一显示在左侧搜索区。 |
| 2 | 系统与现实匹配 | 4/4 | HEAD、Reflog、SHA、Changed Files、Diff 与 Git 工作流一致。 |
| 3 | 用户控制与自由 | 4/4 | 搜索清除、取消、撤销、宽度持久化、键盘调整与双击复位完整。 |
| 4 | 一致性与标准 | 3/4 | 图谱和分栏语义已统一；文件行仍是可点击 div，单选组缺少方向键行为。 |
| 5 | 错误预防 | 3/4 | Reset/Rebase 护栏完善，但异步错误没有就近出现。 |
| 6 | 识别优于记忆 | 3/4 | 主操作清晰；比较手势、j/k/g/G、h/l、Tab 等仍依赖记忆。 |
| 7 | 灵活性与效率 | 4/4 | 自动恢复、键盘导航、比较、拖拽、虚拟化与可访问分栏支持专家流。 |
| 8 | 美观与极简 | 3/4 | 专业且克制；全宽筛选行让固定工具区达到约 120px。 |
| 9 | 错误识别与恢复 | 3/4 | Toast、冲突引导和确认良好，但错误位置削弱诊断。 |
| 10 | 帮助与文档 | 2/4 | 有 tooltip 和一次性提示，但缺少快捷键与搜索范围的就地说明。 |
| **总分** |  | **32/40** | **Good：基础可靠，剩余问题集中且可修。** |

## Anti-Patterns Verdict

**LLM assessment：强通过。** 当前界面不像模板化 AI dashboard：三栏信息架构、提交泳道、HEAD/分支徽章、Diff 工作流与专家交互都具有明确的 Git 产品特征。纸白与朱红已经写入产品规范，视觉风格一致。剩余问题属于实现接缝，而非需要重做审美方向。

**Deterministic scan：0 条。** `detect.mjs --json app/src/views/HistoryView.tsx` 返回 `[]`，无规则命中、无误报。检测器不会识别文件行语义、错误归属和工具栏比例，因此人工评估仍发现以下问题。

**Visual overlays：未生成。** 内置浏览器返回 `Browser is not available`，随后默认浏览器选择返回 `No browser is available`，无法导航或验证脚本注入。未启动 dev server 或 overlay server。当前截图是旧的一行搜索布局，只用于确认旧问题，当前布局判断以最新源码尺寸为准。

## Overall Impression

当前 History 已从“可用但有明显无障碍和首屏空洞”提升为高可信专业工具。最大的机会不是继续加装饰，而是补齐主流程第二步——文件列表——的语义和窄屏结构，同时让搜索范围退回次级视觉权重。

## What's Working

1. **进入即可工作。** 每个仓库恢复有效的提交/文件；无有效记录时选中 HEAD 和首个变更文件，Diff 不再大面积空置。
2. **无障碍改动是真正结构性的。** 文本与品牌色达到 AA；图谱采用 listbox/option；分栏暴露 ARIA 数值，支持方向键、Shift 加速、Home 与双击复位。
3. **视觉语言更统一。** 12px sentence-case 栏头、18px sans 提交标题、44px 空态图标和 23%/18% 自适应列宽都更符合专业桌面工具。

## Priority Issues

### [P1] Changed Files 文件行没有补齐语义与键盘可见性

**Why it matters：** 主流程是提交 → 文件 → Diff。屏幕阅读器可以正确进入提交图谱，却在文件列表失去列表结构、选中状态和清晰的操作路径；File History 仅 hover 显示，键盘焦点下可能仍不可见。

**Fix：** 文件容器使用 `role="listbox"`；文件行使用 `role="option"`、`aria-selected`、可读的状态/路径/增删行标签、roving tabIndex 与 Enter/Space；File History 同时响应 `group-focus-within` 和自身 focus。

**Suggested command：** `$impeccable audit`

### [P2] 两行搜索解决截断，但搜索范围权重过高

**Why it matters：** 当前搜索块为 32px 输入 + 8px 间距 + 32px 全宽筛选 + 16px 内边距，共 88px；加栏头至少约 120px。三个短标签各占约 149px，视觉权重与查询输入相同，固定占用约两条提交行高度。

**Fix：** 保留全宽 32px 输入；第二行改为左对齐、总宽 180–216px 的三段控件，或使用 96–120px 的“Search in”选择器。目标是栏头加搜索控制在 96–104px 内，一次性提示不参与永久固定高度。

**Suggested command：** `$impeccable layout`

### [P2] 工具栏初始 inset 仍是旧值

**Why it matters：** `barH` 初值仍为 80px，而当前固定内容约 120px，带提示时约 145px。ResizeObserver 会纠正，但首帧提交或骨架可能先被玻璃栏遮住，再向下跳。

**Fix：** 使用 `useLayoutEffect` 首次同步测量；短期把初值改到 120px；长期让结构处于正常文档流，仅叠加玻璃材质，避免 JS 管理布局占位。

**Suggested command：** `$impeccable polish`

### [P2] 异步错误没有在失败的栏位就近显示

**Why it matters：** 图谱、搜索、文件与 Diff 查询错误目前汇总到左侧搜索工具栏。Diff 失败时，用户会在右侧看到空白，却要跨越多个栏位寻找说明，容易误判故障来源。

**Fix：** 图谱/搜索错误放在搜索下；文件错误放在 Changed Files 下；Diff 错误放在 Diff 标题下并提供 Retry；pending 用 `role="status"`，错误用 `role="alert"`。

**Suggested command：** `$impeccable harden`

### [P2] 最小列宽允许界面被拖到结构性不可用

**Why it matters：** 图谱最小 220px 时，泳道、时间与 SHA 几乎吃完标题宽度；详情最小 200px 时，Reset/Cherry-pick/Revert/Rebase 会变成多行按钮墙；Diff 也可能只剩约 384px。

**Fix：** 图谱最小约 320px，详情约 300px，Diff 保持至少 560–640px；低于约 1280–1360px 时将详情/文件折叠为 tabs 或 inspector，而不是继续挤压三栏。若必须支持 220px 图谱，按断点先隐藏时间、再隐藏 SHA。

**Suggested command：** `$impeccable adapt`

## Persona Red Flags

### Alex（专家用户）

- 自动恢复、j/k/g/G、h/l、Tab、修饰键比较、拖拽 cherry-pick 和虚拟化都很强。
- 但这些加速器没有可见快捷键入口；全宽筛选行浪费约两条提交行；极窄列宽破坏扫描效率。

### Sam（键盘、屏幕阅读器、低视力）

- AA 对比度、可见焦点、语义化图谱和 ARIA 分栏已经解决上一轮主要阻塞。
- 文件行仍没有 option/selected 语义；File History 依赖 hover；radiogroup 缺少标准方向键切换；加载和错误变化缺少清晰 live region。

### Jordan（较少使用 Git 的用户）

- 自动选择 HEAD/文件比空态说明更能教会界面。
- `-S`/`-G` 差异只藏在 title；Reflog、修饰键比较依旧依赖经验；一次性提示先教高级 cherry-pick，却没有紧凑的 History 快捷键入口。

## Minor Observations

- 保留当前 AA 颜色，不要退回先前的浅灰 metadata。
- 朱红品牌色与 danger 仍较接近，危险操作必须继续同时使用明确文案/图标，不能只靠色相。
- 56px 提交行舒适；可把 48px compact 作为专家密度选项，而不是替换默认值。
- 默认约 369px 的详情栏已接近四个操作 chip 的容纳极限，本地化和窄栏会导致换行。

## Questions to Consider

- 搜索范围是否需要和查询输入拥有相同的视觉权重？
- 最小支持窗口宽度是多少；低于 1360px 时首先折叠哪一栏？
- 快捷键是核心差异化能力时，用户应在哪里学习它们而不增加永久 chrome？
- 每个查询错误是否都应该出现在其结果本应出现的位置？
- File History 是否重要到应该在选中行上常驻，而不是只在 hover 时出现？
