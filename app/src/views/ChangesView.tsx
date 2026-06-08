import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  stageFile, unstageFile, stageHunk, unstageHunk, stageLines, commit, amendCommit,
  getHeadCommit, resolveOurs, resolveTheirs,
  type FileEntryDto, type IpcError,
} from "../ipc";
import { useStatus, useWorkingDiff, useRepoState, invalidateWorktree, invalidateHistory, qk } from "../lib/queries";
import { useListKeyboardNav, isTypingTarget } from "../lib/listNav";
import { RefreshIcon, CheckIcon, FileDiffIcon, PlusIcon, MinusIcon } from "../components/icons";
import { DiffView } from "../components/DiffView";
import { ConflictEditor } from "../components/ConflictEditor";
import { ConflictBanner } from "../components/ConflictBanner";
import { Resizer, useResizableWidth } from "../components/Resizer";
import { useToast } from "../components/Toast";
import { Button } from "../components/ui/Button";

/** 工作区状态 → 颜色 + 单字母徽章 + 中文名(tooltip) */
const STATE_STYLE: Record<string, { letter: string; cls: string; label: string }> = {
  new: { letter: "A", cls: "text-success", label: "新增" },
  added: { letter: "A", cls: "text-success", label: "新增" },
  modified: { letter: "M", cls: "text-accent", label: "修改" },
  deleted: { letter: "D", cls: "text-danger", label: "删除" },
  renamed: { letter: "R", cls: "text-warning", label: "重命名" },
  untracked: { letter: "U", cls: "text-success", label: "未跟踪" },
  conflicted: { letter: "!", cls: "text-danger", label: "冲突" },
};
function styleFor(state: string) {
  return STATE_STYLE[state.toLowerCase()] ?? { letter: "?", cls: "text-fg-muted", label: state };
}

/** 文件路径拆成 目录(灰)+ 文件名(亮),便于扫读 */
function splitPath(path: string) {
  const i = path.lastIndexOf("/");
  return { dir: i >= 0 ? path.slice(0, i + 1) : "", name: i >= 0 ? path.slice(i + 1) : path };
}

export function ChangesView({ repo }: { repo: string }) {
  const qc = useQueryClient();
  const statusQ = useStatus(repo);
  const status = statusQ.data;
  const repoState = useRepoState(repo).data ?? "clean";
  const [message, setMessage] = useState("");
  const [amend, setAmend] = useState(false);
  const [busy, setBusy] = useState(false);
  const [sel, setSel] = useState<{ path: string; staged: boolean } | null>(null);
  const listCol = useResizableWidth("changes.listW", 340, 220, 680);
  const listScrollRef = useRef<HTMLDivElement>(null);
  const toast = useToast();

  // 选中文件的工作区 diff(随 sel 与失效自动重取)
  const diffQ = useWorkingDiff(repo, sel?.path ?? null, sel?.staged ?? false);

  const entries = status?.entries ?? [];
  const conflicts = entries.filter((e) => e.state.toLowerCase() === "conflicted");
  const staged = entries.filter((e) => e.staged);
  const unstaged = entries.filter((e) => !e.staged && e.state.toLowerCase() !== "conflicted");
  // 普通提交需有暂存;amend 可只改信息(允许无暂存)。两者都禁在冲突中、需有信息。
  const canCommit =
    !busy && conflicts.length === 0 && message.trim() !== "" && (amend || staged.length > 0);

  // 扁平的可键盘选择列表(按显示顺序:冲突 → 已暂存 → 未暂存),供 j/k 导航与空格暂存。
  const flatList = [
    ...conflicts.map((e) => ({ path: e.path, staged: false, conflict: true })),
    ...staged.map((e) => ({ path: e.path, staged: true, conflict: false })),
    ...unstaged.map((e) => ({ path: e.path, staged: false, conflict: false })),
  ];
  const flatIndex = sel ? flatList.findIndex((x) => x.path === sel.path && x.staged === sel.staged) : -1;
  useListKeyboardNav({
    count: flatList.length,
    index: flatIndex,
    onSelect: (i) => setSel({ path: flatList[i].path, staged: flatList[i].staged }),
  });

  // 空格:暂存/取消暂存当前选中文件(冲突文件除外——它要显式选我方/对方/已解决)。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== " " || e.metaKey || e.ctrlKey || e.altKey) return;
      if (isTypingTarget(document.activeElement)) return;
      if (!sel || busy) return;
      const item = flatList.find((x) => x.path === sel.path && x.staged === sel.staged);
      if (!item || item.conflict) return;
      e.preventDefault();
      run(() => (item.staged ? unstageFile(repo, item.path) : stageFile(repo, item.path)));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sel, busy, flatList, repo]);

  // 键盘选中变化 → 把该行滚进可视区(已可见则不动)。
  useEffect(() => {
    if (!sel) return;
    listScrollRef.current
      ?.querySelector<HTMLElement>(`[data-fkey="${CSS.escape(`${sel.path}|${sel.staged}`)}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  // 切仓库清空选择与 amend
  useEffect(() => { setSel(null); setAmend(false); }, [repo]);

  // 勾选 amend 且信息为空时,预填上次提交信息(方便在其基础上改)
  async function toggleAmend(next: boolean) {
    setAmend(next);
    if (next && message.trim() === "") {
      try {
        const head = await getHeadCommit(repo);
        const full = head.body ? `${head.summary}\n\n${head.body}` : head.summary;
        setMessage(full);
      } catch { /* 空仓库等:忽略,留空 */ }
    }
  }

  // status 变化后核对选中项(按 路径+暂存侧 匹配,同一文件可同时在两侧):
  // 当前侧还在 → 不动;本侧没了 → 跟随另一侧或清空。diff 由 query 自动重取。
  useEffect(() => {
    if (!sel || !status) return;
    const sameSide = status.entries.some((e) => e.path === sel.path && e.staged === sel.staged);
    if (sameSide) return;
    const other = status.entries.find((e) => e.path === sel.path);
    setSel(other ? { path: sel.path, staged: other.staged } : null);
    // eslint-disable-next-line
  }, [status]);

  async function run(action: () => Promise<void>) {
    setBusy(true);
    try { await action(); invalidateWorktree(qc, repo); }
    catch (e) { toast({ kind: "error", title: (e as IpcError).message ?? String(e) }); }
    finally { setBusy(false); }
  }

  // 批量暂存 / 取消暂存(顺序执行,避免并发写 index 冲突)
  const stageAll = (paths: string[]) => run(async () => { for (const p of paths) await stageFile(repo, p); });
  const unstageAll = (paths: string[]) => run(async () => { for (const p of paths) await unstageFile(repo, p); });
  const doHunk = (path: string, isStaged: boolean, hunkIndex: number) =>
    run(() => (isStaged ? unstageHunk(repo, path, hunkIndex) : stageHunk(repo, path, hunkIndex)));

  // 冲突解决:采用我方/对方(整文件),或手改后标记已解决(= 暂存)
  const useOurs = (path: string) => run(() => resolveOurs(repo, path));
  const useTheirs = (path: string) => run(() => resolveTheirs(repo, path));

  async function doCommit() {
    setBusy(true);
    try {
      const sha = amend ? await amendCommit(repo, message) : await commit(repo, message);
      setMessage("");
      setAmend(false);
      toast({ kind: "success", title: `${amend ? "已修订" : "已提交"} ${sha.slice(0, 7)}` });
      invalidateWorktree(qc, repo);
      invalidateHistory(qc, repo);
    } catch (e) { toast({ kind: "error", title: (e as IpcError).message ?? String(e) }); }
    finally { setBusy(false); }
  }

  // 选中文件可做 hunk 级操作吗?未跟踪 / 冲突文件无 git diff hunk,排除。
  const selEntry = sel && status ? status.entries.find((e) => e.path === sel.path && e.staged === sel.staged) : undefined;
  const isConflict = selEntry?.state.toLowerCase() === "conflicted";
  const hunkAction = sel && selEntry && !isConflict && selEntry.state.toLowerCase() !== "untracked"
    ? { label: sel.staged ? "取消暂存此块" : "暂存此块", disabled: busy, onAct: (hi: number) => doHunk(sel.path, sel.staged, hi) }
    : undefined;
  // 行级暂存仅未暂存侧、可 diff 的跟踪文件
  const lineStage = sel && !sel.staged && selEntry && !isConflict && selEntry.state.toLowerCase() !== "untracked"
    ? { disabled: busy, onStage: (hi: number, lines: number[]) => run(() => stageLines(repo, sel.path, hi, lines)) }
    : undefined;

  const Row = ({ entry, isStaged }: { entry: FileEntryDto; isStaged: boolean }) => {
    const s = styleFor(entry.state);
    const on = sel?.path === entry.path && sel?.staged === isStaged;
    const { dir, name } = splitPath(entry.path);
    return (
      <li
        data-fkey={`${entry.path}|${isStaged}`}
        onClick={() => setSel({ path: entry.path, staged: isStaged })}
        className={`group flex cursor-pointer items-center gap-2 py-1 pl-3 pr-1.5 ${on ? "bg-overlay" : "hover:bg-elevated"}`}
      >
        <span className={`w-3.5 shrink-0 text-center font-mono text-xs font-semibold ${s.cls}`} title={s.label}>{s.letter}</span>
        <span className="min-w-0 flex-1 truncate font-mono text-[13px]" title={entry.path}>
          {dir && <span className="text-fg-subtle">{dir}</span>}
          <span className="text-fg">{name}</span>
        </span>
        <button
          title={isStaged ? "取消暂存" : "暂存"}
          disabled={busy}
          onClick={(e) => { e.stopPropagation(); run(() => (isStaged ? unstageFile(repo, entry.path) : stageFile(repo, entry.path))); }}
          className="grid h-5 w-5 shrink-0 place-items-center rounded text-fg-subtle opacity-0 transition-colors hover:bg-overlay hover:text-fg group-hover:opacity-100 disabled:opacity-40"
        >
          {isStaged ? <MinusIcon width={13} height={13} /> : <PlusIcon width={13} height={13} />}
        </button>
      </li>
    );
  };

  const ConflictRow = ({ entry }: { entry: FileEntryDto }) => {
    const on = sel?.path === entry.path && sel?.staged === false;
    const { dir, name } = splitPath(entry.path);
    return (
      <li
        data-fkey={`${entry.path}|false`}
        onClick={() => setSel({ path: entry.path, staged: false })}
        className={`group flex cursor-pointer items-center gap-2 py-1 pl-3 pr-1.5 ${on ? "bg-overlay" : "hover:bg-elevated"}`}
      >
        <span className="w-3.5 shrink-0 text-center font-mono text-xs font-semibold text-danger" title="冲突">!</span>
        <span className="min-w-0 flex-1 truncate font-mono text-[13px]" title={entry.path}>
          {dir && <span className="text-fg-subtle">{dir}</span>}
          <span className="text-fg">{name}</span>
        </span>
        <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
          <button onClick={(e) => { e.stopPropagation(); useOurs(entry.path); }} disabled={busy}
            title="采用我方(ours)" className="rounded px-1.5 py-0.5 text-[11px] text-accent hover:bg-overlay disabled:opacity-40">我方</button>
          <button onClick={(e) => { e.stopPropagation(); useTheirs(entry.path); }} disabled={busy}
            title="采用对方(theirs)" className="rounded px-1.5 py-0.5 text-[11px] text-accent hover:bg-overlay disabled:opacity-40">对方</button>
          <button onClick={(e) => { e.stopPropagation(); run(() => stageFile(repo, entry.path)); }} disabled={busy}
            title="标记已解决(手动编辑后)" className="rounded px-1.5 py-0.5 text-[11px] text-success hover:bg-overlay disabled:opacity-40">已解决</button>
        </div>
      </li>
    );
  };

  const Section = ({ title, count, accent, onBulk, bulkLabel, bulkIcon, children }: {
    title: string; count: number; accent?: boolean;
    onBulk?: () => void; bulkLabel?: string; bulkIcon?: React.ReactNode;
    children: React.ReactNode;
  }) => (
    <div>
      <div className="group sticky top-0 z-10 flex items-center gap-2 border-b border-line bg-canvas px-3 py-1.5">
        <span className="text-[11px] font-semibold uppercase tracking-wide text-fg-muted">{title}</span>
        <span className={`rounded-full px-1.5 text-[11px] tabular-nums ${accent ? "bg-done/20 text-success" : "bg-elevated text-fg-muted"}`}>{count}</span>
        {count > 0 && onBulk && (
          <button
            onClick={onBulk}
            disabled={busy}
            title={bulkLabel}
            className="ml-auto flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-fg-subtle opacity-0 transition-colors hover:bg-overlay hover:text-fg group-hover:opacity-100 disabled:opacity-40"
          >
            {bulkIcon}{bulkLabel}
          </button>
        )}
      </div>
      {children}
    </div>
  );

  return (
    <div className="flex h-full flex-col">
      {repoState !== "clean" && (
        <ConflictBanner repo={repo} state={repoState} conflicts={conflicts.length} />
      )}
      <div className="flex min-h-0 flex-1">
      {/* 左列:文件列表 + 提交框 */}
      <div className="flex shrink-0 flex-col overflow-hidden" style={{ width: listCol.w }}>
        {/* 工具栏 */}
        <div className="flex shrink-0 items-center gap-3 border-b border-line px-3 py-1.5">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => qc.invalidateQueries({ queryKey: qk.status(repo) })}
            disabled={busy || statusQ.isFetching}
          >
            <RefreshIcon width={13} height={13} className={busy || statusQ.isFetching ? "animate-spin" : ""} /> 刷新
          </Button>
        </div>

        {/* 文件区(滚动) */}
        <div ref={listScrollRef} className="min-h-0 flex-1 overflow-y-auto">
          {conflicts.length > 0 && (
            <Section title="冲突" count={conflicts.length}>
              <ul>
                {conflicts.map((e) => <ConflictRow key={e.path} entry={e} />)}
              </ul>
            </Section>
          )}
          <Section
            title="已暂存" count={staged.length} accent
            onBulk={() => unstageAll(staged.map((e) => e.path))}
            bulkLabel="全部取消暂存"
            bulkIcon={<MinusIcon width={12} height={12} />}
          >
            <ul>
              {staged.map((e) => <Row key={e.path} entry={e} isStaged />)}
              {staged.length === 0 && <li className="px-3 py-2 text-xs text-fg-subtle">暂无已暂存的改动</li>}
            </ul>
          </Section>
          <Section
            title="未暂存" count={unstaged.length}
            onBulk={() => stageAll(unstaged.map((e) => e.path))}
            bulkLabel="全部暂存"
            bulkIcon={<PlusIcon width={12} height={12} />}
          >
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
            <label className="flex cursor-pointer items-center gap-1.5 text-xs text-fg-muted" title="用当前暂存改动修订最近一次提交(会改写历史,勾选后会预填上次信息)">
              <input
                type="checkbox"
                checked={amend}
                onChange={(e) => toggleAmend(e.target.checked)}
                className="accent-accent"
              />
              修订上次提交
            </label>
            <Button variant="commit" size="md" disabled={!canCommit} onClick={doCommit}>
              <CheckIcon width={14} height={14} /> {amend ? "修订提交" : "提交"}
            </Button>
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
              <span className="ml-1.5 text-fg-subtle">
                {isConflict ? "(冲突)" : sel.staged ? "(已暂存)" : "(未暂存)"}
              </span>
            </span>
          ) : "Diff"}
        </div>
        {isConflict && sel ? (
          <ConflictEditor repo={repo} file={sel.path} />
        ) : (
          <DiffView diff={diffQ.data ?? null} loading={diffQ.isLoading} hasFile={!!sel} hunkAction={hunkAction} lineStage={lineStage} />
        )}
      </main>
      </div>
    </div>
  );
}
