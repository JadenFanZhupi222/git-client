import { useEffect, useState } from "react";
import {
  getStatus, stageFile, unstageFile, stageHunk, unstageHunk, commit, getWorkingDiff, onRepoChanged,
  type StatusDto, type FileEntryDto, type FileDiffDto, type IpcError,
} from "../ipc";
import { RefreshIcon, CheckIcon, FileDiffIcon } from "../components/icons";
import { DiffView } from "../components/DiffView";
import { Resizer, useResizableWidth } from "../components/Resizer";
import { useToast } from "../components/Toast";

/** 工作区状态 → 颜色 + 单字母徽章 */
const STATE_STYLE: Record<string, { letter: string; cls: string }> = {
  new: { letter: "A", cls: "text-success" },
  added: { letter: "A", cls: "text-success" },
  modified: { letter: "M", cls: "text-accent" },
  deleted: { letter: "D", cls: "text-danger" },
  renamed: { letter: "R", cls: "text-warning" },
  untracked: { letter: "U", cls: "text-success" },
  conflicted: { letter: "!", cls: "text-danger" },
};
function styleFor(state: string) {
  return STATE_STYLE[state.toLowerCase()] ?? { letter: "?", cls: "text-fg-muted" };
}

export function ChangesView({ repo }: { repo: string }) {
  const [status, setStatus] = useState<StatusDto | null>(null);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [sel, setSel] = useState<{ path: string; staged: boolean } | null>(null);
  const [diff, setDiff] = useState<FileDiffDto | null>(null);
  const [diffLoading, setDiffLoading] = useState(false);
  const listCol = useResizableWidth("changes.listW", 340, 220, 680);
  const toast = useToast();

  async function refresh() {
    try { setStatus(await getStatus(repo)); }
    catch (e) { toast({ kind: "error", title: (e as IpcError).message ?? String(e) }); }
  }
  useEffect(() => { setSel(null); setDiff(null); refresh(); /* eslint-disable-next-line */ }, [repo]);

  // 文件/暂存/提交任何变化都自动刷新 status
  useEffect(() => {
    let un: (() => void) | undefined;
    onRepoChanged(() => refresh()).then((u) => { un = u; });
    return () => un?.();
    // eslint-disable-next-line
  }, [repo]);

  async function loadDiff(path: string, staged: boolean) {
    setDiffLoading(true); setDiff(null);
    try { setDiff(await getWorkingDiff(repo, path, staged)); }
    catch (e) { toast({ kind: "error", title: (e as IpcError).message ?? String(e) }); }
    finally { setDiffLoading(false); }
  }

  function selectFile(path: string, staged: boolean) {
    setSel({ path, staged });
    loadDiff(path, staged);
  }

  // status 变化后核对选中项(按 路径+暂存侧 匹配,因同一文件可同时在两侧):
  // 当前侧还在 → 不动(diff 由具体动作负责重载);本侧没了 → 跟随另一侧或清空。
  useEffect(() => {
    if (!sel || !status) return;
    const sameSide = status.entries.some((e) => e.path === sel.path && e.staged === sel.staged);
    if (sameSide) return;
    const other = status.entries.find((e) => e.path === sel.path);
    if (other) { setSel({ path: sel.path, staged: other.staged }); loadDiff(sel.path, other.staged); }
    else { setSel(null); setDiff(null); }
    // eslint-disable-next-line
  }, [status]);

  async function doHunk(path: string, staged: boolean, hunkIndex: number) {
    setBusy(true);
    try {
      if (staged) await unstageHunk(repo, path, hunkIndex);
      else await stageHunk(repo, path, hunkIndex);
      await refresh();
      await loadDiff(path, staged); // 同侧若有剩余 hunk 则刷新显示
    } catch (e) { toast({ kind: "error", title: (e as IpcError).message ?? String(e) }); }
    finally { setBusy(false); }
  }

  async function run(action: () => Promise<void>) {
    setBusy(true);
    try { await action(); await refresh(); }
    catch (e) { toast({ kind: "error", title: (e as IpcError).message ?? String(e) }); }
    finally { setBusy(false); }
  }

  async function doCommit() {
    setBusy(true);
    try {
      const sha = await commit(repo, message);
      setMessage("");
      toast({ kind: "success", title: `已提交 ${sha.slice(0, 7)}` });
      await refresh();
    } catch (e) { toast({ kind: "error", title: (e as IpcError).message ?? String(e) }); }
    finally { setBusy(false); }
  }

  const staged = status?.entries.filter((e) => e.staged) ?? [];
  const unstaged = status?.entries.filter((e) => !e.staged) ?? [];
  const canCommit = !busy && staged.length > 0 && message.trim() !== "";

  // 选中文件可做 hunk 级操作吗?未跟踪文件无 git diff hunk,排除。
  const selEntry = sel && status ? status.entries.find((e) => e.path === sel.path && e.staged === sel.staged) : undefined;
  const hunkAction = sel && selEntry && selEntry.state.toLowerCase() !== "untracked"
    ? { label: sel.staged ? "取消暂存此块" : "暂存此块", disabled: busy, onAct: (hi: number) => doHunk(sel.path, sel.staged, hi) }
    : undefined;

  const Row = ({ entry, isStaged }: { entry: FileEntryDto; isStaged: boolean }) => {
    const s = styleFor(entry.state);
    const on = sel?.path === entry.path && sel?.staged === isStaged;
    return (
      <li
        onClick={() => selectFile(entry.path, isStaged)}
        className={`group flex cursor-pointer items-center gap-2.5 px-3 py-1.5 ${on ? "bg-overlay" : "hover:bg-elevated"}`}
      >
        <span className={`w-3.5 shrink-0 text-center font-mono text-xs font-semibold ${s.cls}`}>{s.letter}</span>
        <span className="flex-1 truncate font-mono text-[13px] text-fg" title={entry.path}>{entry.path}</span>
        <button
          className="shrink-0 rounded px-2 py-0.5 text-xs text-accent opacity-0 transition-opacity hover:bg-overlay group-hover:opacity-100 disabled:opacity-40"
          disabled={busy}
          onClick={(e) => { e.stopPropagation(); run(() => (isStaged ? unstageFile(repo, entry.path) : stageFile(repo, entry.path))); }}
        >
          {isStaged ? "取消暂存" : "暂存"}
        </button>
      </li>
    );
  };

  const Section = ({ title, count, accent, children }: { title: string; count: number; accent?: boolean; children: React.ReactNode }) => (
    <div>
      <div className="sticky top-0 z-10 flex items-center gap-2 border-b border-line bg-canvas px-3 py-1.5">
        <span className="text-[11px] font-semibold uppercase tracking-wide text-fg-muted">{title}</span>
        <span className={`rounded-full px-1.5 text-[11px] tabular-nums ${accent ? "bg-done/20 text-success" : "bg-elevated text-fg-muted"}`}>{count}</span>
      </div>
      {children}
    </div>
  );

  return (
    <div className="flex h-full">
      {/* 左列:文件列表 + 提交框 */}
      <div className="flex shrink-0 flex-col overflow-hidden" style={{ width: listCol.w }}>
        {/* 工具栏 */}
        <div className="flex shrink-0 items-center gap-3 border-b border-line px-3 py-1.5">
          <button
            onClick={refresh}
            disabled={busy}
            className="flex items-center gap-1.5 rounded px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-elevated hover:text-fg disabled:opacity-40"
          >
            <RefreshIcon width={13} height={13} className={busy ? "animate-spin" : ""} /> 刷新
          </button>
        </div>

        {/* 文件区(滚动) */}
        <div className="min-h-0 flex-1 overflow-y-auto">
          <Section title="已暂存" count={staged.length} accent>
            <ul>
              {staged.map((e) => <Row key={e.path} entry={e} isStaged />)}
              {staged.length === 0 && <li className="px-3 py-2 text-xs text-fg-subtle">暂无已暂存的改动</li>}
            </ul>
          </Section>
          <Section title="未暂存" count={unstaged.length}>
            <ul>
              {unstaged.map((e) => <Row key={e.path} entry={e} isStaged={false} />)}
              {unstaged.length === 0 && <li className="px-3 py-2 text-xs text-fg-subtle">工作区干净</li>}
            </ul>
          </Section>
        </div>

        {/* 提交框(固定底部) */}
        <div className="shrink-0 border-t border-line p-3">
          <textarea
            className="w-full resize-none rounded-md border border-line bg-canvas p-2.5 text-sm text-fg placeholder:text-fg-subtle focus:border-accent focus:outline-none"
            rows={3}
            placeholder="提交信息…  (⌘/Ctrl+Enter 提交)"
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            onKeyDown={(e) => {
              if ((e.metaKey || e.ctrlKey) && e.key === "Enter" && canCommit) {
                e.preventDefault();
                doCommit();
              }
            }}
          />
          <div className="mt-2 flex items-center justify-between">
            <span className="text-xs text-fg-subtle">
              {staged.length > 0 ? `${staged.length} 个改动待提交` : "暂存改动后可提交"}
            </span>
            <button
              className="flex items-center gap-1.5 rounded-md bg-done px-3.5 py-1.5 text-sm font-medium text-white transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40"
              disabled={!canCommit}
              onClick={doCommit}
            >
              <CheckIcon width={14} height={14} /> 提交
            </button>
          </div>
        </div>
      </div>

      <Resizer onDown={listCol.onDown} />

      {/* 右列:选中文件的工作区 diff */}
      <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <div className="flex shrink-0 items-center gap-1.5 border-b border-line px-3 py-2 text-[11px] font-semibold uppercase tracking-wide text-fg-muted">
          <FileDiffIcon width={13} height={13} />
          {sel ? (
            <span className="truncate font-mono normal-case tracking-normal text-fg" title={sel.path}>
              {sel.path}
              <span className="ml-1.5 text-fg-subtle">{sel.staged ? "(已暂存)" : "(未暂存)"}</span>
            </span>
          ) : "Diff"}
        </div>
        <DiffView diff={diff} loading={diffLoading} hasFile={!!sel} hunkAction={hunkAction} />
      </main>
    </div>
  );
}
