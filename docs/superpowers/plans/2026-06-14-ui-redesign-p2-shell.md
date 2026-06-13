# UI 大重构 P2 · Shell 重塑 实现计划

> **For agentic workers:** 用 superpowers:subagent-driven-development 逐任务执行。步骤用 `- [ ]`。

**Goal:** 把「顶栏 + 顶部 Tab + 视图 + 底栏」改成「顶栏 + [常驻左侧栏 + 主区] + 底栏」,顶栏与侧栏用液态玻璃,让外壳质感铺开;历史页升级为三栏让图谱唱主角。

**Architecture:** 纯前端。先做 P2a(左侧栏 + 玻璃外壳,替代 TabBar),再 P2b(历史三栏),最后 P2c(侧栏纳入分支/远程/标签储藏 + 折叠)。后端零改动。

**Tech Stack:** React 19、Tailwind v4 token、`<Glass>`(已就位)、TanStack Query(不动)。

**Scope:** 续 `feat/ui-redesign` 分支(P0+P1 已落)。Spec §5/§6:`docs/superpowers/specs/2026-06-14-ui-redesign-liquid-glass-design.md`。

**全局约定:** 命令在 `app/` 跑;token-only 颜色;pnpm;验证 `pnpm --dir app exec tsc --noEmit` + `pnpm --dir app run test` + `pnpm --dir app run build`;提交中文 + Claude 尾注。

**现状要点(已读):**
- `App.tsx`:`<div flex h-screen flex-col>` 内 = `<header h-11>`(顶栏,含 fetch/pull/push/undo/oplog/远程/主题/命令面板/切仓库)→ `{repo && <TabBar .../>}` → `<div flex-1>` 内按 `tab` 分发各视图 → `{repo && <footer>}`。`tab` state + `setTab` 在 App;动态标签 has{Submodules,Worktrees,Sparse};一个 effect 在动态标签消失时退回 changes;命令面板的「切换视图」命令遍历 views 列表调 `setTab`。
- `TabBar.tsx`:水平按钮条,`Tab` 类型 + `TABS` 基础四项(更改/历史/比较/追溯)+ 动态三项,active 下划线。
- `<Glass>`:`app/src/components/ui/Glass.tsx`,`as` 多态 + `.glass` 类。

---

## P2a · 左侧栏 + 玻璃外壳

### Task P2a.1:Sidebar 组件(竖向视图导航 + 折叠 + 玻璃)

**Files:**
- Create: `app/src/components/Sidebar.tsx`
- (复用)`app/src/components/TabBar.tsx` 的 `Tab` 类型 + icons.tsx

- [ ] **Step 1:实现 `Sidebar.tsx`**

竖向导航,复用 `Tab` 类型(从 `./TabBar` import `type Tab`),玻璃 `<aside>`。每项一个图标 + 标签;active 用青绿底高亮;支持折叠(窄态只图标 + title)。

```tsx
import type { Tab } from "./TabBar";
import { Glass } from "./ui/Glass";
import { cx } from "./ui/Button";
import {
  FileDiffIcon, HistoryIcon, // 已存在于 icons.tsx
} from "./icons";
import type { ReactNode } from "react";

type Item = { id: Tab; label: string; icon: ReactNode };

export function Sidebar({
  active, onChange, collapsed, onToggleCollapse,
  hasSubmodules = false, hasWorktrees = false, hasSparse = false,
}: {
  active: Tab;
  onChange: (t: Tab) => void;
  collapsed: boolean;
  onToggleCollapse: () => void;
  hasSubmodules?: boolean;
  hasWorktrees?: boolean;
  hasSparse?: boolean;
}) {
  const items: Item[] = [
    { id: "changes", label: "更改", icon: <FileDiffIcon width={16} height={16} /> },
    { id: "history", label: "历史", icon: <HistoryIcon width={16} height={16} /> },
    { id: "compare", label: "比较", icon: <FileDiffIcon width={16} height={16} /> },
    { id: "blame", label: "追溯", icon: <HistoryIcon width={16} height={16} /> },
  ];
  if (hasSubmodules) items.push({ id: "submodules", label: "子模块", icon: <FileDiffIcon width={16} height={16} /> });
  if (hasWorktrees) items.push({ id: "worktrees", label: "工作树", icon: <FileDiffIcon width={16} height={16} /> });
  if (hasSparse) items.push({ id: "sparse", label: "稀疏检出", icon: <FileDiffIcon width={16} height={16} /> });

  return (
    <Glass
      as="aside"
      className={cx(
        "flex shrink-0 flex-col gap-0.5 border-r border-line p-2 transition-[width]",
        collapsed ? "w-12 items-center" : "w-44",
      )}
    >
      {items.map((it) => {
        const on = active === it.id;
        return (
          <button
            key={it.id}
            onClick={() => onChange(it.id)}
            title={collapsed ? it.label : undefined}
            className={cx(
              "flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm transition-colors",
              collapsed ? "w-9 justify-center" : "w-full",
              on ? "bg-accent/15 text-accent" : "text-fg-muted hover:bg-overlay hover:text-fg",
            )}
          >
            <span className="shrink-0">{it.icon}</span>
            {!collapsed && <span className="truncate">{it.label}</span>}
          </button>
        );
      })}
      <button
        onClick={onToggleCollapse}
        title={collapsed ? "展开侧栏" : "折叠侧栏"}
        className={cx(
          "mt-auto flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm text-fg-subtle transition-colors hover:bg-overlay hover:text-fg",
          collapsed ? "w-9 justify-center" : "w-full",
        )}
      >
        <span className="shrink-0">{collapsed ? "»" : "«"}</span>
        {!collapsed && <span>折叠</span>}
      </button>
    </Glass>
  );
}
```
> 注:`FileDiffIcon`/`HistoryIcon` 确认存在于 `app/src/components/icons.tsx`(ChangesView/App 已用)。比较/追溯暂复用图标,后续可换更贴切的;若 icons.tsx 有更合适的(如 GitCompareIcon/BlameIcon)优先用,没有就用这两个占位。读 icons.tsx 确认可用图标名后再定。

- [ ] **Step 2:验证编译**

Run: `pnpm --dir app exec tsc --noEmit` — 通过(组件未挂载也应能编译)。

- [ ] **Step 3:提交**
```
git add app/src/components/Sidebar.tsx
git commit -m "feat(ui): 常驻左侧栏 Sidebar 组件(竖向视图导航 + 折叠 + 玻璃)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task P2a.2:把 Sidebar 接进 App 布局,退役 TabBar

**Files:**
- Modify: `app/src/App.tsx`

- [ ] **Step 1:改布局**

把 `App.tsx` 主体从「header → TabBar → 视图分发 → footer」改为「header → 横向行(Sidebar + 视图分发) → footer」。

具体:
1. import 改:删 `import { TabBar, type Tab } from "./components/TabBar";`,改为 `import { Sidebar } from "./components/Sidebar"; import type { Tab } from "./components/TabBar";`(`Tab` 类型仍来自 TabBar 文件;TabBar 组件不再渲染但类型保留)。新增侧栏折叠状态:`const [sideCollapsed, setSideCollapsed] = useState(() => localStorage.getItem("sidebar.collapsed") === "1");` 折叠切换写 localStorage。
2. 删掉原 `{repo && <TabBar active={tab} onChange={setTab} .../>}` 那一行。
3. 把原来包视图分发的 `<div className="min-h-0 flex-1">...</div>`(含 `tab === ...` 三元链)用一个横向 flex 行包起来,Sidebar 在左:
```tsx
{repo ? (
  <div className="flex min-h-0 flex-1">
    <Sidebar
      active={tab}
      onChange={setTab}
      collapsed={sideCollapsed}
      onToggleCollapse={() => { const n = !sideCollapsed; setSideCollapsed(n); localStorage.setItem("sidebar.collapsed", n ? "1" : "0"); }}
      hasSubmodules={hasSubmodules}
      hasWorktrees={hasWorktrees}
      hasSparse={hasSparse}
    />
    <div className="min-h-0 min-w-0 flex-1">
      {tab === "changes" ? <ChangesView repo={repo} /> : tab === "history" ? <HistoryView repo={repo} /> : tab === "compare" ? <CompareView repo={repo} /> : tab === "submodules" ? <SubmodulesView repo={repo} /> : tab === "worktrees" ? <WorktreesView repo={repo} /> : tab === "sparse" ? <SparseCheckoutView repo={repo} /> : <BlameView repo={repo} />}
    </div>
  </div>
) : (
  <EmptyState onPick={pickRepo} />
)}
```
保留:动态标签 has* 逻辑、退回 changes 的 effect、命令面板「切换视图」命令(仍调 setTab,不动)。

- [ ] **Step 2:验证**

Run: `pnpm --dir app exec tsc --noEmit` + `pnpm --dir app run build` + `pnpm --dir app run test` — 全过。

- [ ] **Step 3:真机目测**

Run: `pnpm --dir app run tauri dev` — 左侧出现常驻竖向导航,点项切视图正常,折叠/展开有效且重启保持;动态标签按仓库出现;原各视图内容照常。

- [ ] **Step 4:提交**
```
git add app/src/App.tsx
git commit -m "feat(ui): 左侧栏接入 App 布局,退役顶部 TabBar(竖切视图导航)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task P2a.3:顶栏上液态玻璃

**Files:**
- Modify: `app/src/App.tsx`(顶栏 `<header>`)

- [ ] **Step 1:顶栏换 Glass**

把 `App.tsx` 的 `<header className="flex h-11 shrink-0 items-center gap-3 border-b border-line px-3">` 换成 `<Glass as="header" className="relative z-20 flex h-11 shrink-0 items-center gap-3 border-b border-line px-3">`(import `{ Glass }` from `./components/ui/Glass`)。去掉 header 自身若有的实底背景类(当前无 bg 类,只有 border;玻璃会补背景)。`z-20` 保证浮在内容之上。

> 注意:顶栏在正常流里(不覆盖滚动内容),玻璃折射效果在暗色 + 下方有内容时才明显;这步主要把顶栏纳入玻璃语言。底栏同理可选做,本任务先只做顶栏。

- [ ] **Step 2:验证** `pnpm --dir app exec tsc --noEmit` + `pnpm --dir app run build` 通过。

- [ ] **Step 3:真机目测** 暗色下顶栏呈玻璃质感,按钮/文字清晰;浅色下不破版。

- [ ] **Step 4:提交**
```
git add app/src/App.tsx
git commit -m "feat(ui): 顶栏纳入液态玻璃外壳

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## P2b · 历史页三栏(后续里程碑,落地前细化)
图谱(中)+ 提交详情/Diff(右)同屏。需读 `HistoryView.tsx` 现状后补细化任务。

## P2c · 侧栏纳入分支/远程/标签储藏 + 段折叠(后续里程碑)
左侧栏增加「本地分支 / 远程 / 标签·储藏」段,接现有 branches/remotes/stash/tag 能力;仓库切换器移入侧栏顶部。需细化。

## 验收(P2a)
- 自动门全绿;真机:左侧栏导航 + 折叠 + 顶栏玻璃在明暗下均 OK,无功能回归。
