import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { writeResolved, type IpcError } from "../ipc";
import { useFileText, invalidateWorktree, qk } from "../lib/queries";
import { useToast } from "./Toast";

type Choice = "ours" | "theirs" | "both-ot" | "both-to";
type Seg =
  | { kind: "text"; lines: string[] }
  | { kind: "conflict"; ours: string[]; theirs: string[]; base?: string[]; choice?: Choice };

/** 把含冲突标记的文本解析成 普通段 + 冲突段(ours/theirs/可选 base)。 */
function parseConflicts(text: string): Seg[] {
  const lines = text.split("\n");
  const segs: Seg[] = [];
  let buf: string[] = [];
  let i = 0;
  const flush = () => { if (buf.length) { segs.push({ kind: "text", lines: buf }); buf = []; } };
  while (i < lines.length) {
    if (lines[i].startsWith("<<<<<<<")) {
      flush();
      i++;
      const ours: string[] = [], base: string[] = [], theirs: string[] = [];
      while (i < lines.length && !lines[i].startsWith("|||||||") && !lines[i].startsWith("=======")) ours.push(lines[i++]);
      let hasBase = false;
      if (i < lines.length && lines[i].startsWith("|||||||")) {
        hasBase = true; i++;
        while (i < lines.length && !lines[i].startsWith("=======")) base.push(lines[i++]);
      }
      if (i < lines.length && lines[i].startsWith("=======")) i++;
      while (i < lines.length && !lines[i].startsWith(">>>>>>>")) theirs.push(lines[i++]);
      if (i < lines.length && lines[i].startsWith(">>>>>>>")) i++;
      segs.push({ kind: "conflict", ours, theirs, base: hasBase ? base : undefined });
    } else {
      buf.push(lines[i++]);
    }
  }
  flush();
  return segs;
}

function assemble(segs: Seg[]): string {
  const out: string[] = [];
  for (const s of segs) {
    if (s.kind === "text") out.push(...s.lines);
    else if (s.choice === "ours") out.push(...s.ours);
    else if (s.choice === "theirs") out.push(...s.theirs);
    else if (s.choice === "both-ot") out.push(...s.ours, ...s.theirs);
    else if (s.choice === "both-to") out.push(...s.theirs, ...s.ours);
  }
  return out.join("\n");
}

/** 交互式冲突编辑器:逐冲突块选 我方/对方/两者,组装后写回并标记已解决。 */
export function ConflictEditor({ repo, file }: { repo: string; file: string }) {
  const qc = useQueryClient();
  const toast = useToast();
  const q = useFileText(repo, file, true);
  const [segs, setSegs] = useState<Seg[]>([]);
  const [busy, setBusy] = useState(false);

  // 文件内容到手(或切文件)时解析
  useEffect(() => { if (q.data != null) setSegs(parseConflicts(q.data)); }, [q.data]);

  if (q.isLoading) return <Center>加载中…</Center>;
  if (q.error || q.data == null) return <Center>无法读取文件</Center>;

  const conflictSegs = segs.filter((s) => s.kind === "conflict") as Extract<Seg, { kind: "conflict" }>[];
  const total = conflictSegs.length;
  const chosen = conflictSegs.filter((s) => s.choice).length;
  const allChosen = total > 0 && chosen === total;

  const setChoice = (idx: number, choice: Choice) =>
    setSegs((prev) => prev.map((s, i) => (i === idx && s.kind === "conflict" ? { ...s, choice } : s)));

  async function apply() {
    setBusy(true);
    try {
      await writeResolved(repo, file, assemble(segs));
      invalidateWorktree(qc, repo);
      qc.invalidateQueries({ queryKey: qk.repoState(repo) });
      toast({ kind: "success", title: "已解决并标记" });
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* 操作条 */}
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-1.5 text-xs">
        <span className="text-fg-muted">已选 {chosen}/{total} 块</span>
        <button
          onClick={apply}
          disabled={busy || !allChosen}
          title={allChosen ? "把所选结果写回并标记已解决" : "请先为每个冲突块选择一边"}
          className="ml-auto rounded-md bg-done px-3 py-1 font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-40"
        >
          应用解决
        </button>
      </div>

      {/* 正文 */}
      <div className="fade-in flex-1 overflow-auto font-mono text-[12px] leading-5">
        {segs.map((s, idx) =>
          s.kind === "text" ? (
            s.lines.map((l, j) => (
              <div key={`t${idx}-${j}`} className="whitespace-pre px-3 text-fg">{l || " "}</div>
            ))
          ) : (
            <ConflictBlock key={`c${idx}`} seg={s} onChoose={(c) => setChoice(idx, c)} />
          )
        )}
      </div>
    </div>
  );
}

function ConflictBlock({ seg, onChoose }: { seg: Extract<Seg, { kind: "conflict" }>; onChoose: (c: Choice) => void }) {
  const Side = ({ title, lines, active, tint, onClick }: { title: string; lines: string[]; active: boolean; tint: string; onClick: () => void }) => (
    <div className={`min-w-0 flex-1 border ${active ? "border-accent" : "border-line"} rounded-md overflow-hidden`}>
      <button onClick={onClick} className={`flex w-full items-center justify-between px-2 py-0.5 text-[11px] ${active ? "bg-accent/15 text-accent" : "bg-overlay text-fg-muted hover:text-fg"}`}>
        <span>{title}</span>
        <span>{active ? "✓ 采用" : "采用"}</span>
      </button>
      <div className={tint}>
        {lines.length === 0 ? <div className="px-2 text-fg-subtle">(空)</div> : lines.map((l, i) => (
          <div key={i} className="whitespace-pre px-2 text-fg">{l || " "}</div>
        ))}
      </div>
    </div>
  );
  return (
    <div className="my-1 border-y border-warning/30 bg-warning/5 p-2">
      <div className="flex gap-2">
        <Side title="我方 (ours)" lines={seg.ours} active={seg.choice === "ours"} tint="bg-success/10" onClick={() => onChoose("ours")} />
        <Side title="对方 (theirs)" lines={seg.theirs} active={seg.choice === "theirs"} tint="bg-accent/10" onClick={() => onChoose("theirs")} />
      </div>
      <div className="mt-1 flex items-center gap-1.5 text-[11px]">
        <span className="text-fg-subtle">两者:</span>
        <button onClick={() => onChoose("both-ot")} className={`rounded px-1.5 py-0.5 ${seg.choice === "both-ot" ? "bg-accent/15 text-accent" : "text-fg-muted hover:bg-overlay hover:text-fg"}`}>我方+对方</button>
        <button onClick={() => onChoose("both-to")} className={`rounded px-1.5 py-0.5 ${seg.choice === "both-to" ? "bg-accent/15 text-accent" : "text-fg-muted hover:bg-overlay hover:text-fg"}`}>对方+我方</button>
      </div>
    </div>
  );
}

function Center({ children }: { children: React.ReactNode }) {
  return <div className="flex flex-1 items-center justify-center p-4 text-center text-sm text-fg-subtle">{children}</div>;
}
