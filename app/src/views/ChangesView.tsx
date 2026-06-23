import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  stageFile, unstageFile, stageHunk, unstageHunk, stageLines, commit, amendCommit,
  getHeadCommit, resolveOurs, resolveTheirs,
  type FileEntryDto, type IpcError,
} from "../ipc";
import { useStatus, useWorkingDiff, useRepoState, useCurrentBranch, invalidateWorktree, invalidateHistory, qk } from "../lib/queries";
import { useListKeyboardNav, isTypingTarget } from "../lib/listNav";
import { RefreshIcon, CheckIcon, FileDiffIcon } from "../components/icons";
import { DiffView } from "../components/DiffView";
import { ConflictEditor } from "../components/ConflictEditor";
import { ConflictBanner } from "../components/ConflictBanner";
import { Resizer, useResizableWidth } from "../components/Resizer";
import { useToast } from "../components/Toast";
import { Button } from "../components/ui/Button";
import { FloatBar, FLOAT_BAR_INSET } from "../components/ui/FloatBar";
import { Spine } from "../components/ui/Spine";
import { useT } from "../lib/i18n";
import type { MessageKey } from "../lib/locales/zh";

/** 工作区状态 → 语义色(CSS 变量,供 16×16 色块徽章 color-mix)+ 单字母 + i18n key(tooltip)。
 *  对齐原型 statBadge:M=warning、A=success、D=danger、R=accent、冲突=danger。 */
const STATE_STYLE: Record<string, { letter: string; color: string; key: MessageKey }> = {
  new: { letter: "A", color: "var(--color-success)", key: "file.new" },
  added: { letter: "A", color: "var(--color-success)", key: "file.new" },
  modified: { letter: "M", color: "var(--color-warning)", key: "file.modified" },
  deleted: { letter: "D", color: "var(--color-danger)", key: "file.deleted" },
  renamed: { letter: "R", color: "var(--color-accent)", key: "file.renamed" },
  untracked: { letter: "U", color: "var(--color-success)", key: "file.untracked" },
  conflicted: { letter: "!", color: "var(--color-danger)", key: "file.conflicted" },
};
function styleFor(state: string): { letter: string; color: string; key: MessageKey | null; raw?: string } {
  return STATE_STYLE[state.toLowerCase()] ?? { letter: "?", color: "var(--color-fg-subtle)", key: null, raw: state };
}

/** 16×16 圆角状态色块徽章(色 = 语义色,底 = 同色 15% color-mix)。原型 statBadge 复刻。 */
function StatBadge({ color, letter, title }: { color: string; letter: string; title?: string }) {
  return (
    <span
      title={title}
      className="grid h-4 w-4 shrink-0 place-items-center rounded font-mono text-[10px] font-bold"
      style={{ color, background: `color-mix(in oklab, ${color} 15%, transparent)` }}
    >
      {letter}
    </span>
  );
}

/** 文件路径拆成 目录(灰)+ 文件名(亮),便于扫读 */
function splitPath(path: string) {
  const i = path.lastIndexOf("/");
  return { dir: i >= 0 ? path.slice(0, i + 1) : "", name: i >= 0 ? path.slice(i + 1) : path };
}

export function ChangesView({ repo }: { repo: string }) {
  const t = useT();
  const qc = useQueryClient();
  const statusQ = useStatus(repo);
  const status = statusQ.data;
  const repoState = useRepoState(repo).data ?? "clean";
  const branch = useCurrentBranch(repo).data ?? "main";
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

  // 扁平的可键盘选择列表(按显示顺序:冲突 → 未暂存 → 已暂存),供 j/k 导航与空格暂存。
  const flatList = [
    ...conflicts.map((e) => ({ path: e.path, staged: false, conflict: true })),
    ...unstaged.map((e) => ({ path: e.path, staged: false, conflict: false })),
    ...staged.map((e) => ({ path: e.path, staged: true, conflict: false })),
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
      toast({ kind: "success", title: amend ? t("changes.amended", { sha: sha.slice(0, 7) }) : t("changes.committed", { sha: sha.slice(0, 7) }) });
      invalidateWorktree(qc, repo);
      invalidateHistory(qc, repo);
    } catch (e) { toast({ kind: "error", title: (e as IpcError).message ?? String(e) }); }
    finally { setBusy(false); }
  }

  // 选中文件可做 hunk 级操作吗?未跟踪 / 冲突文件无 git diff hunk,排除。
  const selEntry = sel && status ? status.entries.find((e) => e.path === sel.path && e.staged === sel.staged) : undefined;
  const isConflict = selEntry?.state.toLowerCase() === "conflicted";
  const hunkAction = sel && selEntry && !isConflict && selEntry.state.toLowerCase() !== "untracked"
    ? { label: sel.staged ? t("changes.unstageHunk") : t("changes.stageHunk"), disabled: busy, onAct: (hi: number) => doHunk(sel.path, sel.staged, hi) }
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
        className={`group relative flex cursor-pointer items-center gap-2.5 rounded-md px-3 py-1.5 ${on ? "bg-accent/10" : "hover:bg-elevated"}`}
      >
        {on && <Spine />}
        {/* 暂存复选框:勾选 = 已暂存(accent 实心 + 白勾),空 = 未暂存。点击切换暂存态。 */}
        <button
          title={isStaged ? t("changes.unstage") : t("changes.stage")}
          disabled={busy}
          onClick={(e) => { e.stopPropagation(); run(() => (isStaged ? unstageFile(repo, entry.path) : stageFile(repo, entry.path))); }}
          className="grid h-[18px] w-[18px] shrink-0 place-items-center rounded-[5px] border border-line-strong text-white transition-colors disabled:opacity-40"
          style={{ background: isStaged ? "var(--color-accent)" : "transparent" }}
        >
          {isStaged && <CheckIcon width={11} height={11} />}
        </button>
        <StatBadge color={s.color} letter={s.letter} title={s.key ? t(s.key) : s.raw} />
        <span className="min-w-0 flex-1 truncate font-mono text-[12.5px]" title={entry.path}>
          {dir && <span className="text-fg-subtle">{dir}</span>}
          <span className={on ? "text-fg" : "text-fg-muted"}>{name}</span>
        </span>
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
        className={`group relative flex cursor-pointer items-center gap-2.5 rounded-md px-3 py-1.5 ${on ? "bg-accent/10" : "hover:bg-elevated"}`}
      >
        {on && <Spine />}
        <StatBadge color="var(--color-danger)" letter="!" title={t("file.conflicted")} />
        <span className="min-w-0 flex-1 truncate font-mono text-[12.5px]" title={entry.path}>
          {dir && <span className="text-fg-subtle">{dir}</span>}
          <span className={on ? "text-fg" : "text-fg-muted"}>{name}</span>
        </span>
        <div className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
          <button onClick={(e) => { e.stopPropagation(); useOurs(entry.path); }} disabled={busy}
            title={t("changes.useOursTitle")} className="rounded px-1.5 py-0.5 text-[11px] text-accent hover:bg-overlay disabled:opacity-40">{t("changes.useOurs")}</button>
          <button onClick={(e) => { e.stopPropagation(); useTheirs(entry.path); }} disabled={busy}
            title={t("changes.useTheirsTitle")} className="rounded px-1.5 py-0.5 text-[11px] text-accent hover:bg-overlay disabled:opacity-40">{t("changes.useTheirs")}</button>
          <button onClick={(e) => { e.stopPropagation(); run(() => stageFile(repo, entry.path)); }} disabled={busy}
            title={t("changes.resolvedTitle")} className="rounded px-1.5 py-0.5 text-[11px] text-success hover:bg-overlay disabled:opacity-40">{t("changes.resolved")}</button>
        </div>
      </li>
    );
  };

  const Section = ({ title, count, onBulk, bulkLabel, children }: {
    title: string; count: number;
    onBulk?: () => void; bulkLabel?: string;
    children: React.ReactNode;
  }) => (
    <div>
      <div className="flex items-center gap-2 px-3 pb-1.5 pt-2.5">
        <span className="text-[11px] font-semibold uppercase tracking-[0.04em] text-fg-subtle">{title}</span>
        <span className="rounded-full bg-fg/[0.08] px-1.5 text-[10px] tabular-nums text-fg-subtle">{count}</span>
        {count > 0 && onBulk && (
          <button
            onClick={onBulk}
            disabled={busy}
            title={bulkLabel}
            className="ml-auto rounded-md px-2 py-0.5 text-[11px] text-accent transition-colors hover:bg-accent/10 disabled:opacity-40"
          >
            {bulkLabel}
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
      <div className="relative flex shrink-0 flex-col overflow-hidden" style={{ width: listCol.w }}>
        {/* 浮动液态玻璃工具栏:标题 + 刷新;文件区从其下穿过显折射。 */}
        <FloatBar>
          <span className="flex-1 text-[12.5px] font-semibold text-fg">{t("changes.workingTree")}</span>
          <button
            onClick={() => qc.invalidateQueries({ queryKey: qk.status(repo) })}
            disabled={busy || statusQ.isFetching}
            title={t("changes.refresh")}
            aria-label={t("changes.refresh")}
            className="grid h-7 w-7 shrink-0 place-items-center rounded-md border border-line text-fg-muted transition-colors hover:text-fg disabled:opacity-40"
          >
            <RefreshIcon width={13} height={13} className={busy || statusQ.isFetching ? "animate-spin" : ""} />
          </button>
        </FloatBar>

        {/* 文件区(滚动):未暂存 → 已暂存,贴近底部提交框。 */}
        <div ref={listScrollRef} className="min-h-0 flex-1 overflow-y-auto px-2 pb-3" style={{ paddingTop: FLOAT_BAR_INSET }}>
          {conflicts.length > 0 && (
            <Section title={t("changes.sectionConflicts")} count={conflicts.length}>
              <ul>
                {conflicts.map((e) => <ConflictRow key={e.path} entry={e} />)}
              </ul>
            </Section>
          )}
          <Section
            title={t("changes.sectionUnstaged")} count={unstaged.length}
            onBulk={() => stageAll(unstaged.map((e) => e.path))}
            bulkLabel={t("changes.stageAll")}
          >
            <ul>
              {unstaged.map((e) => <Row key={e.path} entry={e} isStaged={false} />)}
              {unstaged.length === 0 && <li className="px-3 py-2 text-xs text-fg-subtle">{t("changes.clean")}</li>}
            </ul>
          </Section>
          <Section
            title={t("changes.sectionStaged")} count={staged.length}
            onBulk={() => unstageAll(staged.map((e) => e.path))}
            bulkLabel={t("changes.unstageAll")}
          >
            <ul>
              {staged.map((e) => <Row key={e.path} entry={e} isStaged />)}
              {staged.length === 0 && <li className="px-3 py-2 text-xs text-fg-subtle">{t("changes.noStaged")}</li>}
            </ul>
          </Section>
        </div>

        {/* 提交框(固定底部):textarea + footer 统一内盒(borderTop 分隔)。 */}
        <div className="shrink-0 border-t border-line p-3">
          <div className="overflow-hidden rounded-md border border-line bg-elevated/50">
            <textarea
              className="w-full resize-none border-none bg-transparent p-2.5 text-[13px] text-fg placeholder:text-fg-subtle focus:outline-none"
              rows={3}
              placeholder={t("changes.commitPlaceholder")}
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              onKeyDown={(e) => {
                if ((e.metaKey || e.ctrlKey) && e.key === "Enter" && canCommit) {
                  e.preventDefault();
                  doCommit();
                }
              }}
            />
            <div className="flex items-center gap-2 border-t border-line px-2.5 py-2">
              <label className="flex cursor-pointer items-center gap-1.5 text-[11.5px] text-fg-muted" title={t("changes.amendTitle")}>
                <input
                  type="checkbox"
                  checked={amend}
                  onChange={(e) => toggleAmend(e.target.checked)}
                  className="accent-accent"
                />
                {t("changes.amend")}
              </label>
              <span className="font-mono text-[10.5px] tabular-nums text-fg-subtle">{t("changes.stagedCount", { n: staged.length })}</span>
              <Button variant="commit" size="md" disabled={!canCommit} onClick={doCommit} className="ml-auto">
                <CheckIcon width={13} height={13} /> {amend ? t("changes.amendCommit") : t("changes.commitTo", { branch })}
              </Button>
            </div>
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
                {isConflict ? t("changes.tagConflict") : sel.staged ? t("changes.tagStaged") : t("changes.tagUnstaged")}
              </span>
            </span>
          ) : "Diff"}
        </div>
        {isConflict && sel ? (
          <ConflictEditor repo={repo} file={sel.path} />
        ) : (
          <DiffView diff={diffQ.data ?? null} loading={diffQ.isLoading} hasFile={!!sel} repo={repo} hunkAction={hunkAction} lineStage={lineStage} />
        )}
      </main>
      </div>
    </div>
  );
}
