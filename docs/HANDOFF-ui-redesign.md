# 交接:UI 重构 · 液态玻璃(feat/ui-redesign 分支)

> 随仓库走的交接文档。换机器(如到公司)拉本分支后,先读这个再动手。
> 总进度/铁律见 `docs/HANDOFF.md`;本文件只管 **UI 液态玻璃重构** 这条线。
> 最近更新:2026-06-15。分支 `feat/ui-redesign`(**未合 main**)。
> 配套 spec/plan:`docs/superpowers/specs/2026-06-14-ui-redesign-liquid-glass-design.md`、
> `docs/superpowers/plans/2026-06-14-ui-redesign-liquid-glass*.md`。

## 审美方向(硬约束)
- 玻璃拟态 + Xcode/Tower 原生精致 + GitKraken 鲜亮,三者混。
- **硬禁忌:绝对避开「AI 模板式深紫渐变」。** 强调色走青绿(深岩板 + 青绿身份),不要紫调。
- 颜色/字体只用 `app/src/index.css` 的 `@theme` token(`bg-canvas`/`text-fg`/`border-line`/
  `text-accent`/`text-success`…),**禁硬编码 hex**。图标用 `app/src/components/icons.tsx` 内联 SVG。

## 平台事实(已查权威源,别再猜)
- 本项目跑在 **WebView2 = Chromium**(开发机实测 Chromium 149)。`backdrop-filter` 含 SVG
  `url()` 折射(`feDisplacementMap`)**完全支持**。
- 反而是 **Safari/WebKit 不支持** backdrop-filter 里的 SVG url()(WebKit bug #245510,开了 4 年)。
  → `lib/platform.ts` / `lib/transparency.ts` 的「chromium→refract、非 chromium→blur」判定**方向是对的**。
- 玻璃档位三档由 `<html data-glass>` 控制:`refract`(Chromium,SVG 折射)/ `blur`(强磨砂回退)/
  `solid`(无障碍实底)。`#lgWarp` SVG 滤镜在 `main.tsx` 的 `<GlassFilter>`。

## 玻璃为什么之前「看不出来」(已定性,已解决)
玻璃**贴边**(顶栏/侧栏)时背后只有纯色 `bg-canvas` → 模糊/折射一块纯色 = 还是纯色 = 不可见。
**液态玻璃必须浮在「有内容」之上**:要么是命令面板/OpLog 这种真浮层(已生效),要么让滚动内容从
玻璃栏**底下穿过**(下面的「满汉」方案)。

## ⚠️ 关键坑(踩过,务必记住)
`index.css` 里 `.glass { position: relative }` 是 **unlayered** 规则,会**压过** Tailwind 的
`absolute`(它在 `@layer utilities` 里,优先级更低)。所以**直接写 `<Glass className="absolute …">`
不生效** —— 玻璃栏会被当成 relative 留在文档流里,被 flex-1 滚动体顶到容器**底部**。
**正确写法:定位放在外层普通 div,Glass 放里面:**
```tsx
<div className="absolute inset-x-0 top-0 z-10">
  <Glass>…工具栏…</Glass>
</div>
```
同理,想改玻璃边框/背景也压不过 `.glass`,得直接改 `.glass` 或用更高特异性/inline style。

## 「满汉」方案(浮动玻璃工具栏 + 内容从下穿过)—— 已成立
每个主视图:把顶部工具栏做成**浮动玻璃栏**(absolute,外层 div 定位),滚动体铺满整列、顶部留出
栏高的 padding,内容往上滚时从玻璃栏**底下穿过**显折射。

实测体感:**滚到顶、未滚动时背后无内容 → 偏平;一滚动才「亮」起来**(iOS 式正确行为)。

## 已完成(均 tsc + build 过;历史 & 追溯 **已真机确认**「可以,看到了」)
- **P0 换肤 / P1 玻璃引擎 / P2a Shell**:见 spec/plan + 早期提交。
- **OpLogPanel** 接入 `Glass`(真浮层,折射可见)。
- **refract 规则补 `-webkit-backdrop-filter` 前缀**(`index.css`)。
- **满汉 · 历史图谱**(`HistoryView.tsx` 的 `GraphColumn` + `components/CommitGraph.tsx`):
  - `CommitGraph`/`SearchList` 加 `topInset` prop;`CommitGraph` 用 TanStack virtualizer 的
    `paddingStart: topInset` 预留栏高(骨架/空态也加 paddingTop;滚动根加 `h-full`)。
  - `GraphColumn`:列容器 `relative`,滚动体放 `min-h-0 flex-1` 铺满;栏头+搜索+模式包进
    `<div className="absolute inset-x-0 top-0 z-10"><Glass><div ref={barRef}>…</div></Glass></div>`,
    `ResizeObserver` 实测 `barH`(初值 92)→ 喂 `topInset`。
- **满汉 · 追溯(Blame)**(`BlameView.tsx`):同款浮动玻璃工具栏。**没虚拟化**,直接给滚动体
  `style={{ paddingTop: barH }}` 即可(比图谱简单,无需 virtualizer)。

## 下一步(用户意向:「希望更多地方加玻璃」)——按价值排序
1. **CompareView 主列**:同款浮动玻璃工具栏(选择器栏浮起,diff/列表从下穿过)。结构见
   `CompareView.tsx`(root `flex h-full flex-col` + 选择器工具栏 + ComparePanel)。
2. **真正的 app 顶栏浮层**(headline):`App.tsx` 顶栏 `<Glass as="header">` 改 `absolute inset-x-0
   top-0 z-30` 浮起;难点 = 各 view / 侧栏顶部要加 `pt`(≈ 顶栏 44px)避开,否则其工具栏被盖。
   历史图谱那种已自带列内玻璃栏,若再叠 app 顶栏要处理双层 glass 的偏移(列内栏放 `top-11`、
   virtualizer paddingStart = 44 + barH)。**这步改动面大、必须对真机逐版调**,别一把梭。
3. **ChangesView 左列**:工具栏只有「刷新」,内容价值低,可缓;真要做同上。
4. **可选**:整体加强 `.glass` 材质浓度(静止时更明显)—— 注意 `.glass` unlayered,会影响所有玻璃
   (命令面板/OpLog/各栏),要对真机看整体不发腻。或让首行/首条稍探进栏下(静止也有内容折射)。

## 验证命令
- 前端(在 `app/` 下):`npx tsc -p tsconfig.json --noEmit`、`npm run build`
- 真机:`cd app && npm run tauri dev` → 打开仓库 → 历史/追溯页滚动看玻璃栏折射。
- 包管理用 **pnpm**(`pnpm --dir app …`),别用 npm install(会崩 pnpm 结构的 node_modules)。

## 分支处理
- 用户倾向**保留分支继续**,真机验收 OK 再合 main。
- push 由用户发话(铁律)。换机器前**务必先 push 本分支**,公司机器再 pull 接着干。
