import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { Tab } from "./components/TabBar";
import { Sidebar } from "./components/Sidebar";
import { ChangesView } from "./views/ChangesView";
import { HistoryView } from "./views/HistoryView";
import { CompareView } from "./views/CompareView";
import { BlameView } from "./views/BlameView";
import { SubmodulesView } from "./views/SubmodulesView";
import { WorktreesView } from "./views/WorktreesView";
import { SparseCheckoutView } from "./views/SparseCheckoutView";
import { useQueryClient } from "@tanstack/react-query";
import { setUpstream, fetchRemote, pullRemote, pushRemote, undo, redo, checkoutBranch, type IpcError } from "./ipc";
import { FolderIcon, SunIcon, MoonIcon, FetchIcon, PullIcon, PushIcon, SpinnerIcon, ChevronDownIcon, CheckIcon, UndoIcon, RedoIcon, HistoryIcon, SearchIcon, MoreIcon, DropletIcon } from "./components/icons";
import { BranchSwitcher } from "./components/BranchSwitcher";
import { SyncBadge } from "./components/SyncBadge";
import { StashMenu } from "./components/StashMenu";
import { OpLogPanel } from "./components/OpLogPanel";
import { CommandPalette } from "./components/CommandPalette";
import type { Command } from "./lib/commands";
import { useToast } from "./components/Toast";
import { Glass } from "./components/ui/Glass";
import { LaunchGraph } from "./components/LaunchGraph";
import { useRepoWatch, useCurrentBranch, useAheadBehind, useRemotes, useUndoState, useBranches, useSubmodules, useWorktrees, useSparseCheckout, invalidateHistory, invalidateWorktree, qk } from "./lib/queries";
import { applyTheme, applyGlassMode, getStoredTheme, type Theme } from "./lib/theme";
import { getStoredGlassPref, setStoredGlassPref } from "./lib/transparency";

/** 把 git fetch 的原始摘要提炼成简洁细节:优先取 "->" 更新行。 */
function fetchDetail(summary: string): string | undefined {
  if (summary === "已是最新") return undefined;
  const lines = summary.split("\n").map((l) => l.trim()).filter(Boolean);
  const updates = lines.filter((l) => l.includes("->"));
  return (updates.length ? updates : lines.slice(0, 1)).join("\n") || undefined;
}

export default function App() {
  const [repo, setRepo] = useState<string | null>(null);
  // 上次打开的仓库:不自动跳入(保留启动屏作为每次开 app 的第一印象),
  // 而是在启动屏给一个「继续上次」快捷入口(Linear/Things 式「跳回上次」)。
  const lastRepo = localStorage.getItem("repo.last");
  const [tab, setTab] = useState<Tab>("changes");
  const [theme, setTheme] = useState<Theme>(getStoredTheme);
  const [fetching, setFetching] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [pullRebase, setPullRebase] = useState(() => localStorage.getItem("pull.rebase") === "1");
  const [sideCollapsed, setSideCollapsed] = useState(() => localStorage.getItem("sidebar.collapsed") === "1");
  const [pullMenu, setPullMenu] = useState(false);
  const [selectedRemote, setSelectedRemote] = useState<string | null>(null);
  const [remoteMenu, setRemoteMenu] = useState(false);
  const [upMenu, setUpMenu] = useState(false);
  const [undoing, setUndoing] = useState(false);
  const [opLogOpen, setOpLogOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [moreMenu, setMoreMenu] = useState(false);
  const toast = useToast();
  const qc = useQueryClient();

  // 顶层读统一走 query;一处监听文件变化 → 失效(各 view 据此自动重取)
  useRepoWatch(repo);
  const branch = useCurrentBranch(repo ?? "").data ?? null;
  const sync = useAheadBehind(repo ?? "").data ?? null;
  const remotes = useRemotes(repo ?? "").data ?? [];
  const undoState = useUndoState(repo ?? "").data ?? null;
  const canUndo = undoState?.can_undo ?? null;
  const canRedo = undoState?.can_redo ?? null;
  const branches = useBranches(repo ?? "", !!repo).data ?? [];
  const submodules = useSubmodules(repo ?? "").data ?? [];
  const hasSubmodules = submodules.length > 0;
  const worktrees = useWorktrees(repo ?? "").data ?? [];
  const hasWorktrees = worktrees.length > 1; // 只有主工作树时不显示标签
  const sparsePatterns = useSparseCheckout(repo ?? "").data ?? [];
  const hasSparse = sparsePatterns.length > 0; // 未开启稀疏检出时不显示标签

  const busy = fetching || pulling || pushing || undoing;
  // 同步提示:落后 → 建议 Pull;领先 → 建议 Push(无上游时 sync 为 null,不提示)
  const canPull = !!sync && sync.behind > 0;
  const canPush = !!sync && sync.ahead > 0;

  function toggleTheme() {
    const next: Theme = theme === "dark" ? "light" : "dark";
    applyTheme(next);
    setTheme(next);
  }

  async function doFetch() {
    if (!repo) return;
    setFetching(true);
    try {
      const r = await fetchRemote(repo, selectedRemote ?? undefined);
      // refs 变化会触发文件监听 → 各视图自动重载;这里只用 toast 反馈结果。
      toast({
        kind: "success",
        title: r.summary === "已是最新" ? "已是最新" : "已拉取更新",
        detail: fetchDetail(r.summary),
      });
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setFetching(false);
      invalidateHistory(qc, repo); // 兜底:packed-refs 时 watcher 可能不触发
    }
  }

  function setPullMode(rebase: boolean) {
    setPullRebase(rebase);
    localStorage.setItem("pull.rebase", rebase ? "1" : "0");
  }

  async function doPull(rebase: boolean) {
    if (!repo) return;
    setPullMenu(false);
    setPulling(true);
    try {
      const r = await pullRemote(repo, rebase, selectedRemote ?? undefined);
      // 成功后工作区/HEAD 变化触发文件监听 → 图谱自动前进。
      const upToDate = /up to date|已是最新/i.test(r.summary);
      toast({ kind: "success", title: upToDate ? "已是最新" : rebase ? "已拉取并变基" : "已拉取并合并" });
    } catch (e) {
      // 冲突时工作区已留下冲突标记(rebase 会停在中途),可到「更改」页查看。
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setPulling(false);
      invalidateHistory(qc, repo);
    }
  }

  async function doPush() {
    if (!repo) return;
    setPushing(true);
    try {
      const r = await pushRemote(repo, selectedRemote ?? undefined);
      // push 成功后远程跟踪分支前进 → watcher(ref)刷新底栏/角标。
      const upToDate = /up-to-date|up to date|已是最新/i.test(r.summary);
      toast({
        kind: "success",
        title: upToDate ? "已是最新" : r.set_upstream ? "已推送并建立上游" : "已推送",
      });
    } catch (e) {
      // 落后远程时会抛 PUSH_REJECTED:提示用户先 Pull 再推。
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setPushing(false);
      invalidateHistory(qc, repo);
    }
  }

  async function doSetUpstream(upstream: string) {
    if (!repo) return;
    setUpMenu(false);
    try {
      await setUpstream(repo, upstream);
      toast({ kind: "success", title: `已设置上游 ${upstream}` });
      qc.invalidateQueries({ queryKey: qk.aheadBehind(repo) });
    } catch (e) {
      // 远程跟踪分支不存在时会失败——提示用户先 Fetch/Push。
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    }
  }

  // 撤销/重做共用:沿操作时间线移动 HEAD(reset --soft),成功后刷新历史 + 工作区。
  async function doNav(dir: "undo" | "redo") {
    if (!repo) return;
    setUndoing(true);
    try {
      const info = await (dir === "undo" ? undo(repo) : redo(repo));
      const where = dir === "undo" ? "回到" : "前进到";
      // soft(撤销提交)内容回暂存区;hard(撤销 reset 等)已忠实还原工作区。
      const effect = info.worktree_restored ? "工作区已还原到该状态" : "改动回到暂存区";
      toast({
        kind: "success",
        title: `${dir === "undo" ? "已撤销" : "已重做"}:${info.label}`,
        detail: `HEAD ${where} ${info.target_short},${effect}`,
      });
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setUndoing(false);
      // 动了 HEAD + 暂存区:历史(含可否再撤销/重做)与工作区都要刷新。
      invalidateHistory(qc, repo);
      invalidateWorktree(qc, repo);
    }
  }

  // 命令面板「跳转到分支」用:切换分支 + 反馈 + 失效。
  async function doCheckout(name: string) {
    if (!repo) return;
    try {
      await checkoutBranch(repo, name);
      toast({ kind: "success", title: `已切换到分支 ${name}` });
      qc.invalidateQueries({ queryKey: ["branches", repo] });
      invalidateHistory(qc, repo);
      invalidateWorktree(qc, repo);
    } catch (e) {
      // 脏工作区切换失败等 → 提示
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    }
  }

  async function pickRepo() {
    const dir = await open({ directory: true, title: "选择一个 git 仓库" });
    if (typeof dir === "string") setRepo(dir);
  }

  // 切仓库时重置远程选择(回到默认)
  useEffect(() => { setSelectedRemote(null); }, [repo]);

  // 记住当前仓库,供下次启动屏「继续上次」用。
  useEffect(() => { if (repo) localStorage.setItem("repo.last", repo); }, [repo]);

  // 动态标签(子模块/工作树)随仓库出现/消失;切到不再可用的标签时退回「更改」,免得停在空标签。
  useEffect(() => {
    if ((tab === "submodules" && !hasSubmodules) || (tab === "worktrees" && !hasWorktrees) || (tab === "sparse" && !hasSparse)) setTab("changes");
  }, [tab, hasSubmodules, hasWorktrees, hasSparse]);

  // 全局 ⌘K / Ctrl+K 开关命令面板(M3 键盘优先入口)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && (e.key === "k" || e.key === "K")) {
        e.preventDefault();
        setPaletteOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const isMac = navigator.platform.toLowerCase().includes("mac");
  const modLabel = isMac ? "⌘K" : "Ctrl K";

  // 命令面板的命令清单。每次渲染重建,保证 run() 闭包拿到最新 state(如 selectedRemote),
  // 避免 useMemo 漏依赖导致的陈旧闭包;十来条命令,重建成本可忽略。
  const commands: Command[] = [];
  const views: { id: Tab; label: string }[] = [
    { id: "changes", label: "更改" },
    { id: "history", label: "历史" },
    { id: "compare", label: "比较" },
    { id: "blame", label: "追溯" },
  ];
  if (hasSubmodules) views.push({ id: "submodules", label: "子模块" });
  if (hasWorktrees) views.push({ id: "worktrees", label: "工作树" });
  if (hasSparse) views.push({ id: "sparse", label: "稀疏检出" });
  for (const v of views) {
    commands.push({
      id: `view:${v.id}`,
      title: `切换到「${v.label}」`,
      group: "视图",
      keywords: `view tab ${v.id} ${v.label}`,
      disabled: !repo || tab === v.id,
      run: () => setTab(v.id),
    });
  }
  commands.push({
    id: "jump:branch",
    title: "跳转到分支…",
    subtitle: "切换",
    group: "跳转",
    keywords: "checkout switch branch 切换 分支 跳转",
    disabled: !repo || busy || branches.length === 0,
    run: () => {},
    jump: {
      placeholder: "跳转到分支…",
      items: branches.map((b) => ({
        id: b.name,
        label: b.name,
        hint: b.is_head ? "当前" : undefined,
        run: () => {
          if (!b.is_head) doCheckout(b.name);
        },
      })),
    },
  });
  commands.push({
    id: "repo:pick",
    title: repo ? "切换仓库" : "选择仓库",
    group: "仓库",
    keywords: "open repo folder 打开 仓库 切换",
    run: pickRepo,
  });
  commands.push({
    id: "theme:toggle",
    title: theme === "dark" ? "切换到浅色主题" : "切换到暗色主题",
    group: "外观",
    keywords: "theme dark light 主题 暗色 浅色 切换",
    run: toggleTheme,
  });
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
  commands.push(
    { id: "remote:fetch", title: "Fetch", subtitle: "从远程拉取更新", group: "远程", keywords: "拉取 远程 fetch", disabled: !repo || busy, run: doFetch },
    { id: "remote:pull-merge", title: "Pull · 合并", group: "远程", keywords: "拉取 合并 merge pull", disabled: !repo || busy, run: () => doPull(false) },
    { id: "remote:pull-rebase", title: "Pull · 变基", group: "远程", keywords: "拉取 变基 rebase pull", disabled: !repo || busy, run: () => doPull(true) },
    { id: "remote:push", title: "Push", subtitle: "推送当前分支", group: "远程", keywords: "推送 push", disabled: !repo || busy, run: doPush },
  );
  commands.push(
    { id: "undo", title: canUndo ? `撤销：${canUndo.label}` : "撤销", group: "撤销", keywords: "undo 撤销 回退", disabled: !repo || busy || !canUndo, run: () => doNav("undo") },
    { id: "redo", title: canRedo ? `重做：${canRedo.label}` : "重做", group: "撤销", keywords: "redo 重做 前进", disabled: !repo || busy || !canRedo, run: () => doNav("redo") },
  );
  commands.push({
    id: "panel:oplog",
    title: "操作日志",
    subtitle: "本会话写操作时间线",
    group: "面板",
    keywords: "operation log history 操作 日志 时间线",
    disabled: !repo,
    run: () => setOpLogOpen(true),
  });

  // 仓库路径只显示尾部目录名,完整路径放 title 悬浮
  const repoName = repo?.replace(/[/\\]+$/, "").split(/[/\\]/).pop() ?? null;

  return (
    <div className={`flex h-screen flex-col text-fg ${repo ? "bg-canvas" : "launch-ambient"}`}>
      {busy && <TopProgress />}
      {/* 顶栏:轻、紧凑、左标题右仓库 */}
      <Glass as="header" className="relative z-20 flex h-11 shrink-0 items-center gap-3 border-b border-line px-3">
        <div className="flex min-w-0 items-center gap-2">
          <BranchMark />
          {repo ? (
            <span className="max-w-[12rem] truncate font-mono text-sm font-medium text-fg" title={repo}>{repoName}</span>
          ) : (
            <span className="text-sm font-semibold">Git 客户端</span>
          )}
          {repo && (
            <div className="flex min-w-0 items-center gap-2 text-xs">
              <span className="text-fg-subtle">/</span>
              <BranchSwitcher repo={repo} branch={branch} direction="down" />
              <SyncBadge sync={sync} />
            </div>
          )}
        </div>

        <div className="ml-auto flex items-center gap-2">
          {repo && (
            <div className="flex items-center gap-1.5">
              {remotes.length > 1 && (
                <div className="relative">
                  <button
                    onClick={() => setRemoteMenu((o) => !o)}
                    disabled={busy}
                    title="选择远程仓库"
                    className="flex items-center gap-1 rounded-md border border-line-strong bg-elevated px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
                  >
                    <span className="max-w-[8rem] truncate font-mono">{selectedRemote ?? remotes[0]}</span>
                    <ChevronDownIcon width={11} height={11} />
                  </button>
                  {remoteMenu && (
                    <>
                      <div className="fixed inset-0 z-40" onClick={() => setRemoteMenu(false)} />
                      <div className="absolute right-0 top-full z-50 mt-1 w-44 overflow-hidden rounded-md border border-line-strong bg-elevated text-xs menu-in popover">
                        {remotes.map((r) => (
                          <button
                            key={r}
                            onClick={() => { setSelectedRemote(r); setRemoteMenu(false); }}
                            className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
                          >
                            <span className="grid w-3.5 shrink-0 place-items-center text-accent">
                              {(selectedRemote ?? remotes[0]) === r ? <CheckIcon width={12} height={12} /> : null}
                            </span>
                            <span className="truncate font-mono">{r}</span>
                          </button>
                        ))}
                      </div>
                    </>
                  )}
                </div>
              )}
              {/* 撤销/重做:成对收进中性托盘(↩↪ 一眼是「撤销/重做」而非「返回」;不用 accent
                  免被当成主导航键)。任一可用即显示,另一侧不可用时变暗;标签在 title/aria-label。
                  进行中状态由顶部 TopProgress(busy)统一信号,这里不放内联 spinner。 */}
              {(canUndo || canRedo) && (
                <div className="flex items-center gap-0.5 rounded-lg border border-line bg-elevated/60 p-0.5">
                  <button
                    onClick={() => doNav("undo")}
                    disabled={busy || !canUndo}
                    aria-label={canUndo ? `撤销${canUndo.label}` : "撤销"}
                    title={canUndo ? `撤销刚才的「${canUndo.label}」(回到 ${canUndo.target_short}；${canUndo.worktree_restored ? "还原工作区，有未提交改动会先拦下" : "改动回暂存区，不丢工作区"})` : "暂无可撤销的操作"}
                    className="grid h-7 w-7 place-items-center rounded-md text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-35 disabled:hover:bg-transparent disabled:hover:text-fg-muted"
                  >
                    <UndoIcon width={14} height={14} />
                  </button>
                  <button
                    onClick={() => doNav("redo")}
                    disabled={busy || !canRedo}
                    aria-label={canRedo ? `重做${canRedo.label}` : "重做"}
                    title={canRedo ? `重做「${canRedo.label}」(前进到 ${canRedo.target_short})` : "暂无可重做的操作"}
                    className="grid h-7 w-7 place-items-center rounded-md text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-35 disabled:hover:bg-transparent disabled:hover:text-fg-muted"
                  >
                    <RedoIcon width={14} height={14} />
                  </button>
                </div>
              )}
              {/* 同步操作托盘:Fetch · Pull · Push 收成一组,读作一个单元(双层贝塞尔托盘)。
                  各按钮无独立边框,托盘承载边框;可 Pull/Push 时用强调色文字 + ↓N/↑N 角标提示。 */}
              <div className="flex items-center gap-0.5 rounded-lg border border-line bg-elevated/60 p-0.5">
                <button
                  onClick={doFetch}
                  disabled={busy}
                  title="Fetch(从远程拉取更新，不改工作区)"
                  className="flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs text-fg transition-colors hover:bg-overlay disabled:opacity-50"
                >
                  {fetching ? (
                    <SpinnerIcon width={13} height={13} />
                  ) : (
                    <FetchIcon width={13} height={13} />
                  )}
                  {fetching ? "Fetch…" : "Fetch"}
                </button>
                <div className="relative flex items-stretch">
                  <button
                    onClick={() => doPull(pullRebase)}
                    disabled={busy}
                    title={canPull ? `落后上游 ${sync!.behind} 个提交，建议 Pull` : `Pull(${pullRebase ? "变基" : "合并"}到当前分支)`}
                    className={`flex items-center gap-1.5 rounded-l-md px-2.5 py-1 text-xs transition-colors hover:bg-overlay disabled:opacity-50 ${
                      canPull ? "text-accent" : "text-fg"
                    }`}
                  >
                    {pulling ? <SpinnerIcon width={13} height={13} /> : <PullIcon width={13} height={13} />}
                    {pulling ? "Pull…" : pullRebase ? "Pull · 变基" : "Pull"}
                    {canPull && !pulling && (
                      <span className="rounded-full bg-accent/15 px-1 font-mono text-[10px] font-semibold text-accent">
                        ↓{sync!.behind}
                      </span>
                    )}
                  </button>
                  <button
                    onClick={() => setPullMenu((o) => !o)}
                    disabled={busy}
                    title="选择拉取方式"
                    className="grid place-items-center rounded-r-md px-1 text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
                  >
                    <ChevronDownIcon width={11} height={11} />
                  </button>
                  {pullMenu && (
                    <>
                      <div className="fixed inset-0 z-40" onClick={() => setPullMenu(false)} />
                      <div className="absolute right-0 top-full z-50 mt-1.5 w-44 overflow-hidden rounded-md border border-line-strong bg-elevated text-xs menu-in popover">
                        <PullModeItem active={!pullRebase} onClick={() => { setPullMode(false); doPull(false); }}>
                          合并(merge)
                        </PullModeItem>
                        <PullModeItem active={pullRebase} onClick={() => { setPullMode(true); doPull(true); }}>
                          变基(rebase)
                        </PullModeItem>
                      </div>
                    </>
                  )}
                </div>
                <button
                  onClick={doPush}
                  disabled={busy}
                  title={canPush ? `领先上游 ${sync!.ahead} 个提交，建议 Push` : "Push(把当前分支推到远程；首次自动建立上游)"}
                  className={`flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs transition-colors hover:bg-overlay disabled:opacity-50 ${
                    canPush ? "text-success" : "text-fg"
                  }`}
                >
                  {pushing ? (
                    <SpinnerIcon width={13} height={13} />
                  ) : (
                    <PushIcon width={13} height={13} />
                  )}
                  {pushing ? "Push…" : "Push"}
                  {canPush && !pushing && (
                    <span className="rounded-full bg-success/15 px-1 font-mono text-[10px] font-semibold text-success">
                      ↑{sync!.ahead}
                    </span>
                  )}
                </button>
              </div>
              <StashMenu repo={repo} />
            </div>
          )}
          <button
            onClick={() => setPaletteOpen(true)}
            title="命令面板"
            aria-label="命令面板"
            className="hidden items-center gap-1.5 rounded-md border border-line-strong bg-elevated px-2 py-1 text-fg-subtle transition-colors hover:bg-overlay hover:text-fg hover:border-fg-subtle sm:flex"
          >
            <SearchIcon width={13} height={13} />
            <kbd className="font-mono text-[10px]">{modLabel}</kbd>
          </button>

          {/* 溢出菜单:次要外观/会话动作(操作日志 / 主题 / 玻璃)收纳于此,给顶栏减负。 */}
          <div className="relative">
            <button
              onClick={() => setMoreMenu((o) => !o)}
              title="更多"
              aria-label="更多"
              className="grid h-7 w-7 place-items-center rounded-md border border-line-strong bg-elevated text-fg-muted transition-colors hover:bg-overlay hover:text-fg hover:border-fg-subtle"
            >
              <MoreIcon width={15} height={15} />
            </button>
            {moreMenu && (
              <>
                <div className="fixed inset-0 z-40" onClick={() => setMoreMenu(false)} />
                <div className="absolute right-0 top-full z-50 mt-1.5 w-48 overflow-hidden rounded-md border border-line-strong bg-elevated text-xs menu-in popover">
                  {repo && (
                    <MoreItem icon={<HistoryIcon width={14} height={14} />} onClick={() => { setMoreMenu(false); setOpLogOpen(true); }}>
                      操作日志
                    </MoreItem>
                  )}
                  <MoreItem
                    icon={theme === "dark" ? <SunIcon width={14} height={14} /> : <MoonIcon width={14} height={14} />}
                    onClick={() => { toggleTheme(); setMoreMenu(false); }}
                  >
                    {theme === "dark" ? "切换到浅色主题" : "切换到暗色主题"}
                  </MoreItem>
                  <MoreItem
                    icon={<DropletIcon width={14} height={14} />}
                    onClick={() => {
                      const next = getStoredGlassPref() === "reduced" ? "auto" : "reduced";
                      setStoredGlassPref(next);
                      applyGlassMode();
                      setMoreMenu(false);
                    }}
                  >
                    {getStoredGlassPref() === "reduced" ? "开启玻璃效果" : "降低透明度"}
                  </MoreItem>
                </div>
              </>
            )}
          </div>

          <button
            onClick={pickRepo}
            className="flex items-center gap-1.5 rounded-md border border-line-strong bg-elevated px-2.5 py-1 text-xs text-fg transition-colors hover:bg-overlay hover:border-fg-subtle"
          >
            <FolderIcon width={14} height={14} />
            {repo ? "切换仓库" : "选择仓库"}
          </button>
        </div>
      </Glass>

      {/* 主体 */}
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
        <EmptyState onPick={pickRepo} lastRepo={lastRepo} onResume={setRepo} />
      )}

      {/* 底部状态栏:分支 + 仓库路径,IDE 风格 */}
      {repo && (
        <footer className="flex h-6 shrink-0 items-center gap-3 border-t border-line bg-elevated px-3 text-[11px] text-fg-muted">
          {!sync && branch && remotes.length > 0 && (
            <div className="relative">
              <button
                onClick={() => setUpMenu((o) => !o)}
                title="为当前分支设置上游分支"
                className="rounded px-1 text-fg-subtle transition-colors hover:bg-overlay hover:text-fg"
              >
                设置上游
              </button>
              {upMenu && (
                <>
                  <div className="fixed inset-0 z-40" onClick={() => setUpMenu(false)} />
                  <div className="absolute bottom-full left-0 z-50 mb-1 w-48 overflow-hidden rounded-md border border-line-strong bg-elevated text-xs menu-in popover">
                    <div className="border-b border-line px-2.5 py-1 text-[10px] uppercase tracking-wide text-fg-subtle">设为上游</div>
                    {remotes.map((r) => (
                      <button
                        key={r}
                        onClick={() => doSetUpstream(`${r}/${branch}`)}
                        className="block w-full truncate px-2.5 py-1.5 text-left font-mono text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
                      >
                        {r}/{branch}
                      </button>
                    ))}
                  </div>
                </>
              )}
            </div>
          )}
          <span className="ml-auto truncate font-mono text-fg-subtle" title={repo}>
            {repo}
          </span>
        </footer>
      )}

      {paletteOpen && <CommandPalette commands={commands} onClose={() => setPaletteOpen(false)} />}

      {repo && opLogOpen && (
        <OpLogPanel
          repo={repo}
          onClose={() => setOpLogOpen(false)}
          onJumped={() => {
            invalidateHistory(qc, repo);
            invalidateWorktree(qc, repo);
          }}
        />
      )}
    </div>
  );
}

/** Pull 方式菜单项:左侧打勾表示当前模式 */
function PullModeItem({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
    >
      <span className="grid w-3.5 shrink-0 place-items-center text-accent">
        {active ? <CheckIcon width={12} height={12} /> : null}
      </span>
      {children}
    </button>
  );
}

/** 溢出菜单项:左侧图标 + 文字,统一观感 */
function MoreItem({ icon, onClick, children }: { icon?: React.ReactNode; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      className="flex w-full items-center gap-2.5 px-2.5 py-1.5 text-left text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
    >
      {icon && <span className="grid w-4 shrink-0 place-items-center text-fg-subtle">{icon}</span>}
      <span className="flex-1">{children}</span>
    </button>
  );
}

/** 顶部不确定态进度条:非阻塞的全局加载信号(fetch 等后台操作进行时显示) */
function TopProgress() {
  return (
    <div className="pointer-events-none fixed inset-x-0 top-0 z-[70] h-0.5 overflow-hidden bg-accent/15">
      <div className="progress-bar h-full w-1/3 bg-accent" />
    </div>
  );
}

/** 顶栏左侧的小标记,纯装饰 */
function BranchMark() {
  return (
    <span className="grid h-5 w-5 place-items-center rounded bg-accent/15 text-accent">
      <svg width={13} height={13} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round">
        <circle cx="4" cy="3.5" r="1.5" />
        <circle cx="4" cy="12.5" r="1.5" />
        <circle cx="12" cy="3.5" r="1.5" />
        <path d="M4 5v6M12 5v1a3 3 0 0 1-3 3H4" />
      </svg>
    </span>
  );
}

/** 没选仓库时的启动屏 —— 每次开 app 的第一印象。
 *  双层贝塞尔玻璃品牌牌折射背后的氛围光晕,大字标题 + 磁吸 CTA,逐元素电影级入场。 */
function EmptyState({ onPick, lastRepo, onResume }: { onPick: () => void; lastRepo: string | null; onResume: (r: string) => void }) {
  const lastName = lastRepo?.replace(/[/\\]+$/, "").split(/[/\\]/).pop() ?? null;
  return (
    <div className="relative flex flex-1 items-center justify-center overflow-hidden px-8">
      {/* 签名背景:极淡缓行的「活的图谱」,垫在最底,呼应产品本体 */}
      <LaunchGraph />
      {/* 细噪点叠层:给纯数字渐变一层物理颗粒感 */}
      <div className="grain-overlay" />

      <div className="relative z-10 flex w-full max-w-md flex-col items-center text-center">
        {/* 品牌牌:外层贝塞尔托盘 + 内层液态玻璃芯,折射背后图谱与光晕(首个入场元素) */}
        <div
          className="hero-rise rounded-[28px] border border-line/70 bg-elevated/30 p-2"
          style={{ animationDelay: "0ms" }}
        >
          <Glass className="grid h-20 w-20 place-items-center rounded-[20px] text-accent">
            <svg width={34} height={34} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round">
              <circle cx="4" cy="3.5" r="1.6" />
              <circle cx="4" cy="12.5" r="1.6" />
              <circle cx="12" cy="3.5" r="1.6" />
              <path d="M4 5.1v5.8M12 5.1v1.2a3 3 0 0 1-3 3H4" />
            </svg>
          </Glass>
        </div>

        {/* 大字标题(Geist,自信字号,紧字距,平衡换行) */}
        <h1
          className="hero-rise mt-7 text-[2.5rem] font-semibold leading-[1.05] tracking-[-0.03em] text-fg text-balance"
          style={{ animationDelay: "150ms" }}
        >
          Git 客户端
        </h1>

        {/* 磁吸 CTA:全圆角药丸 + 内嵌圆形箭头(button-in-button),按压回弹 */}
        <button
          onClick={onPick}
          className="hero-rise group mt-8 flex items-center gap-3 rounded-full bg-done py-2.5 pl-6 pr-2.5 text-sm font-medium text-white shadow-[0_10px_30px_-8px_color-mix(in_oklab,var(--color-done)_60%,transparent)] transition-[transform,box-shadow,opacity] duration-500 ease-[cubic-bezier(0.32,0.72,0,1)] hover:opacity-95 active:scale-[0.98]"
          style={{ animationDelay: "280ms" }}
        >
          选择仓库
          <span className="grid h-8 w-8 place-items-center rounded-full bg-white/15 transition-transform duration-500 ease-[cubic-bezier(0.32,0.72,0,1)] group-hover:translate-x-0.5">
            <svg width={15} height={15} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round">
              <path d="M3.5 8h9M8.5 4l4 4-4 4" />
            </svg>
          </span>
        </button>

        {/* 继续上次:跳回上次打开的仓库(Linear/Things 式) */}
        {lastRepo && lastName && (
          <button
            onClick={() => onResume(lastRepo)}
            title={lastRepo}
            className="hero-rise group mt-4 flex max-w-full items-center gap-2 rounded-full border border-line bg-elevated/50 py-1.5 pl-3.5 pr-4 text-xs text-fg-muted backdrop-blur-sm transition-[transform,color,background-color] duration-500 ease-[cubic-bezier(0.32,0.72,0,1)] hover:bg-elevated hover:text-fg active:scale-[0.98]"
            style={{ animationDelay: "330ms" }}
          >
            <HistoryIcon width={13} height={13} className="shrink-0 text-fg-subtle transition-colors group-hover:text-accent" />
            <span className="shrink-0">继续上次</span>
            <span className="truncate font-mono text-fg-subtle">{lastName}</span>
          </button>
        )}

        {/* 键盘提示 */}
        <p className="hero-rise mt-5 text-xs text-fg-subtle" style={{ animationDelay: "400ms" }}>
          <kbd className="rounded border border-line-strong bg-elevated px-1.5 py-0.5 font-mono text-[11px] text-fg-muted">⌘K</kbd> 打开命令面板
        </p>
      </div>
    </div>
  );
}
