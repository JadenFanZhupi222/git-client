import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { TabBar, type Tab } from "./components/TabBar";
import { ChangesView } from "./views/ChangesView";
import { HistoryView } from "./views/HistoryView";
import { getCurrentBranch, getAheadBehind, watchRepo, onRepoChanged, fetchRemote, pullRemote, pushRemote, type AheadBehindDto, type IpcError } from "./ipc";
import { FolderIcon, SunIcon, MoonIcon, FetchIcon, PullIcon, PushIcon, SpinnerIcon } from "./components/icons";
import { BranchSwitcher } from "./components/BranchSwitcher";
import { SyncBadge } from "./components/SyncBadge";
import { useToast } from "./components/Toast";
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
  const [branch, setBranch] = useState<string | null>(null);
  const [sync, setSync] = useState<AheadBehindDto | null>(null);
  const [theme, setTheme] = useState<Theme>(getStoredTheme);
  const [fetching, setFetching] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [pushing, setPushing] = useState(false);
  const toast = useToast();
  const busy = fetching || pulling || pushing;
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
      const r = await fetchRemote(repo);
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
      getAheadBehind(repo).then(setSync).catch(() => {}); // 兜底:packed-refs 时 watcher 可能不触发
    }
  }

  async function doPull() {
    if (!repo) return;
    setPulling(true);
    try {
      const r = await pullRemote(repo);
      // 成功后工作区/HEAD 变化触发文件监听 → 图谱自动前进。
      const upToDate = /up to date|已是最新/i.test(r.summary);
      toast({ kind: "success", title: upToDate ? "已是最新" : "已拉取并合并" });
    } catch (e) {
      // 冲突时工作区已留下冲突标记,可到「更改」页查看冲突文件。
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setPulling(false);
      getAheadBehind(repo).then(setSync).catch(() => {});
    }
  }

  async function doPush() {
    if (!repo) return;
    setPushing(true);
    try {
      const r = await pushRemote(repo);
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
      getAheadBehind(repo).then(setSync).catch(() => {});
    }
  }

  async function pickRepo() {
    const dir = await open({ directory: true, title: "选择一个 git 仓库" });
    if (typeof dir === "string") setRepo(dir);
  }

  // 仓库变化时:刷新底栏分支名 + ahead/behind、启动文件监听、订阅变化事件
  useEffect(() => {
    if (!repo) { setBranch(null); setSync(null); return; }
    const loadInfo = () => {
      getCurrentBranch(repo).then(setBranch).catch(() => setBranch(null));
      getAheadBehind(repo).then(setSync).catch(() => setSync(null));
    };
    loadInfo();
    watchRepo(repo).catch(() => {});
    let un: (() => void) | undefined;
    // ref 变化(提交/切分支/fetch/pull/push 后远程跟踪变动)→ 重算分支与同步状态
    onRepoChanged((kind) => { if (kind === "ref") loadInfo(); }).then((u) => { un = u; });
    return () => un?.();
  }, [repo]);

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
              <button
                onClick={doPull}
                disabled={busy}
                title={canPull ? `落后上游 ${sync!.behind} 个提交,建议 Pull` : "Pull(拉取并合并到当前分支)"}
                className={`flex items-center gap-1.5 rounded-md border bg-elevated px-2.5 py-1 text-xs text-fg transition-colors hover:bg-overlay hover:border-fg-subtle disabled:opacity-50 ${
                  canPull ? "border-accent/60 ring-1 ring-accent/40" : "border-line-strong"
                }`}
              >
                {pulling ? (
                  <SpinnerIcon width={13} height={13} />
                ) : (
                  <PullIcon width={13} height={13} />
                )}
                {pulling ? "Pull…" : "Pull"}
                {canPull && !pulling && (
                  <span className="rounded-full bg-accent/15 px-1 font-mono text-[10px] font-semibold text-accent">
                    ↓{sync!.behind}
                  </span>
                )}
              </button>
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

      {repo && <TabBar active={tab} onChange={setTab} />}

      {/* 主体 */}
      {repo ? (
        <div className="min-h-0 flex-1">
          {tab === "changes" ? <ChangesView repo={repo} /> : <HistoryView repo={repo} />}
        </div>
      ) : (
        <EmptyState onPick={pickRepo} />
      )}

      {/* 底部状态栏:分支 + 仓库路径,IDE 风格 */}
      {repo && (
        <footer className="flex h-6 shrink-0 items-center gap-3 border-t border-line bg-elevated px-3 text-[11px] text-fg-muted">
          <BranchSwitcher repo={repo} branch={branch} onSwitched={setBranch} />
          <SyncBadge sync={sync} />
          <span className="ml-auto truncate font-mono text-fg-subtle" title={repo}>
            {repo}
          </span>
        </footer>
      )}
    </div>
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
      <button
        onClick={onPick}
        className="rounded-md bg-done px-4 py-2 text-sm font-medium text-white transition-opacity hover:opacity-90"
      >
        选择仓库
      </button>
    </div>
  );
}
