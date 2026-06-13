# UI 大重构(P0 token 换肤 + P1 玻璃引擎)实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 app 换成「深岩板 + 青绿」身份,并搭好一套可复用的液态玻璃引擎(真折射 + 跨平台降级 + 降透明度兜底),用命令面板做第一个真集成验证。

**Architecture:** 纯前端,后端零改动。先改 `index.css` 设计 token 让全 app 自动变色(P0);再 vendor 一个 MIT 折射库 + 自写 `useLiquidGlass` hook + `<Glass>` 容器,只贴外壳浮层,Diff/图谱保持实底(P1)。降级靠「降低透明度」偏好 + `prefers-reduced-transparency`。

**Tech Stack:** React 19、Tailwind v4(`@theme` CSS 变量)、Vitest、TanStack Query(不动)、Tauri 2 / WebView2。

**Scope:** 本计划只覆盖 spec 的 P0 + P1(spec 第 9 节)。**P2 Shell 重塑 / P3 全面套玻璃 / P4 打磨 / Phase 2 OS 窗口玻璃** 待 P0+P1 真机验收后另写计划(它们依赖玻璃引擎落地后的手感)。Spec:`docs/superpowers/specs/2026-06-14-ui-redesign-liquid-glass-design.md`。

**全局约定(每个任务都遵守):**
- 命令在 `app/` 下跑(或 `pnpm --dir app`)。验证三件套:`pnpm --dir app exec tsc --noEmit`、`pnpm --dir app run test`、`pnpm --dir app run build`。
- 颜色/字体只用 `@theme` token,组件内禁硬编码 hex(玻璃材质里的 `rgba(255,255,255,...)` 高光例外——它是材质光学量,不是品牌色)。
- 提交信息中文 + 尾注 `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`。
- 已在 `feat/ui-redesign` 分支。

---

## 文件结构(本计划新建/修改)

| 文件 | 责任 |
|---|---|
| `app/src/index.css`(改) | 深岩板+青绿 token(明暗两套)+ radius/shadow token + amber 备用主题 + reduce-transparency 规则 + 折射 SVG 滤镜 |
| `app/src/lib/transparency.ts`(建) | 「降低透明度」偏好的纯逻辑 + 读写 + 系统查询 |
| `app/src/lib/transparency.test.ts`(建) | 上面纯逻辑的 vitest |
| `app/src/lib/liquidGlass/refraction.ts`(建,vendored MIT) | 从 `rizroze/liquid-glass` 抄入的折射核心(生成位移图 + 滤镜) |
| `app/src/lib/useLiquidGlass.ts`(建) | React hook:把折射滤镜绑到 ref 元素,卸载清理 |
| `app/src/components/ui/Glass.tsx`(建) | 玻璃容器组件:包 useLiquidGlass + 三态降级(折射/blur/实底) |
| `app/src/components/CommandPalette.tsx`(改) | 容器换成 `<Glass>`,作第一个真集成 |
| `app/src/lib/platform.ts`(建) | 判定是否 Chromium(决定能否真折射) |
| `app/src/lib/platform.test.ts`(建) | 上面的 vitest |

---

# P0 · Token 换肤

### Task 0.1:深岩板 + 青绿 token(明暗两套)

**Files:**
- Modify: `app/src/index.css:8-86`(`@theme` 块 + `:root[data-theme="dark"]` 块)

- [ ] **Step 1:改写 `@theme` 浅色 token**

把 `app/src/index.css` 第 8–35 行的 `@theme { ... }` 整块替换为(青绿身份 + 新增 radius/shadow token):

```css
@theme {
  /* 浅色:白底 + 青绿身份。靠渐深 + 细边框表达层次。 */
  --color-canvas: #ffffff;
  --color-elevated: #f3f6f9;
  --color-overlay: #e7eef2;

  --color-line: #e3e8ee;
  --color-line-strong: #ccd5dd;

  --color-fg: #16202c;
  --color-fg-muted: #51606e;
  --color-fg-subtle: #7b8794;

  --color-accent: #0d9488;          /* 青绿(白底加深保 AA) */
  --color-accent-emphasis: #0f766e;
  --color-success: #0f9d6b;
  --color-danger: #d64545;
  --color-warning: #c2691c;
  --color-done: #0d9488;            /* 提交按钮:青绿实心 + 白字 */

  --font-sans: ui-sans-serif, system-ui, "Segoe UI", "Microsoft YaHei", sans-serif;
  --font-mono: ui-monospace, "Cascadia Code", "JetBrains Mono", "SFMono-Regular", Consolas, monospace;

  /* 圆角阶梯 */
  --radius-sm: 6px;
  --radius-md: 10px;
  --radius-lg: 16px;
  --radius-pill: 999px;
}
```

- [ ] **Step 2:改写暗色覆盖块(主角)**

把第 56–86 行 `:root[data-theme="dark"] { ... }` 内的颜色 token 替换为深岩板+青绿(保留 `color-scheme: dark;` 与泳道部分留给 Task 0.3):

```css
:root[data-theme="dark"] {
  color-scheme: dark;

  --color-canvas: #0a0f18;
  --color-elevated: #121a28;
  --color-overlay: #1a2333;

  --color-line: rgba(120, 140, 170, 0.14);
  --color-line-strong: rgba(120, 140, 170, 0.28);

  --color-fg: #e6edf6;
  --color-fg-muted: #8b9bb0;
  --color-fg-subtle: #5f6e85;

  --color-accent: #2dd4bf;
  --color-accent-emphasis: #14b8a6;
  --color-success: #34c78c;
  --color-danger: #f46e6e;
  --color-warning: #f0883e;
  --color-done: #16b3a3;
}
```

- [ ] **Step 3:验证编译 + 构建**

Run: `pnpm --dir app exec tsc --noEmit; pnpm --dir app run build`
Expected: 两者均通过(CSS 改动不影响 tsc;build 成功)。

- [ ] **Step 4:真机目测(暗色主角)**

Run: `pnpm --dir app run tauri dev`
Expected: 切到暗色主题后,背景为深岩蓝黑、强调/按钮为青绿、增删色为绿/红、警告为暖橙;Diff 与图谱文字清晰可读。浅色主题强调色为青绿、对比正常。

- [ ] **Step 5:提交**

```bash
git add app/src/index.css
git commit -m "feat(ui): 深岩板+青绿设计 token(明暗两套)+ 圆角阶梯

P0 换肤第一刀:UI 强调色改青绿,暗色为主角屏,甩掉紫色。新增 radius token。

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.2:图谱泳道调色板(明暗两套,与青绿协调)

**Files:**
- Modify: `app/src/index.css:39-49`(`:root` 浅色泳道)与暗色块内泳道段

- [ ] **Step 1:改写浅色泳道(`:root` 内 `--lane-*`)**

把第 39–49 行 `:root { color-scheme: light; --lane-0..7 }` 的泳道值替换为(白底加深、与青绿协调):

```css
:root {
  color-scheme: light;
  --lane-0: #0d9488; /* 青绿 */
  --lane-1: #2563eb; /* 蓝 */
  --lane-2: #c2691c; /* 橙 */
  --lane-3: #bf3989; /* 品红 */
  --lane-4: #7c4dd6; /* 紫(仅 8 色之一) */
  --lane-5: #d64545; /* 红 */
  --lane-6: #0e7490; /* 青 */
  --lane-7: #b08400; /* 琥珀 */
}
```

- [ ] **Step 2:改写暗色泳道**

在暗色块(Task 0.1 Step 2 之后,原第 78–85 行泳道注释处)写入鲜亮多色:

```css
  /* 泳道:暗底用鲜亮一组,给图谱活力(紫仅作 8 色之一) */
  --lane-0: #2dd4bf;
  --lane-1: #5aa9ff;
  --lane-2: #f0883e;
  --lane-3: #bf3989;
  --lane-4: #a371f7;
  --lane-5: #f46e6e;
  --lane-6: #34c78c;
  --lane-7: #e3b341;
```

- [ ] **Step 3:验证构建**

Run: `pnpm --dir app run build`
Expected: 通过。

- [ ] **Step 4:真机目测图谱**

Run: `pnpm --dir app run tauri dev` → 切到「历史」
Expected: 图谱泳道为鲜亮多色,线条在深底上清晰;无两条相邻泳道撞色到难分。

- [ ] **Step 5:提交**

```bash
git add app/src/index.css
git commit -m "feat(ui): 图谱泳道调色板适配青绿身份(明暗两套)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 0.3:amber 备用主题块(实现但不默认)

**Files:**
- Modify: `app/src/index.css`(在暗色块之后追加)

- [ ] **Step 1:追加 amber-dark 覆盖块**

在 `:root[data-theme="dark"] { ... }` 之后追加(暖石墨 + 琥珀,结构同暗色):

```css
/* 备用主题:暖石墨 + 琥珀。由 <html data-theme="amber-dark"> 激活。先实现,暂不进切换器。 */
:root[data-theme="amber-dark"] {
  color-scheme: dark;

  --color-canvas: #15130f;
  --color-elevated: #221e18;
  --color-overlay: #2c271f;

  --color-line: rgba(170, 150, 120, 0.16);
  --color-line-strong: rgba(170, 150, 120, 0.30);

  --color-fg: #f0e9df;
  --color-fg-muted: #9a8e7c;
  --color-fg-subtle: #6f6556;

  --color-accent: #f5b54b;
  --color-accent-emphasis: #e0a235;
  --color-success: #46be82;
  --color-danger: #e86e64;
  --color-warning: #e0734a;
  --color-done: #d89a2e;

  --lane-0: #f5b54b;
  --lane-1: #5fa8d3;
  --lane-2: #e0734a;
  --lane-3: #d06ba0;
  --lane-4: #a78bdb;
  --lane-5: #e86e64;
  --lane-6: #46be82;
  --lane-7: #c9a23a;
}
```

- [ ] **Step 2:临时验证(手动)**

Run: `pnpm --dir app run tauri dev`,在浏览器 devtools 控制台执行 `document.documentElement.setAttribute('data-theme','amber-dark')`。
Expected: 全 app 切到暖石墨+琥珀;切回 `dark` 恢复。验证后无需保留 DOM 改动。

- [ ] **Step 3:验证构建并提交**

Run: `pnpm --dir app run build`(通过)

```bash
git add app/src/index.css
git commit -m "feat(ui): amber 暖石墨备用主题块(实现未默认)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

# P1 · 玻璃引擎

### Task 1.1:平台判定(是否 Chromium → 能否真折射)

**Files:**
- Create: `app/src/lib/platform.ts`
- Test: `app/src/lib/platform.test.ts`

- [ ] **Step 1:写失败测试**

`app/src/lib/platform.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { isChromiumUA } from "./platform";

describe("isChromiumUA", () => {
  it("Edge/Chrome 的 UA 判为 Chromium", () => {
    expect(isChromiumUA("Mozilla/5.0 ... Chrome/124.0 Safari/537.36")).toBe(true);
    expect(isChromiumUA("Mozilla/5.0 ... Edg/124.0")).toBe(true);
  });
  it("纯 Safari(含 WKWebView)与 Firefox 判为非 Chromium", () => {
    expect(isChromiumUA("Mozilla/5.0 ... Version/17.0 Safari/605.1.15")).toBe(false);
    expect(isChromiumUA("Mozilla/5.0 ... Firefox/126.0")).toBe(false);
  });
});
```

- [ ] **Step 2:跑测试确认失败**

Run: `pnpm --dir app run test -- platform`
Expected: FAIL（`isChromiumUA` 未定义）。

- [ ] **Step 3:实现**

`app/src/lib/platform.ts`:

```ts
/** 是否 Chromium 内核 UA。Chrome/Edge 的 UA 含 "Chrome/" 或 "Edg/";
 *  WKWebView/Safari 含 "Safari" 但无 "Chrome/",Firefox 含 "Firefox"。据此区分。 */
export function isChromiumUA(ua: string): boolean {
  return /\bChrome\/|\bEdg\//.test(ua);
}

/** 运行时:当前 webview 是否 Chromium(真折射前提)。 */
export function supportsRefraction(): boolean {
  return typeof navigator !== "undefined" && isChromiumUA(navigator.userAgent);
}
```

- [ ] **Step 4:跑测试确认通过**

Run: `pnpm --dir app run test -- platform`
Expected: PASS。

- [ ] **Step 5:提交**

```bash
git add app/src/lib/platform.ts app/src/lib/platform.test.ts
git commit -m "feat(ui): 平台判定 supportsRefraction(Chromium 才能真折射)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 1.2:「降低透明度」偏好(纯逻辑 + 读写)

**Files:**
- Create: `app/src/lib/transparency.ts`
- Test: `app/src/lib/transparency.test.ts`

- [ ] **Step 1:写失败测试**

`app/src/lib/transparency.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { resolveGlassMode } from "./transparency";

describe("resolveGlassMode", () => {
  it("用户显式关闭透明 → solid", () => {
    expect(resolveGlassMode({ pref: "reduced", systemReduce: false, chromium: true })).toBe("solid");
  });
  it("系统要求降低透明 → solid", () => {
    expect(resolveGlassMode({ pref: "auto", systemReduce: true, chromium: true })).toBe("solid");
  });
  it("auto + Chromium + 不降透明 → refract", () => {
    expect(resolveGlassMode({ pref: "auto", systemReduce: false, chromium: true })).toBe("refract");
  });
  it("auto + 非 Chromium + 不降透明 → blur", () => {
    expect(resolveGlassMode({ pref: "auto", systemReduce: false, chromium: false })).toBe("blur");
  });
});
```

- [ ] **Step 2:跑测试确认失败**

Run: `pnpm --dir app run test -- transparency`
Expected: FAIL（`resolveGlassMode` 未定义）。

- [ ] **Step 3:实现**

`app/src/lib/transparency.ts`:

```ts
export type GlassPref = "auto" | "reduced";
export type GlassMode = "refract" | "blur" | "solid";

/** 纯逻辑:由偏好 + 系统设置 + 是否 Chromium 决定玻璃渲染档位。 */
export function resolveGlassMode(input: {
  pref: GlassPref;
  systemReduce: boolean;
  chromium: boolean;
}): GlassMode {
  if (input.pref === "reduced" || input.systemReduce) return "solid";
  return input.chromium ? "refract" : "blur";
}

const KEY = "glass.pref";

export function getStoredGlassPref(): GlassPref {
  return localStorage.getItem(KEY) === "reduced" ? "reduced" : "auto";
}

export function setStoredGlassPref(pref: GlassPref): void {
  localStorage.setItem(KEY, pref);
}

/** 系统是否要求降低透明度(SSR/jsdom 无 matchMedia 时安全回退 false)。 */
export function systemReducesTransparency(): boolean {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-reduced-transparency: reduce)").matches
    : false;
}
```

- [ ] **Step 4:跑测试确认通过**

Run: `pnpm --dir app run test -- transparency`
Expected: PASS（4 个用例全过）。

- [ ] **Step 5:提交**

```bash
git add app/src/lib/transparency.ts app/src/lib/transparency.test.ts
git commit -m "feat(ui): 降低透明度偏好 resolveGlassMode(折射/blur/实底三档)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 1.3:折射 SVG 滤镜(写进 index.css)

**Files:**
- Modify: `app/src/index.css`(追加 reduce-transparency 媒体规则 + 折射 filter 说明)
- Modify: `app/src/main.tsx`(挂载隐藏 SVG 折射滤镜,仅一次)

- [ ] **Step 1:在 index.css 追加降级与玻璃工具规则**

在 `index.css` 末尾追加:

```css
/* 液态玻璃:三档由 <html data-glass="..."> 控制(refract/blur/solid)。
   .glass 基类只管表面 + 描边 + 阴影;backdrop-filter 由档位决定。 */
.glass {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.14), rgba(255, 255, 255, 0.03) 42%, rgba(255, 255, 255, 0.06));
  border: 1px solid rgba(255, 255, 255, 0.16);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.45), inset 0 1px 0 rgba(255, 255, 255, 0.4);
}
:root[data-glass="refract"] .glass { backdrop-filter: url(#lgWarp) blur(3px) saturate(165%); -webkit-backdrop-filter: blur(10px) saturate(165%); }
:root[data-glass="blur"] .glass { backdrop-filter: blur(12px) saturate(160%); -webkit-backdrop-filter: blur(12px) saturate(160%); }
:root[data-glass="solid"] .glass { background: var(--color-elevated); border-color: var(--color-line-strong); backdrop-filter: none; }

/* 浅色下玻璃较难出彩:默认收一档对比 */
:root:not([data-theme]) .glass { background: linear-gradient(135deg, rgba(255,255,255,.6), rgba(255,255,255,.35)); border-color: var(--color-line-strong); }
```

- [ ] **Step 2:在 main.tsx 挂载隐藏折射滤镜**

打开 `app/src/main.tsx`,在渲染 `<App/>` 外层加入一个隐藏 SVG(只需一次,全局复用 `#lgWarp`)。在 root render 的 JSX 里、`<App/>` 之前插入:

```tsx
function GlassFilter() {
  return (
    <svg width="0" height="0" style={{ position: "absolute" }} aria-hidden>
      <filter id="lgWarp" x="-20%" y="-20%" width="140%" height="140%" colorInterpolationFilters="sRGB">
        <feTurbulence type="fractalNoise" baseFrequency="0.008 0.012" numOctaves={2} seed={7} result="noise" />
        <feGaussianBlur in="noise" stdDeviation={1.4} result="snoise" />
        <feDisplacementMap in="SourceGraphic" in2="snoise" scale={30} xChannelSelector="R" yChannelSelector="G" />
      </filter>
    </svg>
  );
}
```
然后把 root 渲染改为同时渲染 `<GlassFilter/>` 与 `<App/>`(用 Fragment 包裹)。

> 说明:这是 MVP 折射(turbulence 位移),够验证管线与观感。Task 1.5 之后若要更接近 iOS 的「边缘透镜」,再在后续计划用 Task 1.4 的 vendored 库替换为按 Snell 计算的位移图。

- [ ] **Step 3:验证构建**

Run: `pnpm --dir app exec tsc --noEmit; pnpm --dir app run build`
Expected: 通过。

- [ ] **Step 4:提交**

```bash
git add app/src/index.css app/src/main.tsx
git commit -m "feat(ui): 液态玻璃 CSS 三档(refract/blur/solid)+ 全局折射 SVG 滤镜

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 1.4:`<Glass>` 容器 + `data-glass` 接线

**Files:**
- Create: `app/src/components/ui/Glass.tsx`
- Modify: `app/src/lib/theme.ts`（追加 applyGlassMode 接线,复用现有 applyTheme 习惯）
- Modify: `app/src/main.tsx`（首屏根据偏好设 `data-glass`）

- [ ] **Step 1:在 theme.ts 追加玻璃档位应用**

在 `app/src/lib/theme.ts` 末尾追加(不动现有 theme 逻辑):

```ts
import { resolveGlassMode, getStoredGlassPref, systemReducesTransparency } from "./transparency";
import { supportsRefraction } from "./platform";

/** 计算当前玻璃档位并写到 <html data-glass>。在首屏与偏好变更时调用。 */
export function applyGlassMode(): void {
  const mode = resolveGlassMode({
    pref: getStoredGlassPref(),
    systemReduce: systemReducesTransparency(),
    chromium: supportsRefraction(),
  });
  document.documentElement.setAttribute("data-glass", mode);
}
```

- [ ] **Step 2:首屏接线**

在 `app/src/main.tsx` 渲染前调用一次。在 import 区加 `import { applyGlassMode } from "./lib/theme";`,并在 `ReactDOM.createRoot(...).render(...)` 之前加 `applyGlassMode();`。

> 现有 main.tsx 应已有 `applyTheme(getStoredTheme())` 之类首屏调用;若无,顺手补上读取主题。检查后再决定是否补。

- [ ] **Step 3:写 `<Glass>` 容器**

`app/src/components/ui/Glass.tsx`:

```tsx
import type { HTMLAttributes, ReactNode } from "react";
import { cx } from "./Button";

/** 液态玻璃容器。只用于外壳浮层(顶栏/侧栏/菜单/面板/Toast/弹层),
 *  禁止包 Diff/图谱/文件列表等密集内容。渲染档位由 <html data-glass> 决定。 */
export function Glass({
  as: Tag = "div",
  className,
  children,
  ...rest
}: {
  as?: "div" | "nav" | "aside" | "header" | "section";
  children?: ReactNode;
} & HTMLAttributes<HTMLElement>) {
  return (
    <Tag className={cx("glass", className)} {...rest}>
      {children}
    </Tag>
  );
}
```

- [ ] **Step 4:验证构建**

Run: `pnpm --dir app exec tsc --noEmit; pnpm --dir app run build`
Expected: 通过。

- [ ] **Step 5:提交**

```bash
git add app/src/components/ui/Glass.tsx app/src/lib/theme.ts app/src/main.tsx
git commit -m "feat(ui): <Glass> 容器 + applyGlassMode 接线(首屏定档位)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 1.5:命令面板套玻璃(第一个真集成)+ 降透明度开关

**Files:**
- Modify: `app/src/components/CommandPalette.tsx`（外层容器换 `<Glass>`）
- Modify: `app/src/App.tsx`（命令面板加一条「降低透明度」开关命令)

- [ ] **Step 1:命令面板容器换 Glass**

打开 `app/src/components/CommandPalette.tsx`,找到面板主体容器(那个带 `bg-elevated`/`border`/`shadow` 的浮层 div),把它的 `className` 里的实底背景类去掉,改为外面包一层 `<Glass className="...保留圆角/宽度/定位...">`(import:`import { Glass } from "./ui/Glass";`)。遮罩层(backdrop)保持不变。

> 验证点:玻璃只包面板浮层本体;搜索结果列表内部不再单独糊背景。

- [ ] **Step 2:加「降低透明度」开关命令**

在 `app/src/App.tsx` 的命令清单里(`commands.push(...)` 区,靠近 `theme:toggle` 那条)新增:

```tsx
commands.push({
  id: "glass:toggle",
  title: getStoredGlassPref() === "reduced" ? "开启玻璃透明效果" : "降低透明度(玻璃转实底)",
  group: "外观",
  keywords: "glass transparency 玻璃 透明 实底 无障碍",
  run: () => {
    const next = getStoredGlassPref() === "reduced" ? "auto" : "reduced";
    setStoredGlassPref(next);
    applyGlassMode();
  },
});
```
import 区补:`import { getStoredGlassPref, setStoredGlassPref } from "./lib/transparency"; import { applyGlassMode } from "./lib/theme";`

- [ ] **Step 3:验证编译 + 构建 + 测试**

Run: `pnpm --dir app exec tsc --noEmit; pnpm --dir app run test; pnpm --dir app run build`
Expected: 全过(现有 vitest 不回归,新加的 platform/transparency 测试通过)。

- [ ] **Step 4:真机验收(Windows / Chromium)**

Run: `pnpm --dir app run tauri dev`
Expected:
1. ⌘K 打开命令面板,面板是玻璃质感,背后图谱/内容被折射+提亮,边缘有镜面高光。
2. 跑「降低透明度」命令 → 面板秒变近实底;再跑一次切回。
3. 暗色/浅色主题下都不破版、文字清晰。
4. devtools 改 `document.documentElement.dataset.glass='blur'` 模拟非 Chromium → 退化为纯 blur 不报错。

- [ ] **Step 5:提交**

```bash
git add app/src/components/CommandPalette.tsx app/src/App.tsx
git commit -m "feat(ui): 命令面板套液态玻璃 + 降低透明度开关命令(首个真集成)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## 验收(P0+P1 收口)
- 自动门:`pnpm --dir app exec tsc --noEmit`、`pnpm --dir app run test`、`pnpm --dir app run build` 全绿。
- 真机(Windows):青绿身份生效、图谱鲜亮、命令面板真折射、降透明度开关有效。
- 不回归:现有所有功能与交互照常(本阶段只动表现层 + 新增独立模块)。
- 合分支:`feat/ui-redesign` 暂不合 main,留待 P2/P3/P4 在同分支继续;真机验收 OK 后由用户决定 push 节奏。

## 后续计划(另写,依赖本阶段落地)
- **P2 Shell 重塑**:`Sidebar` + `TopBar` + 主区三栏/两栏重排,`TabBar` 退役。
- **P3 全面套玻璃**:其余菜单/弹层/Toast/各 `*Panel` 接 `<Glass>`;底栏微调。
- **P4 打磨**:GSAP 微交互、amber 进切换器、真机验收清单全过;按需用 vendored rizroze 库把 MVP turbulence 折射升级为按 Snell 计算的边缘透镜。
- **Phase 2(可选)**:OS 级窗口 vibrancy(Win11 Mica/Acrylic、macOS vibrancy),需改 Tauri 窗口配置,独立 spec。
