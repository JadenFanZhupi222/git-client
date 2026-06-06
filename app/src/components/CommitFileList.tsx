import { type FileChangeDto } from "../ipc";

/** 提交内文件状态 → 颜色 + 单字母 */
const STYLE: Record<string, { letter: string; cls: string }> = {
  added: { letter: "A", cls: "text-success" },
  modified: { letter: "M", cls: "text-accent" },
  deleted: { letter: "D", cls: "text-danger" },
  renamed: { letter: "R", cls: "text-warning" },
};

export function CommitFileList({
  files, selected, onSelect,
}: { files: FileChangeDto[]; selected: string | null; onSelect: (path: string) => void }) {
  if (files.length === 0) return <div className="p-3 text-xs text-fg-subtle">无改动文件</div>;
  return (
    <div className="h-full overflow-y-auto">
      {files.map((f) => {
        const s = STYLE[f.status] ?? { letter: "?", cls: "text-fg-muted" };
        const on = selected === f.path;
        // 文件名与所在目录分开显示:文件名亮、目录灰,便于扫读
        const slash = f.path.lastIndexOf("/");
        const dir = slash >= 0 ? f.path.slice(0, slash + 1) : "";
        const name = slash >= 0 ? f.path.slice(slash + 1) : f.path;
        return (
          <div
            key={f.path}
            onClick={() => onSelect(f.path)}
            title={f.path}
            className={`flex cursor-pointer items-center gap-2 px-3 py-1.5 font-mono text-[13px] transition-colors ${
              on ? "bg-overlay" : "hover:bg-elevated"
            }`}
          >
            <span className={`w-3.5 shrink-0 text-center text-xs font-semibold ${s.cls}`}>{s.letter}</span>
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
          </div>
        );
      })}
    </div>
  );
}
