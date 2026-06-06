import { type CommitDto } from "../ipc";
import { formatRelative } from "../lib/time";

export function CommitList({
  commits, branch, selectedId, onSelect, onLoadMore, loading, hasMore,
}: {
  commits: CommitDto[]; branch: string | null; selectedId: string | null;
  onSelect: (c: CommitDto) => void; onLoadMore: () => void; loading: boolean; hasMore: boolean;
}) {
  return (
    <div className="overflow-y-auto">
      {commits.map((c, i) => {
        const on = selectedId === c.id;
        const head = i === 0;
        return (
          <div
            key={c.id}
            onClick={() => onSelect(c)}
            className={`flex cursor-pointer gap-2.5 border-l-2 px-3 py-2 transition-colors ${
              on ? "border-accent-emphasis bg-overlay" : "border-transparent hover:bg-elevated"
            }`}
          >
            {/* 提交轨:节点 + 连线 */}
            <div className="flex w-3 flex-col items-center pt-1">
              <div
                className={`h-2.5 w-2.5 shrink-0 rounded-full ring-2 ${
                  head ? "bg-accent ring-accent/25" : "bg-fg-muted ring-transparent"
                }`}
              />
              {i < commits.length - 1 && <div className="mt-1 w-px flex-1 bg-line-strong" />}
            </div>

            <div className="min-w-0 flex-1">
              {head && (
                <span className="mb-1 inline-flex items-center rounded-full border border-success/40 bg-success/10 px-1.5 text-[10px] font-medium text-success">
                  HEAD{branch ? ` → ${branch}` : ""}
                </span>
              )}
              <div className={`truncate text-[13px] ${on ? "text-fg" : "text-fg"}`}>{c.summary}</div>
              <div className="mt-0.5 truncate font-mono text-[11px] text-fg-muted">
                {c.short_id} · {formatRelative(c.timestamp)}
              </div>
            </div>
          </div>
        );
      })}

      {hasMore ? (
        <button
          className="w-full py-2.5 text-xs text-accent transition-colors hover:bg-elevated disabled:opacity-40"
          onClick={onLoadMore}
          disabled={loading}
        >
          {loading ? "加载中…" : "加载更多"}
        </button>
      ) : (
        commits.length > 0 && (
          <div className="py-2.5 text-center text-[11px] text-fg-subtle">已到历史开端</div>
        )
      )}
    </div>
  );
}
