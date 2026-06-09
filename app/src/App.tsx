import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { TabBar, type Tab } from "./components/TabBar";
import { ChangesView } from "./views/ChangesView";
import { HistoryView } from "./views/HistoryView";
import { CompareView } from "./views/CompareView";
import { BlameView } from "./views/BlameView";
import { SubmodulesView } from "./views/SubmodulesView";
import { WorktreesView } from "./views/WorktreesView";
import { useQueryClient } from "@tanstack/react-query";
import { setUpstream, fetchRemote, pullRemote, pushRemote, undo, redo, checkoutBranch, type IpcError } from "./ipc";
import { FolderIcon, SunIcon, MoonIcon, FetchIcon, PullIcon, PushIcon, SpinnerIcon, ChevronDownIcon, CheckIcon, UndoIcon, RedoIcon, HistoryIcon, SearchIcon } from "./components/icons";
import { BranchSwitcher } from "./components/BranchSwitcher";
import { SyncBadge } from "./components/SyncBadge";
import { StashMenu } from "./components/StashMenu";
import { OpLogPanel } from "./components/OpLogPanel";
import { CommandPalette } from "./components/CommandPalette";
import type { Command } from "./lib/commands";
import { useToast } from "./components/Toast";
import { Button } from "./components/ui/Button";
import { useRepoWatch, useCurrentBranch, useAheadBehind, useRemotes, useUndoState, useBranches, useSubmodules, useWorktrees, invalidateHistory, invalidateWorktree, qk } from "./lib/queries";
import { applyTheme, getStoredTheme, type Theme } from "./lib/theme";

/** 把 git fetch 的原始摘要提炼成简洁细节:优先取 "->" 更新行。 */
function fetchDetail(summary: string): string | undefined {
  if (summary === "已是最新") return undefined;
  const lines = summary.split("\n").map((l) => l.trim()).filter(Boolean);
  const updates = lines.filter((l) => l.includes("->"));
  return (updates.length ? updates : lines.slice(0, 1)).join("\n") || undefined;
}

export default function App() {
  const [repo, setRepo] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("changes");
  const [theme, setTheme] = useState<Theme>(getStoredTheme);
  const [fetching, setFetching] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [pullRebase, setPullRebase] = useState(() => localStorage.getItem("pull.rebase") === "1");
  const [pullMenu, setPullMenu] = useState(false);
  const [selectedRemote, setSelectedRemote] = useState<string | null>(null);
  const [remoteMenu, setRemoteMenu] = useState(false);
  const [upMenu, setUpMenu] = useState(false);
  const [undoing, setUndoing] = useState(false);
  const [opLogOpen, setOpLogOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
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

  // 动态标签(子模块/工作树)随仓库出现/消失;切到不再可用的标签时退回「更改」,免得停在空标签。
  useEffect(() => {
    if ((tab === "submodules" && !hasSubmodules) || (tab === "worktrees" && !hasWorktrees)) setTab("changes");
  }, [tab, hasSubmodules, hasWorktrees]);

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
  commands.push(
    { id: "remote:fetch", title: "Fetch", subtitle: "从远程拉取更新", group: "远程", keywords: "拉取 远程 fetch", disabled: !repo || busy, run: doFetch },
    { id: "remote:pull-merge", title: "Pull · 合并", group: "远程", keywords: "拉取 合并 merge pull", disabled: !repo || busy, run: () => doPull(false) },
    { id: "remote:pull-rebase", title: "Pull · 变基", group: "远程", keywords: "拉取 变基 rebase pull", disabled: !repo || busy, run: () => doPull(true) },
    { id: "remote:push", title: "Push", subtitle: "推送当前分支", group: "远程", keywords: "推送 push", disabled: !repo || busy, run: doPush },
  );
  commands.push(
    { id: "undo", title: canUndo ? `撤销:${canUndo.label}` : "撤销", group: "撤销", keywords: "undo 撤销 回退", disabled: !repo || busy || !canUndo, run: () => doNav("undo") },
    { id: "redo", title: canRedo ? `重做:${canRedo.label}` : "重做", group: "撤销", keywords: "redo 重做 前进", disabled: !repo || busy || !canRedo, run: () => doNav("redo") },
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
    <div className="flex h-screen flex-col bg-canvas text-fg">
      {busy && <TopProgress />}
      {/* 顶栏:轻、紧凑、左标题右仓库 */}
      <header className="flex h-11 shrink-0 items-center gap-3 border-b border-line px-3">
        <div className="flex items-center gap-2 font-semibold">
          <BranchMark />
          <span className="text-sm">Git 客户端</span>
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
                      <div className="absolute right-0 top-full z-50 mt-1 w-44 overflow-hidden rounded-md border border-line-strong bg-elevated text-xs shadow-lg">
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
              {canUndo && (
                <button
                  onClick={() => doNav("undo")}
                  disabled={busy}
                  title={`撤销刚才的「${canUndo.label}」(回到 ${canUndo.target_short};${canUndo.worktree_restored ? "还原工作区,有未提交改动会先拦下" : "改动回暂存区,不丢工作区"})`}
                  className="flex items-center gap-1.5 rounded-md border border-accent/60 bg-accent/10 px-2.5 py-1 text-xs text-accent transition-colors hover:bg-accent/20 disabled:opacity-50"
                >
                  {undoing ? <SpinnerIcon width={13} height={13} /> : <UndoIcon width={13} height={13} />}
                  {`撤销${canUndo.label}`}
                </button>
              )}
              {canRedo && (
                <button
                  onClick={() => doNav("redo")}
                  disabled={busy}
                  title={`重做「${canRedo.label}」(前进到 ${canRedo.target_short})`}
                  className="flex items-center gap-1.5 rounded-md border border-line-strong bg-elevated px-2.5 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg hover:border-fg-subtle disabled:opacity-50"
                >
                  <RedoIcon width={13} height={13} />
                  {`重做${canRedo.label}`}
                </button>
              )}
              <button
                onClick={() => setOpLogOpen(true)}
                title="操作日志(本会话写操作时间线,可点回跳)"
                aria-label="操作日志"
                className="grid h-7 w-7 place-items-center rounded-md border border-line-strong bg-elevated text-fg-muted transition-colors hover:bg-overlay hover:text-fg hover:border-fg-subtle"
              >
                <HistoryIcon width={14} height={14} />
              </button>
              <button
                onClick={doFetch}
                disabled={busy}
                title="Fetch(从远程拉取更新,不改工作区)"
                className="flex items-center gap-1.5 rounded-md border border-line-strong bg-elevated px-2.5 py-1 text-xs text-fg transition-colors hover:bg-overlay hover:border-fg-subtle disabled:opacity-50"
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
                  title={canPull ? `落后上游 ${sync!.behind} 个提交,建议 Pull` : `Pull(${pullRebase ? "变基" : "合并"}到当前分支)`}
                  className={`flex items-center gap-1.5 rounded-l-md border bg-elevated px-2.5 py-1 text-xs text-fg transition-colors hover:bg-overlay hover:border-fg-subtle disabled:opacity-50 ${
                    canPull ? "border-accent/60" : "border-line-strong"
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
                  className={`grid place-items-center rounded-r-md border border-l-0 bg-elevated px-1 text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50 ${
                    canPull ? "border-accent/60" : "border-line-strong"
                  }`}
                >
                  <ChevronDownIcon width={11} height={11} />
                </button>
                {pullMenu && (
                  <>
                    <div className="fixed inset-0 z-40" onClick={() => setPullMenu(false)} />
                    <div className="absolute right-0 top-full z-50 mt-1 w-44 overflow-hidden rounded-md border border-line-strong bg-elevated text-xs shadow-lg">
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
                title={canPush ? `领先上游 ${sync!.ahead} 个提交,建议 Push` : "Push(把当前分支推到远程;首次自动建立上游)"}
                className={`flex items-center gap-1.5 rounded-md border bg-elevated px-2.5 py-1 text-xs text-fg transition-colors hover:bg-overlay hover:border-fg-subtle disabled:opacity-50 ${
                  canPush ? "border-success/60 ring-1 ring-success/40" : "border-line-strong"
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
              <StashMenu repo={repo} />
            </div>
          )}
          {repo && (
            <span
              title={repo}
              className="max-w-[16rem] truncate rounded-md bg-elevated px-2 py-1 font-mono text-xs text-fg-muted"
            >
              {repoName}
            </span>
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
          <button
            onClick={toggleTheme}
            title={theme === "dark" ? "切换到浅色" : "切换到暗色"}
            aria-label="切换主题"
            className="grid h-7 w-7 place-items-center rounded-md border border-line-strong bg-elevated text-fg-muted transition-colors hover:bg-overlay hover:text-fg hover:border-fg-subtle"
          >
            {theme === "dark" ? <SunIcon width={14} height={14} /> : <MoonIcon width={14} height={14} />}
          </button>
          <button
            onClick={pickRepo}
            className="flex items-center gap-1.5 rounded-md border border-line-strong bg-elevated px-2.5 py-1 text-xs text-fg transition-colors hover:bg-overlay hover:border-fg-subtle"
          >
            <FolderIcon width={14} height={14} />
            {repo ? "切换仓库" : "选择仓库"}
          </button>
        </div>
      </header>

      {repo && <TabBar active={tab} onChange={setTab} hasSubmodules={hasSubmodules} hasWorktrees={hasWorktrees} />}

      {/* 主体 */}
      {repo ? (
        <div className="min-h-0 flex-1">
          {tab === "changes" ? <ChangesView repo={repo} /> : tab === "history" ? <HistoryView repo={repo} /> : tab === "compare" ? <CompareView repo={repo} /> : tab === "submodules" ? <SubmodulesView repo={repo} /> : tab === "worktrees" ? <WorktreesView repo={repo} /> : <BlameView repo={repo} />}
        </div>
      ) : (
        <EmptyState onPick={pickRepo} />
      )}

      {/* 底部状态栏:分支 + 仓库路径,IDE 风格 */}
      {repo && (
        <footer className="flex h-6 shrink-0 items-center gap-3 border-t border-line bg-elevated px-3 text-[11px] text-fg-muted">
          <BranchSwitcher repo={repo} branch={branch} />
          <SyncBadge sync={sync} />
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
                  <div className="absolute bottom-full left-0 z-50 mb-1 w-48 overflow-hidden rounded-md border border-line-strong bg-elevated text-xs shadow-lg">
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

/** 没选仓库时的引导空态 */
function EmptyState({ onPick }: { onPick: () => void }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-4 text-center">
      <div className="grid h-16 w-16 place-items-center rounded-2xl bg-elevated text-fg-subtle">
        <FolderIcon width={30} height={30} />
      </div>
      <div>
        <p className="text-base font-medium text-fg">还没有打开仓库</p>
        <p className="mt-1 text-sm text-fg-muted">选择一个本地 git 仓库开始工作。</p>
      </div>
      <Button variant="commit" size="md" onClick={onPick}>
        选择仓库
      </Button>
    </div>
  );
}
