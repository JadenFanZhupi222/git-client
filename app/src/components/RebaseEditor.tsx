import { useState } from "react";
import { interactiveRebase, type CommitDto, type RebaseActionKind, type RebaseStepInput, type IpcError } from "../ipc";
import { useToast } from "./Toast";
import { CloseIcon } from "./icons";

type Row = { sha: string; short: string; summary: string; action: RebaseActionKind; message: string };

const ACTIONS: { value: RebaseActionKind; label: string }[] = [
  { value: "pick", label: "保留 pick" },
  { value: "reword", label: "改信息 reword" },
  { value: "squash", label: "合并(留信息) squash" },
  { value: "fixup", label: "合并(丢信息) fixup" },
  { value: "drop", label: "删除 drop" },
];

/** 交互式 rebase 编辑器。commits 按 oldest→newest;base=最旧提交的父 SHA(null→--root)。 */
export function RebaseEditor({
  repo, commits, base, onClose, onConflict, onDone,
}: {
  repo: string;
  commits: CommitDto[];
  base: string | null;
  onClose: () => void;
  onConflict: () => void;
  onDone: () => void;
}) {
  const toast = useToast();
  const [rows, setRows] = useState<Row[]>(
    commits.map((c) => ({ sha: c.id, short: c.short_id, summary: c.summary, action: "pick", message: c.summary })),
  );
  const [busy, setBusy] = useState(false);

  function move(i: number, dir: -1 | 1) {
    const j = i + dir;
    if (j < 0 || j >= rows.length) return;
    setRows((rs) => {
      const next = [...rs];
      [next[i], next[j]] = [next[j], next[i]];
      return next;
    });
  }
  function setAction(i: number, action: RebaseActionKind) {
    setRows((rs) => rs.map((r, k) => (k === i ? { ...r, action } : r)));
  }
  function setMessage(i: number, message: string) {
    setRows((rs) => rs.map((r, k) => (k === i ? { ...r, message } : r)));
  }

  const firstKept = rows.find((r) => r.action !== "drop");
  const firstKeptInvalid = firstKept && (firstKept.action === "fixup" || firstKept.action === "squash");
  const allDropped = !firstKept;
  const canStart = !busy && !firstKeptInvalid && !allDropped;

  async function start() {
    setBusy(true);
    const steps: RebaseStepInput[] = rows.map((r) => ({
      sha: r.sha,
      action: r.action,
      message: r.action === "reword" || r.action === "squash" ? r.message : undefined,
    }));
    try {
      await interactiveRebase(repo, base, steps);
      toast({ kind: "success", title: "变基完成" });
      onDone();
    } catch (e) {
      const err = e as IpcError;
      if (err.code === "MERGE_CONFLICT") {
        toast({ kind: "error", title: "变基有冲突,请到「更改」页解决" });
        onConflict();
      } else {
        toast({ kind: "error", title: err.message ?? String(e) });
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        className="flex max-h-[80vh] w-[640px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex shrink-0 items-center justify-between border-b border-line px-4 py-3">
          <h2 className="text-sm font-semibold text-fg">交互式变基 · {rows.length} 个提交</h2>
          <button onClick={onClose} className="text-fg-subtle hover:text-fg"><CloseIcon width={15} height={15} /></button>
        </div>

        <p className="shrink-0 border-b border-line bg-warning/10 px-4 py-2 text-xs text-warning">
          ⚠ 会改写提交历史。若这些提交已 push,变基后需 force push。冲突可在「更改」页解决或中止。
        </p>

        <div className="min-h-0 flex-1 overflow-y-auto p-2">
          {rows.map((r, i) => (
            <div key={r.sha} className="rounded px-2 py-1.5 hover:bg-elevated">
              <div className="flex items-center gap-2">
                <div className="flex shrink-0 flex-col">
                  <button onClick={() => move(i, -1)} disabled={i === 0} className="leading-none text-fg-subtle hover:text-fg disabled:opacity-30">▲</button>
                  <button onClick={() => move(i, 1)} disabled={i === rows.length - 1} className="leading-none text-fg-subtle hover:text-fg disabled:opacity-30">▼</button>
                </div>
                <select
                  value={r.action}
                  onChange={(e) => setAction(i, e.target.value as RebaseActionKind)}
                  className="shrink-0 rounded border border-line-strong bg-canvas px-1.5 py-1 text-xs text-fg focus:border-accent focus:outline-none"
                >
                  {ACTIONS.map((a) => <option key={a.value} value={a.value}>{a.label}</option>)}
                </select>
                <span className="shrink-0 font-mono text-[11px] text-accent">{r.short}</span>
                <span className={`min-w-0 flex-1 truncate text-[13px] ${r.action === "drop" ? "text-fg-subtle line-through" : "text-fg"}`} title={r.summary}>
                  {r.summary}
                </span>
              </div>
              {(r.action === "reword" || r.action === "squash") && (
                <input
                  value={r.message}
                  onChange={(e) => setMessage(i, e.target.value)}
                  placeholder="新的提交信息"
                  className="mt-1 ml-7 w-[calc(100%-1.75rem)] rounded border border-line-strong bg-canvas px-2 py-1 text-xs text-fg focus:border-accent focus:outline-none"
                />
              )}
            </div>
          ))}
        </div>

        <div className="flex shrink-0 items-center justify-between border-t border-line px-4 py-3">
          <span className="text-xs text-danger">
            {firstKeptInvalid ? "第一个保留的提交不能是 squash/fixup" : allDropped ? "至少保留一个提交" : ""}
          </span>
          <div className="flex gap-2">
            <button onClick={onClose} className="rounded-md border border-line-strong px-3 py-1.5 text-sm text-fg-muted hover:bg-elevated">取消</button>
            <button
              onClick={start}
              disabled={!canStart}
              className="rounded-md bg-accent px-3.5 py-1.5 text-sm font-medium text-white transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40"
            >
              开始变基
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
