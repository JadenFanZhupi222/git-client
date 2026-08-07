import { useEffect, useRef } from "react";
import { type FileChangeDto } from "../ipc";
import { HistoryIcon } from "./icons";
import { Spine } from "./ui/Spine";
import { useT } from "../lib/i18n";
import type { MessageKey } from "../lib/locales/zh";

/** 提交内文件状态 → 语义色 + 单字母(对齐原型 statBadge:M=warning、A=success、D=danger、R=accent)。 */
const STYLE: Record<string, { letter: string; color: string }> = {
  added: { letter: "A", color: "var(--color-success)" },
  modified: { letter: "M", color: "var(--color-warning)" },
  deleted: { letter: "D", color: "var(--color-danger)" },
  renamed: { letter: "R", color: "var(--color-accent)" },
};

const STATUS_LABEL: Record<string, MessageKey> = {
  added: "file.new",
  modified: "file.modified",
  deleted: "file.deleted",
  renamed: "file.renamed",
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
    boxRef.current?.querySelector<HTMLElement>(`[data-path="${CSS.escape(selected)}"]`)?.scrollIntoView?.({ block: "nearest" });
  }, [selected]);

  if (files.length === 0) return <div className="p-3 text-xs text-fg-subtle">{t("file.noChanges")}</div>;
  const selectedInList = files.some((file) => file.path === selected);
  const focusOption = (path: string) => {
    requestAnimationFrame(() => {
      boxRef.current?.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`)?.focus();
    });
  };
  return (
    <div ref={boxRef} role="listbox" aria-label={t("history.changedFiles")} className="h-full overflow-y-auto">
      {files.map((f, index) => {
        const s = STYLE[f.status] ?? { letter: "?", color: "var(--color-fg-subtle)" };
        const on = selected === f.path;
        const status = t(STATUS_LABEL[f.status] ?? "file.modified");
        const stats = `${f.additions > 0 ? `, +${f.additions}` : ""}${f.deletions > 0 ? `, -${f.deletions}` : ""}`;
        // 文件名与所在目录分开显示:文件名亮、目录灰,便于扫读
        const slash = f.path.lastIndexOf("/");
        const dir = slash >= 0 ? f.path.slice(0, slash + 1) : "";
        const name = slash >= 0 ? f.path.slice(slash + 1) : f.path;
        return (
          <div key={f.path} role="presentation" className="group relative flex min-w-0">
          <button
            type="button"
            data-path={f.path}
            role="option"
            aria-selected={on}
            aria-label={`${status}: ${f.path}${stats}`}
            tabIndex={on || (!selectedInList && index === 0) ? 0 : -1}
            onClick={() => onSelect(f.path)}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onSelect(f.path);
                return;
              }
              const nextIndex = event.key === "ArrowDown"
                ? Math.min(files.length - 1, index + 1)
                : event.key === "ArrowUp"
                  ? Math.max(0, index - 1)
                  : event.key === "Home"
                    ? 0
                    : event.key === "End"
                      ? files.length - 1
                      : -1;
              if (nextIndex >= 0) {
                event.preventDefault();
                event.stopPropagation();
                const nextPath = files[nextIndex].path;
                onSelect(nextPath);
                focusOption(nextPath);
              }
            }}
            title={f.path}
            className={`relative flex min-w-0 flex-1 cursor-pointer items-center gap-2 px-3 py-1.5 text-left font-mono text-[13px] outline-none transition-colors ${onFileHistory ? "pr-10" : ""} ${
              on ? "bg-accent/10" : "hover:bg-elevated focus-visible:bg-elevated"
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
            {/* 增删行数(diff --stat);二进制为 0/0 → 不显示 */}
            {(f.additions > 0 || f.deletions > 0) && (
              <span className="shrink-0 text-[11px] tabular-nums">
                {f.additions > 0 && <span className="text-success">+{f.additions}</span>}
                {f.additions > 0 && f.deletions > 0 && " "}
                {f.deletions > 0 && <span className="text-danger">−{f.deletions}</span>}
              </span>
            )}
          </button>
          {onFileHistory && (
            <button
              type="button"
              tabIndex={on ? 0 : -1}
              onClick={() => onFileHistory(f.path)}
              title={t("file.history")}
              aria-label={`${t("file.historyAria")}: ${f.path}`}
              className={`absolute right-2 top-1/2 grid h-6 w-6 -translate-y-1/2 place-items-center rounded text-fg-subtle opacity-0 transition-colors hover:bg-overlay hover:text-accent focus:opacity-100 focus-visible:outline-offset-0 group-hover:opacity-100 group-focus-within:opacity-100 ${on ? "!opacity-100" : ""}`}
            >
              <HistoryIcon width={13} height={13} />
            </button>
          )}
          </div>
        );
      })}
    </div>
  );
}
