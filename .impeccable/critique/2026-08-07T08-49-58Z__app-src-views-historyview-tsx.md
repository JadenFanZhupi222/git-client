---
target: History 视图搜索框与整体比例
total_score: 29
p0_count: 0
p1_count: 2
timestamp: 2026-08-07T08-49-58Z
slug: app-src-views-historyview-tsx
---
# Strata History 视图设计评审

## Design Health Score

| # | Heuristic | Score | Key issue |
|---|---|---:|---|
| 1 | Visibility of System Status | 3/4 | 分支、同步与活动视图清楚，但已有提交时详情区仍以空态开始。 |
| 2 | Match System / Real World | 4/4 | Graph、HEAD、SHA、Reflog、Changed Files、Diff 符合专业 Git 心智模型。 |
| 3 | User Control and Freedom | 3/4 | 支持清除、撤销、拖拽列宽和退出比较；列宽缺少键盘调整与重置。 |
| 4 | Consistency and Standards | 3/4 | 三栏与通用控件一致，但裸 input 的直角红色焦点框不属于现有控件语言。 |
| 5 | Error Prevention | 3/4 | 危险操作有确认与忙碌态；部分上下文操作仍较隐蔽。 |
| 6 | Recognition Rather Than Recall | 2/4 | 右键、比较、j/k 与面板切换依赖记忆。 |
| 7 | Flexibility and Efficiency | 3/4 | 专家快捷操作丰富，但缺少可发现性，分隔条只支持指针。 |
| 8 | Aesthetic and Minimalist Design | 3/4 | 整体克制且专业；搜索区堆叠和双空态削弱首屏重心。 |
| 9 | Error Recovery | 3/4 | Toast、冲突路径和撤销较完整；个别后端错误文案可能直接泄露。 |
| 10 | Help and Documentation | 2/4 | 有 tooltip 与一次性拖拽提示，但没有快捷键或搜索模式帮助。 |
| **Total** |  | **29/40** | **Good：基础可靠，需补可访问性与首屏任务落点。** |

## Anti-Patterns Verdict

**LLM assessment:** 不像通用 AI dashboard。三栏 Git 工作流、图谱泳道、提交元数据对齐和专家操作都具有真实产品针对性。信任感主要被红色焦点/危险语义混用、过浅小字和初始大面积空态削弱。

**Deterministic scan:** `detect.mjs --json app/src/views/HistoryView.tsx` 返回 `[]`，0 条规则命中、无误报。检测器没有覆盖本次主要问题：占位文字对比度、裸 input 比例、焦点语义与无响应式三栏压缩。

**Visual overlays:** 未生成。浏览器自动化可用，但 native Tauri 的仓库状态和 IPC 未能在独立 Vite 页面复现；启动尝试在创建进程前被重复的 `Path`/`PATH` 环境键阻止。未启动 dev/live server，无残留进程。

## Overall Impression

整体比例并没有全面失控。截图的逻辑宽度约为 2048px（原始 PNG 2560px，约 125% 缩放），栏宽约为 176 / 468 / 368 / 1036px：侧栏 9%、图谱 23%、详情 18%、Diff 50%，这是合理的专业 Git 桌面布局。搜索框横向已接近图谱列满宽；真正偏小的是约 22px 的高度、12px 字号和不完整的输入表面。最大的机会不是把所有控件一起放大，而是统一密度基线，并让 History 打开后立即落在 HEAD 的详情与 Diff 上。

## What's Working

1. 三栏信息架构优秀：图谱、提交事实/文件、Diff 同屏，避免频繁弹窗和视图跳转。
2. 截图当前栏宽比例合理，侧栏 176px 展开 / 48px 收起也符合桌面工具密度。
3. 图谱和专家操作是真实产品能力：虚拟化、泳道、比较、拖拽 cherry-pick、键盘导航和可调列宽都支持专业工作流。

## Priority Issues

### [P1] 小号辅助文字不满足 WCAG AA

- **Why:** `#9a9082` 在 canvas/elevated 上只有约 2.78:1 / 3.01:1；搜索 placeholder、作者、时间、SHA 等核心扫描信息显得发灰。强调红在 elevated 上约 4.42:1，也略低于小字 4.5:1 要求。
- **Fix:** placeholder 与信息元数据改用 `text-fg-muted`；若 `fg-subtle` 承担文字，至少加深到约 `#756b5f`。subtle 仅用于装饰图标、分隔线和 disabled 内容。
- **Suggested command:** `$impeccable audit`

### [P1] 列表与分隔条缺少语义键盘可访问性

- **Why:** 提交/搜索结果是可点击 `div`，分隔条是 mouse-only `div`；视觉键盘监听不能向读屏器表达列表、选中态与当前列宽。
- **Fix:** 图谱使用 `listbox/option/aria-selected` 与 roving tabindex；分隔条使用 `role=separator`、`aria-valuemin/max/now`，方向键 16px、Shift+方向键 48px，Home 或双击恢复默认。
- **Suggested command:** `$impeccable harden`

### [P2] 搜索框纵向过小且焦点看起来像错误态

- **Why:** 当前逻辑尺寸约 432×22px，比例过扁；透明裸 input 配合直角朱红 outline 成为画面最强元素，并与 danger 语义混淆。Message/Content/Regex 另占一行，又让搜索工具条整体偏高。
- **Fix:** 使用 36–38px 搜索工具行；input 高 30–32px、6px 圆角、1px neutral border、12–13px 字号和 8px 图标内边距。模式切换压成同一行的 132–144px segmented control；图谱列低于 380px 时再换行。焦点用与危险色分离的 2px identity ring。
- **Suggested command:** `$impeccable layout`

### [P2] 有数据却以双空态开场

- **Why:** `selected` 初始为 null，导致图谱已有数据时右侧约 75% 仍是两个空提示，削弱任务就绪感，也放大了所有“小控件”的视觉失衡。
- **Fix:** 首次加载自动选中 HEAD/第一条提交；文件列表完成后选中首个文件并显示 Diff；按仓库恢复最后一次有效选择。
- **Suggested command:** `$impeccable onboard`

### [P2] 默认列宽与超宽屏/窄屏策略不足

- **Why:** 源码默认图谱 320px、详情 288px，截图中较好的 468/368px 来自持久化列宽；History 没有断点折叠，窄屏时固定列会挤压 Diff。
- **Fix:** 图谱默认 `clamp(360px, 25vw, 480px)`，详情 `clamp(320px, 20vw, 400px)`，Diff 保留至少 640px；空间不足时优先折叠侧栏，再把详情转为可切换面板。
- **Suggested command:** `$impeccable adapt`

## Persona Red Flags

**Alex（专家用户）:** 首屏需点击提交、再点击文件才出现 Diff；快捷键和比较操作不可发现；分隔条没有双击重置或键盘精调。

**Sam（键盘/读屏/低视力用户）:** 11–12px subtle 元数据对比度不足；提交行缺少 option/selected 语义；分隔条鼠标专用；红色焦点与错误/危险状态混淆。

**Jordan（首次使用者）:** Reflog、Message/Content/Regex、`-S/-G` 和比较手势依赖 Git 经验；首屏提示先教 cherry-pick，却没有先完成“选提交 → 选文件 → 看 Diff”的基础引导。

## Minor Observations

- 提交行约 56px，舒适但不算紧凑；可提供 48px compact / 56px comfortable 密度选项。
- Pane header 的 11px uppercase tracked 可以改为 12px sentence case semibold，专业工具感会更自然。
- 56×56px、18px 圆角的空态图标偏消费级，可收至 40–44px、10–12px 圆角。
- PRODUCT.md 仍描述 teal 身份色，而当前实现以朱红作为 accent；这是设计系统文档与代码的语义漂移。

## Questions to Consider

- History 已知 HEAD 时，为什么不直接展示最新提交和第一个 Diff？
- 焦点、选中、主泳道与危险动作都使用红色，用户还能否快速区分状态意义？
- 搜索的三个模式是否值得永久占一整行，还是同一行的 scope control 更符合专业工具密度？
