import { type FileDiffDto } from "../ipc";

/** 单文件行级 diff 渲染(unified 视图:旧/新双列行号 + 增删着色)。 */
export function DiffView({
  diff,
  loading,
  hasFile,
  hunkAction,
}: {
  diff: FileDiffDto | null;
  loading: boolean;
  hasFile: boolean;
  /** 可选:每个 hunk 头部显示一个动作按钮(Changes 视图的「暂存/取消暂存此块」)。 */
  hunkAction?: { label: string; onAct: (hunkIndex: number) => void; disabled?: boolean };
}) {
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
      {diff.hunks.map((h, hi) => (
        <div key={hi}>
          <div className="group flex select-none items-center gap-2 bg-overlay px-3 py-0.5 text-[11px] text-accent/80">
            <span className="truncate">{h.header}</span>
            {hunkAction && (
              <button
                disabled={hunkAction.disabled}
                onClick={() => hunkAction.onAct(hi)}
                className="ml-auto shrink-0 rounded px-1.5 text-[10px] text-accent opacity-0 transition-opacity hover:bg-overlay group-hover:opacity-100 disabled:opacity-40"
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
            return (
              <div key={li} className={`flex ${rowBg}`}>
                <Gutter n={l.old_lineno} />
                <Gutter n={l.new_lineno} border />
                <span className={`w-4 shrink-0 select-none text-center ${signCls}`}>{sign}</span>
                <span className="flex-1 whitespace-pre pr-3 text-fg">{l.content || " "}</span>
              </div>
            );
          })}
        </div>
      ))}
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
