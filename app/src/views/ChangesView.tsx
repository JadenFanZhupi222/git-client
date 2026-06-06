import { useEffect, useState } from "react";
import { getStatus, stageFile, unstageFile, commit, onRepoChanged, type StatusDto, type FileEntryDto, type IpcError } from "../ipc";
import { RefreshIcon, CheckIcon } from "../components/icons";

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
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    setError(null);
    try { setStatus(await getStatus(repo)); }
    catch (e) { setError((e as IpcError).message ?? String(e)); }
  }
  useEffect(() => { refresh(); /* eslint-disable-next-line */ }, [repo]);

  // 文件/暂存/提交任何变化都自动刷新 status
  useEffect(() => {
    let un: (() => void) | undefined;
    onRepoChanged(() => refresh()).then((u) => { un = u; });
    return () => un?.();
    // eslint-disable-next-line
  }, [repo]);

  async function run(action: () => Promise<void>) {
    setBusy(true); setError(null);
    try { await action(); await refresh(); }
    catch (e) { setError((e as IpcError).message ?? String(e)); }
    finally { setBusy(false); }
  }

  async function doCommit() {
    setBusy(true); setError(null);
    try {
      const sha = await commit(repo, message);
      setInfo(`已提交 ${sha.slice(0, 7)}`); setMessage(""); await refresh();
    } catch (e) { setError((e as IpcError).message ?? String(e)); }
    finally { setBusy(false); }
  }

  const staged = status?.entries.filter((e) => e.staged) ?? [];
  const unstaged = status?.entries.filter((e) => !e.staged) ?? [];
  const canCommit = !busy && staged.length > 0 && message.trim() !== "";

  const Row = ({ entry, isStaged }: { entry: FileEntryDto; isStaged: boolean }) => {
    const s = styleFor(entry.state);
    return (
      <li className="group flex items-center gap-2.5 px-3 py-1.5 hover:bg-elevated">
        <span className={`w-3.5 shrink-0 text-center font-mono text-xs font-semibold ${s.cls}`}>{s.letter}</span>
        <span className="flex-1 truncate font-mono text-[13px] text-fg" title={entry.path}>{entry.path}</span>
        <button
          className="shrink-0 rounded px-2 py-0.5 text-xs text-accent opacity-0 transition-opacity hover:bg-overlay group-hover:opacity-100 disabled:opacity-40"
          disabled={busy}
          onClick={() => run(() => (isStaged ? unstageFile(repo, entry.path) : stageFile(repo, entry.path)))}
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
    <div className="flex h-full flex-col">
      {/* 工具栏 */}
      <div className="flex shrink-0 items-center gap-3 border-b border-line px-3 py-1.5">
        <button
          onClick={refresh}
          disabled={busy}
          className="flex items-center gap-1.5 rounded px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-elevated hover:text-fg disabled:opacity-40"
        >
          <RefreshIcon width={13} height={13} className={busy ? "animate-spin" : ""} /> 刷新
        </button>
        {error && <span className="truncate text-xs text-danger" title={error}>错误:{error}</span>}
        {info && !error && <span className="flex items-center gap-1 text-xs text-success"><CheckIcon width={13} height={13} />{info}</span>}
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
  );
}
