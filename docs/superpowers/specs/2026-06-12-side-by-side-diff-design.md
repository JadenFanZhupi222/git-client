# M5.2 并排 diff(side-by-side)· 设计

> 里程碑:M5 · 更深的 diff 与历史(第二刀)。
> 目标:在现有 unified DiffView 之外,新增左右双栏(旧 | 新)的并排视图,复用 M5.1 已算好的词级 `emphasis` 段。
> 日期:2026-06-12。

## 背景与现状

- `DiffView`(`app/src/components/DiffView.tsx`)只有 unified 视图:旧/新双列行号 + 整行增删着色,M5.1 已给增删行加了词级 `emphasis`(`Vec<Seg{text,changed}>`)逐段高亮。
- DiffView 被 3 处复用(`ChangesView`、`HistoryView`、`ComparePanel`),自身渲染整个 diff 区,**外部没有共享工具栏**。
- 数据层已完备:`DiffLineDto { kind, old_lineno, new_lineno, content, emphasis }`。**本刀零 Rust 改动**,纯前端。

## 决策

- **视图切换放进 DiffView 内部**(一个极薄 header bar:统一 / 并排),3 处调用全受益,不改调用方。偏好持久化到 `localStorage`(仿 `lib/theme.ts`),跨文件/会话记住。
- **布局:列优先(column-major),逐 hunk**。每个 hunk = 全宽 header + 一个左右两列块(左旧/右新)。左右两列行数配平 → 同序行天然等高对齐;每列各自 `overflow-x-auto` 独立横滚(近 JetBrains 手感),不互相牵扯。
- **配对复用 emphasis,不重算**:hunk 内「一段连续删行」配「一段连续增行」,`del[i] ↔ add[i]`,多出的行对侧留空白单元格。与 M5.1 `annotate_word_level` 的配对口径一致,故 `emphasis` 直接可用。
- **行内容渲染抽成共享组件** `LineContent`,unified 与并排共用,避免逐段高亮逻辑重复。

## 行模型(前端纯函数)

```ts
type SbsCell = { line: DiffLineDto; li: number } | null;  // li = 该行在 hunk.lines 里的原始下标
type SbsRow  = { left: SbsCell; right: SbsCell };
```

`buildSbsRows(lines)`:
1. 顺序遍历。`context` 行 → 一行 `{left, right}` 两侧同内容、各自行号。
2. 否则收一段连续 `del` 进 `dels`、紧接一段连续 `add` 进 `adds`;`max = max(len)`,逐 `k` 产出 `{left: dels[k]??null, right: adds[k]??null}`。
3. 纯删块(无 add)→ 右侧全空;纯增块(无 del)→ 左侧全空;两者天然落入同一规则。

**不变式**:左右两列长度恒等(== rows.length),保证逐行等高对齐。空单元格渲染为同高占位行(微弱底色),区分「无对应行」。

## 渲染

- header bar:两个 token 化按钮(统一 / 并排),`bg-overlay` 细条;仅在真正渲染 hunks 时显示(binary/lfs/too_large/空 不显示)。
- 并排:外层逐 hunk → 全宽 header → `flex`{ 左列 `flex-1 overflow-x-auto border-r`、右列 `flex-1 overflow-x-auto` }。每列内部复用 `min-w-max` + 行 `min-w-full` 撑满最长行的着色(沿用 unified 的横滚对齐技巧)。
- 单元格:`[选择标记?] [行号 gutter] [+/-/空 sign] [LineContent]`,固定行高(`leading-5`)。`add → bg-success/10`、`del → bg-danger/10`、`context` 无底色;空单元格 `bg-overlay/40` 占位。
- `LineContent`:`emphasis` 有值则逐段渲染(changed 段 add→`bg-success/30`、del→`bg-danger/30`,深一档),否则 `content || " "`。颜色只用既有 token,禁硬编码 hex。

## 行级暂存(并排里照常工作)

- 选中集仍是共享 `Set<"${hi}:${li}">`,key 用 `li`(原始下标)。并排里点左(del)/右(add)可选单元格切换;hunk header 的「暂存选中行 (n)」按钮逻辑不变(它本就遍历 `h.lines` 按 `selected.has(hi:li)` 取行),两视图通用。
- `hunkAction`(暂存/取消暂存整块)也在 header,两视图通用。

## 测试

- 本刀纯前端 + 无新增 Rust:保证 `npx tsc --noEmit` 干净、`pnpm/npm run build` 通过。
- 手验:统一 ↔ 并排切换、偏好持久化;增删配对左右对齐、词级高亮在并排里正确;纯增/纯删/不等数量块的空占位;行级暂存在并排里可选可暂存;binary/lfs/too_large 不显示切换条。
- vitest 仍留到 M5 Trustworthy 阶段统一补(与 M5.1 同口径)。

## 竖切顺序

`ipc.ts`(无改动,类型已够)→ DiffView 抽 `LineContent` 共享组件(unified 改用它,行为不变)→ 加视图模式 state + localStorage 持久化 + header 切换条 → `buildSbsRows` + 并排渲染 → `tsc/build` + 手验 → commit。

## 明确不做(YAGNI)

- 左右两列垂直同步滚动 / 折叠相同块 / 块移动检测:超出本刀。
- 字符级粒度、moved-block:同 M5.1,不做。
- 视图模式做成全局设置项面板:localStorage 隐式记忆足够,不开设置 UI。
