import { useEffect, useRef } from "react";
import { type FileChangeDto } from "../ipc";
import { HistoryIcon } from "./icons";
import { Spine } from "./ui/Spine";
import { useT } from "../lib/i18n";

/** 提交内文件状态 → 语义色 + 单字母(对齐原型 statBadge:M=warning、A=success、D=danger、R=accent)。 */
const STYLE: Record<string, { letter: string; color: string }> = {
  added: { letter: "A", color: "var(--color-success)" },
  modified: { letter: "M", color: "var(--color-warning)" },
  deleted: { letter: "D", color: "var(--color-danger)" },
  renamed: { letter: "R", color: "var(--color-accent)" },
};

export function CommitFileList({
  files, selected, onSelect, onFileHistory,
}: {
  files: FileChangeDto[];
  selected: string | null;
  onSelect: (path: string) => void;
  /** 可选:行内悬浮出现「文件历史」按钮,点击查看该文件的 git log --follow。 */
  onFileHistory?: (path: string) => void;
}) {
  const t = useT();
  const boxRef = useRef<HTMLDivElement>(null);
  // 键盘选中变化 → 把该行滚进可视区(已可见则不动)。
  useEffect(() => {
    if (!selected) return;
    boxRef.current?.querySelector<HTMLElement>(`[data-path="${CSS.escape(selected)}"]`)?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  if (files.length === 0) return <div className="p-3 text-xs text-fg-subtle">{t("file.noChanges")}</div>;
  return (
    <div ref={boxRef} className="h-full overflow-y-auto">
      {files.map((f) => {
        const s = STYLE[f.status] ?? { letter: "?", color: "var(--color-fg-subtle)" };
        const on = selected === f.path;
        // 文件名与所在目录分开显示:文件名亮、目录灰,便于扫读
        const slash = f.path.lastIndexOf("/");
        const dir = slash >= 0 ? f.path.slice(0, slash + 1) : "";
        const name = slash >= 0 ? f.path.slice(slash + 1) : f.path;
        return (
          <div
            key={f.path}
            data-path={f.path}
            onClick={() => onSelect(f.path)}
            title={f.path}
            className={`group relative flex cursor-pointer items-center gap-2 px-3 py-1.5 font-mono text-[13px] transition-colors ${
              on ? "bg-accent/10" : "hover:bg-elevated"
            }`}
          >
            {on && <Spine />}
            <span
              className="grid h-4 w-4 shrink-0 place-items-center rounded text-[10px] font-bold"
              style={{ color: s.color, background: `color-mix(in oklab, ${s.color} 15%, transparent)` }}
            >
              {s.letter}
            </span>
            <span className="min-w-0 flex-1 truncate">
              {dir && <span className="text-fg-subtle">{dir}</span>}
              <span className="text-fg">{name}</span>
            </span>
            {onFileHistory && (
              <button
                onClick={(e) => { e.stopPropagation(); onFileHistory(f.path); }}
                title={t("file.history")}
                aria-label={t("file.historyAria")}
                className="shrink-0 rounded p-0.5 text-fg-subtle opacity-0 transition-colors hover:text-accent group-hover:opacity-100"
              >
                <HistoryIcon width={13} height={13} />
              </button>
            )}
            {/* 增删行数(diff --stat);二进制为 0/0 → 不显示 */}
            {(f.additions > 0 || f.deletions > 0) && (
              <span className="shrink-0 text-[11px] tabular-nums">
                {f.additions > 0 && <span className="text-success">+{f.additions}</span>}
                {f.additions > 0 && f.deletions > 0 && " "}
                {f.deletions > 0 && <span className="text-danger">−{f.deletions}</span>}
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}
