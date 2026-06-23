# 交接：Strata —— git 客户端 UI 重设计（Paper & Ink）

> 给 Claude Code 的实现交接。目标仓库就是你本地的 **`git-client`**（Tauri 2.x + React + Tailwind v4 + 多 crate Rust）。
> 本包里的 `Strata.dc.html` 是**设计参考原型（HTML 写的）**——它演示最终的样子与交互，**不是要直接搬进去的生产代码**。
> 任务是：**在现有 `git-client/app` 的技术栈与既有结构里，把这套设计实现出来**——继续用它的 token 体系（`app/src/index.css` 的 `@theme`）、组件、TanStack Query、虚拟化等，而不是引入新框架。

---

## 1. 这是什么 / 改了什么

这是对现有 git 客户端的一次**整体视觉与排版重设计**，方向代号 **Paper & Ink**：

- **彻底告别原来的"青绿开发者风"**，换成**明亮的编辑感**：纸白画布、近黑墨色、一点**朱红（vermillion）**作唯一强调色。
- 暗色不是冷蓝，而是同族的 **"Ink"** 反相：暖墨黑底 + 纸白字 + 更亮的朱红。
- **保留并强化液态玻璃**（仅用于浮动工具栏/浮层），材质改为"磨砂羊皮纸"质感。
- **三声部排版**：`Instrument Serif`（编辑性大字时刻）+ `Geist`（UI）+ `Geist Mono`（数据）；中文配 `Noto Serif SC` / `Noto Sans SC`。
- 新增**中 / English 语言切换**。
- 几处"杂志级"版式细节：编辑性启动屏、追溯页"页边批注"、比较页统计头、提交详情衬线标题。

保真度：**高保真（hi-fi）**。颜色/字号/间距/交互都已确定，按 `Strata.dc.html` 像素级复刻，但用仓库现有的 Tailwind 工具类 + 组件实现。

---

## 2. 落地到现有代码的总体策略（重要）

现有仓库已经是 **token 驱动**（`app/src/index.css` 里 `@theme` + `:root[data-theme="dark"]`），这是最大优势：**大部分换肤改一处 token 即可全局生效**。建议顺序：

1. **换 token（`app/src/index.css`）**：把下面第 4 节的 Paper & Ink 值替换进 `@theme`（浅色）与 `:root[data-theme="dark"]`（暗色），并替换泳道色 `--lane-0..7`、`--add-bg/--del-bg`。**保持变量名不变**，组件无需改引用。
2. **加字体**：见第 5 节。新增 `Instrument Serif` + `Noto Serif SC` + `Noto Sans SC`，并在 `index.css` 暴露一个 `.serif` / `font-serif` 工具。
3. **玻璃材质**：把现有 `.glass` 的材质调成"磨砂羊皮纸"（第 6 节）。
4. **排版升级**：在指定位置（启动屏标题、提交详情标题、各视图标题、blame 文件名/说明）应用 `Instrument Serif`。
5. **版式细节**：按第 7 节逐视图实现（启动屏、blame 页边批注、compare 统计头等）。
6. **i18n**：按第 8 节加语言切换。
7. **强调色语义**：原来 accent=青绿，现在 accent=朱红。注意**朱红同时是"删除/危险"的家族色**——diff 的删除行、danger 用更"砖红"的 `--danger`，与作为 UI 强调的 `--accent` 区分开（值见第 4 节），避免"提交按钮"和"删除"撞色。

> 不要把密集数据区（diff / 图谱 / 文件列表）塞进玻璃容器——玻璃只用于外壳浮层。

---

## 3. 需要改动的现有文件（参照）

| 文件 | 改动 |
|---|---|
| `app/src/index.css` | 替换 `@theme` / 暗色 token、泳道色、`.glass` 材质；加 `@font-face` 与 `.serif`；加 Noto 的 `<link>`（或放 `index.html`） |
| `app/index.html` | 加 Google Fonts `<link>`（Noto Serif/Sans SC）与 fontsource 字体 |
| `app/src/App.tsx` | 顶栏加**语言切换**按钮（主题键旁）；启动屏 `EmptyState` 改成编辑性版式；标题用 serif；接 i18n |
| `app/src/components/CommitDetail.tsx` | 提交标题（subject）改 `Instrument Serif` 大字 |
| `app/src/views/BlameView.tsx` + `app/src/components/*Blame*` | 改"页边批注"版式：年龄热度脊、按提交分组隔行+分隔线、顶部衬线文件名 + 作者说明句 |
| `app/src/views/CompareView.tsx` + `ComparePanel.tsx` | 左列加编辑性统计头（大号衬线 `+170 / −32`），ref 标签 从/到 |
| `app/src/views/SubmodulesView.tsx`/`WorktreesView.tsx`/`SparseCheckoutView.tsx` | 标题 serif、表格观感对齐新 token |
| `app/src/components/CommandPalette.tsx` | 文案接 i18n；加"切换语言"命令 |
| `app/src/components/Sidebar.tsx` | 激活项/脊用 accent（朱红）；标签接 i18n |
| `app/src/lib/theme.ts` | 主题逻辑不变；可在此或新建 `lib/i18n.ts` 管语言 |

新建：`app/src/lib/i18n.ts`（语言状态 + 字典 + `t()`）。

---

## 4. 设计 Token（精确值）

### 浅色 "Paper"（`@theme` 默认）
```
--color-canvas:    #f4f1ea   /* 纸白画布 */
--color-elevated:  #fbfaf5   /* 抬升面（面板/按钮底） */
--color-overlay:   #e9e4d8   /* 悬停/凹陷 */
--color-line:      rgba(38,28,18,0.12)
--color-line-strong: rgba(38,28,18,0.20)
--color-fg:        #1a160f   /* 墨黑 */
--color-fg-muted:  #6a6053
--color-fg-subtle: #9a9082
--color-accent:         #d83a22  /* 朱红（UI 强调；白底偏深保 AA） */
--color-accent-emphasis:#bb2f1a
--color-success:   #2f7d4f   /* diff 新增 / 暂存 */
--color-danger:    #bb2f1a   /* diff 删除 / 危险（砖红，区别于 accent） */
--color-warning:   #b5791f   /* 修改 M / tag */
--add-bg: rgba(47,125,79,0.10)
--del-bg: rgba(187,47,26,0.08)
```

### 暗色 "Ink"（`:root[data-theme="dark"]`）
```
--color-canvas:    #16130f   /* 暖墨黑 */
--color-elevated:  #1e1a14
--color-overlay:   #2a241c
--color-line:      rgba(232,214,184,0.13)
--color-line-strong: rgba(232,214,184,0.24)
--color-fg:        #f1ebe0   /* 纸白 */
--color-fg-muted:  #a99e8d
--color-fg-subtle: #6f6556
--color-accent:         #ff6a4d  /* 亮朱红 */
--color-accent-emphasis:#e2452d
--color-success:   #5aa06a
--color-danger:    #ff7a5e
--color-warning:   #e0a94e
--add-bg: rgba(90,160,106,0.13)
--del-bg: rgba(255,122,94,0.12)
```

### 图谱泳道色（数据可视化，与语义 token 分开）
浅色：`--lane-0 #d83a22`（主干=朱红）, `--lane-1 #3f6789`（黛蓝）, `--lane-2 #b5791f`（赭）, `--lane-3 #8a4a6b`（黛紫）。
暗色：`--lane-0 #ff6a4d`, `--lane-1 #6f9fc4`, `--lane-2 #e0a94e`, `--lane-3 #c98aa8`。
（如需 8 道，沿同色相延展更浅/更深各一档。）

### 圆角 / 缓动（沿用现有）
`--radius-sm 6 / -md 10 / -lg 16 / -pill 999`；`--ease-spring cubic-bezier(.32,.72,0,1)`、`--ease-out-quint cubic-bezier(.22,1,.36,1)`。

---

## 5. 字体系统

三套拉丁 + 两套中文：

- **Instrument Serif**（编辑性大字：启动屏标题、提交标题、视图标题、blame 文件名）。仅 400，含 italic。
- **Geist Variable**（UI 正文/按钮，沿用现有）。
- **Geist Mono Variable**（数据：SHA / 行号 / 路径 / 计数 / 时间，沿用现有）。
- **Noto Serif SC**（中文衬线，配 Instrument Serif）。
- **Noto Sans SC**（中文 UI，配 Geist）。

加载（`index.html` 或 `index.css`）：
```html
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Noto+Sans+SC:wght@300;400;500;600&family=Noto+Serif+SC:wght@400;600;700&display=swap">
```
```css
@font-face{ font-family:"Geist Variable"; font-weight:100 900; src:url("https://cdn.jsdelivr.net/npm/@fontsource-variable/geist@5/files/geist-latin-wght-normal.woff2") format("woff2"); }
@font-face{ font-family:"Geist Mono Variable"; font-weight:100 900; src:url("https://cdn.jsdelivr.net/npm/@fontsource-variable/geist-mono@5/files/geist-mono-latin-wght-normal.woff2") format("woff2"); }
@font-face{ font-family:"Instrument Serif"; font-weight:400; font-style:normal; src:url("https://cdn.jsdelivr.net/npm/@fontsource/instrument-serif@5/files/instrument-serif-latin-400-normal.woff2") format("woff2"); }
@font-face{ font-family:"Instrument Serif"; font-weight:400; font-style:italic; src:url("https://cdn.jsdelivr.net/npm/@fontsource/instrument-serif@5/files/instrument-serif-latin-400-italic.woff2") format("woff2"); }
```
> 生产环境建议把字体自托管进 `app/public`，避免依赖 CDN。Tauri 离线可用更重要。

字体栈：
- UI：`"Geist Variable","Noto Sans SC",ui-sans-serif,system-ui,"PingFang SC","Microsoft YaHei",sans-serif`
- 衬线 `.serif`：`"Instrument Serif","Noto Serif SC",ui-serif,Georgia,"Songti SC",serif`
- Mono：`"Geist Mono Variable",ui-monospace,monospace`

**衬线只用于"编辑性时刻"**：启动屏 "Strata" 巨字（~108px）+ 斜体朱红副题（~31px）；提交详情 subject（~25px / 400）；各视图标题（子模块/工作树等 ~27px）；blame 顶部文件名（~24px）。其余一律 Geist。

---

## 6. 液态玻璃材质（磨砂羊皮纸）

只用于**浮动工具栏 / 浮层**（顶栏、各视图浮动玻璃工具栏、命令面板、菜单），内容从其下方滚过显折射。`.glass` 改成：

- 浅色：`background: linear-gradient(135deg, rgba(255,253,248,.74), rgba(250,247,240,.52))`；`border:1px solid rgba(255,255,255,.85)`；`backdrop-filter: blur(22px) saturate(185%)`；柔和暖投影 `0 1px 2px rgba(60,44,28,.05), 0 22px 50px -18px rgba(60,44,28,.22)`。
- 暗色：`background: linear-gradient(135deg, rgba(60,50,38,.55), rgba(28,22,16,.36))`；`border:1px solid rgba(230,210,180,.18)`；投影 `0 10px 34px rgba(0,0,0,.42)`。
- **招牌高光发丝边**用 `::before`（`padding:1px` + `mask` 抠出边）：`linear-gradient(135deg, rgba(255,255,255,.9), transparent 42%, transparent 64%, rgba(255,255,255,.55))`。
- 现有仓库已有 `data-glass` 三档（refract/blur/solid）与平台判定，**保留**；这里只换材质数值。

> 重要坑（仓库 `HANDOFF-ui-redesign.md` 已记）：`index.css` 里 `.glass{position:relative}` 是 unlayered 规则，会压过 Tailwind 的 `absolute`。浮动玻璃栏要**定位放外层普通 div、`<Glass>` 放里面**。

---

## 7. 逐视图规格（对照 `Strata.dc.html`）

通用骨架：顶栏（浮动玻璃，46px 高）→ 左侧栏（212px，含可移动的 2px 朱红"脊"标记激活项）→ 视图区 → 底部状态栏（24px）。每个主视图顶部是**浮动玻璃工具栏**（absolute，外层 div 定位），滚动体顶部留出栏高 padding，内容从栏下穿过显折射。

### 7.1 启动屏（`EmptyState`）—— 编辑性封面
- 纸面左对齐单栏（`max-width 560`，左 padding `clamp(40px,8vw,128px)`）；背景 `--ambient`（朱红极淡径向 + 暖角）+ 细噪点叠层。
- 顶部一行：玻璃方牌（44px，内含朱红 git-graph 标记）+ 38px 朱红短横 + mono 眉签 `本地优先 · 纯 Rust 内核`。
- `Instrument Serif` 巨字 **Strata**（~108px / 400 / line-height .9）。
- 斜体朱红副题（~31px）：中文 `大仓库也跟手。`／EN `Smooth, even on huge repos.`
- 正文（15px，fg-muted，max 30rem）：中文 `纯 Rust 内核，十几万次提交滚动也不卡。所有操作都在本地，代码不离开这台电脑。`／EN `A pure-Rust core keeps scrolling smooth across hundreds of thousands of commits. Everything runs locally — your code never leaves this machine.`
- CTA：朱红药丸 `选择仓库/Open repository`（内嵌圆形箭头）+ 描边 `克隆/Clone`、`新建/New`；下方分隔线后 `继续上次/Resume strata-git →` 与 `⌘K 命令面板` 提示。
- 右侧：缓行的墨色/朱红提交图谱（书脊母题，drift 26s 无缝循环）+ 一条竖直渐隐分隔线。
- 入场：各元素 `hero-rise`（上浮 + 去模糊，stagger 延迟），尊重 `prefers-reduced-motion`。
- 右上角放**语言切换**按钮（圆药丸，地球图标 + `EN`/`中`）。

### 7.2 顶栏（`App.tsx` header）
左：主页键（朱红 git-graph 小标）+ mono 仓库名 + `/` + 分支切换器 + 同步角标（↑2 用 success/accent）。右：撤销/重做托盘、Fetch·Pull·Push 托盘（Push 可推时 success 角标）、⌘K 入口、**语言键（EN/中）**、主题键。全部浮动玻璃。

### 7.3 历史（hero）
- 左列浮动玻璃工具栏：搜索框占位 `搜索提交 · 作者 · SHA` / `Search commits · author · SHA`、`图谱/列表` 分段、`13 提交` 计数。
- **活的图谱**：垂直泳道（朱红主干 + 黛蓝/赭/黛紫分支），合并节点=空心环（`fill: canvas, stroke: lane`），普通=实心，**HEAD 加外环**，选中节点 `drop-shadow` 光晕。曲线用 `M x1 y1 C x1 y1+28, x2 y2-28, x2 y2`。行高 56。选中行：左侧朱红"脊" + accent 9% 淡底。
- 提交行：refs 徽章（head=accent / remote=fg-muted / tag=warning）+ 消息（选中加粗）+ 作者（mono）+ 时间（右对齐 mono）+ 短 SHA（选中 accent）。
- 右：提交详情面板（388px）。**subject 用 `Instrument Serif` ~25px/400**；sha 徽章 + 复制；作者头像（首字母方块，accent 底）+ `who@strata.dev · 时间` + `已验签/Verified`（success）；`变更文件/Changed files` 列表（M/A/D 状态色块 + 路径 + +/−）；底部统一 diff（add=success/`--add-bg`，del=danger/`--del-bg`，hunk 头=accent）。

### 7.4 更改
左列（352px）浮动工具栏 `工作区更改/Working tree` + 刷新；`未暂存/Unstaged`、`已暂存/Staged` 两段（计数徽章 + `全部暂存/全部取消`）；文件行含暂存复选框（暂存=accent 实心勾）、状态色块、路径、+/−；选中行朱红脊。底部提交盒：textarea（占位 `提交信息 — 概述本次改动…`）+ `修正(amend)` + `2 暂存` + 朱红 `提交到 main/Commit to main`（带勾图标）。右：统一 diff（同详情）。

### 7.5 比较
左列（340px）浮动工具栏：ref 药丸 `从/Base origin/main → 到/Head main` + 交换键。左列顶部**编辑性统计头**：大号 `Instrument Serif` `+170`（success）/ `−32`（danger），下面一句 `main 领先 origin/main 2 个提交 · 4 文件改动` / `main is 2 commits ahead of origin/main · 4 files changed`；其下文件列表（选中朱红脊）。右：统一 diff。

### 7.6 追溯 Blame —— 页边批注
- 浮动工具栏：朱红 blame 图标 + mono 文件路径 + `15 行 · 5 次提交` + 年龄热度图例（`新→旧` / `new→old`，朱红渐变到 fg-subtle）。
- 滚动体顶部**编辑性头**：`Instrument Serif` 文件名 `diffRows.ts` + 说明句 `15 行，5 次提交，跨 6 天 — 主要由 jiang.k 与 m.wei 维护。`（EN：`15 lines across 5 commits over 6 days — mostly by jiang.k and m.wei.`）。
- 每行：左 **gutter（~252px）**=年龄热度脊（3px，首行满色、续行 .3 透明）+ sha（热度色 mono）+ 作者 + 相对时间；右=行号（mono，右对齐，淡）+ 代码（mono）。
- **按提交分组**：每组首行加 `border-top:1px solid line`，隔组淡底 `fg 2.5%`，让 commit 块可读。
- 热度档（age 0→4）：`accent` → `mix(accent 68% fg-subtle)` → `40%` → `20%` → `fg-subtle`。

### 7.7 子模块 / 工作树 / 稀疏检出
居中卡片表格（max 860）：标题 `Instrument Serif` ~27px + mono `N 项/N items`；表头 mono 小号大写；行 hover 淡底；首列与末列 mono。

### 7.8 命令面板（⌘K / Ctrl K）
玻璃面板（560 宽，14vh 顶距，`panel-in` 入场）：搜索框（占位 `输入命令或搜索…/Type a command or search…`）+ `esc`；分组 `视图/Views`、`远程/Remote`、`外观/Appearance`（含主题、**切换语言**、操作日志）。每项左图标 + 标题（+ 可选快捷键 kbd）。全局 `⌘K/Ctrl+K` 开关、`Esc` 关。

---

## 8. 交互与动效

- **入场**：`hero-rise`（启动屏，上浮+去模糊 .7s stagger）、`fade-in`（视图切换 .18s）、`panel-in`/`overlay-in`（浮层）、`menu-in`（下拉）。
- **活的图谱**：新提交 `commit-enter`（朱红微染渐隐）、节点 `node-pop`；hover 泳道提亮；选中节点光晕。
- **物理按压**：所有未禁用按钮 `:active{ transform:scale(.98) }`。
- **脊滑动**：侧栏激活脊 + 列表选中脊用 `transition: top .28s var(--ease-spring)` 平滑移动。
- 全部动效在 `prefers-reduced-motion: reduce` 下降级为淡入/瞬时。

## 9. 状态

`theme: 'light'|'dark'`（沿用 `lib/theme.ts` 与 `<html data-theme>`）、`lang: 'zh'|'en'`（新增，建议持久化到 localStorage + `<html lang>`）、`tab`（当前视图）、各视图局部选中（选中提交/文件/blame 修订）。沿用现有 TanStack Query 取数与文件监听失效。

## 10. 国际化（i18n）

- 新建 `app/src/lib/i18n.ts`：`lang` 状态（localStorage 记忆，默认跟随 `navigator.language` 或 zh）+ 字典 + `t(key)`。原型里用的是"中文为源、EN 查表"的 `tr()`，迁移时建议改为标准 **key→{zh,en}** 字典，便于扩展更多语言。
- 覆盖面：启动屏、导航、顶栏/底栏、命令面板、历史（含演示提交信息/相对时间）、更改、比较、追溯（含作者说明句）、子模块/工作树/稀疏检出（标题/表头/单元格）。
- 入口：启动屏右上角语言键、顶栏主题键旁语言键、命令面板"切换语言"。
- 真实数据（真正的提交信息、作者、路径、SHA）**不翻译**；只翻译 UI 文案与（演示里的）相对时间格式。相对时间生产中应走现有 `lib/time.ts` 的本地化格式。

## 11. 无障碍（沿用 PRODUCT.md 目标）

WCAG AA（正文≥4.5:1；朱红 `--accent` 白底用 `#d83a22` 偏深达标）；`:focus-visible` 朱红描边；浮层 Esc + 焦点陷阱；颜色不作唯一信息载体（同步状态/节点空心实心 + tooltip）；reduce-motion 降级；保留"降低透明度"（玻璃转实底 `data-glass="solid"`）。

---

## 12. 怎么用这个包（给用户）

1. 把本文件夹 `design_handoff_strata_redesign/` 放进你的 **`git-client` 仓库根目录**（和 `app/`、`crates/` 平级）。
2. 在该仓库打开 **Claude Code**，给它类似指令：
   > 阅读 `design_handoff_strata_redesign/README.md` 和里面的 `Strata.dc.html` 原型，把这套 "Paper & Ink" 重设计实现到 `app/src`。先按 README 第 2 节的顺序：① 改 `index.css` 的 token（第 4 节值）② 加字体（第 5 节）③ 玻璃材质（第 6 节）④ 排版升级 ⑤ 逐视图版式（第 7 节）⑥ i18n（第 10 节）。沿用现有 Tailwind token 工具类与组件结构，不要引入新框架。每完成一步用 `npx tsc -p tsconfig.json --noEmit` 与 `npm run build` 自查。
3. 让它**小步来、逐视图对照** `Strata.dc.html`，并在真机 `pnpm --dir app tauri dev` 下逐版验收玻璃折射与版式（仓库铁律：包管理用 pnpm；push 由你发话）。

## 13. 文件清单

- `Strata.dc.html` —— 高保真可交互原型（**唯一视觉真相**；可直接在浏览器打开，点侧栏切视图、右上角切主题/语言、⌘K 开命令面板）。
- `README.md` —— 本文件。
