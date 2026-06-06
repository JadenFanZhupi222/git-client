import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { TabBar, type Tab } from "./components/TabBar";
import { ChangesView } from "./views/ChangesView";
import { HistoryView } from "./views/HistoryView";
import { getCurrentBranch, watchRepo, onRepoChanged } from "./ipc";
import { FolderIcon, BranchIcon } from "./components/icons";

export default function App() {
  const [repo, setRepo] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("changes");
  const [branch, setBranch] = useState<string | null>(null);

  async function pickRepo() {
    const dir = await open({ directory: true, title: "选择一个 git 仓库" });
    if (typeof dir === "string") setRepo(dir);
  }

  // 仓库变化时:刷新底栏分支名、启动文件监听、订阅变化事件
  useEffect(() => {
    if (!repo) { setBranch(null); return; }
    const loadBranch = () => getCurrentBranch(repo).then(setBranch).catch(() => setBranch(null));
    loadBranch();
    watchRepo(repo).catch(() => {});
    let un: (() => void) | undefined;
    onRepoChanged((kind) => { if (kind === "ref") loadBranch(); }).then((u) => { un = u; });
    return () => un?.();
  }, [repo]);

  // 仓库路径只显示尾部目录名,完整路径放 title 悬浮
  const repoName = repo?.replace(/[/\\]+$/, "").split(/[/\\]/).pop() ?? null;

  return (
    <div className="flex h-screen flex-col bg-canvas text-fg">
      {/* 顶栏:轻、紧凑、左标题右仓库 */}
      <header className="flex h-11 shrink-0 items-center gap-3 border-b border-line px-3">
        <div className="flex items-center gap-2 font-semibold">
          <BranchMark />
          <span className="text-sm">Git 客户端</span>
        </div>

        <div className="ml-auto flex items-center gap-2">
          {repo && (
            <span
              title={repo}
              className="max-w-[16rem] truncate rounded-md bg-elevated px-2 py-1 font-mono text-xs text-fg-muted"
            >
              {repoName}
            </span>
          )}
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
          <span className="flex items-center gap-1 text-accent">
            <BranchIcon width={12} height={12} />
            {branch ?? "—"}
          </span>
          <span className="ml-auto truncate font-mono text-fg-subtle" title={repo}>
            {repo}
          </span>
        </footer>
      )}
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
