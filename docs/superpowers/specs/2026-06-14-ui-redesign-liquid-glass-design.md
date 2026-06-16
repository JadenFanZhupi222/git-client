# UI 大重构 · 设计 spec(布局重塑 + 液态玻璃视觉)

> 日期:2026-06-14
> 目标:把已经功能完备的 git 客户端,从「GitHub 风顶部 Tab」升级到一款达到世界级水准、有独特视觉个性的桌面应用。
> 前提:后端(crates + 命令)零改动;这是一次**纯前端 + Tauri 窗口配置**的重构。

---

## 1. 目标与范围

### 1.1 目标
- **重塑布局**:用常驻左侧栏当骨架,替代现在的「顶栏一排 Tab」,信息密度和导航完整度对齐 Tower / Fork。
- **重塑视觉**:一套非紫的独特调色板 + iOS 26 「液态玻璃」材质,甩掉「AI 模板感」。
- 保持现有全部功能可用,不丢交互(行级暂存、图谱右键菜单、命令面板、明暗主题等)。

### 1.2 明确不做(YAGNI / 后续)
- 后端 crate、IPC 命令、DTO **一律不动**。
- 不做 WebGL/shader 真折射(成本/性能/维护最大);折射走 SVG 库 + 跨平台回退。
- OS 级窗口玻璃(Win11 Mica/Acrylic、macOS vibrancy)列为**可选 Phase 2**,不在本次必做范围。
- 不重写图谱 lane 算法、diff 解析等纯逻辑——只换它们的"皮"。

### 1.3 成功标准
- 真机(Windows/WebView2)上:外壳浮层有可见的液态玻璃折射 + 镜面边;Diff/图谱清晰可读不糊。
- mac/Linux 上自动退化为高质量 blur 玻璃,不破版、不报错。
- `prefers-reduced-transparency` 或应用内「降低透明度」开关打开时,玻璃退化为近实底。
- `npx tsc --noEmit` 干净、`npm run build` 通过、现有 vitest 全绿。
- 颜色/字体仍只来自设计 token,组件内禁硬编码 hex(沿用现有铁律)。

---

## 2. 设计原则(这是「世界级」与「乱糊玻璃」的分水岭)

1. **玻璃只上"外壳",不上"密集内容"。** 顶栏、左侧栏、命令面板、右键菜单、Toast、浮动弹层、对话框 → 液态玻璃;Diff 正文、图谱行、文件列表 → 近实底,可读性优先。(苹果自身做法。)
2. **非紫身份。** UI 强调色绝不用紫色(避开 AI 模板观感)。图谱泳道是数据可视化色,可含一个品红/紫作为 8 色之一,但 UI 主色恒为青绿。
3. **折射要克制。** 位移 scale 取低值(~30),只在边缘可感;过强会让背后内容扭曲到不专业。
4. **可达性兜底。** 尊重 `prefers-reduced-transparency` / `prefers-reduced-motion`;玻璃和动效都有降级路径。
5. **性能纪律。** 液态玻璃(尤其 SVG 折射)只贴少量浮层;长列表(图谱/diff 虚拟化)区域不上玻璃滤镜,避免滚动掉帧。

---

## 3. 视觉语言(设计 token)

所有 token 进 `app/src/index.css` 的 `@theme`(浅色默认)+ `:root[data-theme="dark"]`(暗色覆盖),沿用现状机制。**暗色是这次的主角屏**(玻璃在暗底最出彩),浅色同步提供。

### 3.1 主身份:深岩板 + 青绿(Deep Slate + Teal)

**暗色(主角):**
| token | 值 | 用途 |
|---|---|---|
| `--color-canvas` | `#0a0f18` | 应用最底 |
| `--color-elevated` | `#121a28` | 面板/列表实底 |
| `--color-overlay` | `#1a2333` | 选中行/更高层 |
| `--color-line` | `rgba(120,140,170,.14)` | 发丝分隔线 |
| `--color-line-strong` | `rgba(120,140,170,.28)` | 强描边/滚动条 |
| `--color-fg` | `#e6edf6` | 主文字 |
| `--color-fg-muted` | `#8b9bb0` | 次要文字 |
| `--color-fg-subtle` | `#5f6e85` | 占位/禁用 |
| `--color-accent` | `#2dd4bf` | 青绿:链接/焦点/当前提交/主按钮 |
| `--color-accent-emphasis` | `#14b8a6` | 强调边/hover |
| `--color-success` | `#34c78c` | 新增/成功 |
| `--color-danger` | `#f46e6e` | 删除/错误 |
| `--color-warning` | `#f0883e` | 重命名/警告(暖橙,非黄) |
| `--color-done` | `#16b3a3` | 提交按钮(青绿实心) |

**浅色:** 同色相、为白底加深处理——`canvas #ffffff`、`elevated #f3f6f9`、`overlay #e7eef2`、`accent #0d9488`(青绿加深保证 AA 对比)、`success #0f9d6b`、`danger #d64545`、`warning #c2691c`、`fg #16202c`。具体值实现时按 AA 对比微调。

### 3.2 备用主题:暖石墨 + 琥珀(可选)
留一套 `:root[data-theme="amber-dark"]` 备用覆盖块(canvas `#15130f`、accent `#f5b54b`、success `#46be82`…),先实现但不设为默认。主题切换器后续可暴露第三档。

### 3.3 图谱泳道(数据可视化色,与 UI token 分离)
暗色泳道沿用现有 8 色结构但与青绿身份协调,鲜亮多色给图谱活力:
`--lane-0 #2dd4bf`(青)、`#5aa9ff`(蓝)、`#f0883e`(橙)、`#bf3989`(品红)、`#a371f7`(紫·仅作 8 色之一)、`#f46e6e`(红)、`#34c78c`(翠)、`#e3b341`(琥珀)。浅色泳道为白底加深。

### 3.4 字体 / 圆角 / 阴影
- 字体沿用现有 `--font-sans` / `--font-mono`。
- 新增圆角阶梯 token:`--radius-sm 6px` / `--radius-md 10px` / `--radius-lg 16px` / `--radius-pill 999px`。
- 新增高度阴影 token:`--shadow-glass`(玻璃浮层用,含外阴影 + inset 顶高光)。

---

## 4. 液态玻璃材质实现

### 4.1 依赖策略
- **Vendor 进项目**(抄文件,非 npm 依赖):把 `rizroze/liquid-glass`(MIT)核心文件放 `app/src/lib/liquidGlass/`。理由:零运行时依赖、完全可控、符合工程口味。在文件头注明来源与 MIT 许可。
- 外包一层我们自己的 React hook **`useLiquidGlass(ref, opts)`**(`app/src/lib/useLiquidGlass.ts`):生成/管理 SVG 折射滤镜实例,绑定到 ref 元素,组件卸载时清理(revoke/remove filter)。

### 4.2 材质规格(三层叠加)
1. **折射底**:`backdrop-filter: url(#lgWarp) blur(3px) saturate(165%)`,`feDisplacementMap` scale ≈ 30。
2. **表面**:`background: linear-gradient(135deg, rgba(255,255,255,.14), rgba(255,255,255,.03))` + `border: 1px rgba(255,255,255,.16)`。
3. **镜面/色散边**:`::before` 用 mask 描边渐变(青→蓝),`box-shadow` 含 `inset 0 1px 0 rgba(255,255,255,.45)` 顶高光 + 外投影。

### 4.3 应用边界(只贴这些)
顶栏药丸工具栏 / 左侧栏 / 命令面板 / 右键菜单(`CommitContextMenu`、各下拉菜单)/ Toast / 模态对话框(`ConfirmDialog`、`RebaseEditor` 等弹层)/ 各 `*Panel` overlay(OpLog/Reflog/FileHistory/LineHistory)。

**禁止贴**:`DiffView` 内容区、`CommitGraph` 行、文件列表行、虚拟化滚动容器内部。

### 4.4 跨平台与降级(库已内建,我们只接线)
- Chromium(Windows WebView2):真 `feDisplacementMap` 折射 + 色散。
- WebKit(macOS WKWebView)/ Linux WebKitGTK:库**自动**退 `backdrop-filter: blur(12px)`。
- `prefers-reduced-transparency: reduce` **或** 应用内「降低透明度」开关(存 localStorage,仿 `theme.ts`):玻璃类切换为近实底(`--color-elevated` 实色 + 普通边框),关掉 backdrop-filter。
- `prefers-reduced-motion`:关掉玻璃高光流动 / morph 动效。

### 4.5 性能
- 折射滤镜实例数量受控(每类浮层一个共享 filter id,不每元素新建)。
- 浮层出现时才挂滤镜;长列表区域零玻璃。

---

## 5. 布局 / 信息架构(App Shell 重塑)

### 5.1 总体骨架
```
┌──────────────────────────────────────────────┐
│  顶栏(玻璃药丸):⎇分支 ↑↓  ⟳Fetch ↧Pull ↥Push  ⌘K  主题 │
├────────────┬─────────────────────────────────┤
│ 左侧栏(玻璃)│            主工作区(随选择变形)             │
│ ▾ 仓库切换器 │                                          │
│ 工作区      │   历史 → 三栏:图谱 | 提交详情+Diff           │
│  更改 / 历史 │   更改 → 两栏:文件列表 | Diff               │
│ 本地分支    │   比较/追溯/子模块/工作树/稀疏 → 对应内容        │
│ 远程        │                                          │
│ 标签 · 储藏  │                                          │
├────────────┴─────────────────────────────────┤
│  底栏:⎇分支 · ahead/behind · 仓库路径                    │
└──────────────────────────────────────────────┘
```

### 5.2 左侧栏(新增核心组件 `Sidebar`)
- **仓库切换器**(顶部):当前仓库名 + 下拉切换/打开(替代现在顶栏「选择仓库」按钮的主入口)。
- **工作区段**:更改(带未提交数角标)、历史。点击切换主工作区(替代现 `TabBar` 的 changes/history)。
- **本地分支段**:可折叠;每条带 head 标记 + ahead/behind 角标;右键复用现有分支操作;双击/点击切换(接 `checkoutBranch`)。
- **远程段**:可折叠,展开列远程分支。
- **标签 · 储藏段**:可折叠,接现有 `TagManager` / `StashMenu` 能力。
- **动态段**:子模块/工作树/稀疏检出按现有 `has*` 逻辑**按需出现**(沿用 HANDOFF 记的「动态标签套路」,改成动态段)。
- **可折叠成窄图标条**:存 localStorage;窄态只留图标 + tooltip。

### 5.3 主工作区(各视图重排到 shell 内)
- **历史(三栏)**:左侧栏 | 中=`CommitGraph`+搜索栏(沿用信息/内容/正则三态) | 右=提交详情(`CommitDetail`)+ 文件列表(`CommitFileList`)+ `DiffView`。这是主角屏,图谱唱主角。
- **更改(两栏)**:左侧栏 | 中=暂存/未暂存文件列表 | 右=`DiffView`(行级/hunk 暂存照旧)。
- **比较 / 追溯 / 子模块 / 工作树 / 稀疏检出**:左侧栏不动,中右区换成现有对应 view 的内容。
- 中/右栏之间、侧栏与主区之间用现有 `Resizer` 可拖拽调宽(宽度存 localStorage)。

### 5.4 顶栏与底栏
- **顶栏**变玻璃药丸条:左=当前分支 + ahead/behind 角标;右=Fetch/Pull(带模式下拉)/Push、命令面板入口(⌘K)、主题切换、远程选择器、撤销/重做、操作日志入口。逻辑全部从现 `App.tsx` 顶栏平移,只换外观与容器。
- **底栏**保留(`BranchSwitcher` + `SyncBadge` + 路径),微调为玻璃质感。

### 5.5 路由/状态
- 现有 `tab` 状态从 `TabBar` 迁到 `Sidebar` 选中态;`App.tsx` 的视图分发 switch 保留,数据源(TanStack Query hooks)全部不变。
- `CommandPalette` 的「切换视图」命令改成切换侧栏选中段,逻辑等价。

---

## 6. 组件落点(映射到现有文件)

| 现状 | 重构动作 |
|---|---|
| `index.css` `@theme` | 改写为深岩板+青绿 token(明暗两套)+ 新增 radius/shadow token + amber 备用主题 + reduce-transparency 规则 |
| `App.tsx`(顶栏+TabBar+视图+底栏) | 拆成 shell:`<Sidebar>` + `<TopBar>` + 主区分发 + `<StatusBar>`;业务逻辑(fetch/pull/push/undo…)原样保留 |
| `components/TabBar.tsx` | 退役,能力并入新 `Sidebar`(或保留为窄屏兜底,待定) |
| 新增 `components/Sidebar.tsx` | 左侧栏骨架(段 + 折叠 + 仓库切换器) |
| 新增 `components/TopBar.tsx` | 顶栏药丸玻璃条(从 App.tsx 抽出) |
| 新增 `lib/liquidGlass/`(vendored MIT) + `lib/useLiquidGlass.ts` | 玻璃材质引擎 + hook |
| 新增 `components/ui/Glass.tsx` | 玻璃容器组件(包 useLiquidGlass + 降级),浮层统一用它 |
| `lib/theme.ts` | 增加「降低透明度」偏好读写(仿 theme 存取) |
| 各菜单/弹层组件 | 容器换成 `<Glass>`;内部内容不变 |
| `DiffView` / `CommitGraph` / 文件列表 | **不动结构**,仅跟随 token 变色;确保不被玻璃波及 |

迁移策略:**token 先行**(先换调色板,全 app 自动变色,验证可读性)→ 再做 shell 骨架(Sidebar/TopBar)→ 再逐个浮层套 `<Glass>`。每步可独立编译/真机看,符合竖切纪律。

---

## 7. 可达性 / 跨平台

- 键盘:沿用现有 `listNav` / `useModalListNav`;侧栏段可键盘展开/折叠 + 上下移动。
- 焦点可见性:`:focus-visible` 在玻璃上仍清晰(青绿 outline)。
- 对比度:玻璃浮层上的文字保证 AA(必要时加半透明暗垫层)。
- `prefers-reduced-transparency` / `prefers-reduced-motion` 全程尊重。
- WebKit 平台回退路径必测(至少本机 Windows 验证 Chromium 路径,mac/Linux 路径靠库保证 + 代码审查)。

## 8. 测试 / 验证
- 自动门:`npx tsc --noEmit`、`npm run build`、现有 `vitest`(diffRows/graphGeometry/mergeModel/commands/listNav)保持全绿(本次主要动表现层,逻辑测试不应回归)。
- 新增纯函数(如侧栏段折叠状态、宽度持久化)按需补 vitest。
- 真机验收清单:Windows 折射可见且 60fps、Diff 可读、降透明度开关、明暗切换、备用 amber 主题、侧栏折叠、三栏拖拽。

## 9. 分期落地(建议,具体计划交给 writing-plans)
- **P0 token 换肤**:深岩板+青绿 token 上线,全 app 变色,验证可读性与对比度。
- **P1 玻璃引擎**:vendor 库 + `useLiquidGlass` + `<Glass>` + 降级开关;先套一个浮层(命令面板)验证真折射。
- **P2 Shell 重塑**:`Sidebar` + `TopBar` + 主区三栏/两栏重排;`TabBar` 退役。
- **P3 全面套玻璃**:其余浮层/菜单/Toast/弹层接 `<Glass>`;底栏微调。
- **P4 打磨**:动效(GSAP 微交互,克制)、amber 备用主题、真机验收清单全过。
- **Phase 2(可选,独立 spec)**:OS 级窗口 vibrancy(Win11 Mica/Acrylic、macOS vibrancy)。

## 10. 待定 / 开放问题
- 浅色主题的玻璃观感需真机调(浅底玻璃较难出彩,可能浅色下默认降透明度)。
- `TabBar` 是彻底退役还是保留为极窄屏兜底。
- 顶栏药丸在按钮很多时的横向空间(可能需要分组/溢出收纳)。
- 备用 amber 主题是否进主题切换器(还是先留代码不暴露)。

---

*核心一句话:常驻左侧栏给骨架,历史页三栏让自研图谱唱主角,深岩板+青绿甩掉紫色 AI 模板感,液态玻璃只上外壳 + 跨平台优雅降级——结构、辨识度、质感一次到位,且后端零改动。*
