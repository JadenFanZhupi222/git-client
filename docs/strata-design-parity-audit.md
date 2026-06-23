# Strata「Paper & Ink」实现 vs 原型 · 一致性核对与修复清单

> 核对日期:2026-06-23。
> 原型(唯一视觉真相):`design_handoff_strata_redesign/Strata.dc.html`(配 `support.js`)。
> 设计交接说明:`design_handoff_strata_redesign/README.md`(逐视图规格见其第 7 节)。
> 本文档既是核对结论,也是后续逐步修复的进度跟踪表。每修一项把 `[ ]` 改成 `[x]`。

---

## 0. 结论速览

| 层 | 一致度 | 判断 |
|---|---|---|
| 地基(token / 字体 / 玻璃材质 / 动效) | ~90% | **像素级到位** |
| 启动屏 EmptyState | ~95% | 最忠实,仅 2px 级取整差 |
| 顶栏 header | ~85% | 要素齐,信息架构略有重组 |
| 追溯 Blame | ~80% | 结构忠实,仅像素级偏差 |
| 命令面板 | ~70% | 功能更强,视觉/结构出入较多 |
| 比较 Compare | ~45% | 统计头在,招牌皮肤未上身 |
| 历史 History | ~40% | 图谱几何重写、详情面板招牌细节缺失 |
| 更改 Changes | ~35% | 玻璃栏/填充复选框/左脊/增删数/统计计数多缺 |
| 子模块/工作树/稀疏 | ~25% | 居中表格版式整体未实现 |

**性质**:这不是实现 bug,而是一次**有意的工程化落地**——保留 token 体系、衬线大字、diff 配色、i18n,并叠加了原型没有的真实功能(虚拟化、可拖拽分栏、行级暂存、签名校验、行历史)。但 README 第 1/20 节要求"高保真、像素级复刻",据此标准视图层有**系统性偏离**。

**反复出现的四个招牌缺口**(贯穿多视图):
1. 浮动玻璃工具栏(除 Blame 外几乎都退化为普通 `border-b`)
2. accent 发光左脊选中态(多数视图用 `bg-overlay` + 普通 `border-l-2`)
3. 16×16 stat 色块徽章(多处退化为单字母彩色文字)
4. 7.7 居中表格版式(改走了另一套图标卡片语言)

---

## ✅ 已对齐(地基层,无需改动,仅存档)

- [x] Token 全量:`@theme` 浅/暗色值与 README §4 逐字一致(canvas/fg/accent/success/danger/warning、`--color-add-bg` `rgba(47,125,79,.10)`、`--color-del-bg` `rgba(187,47,26,.08)`)
- [x] 泳道色 0–7(原型给 0–3,实现按同色相延展补 4–7)
- [x] 字体:Geist + Geist Mono 自托管,Instrument Serif 400+italic,Noto SC 走 CDN `<link>`,`.serif` 工具类
- [x] 玻璃材质:磨砂羊皮纸渐变 + `::before` 发丝边 + 浅/暗双套 + 三档 refract/blur/solid
- [x] 动效:hero-rise stagger、panel-in/overlay-in/menu-in、commit-enter/node-pop、物理按压 `:active scale(.98)`
- [x] 启动屏 EmptyState:巨字 108px / 副题 / 正文 / CTA 药丸 / stagger 延迟逐项复刻

> 启动屏待复核(低优先,可选):`LaunchGraph.tsx` 内部 drift 26s / 定位 / opacity .5 / width 300px;短横 36 vs 38px;语言键位置 top/right 2px 取整差。

---

## 修复计划(按性价比排序)

### 步骤 1 — 浮动玻璃工具栏 + accent 发光左脊(贯穿性招牌)✅ 基础完成

> 一次确立改法,复用到历史/更改/比较三视图。原型铁律(见 README §6 末):浮动玻璃栏要"定位放外层普通 div、`<Glass>` 放里面",否则 `.glass{position:relative}` 压过 Tailwind `absolute`。
>
> **范围重组**:为避免重复改动同一视图,更改页 floatBar 并入步骤 2、比较页 floatBar 并入步骤 4、历史图谱行脊并入步骤 3。步骤 1 只交付**共享原语**与两个共享列表组件的脊。

**共享原语**
- [x] 新建 `components/ui/FloatBar.tsx`:浮动磨砂玻璃工具栏(外层定位 + Glass,radius 12 / padding 7·10 / minHeight 40;导出 `FLOAT_BAR_INSET=58`)
- [x] 新建 `components/ui/Spine.tsx`:发光左脊(2px 朱红 + `boxShadow:0 0 8px accent`,顶底各留 7px)

**accent 发光左脊选中态**(`accent 10%` 淡底 + 脊辉光,替换 `bg-overlay`)
- [x] `CommitFileList.tsx`(历史"变更文件"列 + 比较文件列共用)
- [x] 历史搜索结果列表 `SearchList`(替换原 `border-l-2 border-accent-emphasis`)
- [ ] 更改文件行 → 随步骤 2(与填充复选框同处一行,合并改)
- [ ] 比较文件行 → 随步骤 4(与左列 floatBar 重组同改)
- [ ] 历史图谱提交行 → 随步骤 3(由 CommitGraph 渲染)
- [ ] 侧栏 `Sidebar.tsx` 激活脊核对是否用同一套

**浮动玻璃工具栏**
- [x] 历史 `HistoryView.tsx`:已存在(搜索 + 模式 + 计数,多行用 ResizeObserver 实测高度,属功能增强保留)
- [ ] 更改 `ChangesView.tsx` → 随步骤 2
- [ ] 比较 `CompareView.tsx` → 随步骤 4

### 步骤 2 — 更改页填充复选框 + 提交盒内盒结构 ✅ 完成

- [x] 浮动玻璃工具栏(`FloatBar`:工作区更改 + 刷新);滚动体 `FLOAT_BAR_INSET` 留白
- [x] 文件行 + 冲突行加发光左脊(`Spine` + `bg-accent/10`)
- [x] 暂存复选框:accent 实心勾(18×18 描边,staged→accent 底 + 白勾),替换 hover +/− 图标
- [x] 未暂存/已暂存两段顺序对齐原型(冲突 → 未暂存 → 已暂存,含键盘 flatList);label `fg-subtle`
- [x] "全部暂存/全部取消"常驻 accent(去掉 hover 才现)
- [x] stat 色块化(`StatBadge` 16×16 圆角 color-mix;M=warning、R=accent)
- [x] 提交盒:textarea + footer 统一内盒(borderTop);"N 暂存"计数;提交按钮 `提交到 {branch}`
- [ ] ~~文件行补 `+add / −del` 增删数~~ —— **跳过**:`FileEntryDto` 只有 `{path,state,staged}`,无行计数;补齐需后端每文件 diff-stat,留作独立增强(非视觉回归)

### 步骤 3 — 历史图谱节点细节 + 提交详情面板 ✅ 完成

**图谱几何与节点**(`CommitGraph.tsx` / `graphGeometry.ts`)
- [x] 行高 56(ROW_H 48→56);LANE_W 16→22(更宽泳道,曲线控制点随 MID 自动到 14=半段)
- [x] 节点分级:选中 r=6 / 普通 r=5 / 合并 r=4.5
- [x] HEAD 加外环(r+3 淡环,stroke lane / opacity .5)
- [x] 选中节点 `drop-shadow(0 0 6px lane)` 辉光
- [x] 合并节点空心环 `fill:canvas, stroke:lane`;同步状态语义移交行首竖色条(不再占用节点形态)
- [x] lane hover 暗化 opacity 0.6(原 0.2);非高亮线宽 1.8
- [x] refs 徽章:`rounded-[5px]`;head/local 用 accent(原 success);tag 用 tag-svg(替换 `⌖`);remote 中性

**提交行版式**(`CommitLines.tsx` / `HistoryView.tsx`)
- [x] 拆出右对齐独立列:time(62px)/ short SHA(54px,选中 accent);消息列 summary(选中加粗,合并 fg-muted)+ 作者 mono

**提交详情面板**(`CommitDetail.tsx`)
- [x] 顶部 sha 徽章 + 复制按钮(navigator.clipboard,复制后短暂显勾)
- [x] 作者首字母方块 avatar(accent/16 底)+ `邮箱 · 时间` 排版
- [x] "变更文件"区补 `+adds −dels` 汇总(History MidColumn 头,真实 diff-stat)
- [x] stat 色块化(`CommitFileList` 16×16 color-mix;M=warning、R=accent)
- [x] subject serif 25px/400(已到位)
- [ ] (可选)宽度 388 与独立右栏 —— 当前是中列上半区可拖拽 288,属功能增强,**保留,不强制改**

### 步骤 4 — 比较页 ref 药丸 + 统计头微调 ✅ 完成

- [x] ref 药丸:眉签(从/到)+ mono 值 + chevron 的药丸样式;**内核仍是原生 `<select>`**(保留 WebKit 防双触发可靠性)
- [x] swap 交换按钮
- [x] 浮动玻璃工具栏 + 统计头**移入左列**(`ComparePanel` 新增 `toolbar`/`statHead` 槽;历史内联比较不传,保持朴素布局)
- [x] 统计头字号 30px;位置移入左列滚动体顶部
- [x] 左列宽默认 340(有 toolbar 时)
- [ ] ~~说明句补"领先 N 个提交"~~ —— **跳过**:需 from/to 的 ahead 计数(额外查询),现有 API 无;保留"{to} vs {from} · N 文件改动"

### 步骤 5 — 7.7 子模块/工作树/稀疏 居中表格版式重写 ✅ 完成

- [x] 新建 `components/ui/CardTable.tsx`(`SecondaryHeader` + `CardTable` + `CardRow` + `Cell`)
- [x] 三视图改"居中卡片表格"(max 860):列头 mono 小号大写、`flex 2 / flex 1` 单元
- [x] 34×34 accent 图标瓦片(SecondaryHeader)
- [x] 标题 `<h2>` line-height 1.1;"N 项"移到标题下方 `<p>`(mono)
- [x] Worktrees 首列路径用 mono(Cell first);当前工作树行加极淡 accent 底
- [x] Sparse 恢复"类型"列(由 `!` 前缀派生 include/exclude)
- [ ] ~~Sparse "匹配"列(文件数)~~ —— **跳过**:后端只回 pattern 字符串,无每模式匹配计数
- 子模块保留 init/update 动作(CardRow `trailing` 槽,不破表格对齐)

### 步骤 6 — 命令面板小修(`CommandPalette.tsx`)

- [ ] 顶距 `14vh`(现 `mt-[12vh]`)
- [ ] 宽度 `560px`(现 `34rem` = 544px)
- [ ] 命令行左图标(README §7.8「每项左图标 + 标题(+ 可选快捷键 kbd)」;现无左图标、右侧是分组 chip)
- [ ] 遮罩色 `#05080d / 55%`(现 `bg-black/40`)
- [ ] (可选)"操作日志"归回"外观"组(现独立 panel 组)

### 步骤 7 — 顶栏信息架构(低优先,需先与用户确认是否改)

> 实现把主题/玻璃/操作日志/远程收进溢出菜单(MoreMenu),顶栏多了"选择仓库"键 —— 这是为顶栏减负的有意设计。是否回退到原型的"主题键一级独立"待定。

- [ ] (待确认)主题键提为顶栏一级独立按钮
- [ ] (待确认)主页键(BranchMark)可点击回启动屏 + 尺寸 24px(现 20px 不可点)
- [ ] 顶栏高度 46px(现 44px `h-11`)

### 收尾 — token 纪律

- [ ] 排查硬编码绕过 token 处(如 del 行用 `bg-danger/10` 而非 `--color-del-bg` 8%),收敛回 token

---

## 决策备注

- 多处"功能增强"(可拖拽分栏宽、行级暂存、签名校验、分支跳转子模式、真实禁用态)**优于原型静态 demo,一律保留**,不为像素复刻而砍功能。
- 与原型冲突且为纯视觉的项才按原型改。功能性增强与原型版式冲突时,以"保功能、补皮肤"为原则。
