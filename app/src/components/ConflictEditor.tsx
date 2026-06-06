import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { writeResolved, type IpcError } from "../ipc";
import { useFileText, invalidateWorktree, qk } from "../lib/queries";
import { useToast } from "./Toast";

type Seg =
  | { kind: "text"; lines: string[] }
  | { kind: "conflict"; ours: string[]; theirs: string[]; base?: string[]; sel?: string[]; touched?: boolean };

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

/** 组装结果:冲突段输出勾选的行(我方选中在前、对方选中在后)。 */
function assemble(segs: Seg[]): string {
  const out: string[] = [];
  for (const s of segs) {
    if (s.kind === "text") { out.push(...s.lines); continue; }
    const sel = new Set(s.sel ?? []);
    s.ours.forEach((l, i) => { if (sel.has(`o${i}`)) out.push(l); });
    s.theirs.forEach((l, i) => { if (sel.has(`t${i}`)) out.push(l); });
  }
  return out.join("\n");
}

const oursKeys = (s: Extract<Seg, { kind: "conflict" }>) => s.ours.map((_, i) => `o${i}`);
const theirsKeys = (s: Extract<Seg, { kind: "conflict" }>) => s.theirs.map((_, i) => `t${i}`);

/** 交互式冲突编辑器:逐行勾选我方/对方(可各取各舍),组装写回并标记已解决。 */
export function ConflictEditor({ repo, file }: { repo: string; file: string }) {
  const qc = useQueryClient();
  const toast = useToast();
  const q = useFileText(repo, file, true);
  const [segs, setSegs] = useState<Seg[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => { if (q.data != null) setSegs(parseConflicts(q.data)); }, [q.data]);

  if (q.isLoading) return <Center>加载中…</Center>;
  if (q.error || q.data == null) return <Center>无法读取文件</Center>;

  const conflictSegs = segs.filter((s) => s.kind === "conflict") as Extract<Seg, { kind: "conflict" }>[];
  const total = conflictSegs.length;
  const chosen = conflictSegs.filter((s) => s.touched).length;
  const allChosen = total > 0 && chosen === total;

  const update = (idx: number, fn: (s: Extract<Seg, { kind: "conflict" }>) => Seg) =>
    setSegs((prev) => prev.map((s, i) => (i === idx && s.kind === "conflict" ? fn(s) : s)));
  const setSel = (idx: number, keys: string[]) => update(idx, (s) => ({ ...s, sel: keys, touched: true }));
  const toggle = (idx: number, key: string) =>
    update(idx, (s) => {
      const set = new Set(s.sel ?? []);
      set.has(key) ? set.delete(key) : set.add(key);
      return { ...s, sel: [...set], touched: true };
    });

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
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-1.5 text-xs">
        <span className="text-fg-muted">已处理 {chosen}/{total} 块 · 勾选要保留的行(两边可各取各舍)</span>
        <button
          onClick={apply}
          disabled={busy || !allChosen}
          title={allChosen ? "把勾选结果写回并标记已解决" : "请处理每个冲突块"}
          className="ml-auto rounded-md bg-done px-3 py-1 font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-40"
        >
          应用解决
        </button>
      </div>

      <div className="fade-in flex-1 overflow-auto font-mono text-[12px] leading-5">
        {segs.map((s, idx) =>
          s.kind === "text" ? (
            s.lines.map((l, j) => <div key={`t${idx}-${j}`} className="whitespace-pre px-3 text-fg-muted">{l || " "}</div>)
          ) : (
            <ConflictBlock
              key={`c${idx}`}
              seg={s}
              onToggle={(k) => toggle(idx, k)}
              onAllOurs={() => setSel(idx, oursKeys(s))}
              onAllTheirs={() => setSel(idx, theirsKeys(s))}
              onBoth={() => setSel(idx, [...oursKeys(s), ...theirsKeys(s)])}
              onNone={() => setSel(idx, [])}
            />
          )
        )}
      </div>
    </div>
  );
}

function ConflictBlock({ seg, onToggle, onAllOurs, onAllTheirs, onBoth, onNone }: {
  seg: Extract<Seg, { kind: "conflict" }>;
  onToggle: (key: string) => void;
  onAllOurs: () => void;
  onAllTheirs: () => void;
  onBoth: () => void;
  onNone: () => void;
}) {
  const sel = new Set(seg.sel ?? []);
  const Col = ({ title, lines, prefix, tint }: { title: string; lines: string[]; prefix: "o" | "t"; tint: string }) => (
    <div className="min-w-0 flex-1 overflow-hidden rounded-md border border-line">
      <div className="bg-overlay px-2 py-0.5 text-[11px] text-fg-muted">{title}</div>
      {lines.length === 0 ? (
        <div className="px-2 py-0.5 text-fg-subtle">(空)</div>
      ) : lines.map((l, i) => {
        const key = `${prefix}${i}`;
        const on = sel.has(key);
        return (
          <div
            key={i}
            onClick={() => onToggle(key)}
            className={`flex cursor-pointer items-start gap-1 px-1 ${on ? tint : "opacity-50 hover:opacity-100"}`}
          >
            <span className="w-3 shrink-0 select-none text-center text-[10px] text-accent">{on ? "✓" : "·"}</span>
            <span className="whitespace-pre text-fg">{l || " "}</span>
          </div>
        );
      })}
    </div>
  );
  return (
    <div className="my-1 border-y border-warning/30 bg-warning/5 p-2">
      <div className="mb-1 flex items-center gap-1.5 text-[11px]">
        <span className="text-fg-subtle">{seg.touched ? "✓ 已处理" : "选择保留的行:"}</span>
        <button onClick={onAllOurs} className="rounded px-1.5 py-0.5 text-fg-muted hover:bg-overlay hover:text-fg">全选我方</button>
        <button onClick={onAllTheirs} className="rounded px-1.5 py-0.5 text-fg-muted hover:bg-overlay hover:text-fg">全选对方</button>
        <button onClick={onBoth} className="rounded px-1.5 py-0.5 text-fg-muted hover:bg-overlay hover:text-fg">两者</button>
        <button onClick={onNone} className="rounded px-1.5 py-0.5 text-fg-muted hover:bg-overlay hover:text-fg">都不要</button>
      </div>
      <div className="flex gap-2">
        <Col title="我方 (ours)" lines={seg.ours} prefix="o" tint="bg-success/15" />
        <Col title="对方 (theirs)" lines={seg.theirs} prefix="t" tint="bg-accent/15" />
      </div>
    </div>
  );
}

function Center({ children }: { children: React.ReactNode }) {
  return <div className="flex flex-1 items-center justify-center p-4 text-center text-sm text-fg-subtle">{children}</div>;
}
