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
      {commits.map((c, i) => (
        <div key={c.id} onClick={() => onSelect(c)}
          className={`flex gap-2 px-3 py-2 cursor-pointer ${selectedId === c.id ? "bg-[#161b22] border-l-2 border-[#3b82f6]" : "border-l-2 border-transparent hover:bg-[#161b22]"}`}>
          <div className="flex flex-col items-center w-3 pt-1">
            <div className={`w-2.5 h-2.5 rounded-full ${i === 0 ? "bg-[#3b82f6]" : "bg-[#8b949e]"}`} />
            <div className="flex-1 w-px bg-[#30363d] mt-1" />
          </div>
          <div className="min-w-0 flex-1">
            {i === 0 && (
              <span className="inline-block text-[10px] text-[#3fb950] bg-[#10311a] border border-[#1f6f33] rounded-full px-1.5 mb-1">
                HEAD{branch ? ` → ${branch}` : ""}
              </span>
            )}
            <div className="text-sm text-[#e6edf3] truncate">{c.summary}</div>
            <div className="text-[11px] text-[#8b949e] font-mono">{c.short_id} · {formatRelative(c.timestamp)}</div>
          </div>
        </div>
      ))}
      {hasMore ? (
        <button className="w-full py-2 text-xs text-[#58a6ff] disabled:opacity-40" onClick={onLoadMore} disabled={loading}>
          {loading ? "加载中…" : "加载更多"}
        </button>
      ) : (
        commits.length > 0 && <div className="w-full py-2 text-center text-[11px] text-[#6e7681]">已到历史开端</div>
      )}
    </div>
  );
}
