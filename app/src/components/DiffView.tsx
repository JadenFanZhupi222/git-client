import { useEffect, useState } from "react";
import { type FileDiffDto } from "../ipc";

/** 单文件行级 diff 渲染(unified 视图:旧/新双列行号 + 增删着色)。 */
export function DiffView({
  diff,
  loading,
  hasFile,
  hunkAction,
  lineStage,
}: {
  diff: FileDiffDto | null;
  loading: boolean;
  hasFile: boolean;
  /** 可选:每个 hunk 头部显示一个动作按钮(Changes 视图的「暂存/取消暂存此块」)。 */
  hunkAction?: { label: string; onAct: (hunkIndex: number) => void; disabled?: boolean };
  /** 可选:开启行级选择暂存(仅未暂存改动)。点 +/- 行选中,hunk 头出现「暂存选中行」。 */
  lineStage?: { onStage: (hunkIndex: number, lines: number[]) => void; disabled?: boolean };
}) {
  // 选中的行:键 `${hunkIdx}:${lineIdx}`。diff 变化(切文件)时清空。
  const [selected, setSelected] = useState<Set<string>>(new Set());
  useEffect(() => { setSelected(new Set()); }, [diff]);
  const toggle = (key: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(key) ? next.delete(key) : next.add(key);
      return next;
    });
  if (!hasFile) {
    return <Center>选择一个文件查看 diff</Center>;
  }
  if (loading) {
    return (
      <div className="space-y-1 p-3">
        {Array.from({ length: 10 }).map((_, i) => (
          <div key={i} className="skeleton h-4" style={{ width: `${85 - (i % 4) * 12}%` }} />
        ))}
      </div>
    );
  }
  if (!diff) {
    return <Center>无法加载 diff</Center>;
  }
  if (diff.is_binary) {
    return <Center>二进制文件,无法显示行级 diff</Center>;
  }
  if (diff.hunks.length === 0) {
    return <Center>该文件在此提交中无文本改动</Center>;
  }

  return (
    <div className="fade-in flex-1 overflow-auto font-mono text-[12px] leading-5">
      {diff.hunks.map((h, hi) => {
        const selCount = h.lines.filter((_, li) => selected.has(`${hi}:${li}`)).length;
        return (
        <div key={hi}>
          <div className="group flex select-none items-center gap-2 bg-overlay px-3 py-0.5 text-[11px] text-accent/80">
            <span className="truncate">{h.header}</span>
            {lineStage && selCount > 0 && (
              <button
                disabled={lineStage.disabled}
                onClick={() => {
                  const lines = h.lines.map((_, li) => li).filter((li) => selected.has(`${hi}:${li}`));
                  lineStage.onStage(hi, lines);
                }}
                className="ml-auto shrink-0 rounded border border-accent bg-accent/15 px-1.5 py-px text-[10px] text-accent transition-colors hover:bg-accent/25 disabled:opacity-40"
              >
                暂存选中行 ({selCount})
              </button>
            )}
            {hunkAction && (
              <button
                disabled={hunkAction.disabled}
                onClick={() => hunkAction.onAct(hi)}
                className={`shrink-0 rounded border border-line-strong bg-elevated px-1.5 py-px text-[10px] text-fg-muted opacity-0 transition-colors hover:border-accent hover:bg-overlay hover:text-accent group-hover:opacity-100 disabled:opacity-40 ${lineStage && selCount > 0 ? "" : "ml-auto"}`}
              >
                {hunkAction.label}
              </button>
            )}
          </div>
          {h.lines.map((l, li) => {
            const add = l.kind === "add";
            const del = l.kind === "del";
            const rowBg = add ? "bg-success/10" : del ? "bg-danger/10" : "";
            const sign = add ? "+" : del ? "-" : " ";
            const signCls = add ? "text-success" : del ? "text-danger" : "text-fg-subtle";
            const selectable = !!lineStage && (add || del);
            const key = `${hi}:${li}`;
            const on = selected.has(key);
            return (
              <div
                key={li}
                onClick={selectable ? () => toggle(key) : undefined}
                className={`flex ${rowBg} ${selectable ? "cursor-pointer" : ""} ${on ? "ring-1 ring-inset ring-accent/60" : ""}`}
              >
                {lineStage && (
                  <span className="w-4 shrink-0 select-none text-center text-[10px] text-accent">
                    {selectable ? (on ? "✓" : "·") : ""}
                  </span>
                )}
                <Gutter n={l.old_lineno} />
                <Gutter n={l.new_lineno} border />
                <span className={`w-4 shrink-0 select-none text-center ${signCls}`}>{sign}</span>
                <span className="flex-1 whitespace-pre pr-3 text-fg">{l.content || " "}</span>
              </div>
            );
          })}
        </div>
        );
      })}
    </div>
  );
}

function Gutter({ n, border }: { n: number | null; border?: boolean }) {
  return (
    <span
      className={`w-10 shrink-0 select-none px-1.5 text-right text-fg-subtle ${
        border ? "border-r border-line" : ""
      }`}
    >
      {n ?? ""}
    </span>
  );
}

function Center({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 items-center justify-center p-4 text-center text-sm text-fg-subtle">
      {children}
    </div>
  );
}
