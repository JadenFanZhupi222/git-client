import { type CommitDto, type GraphRowDto } from "../ipc";
import { formatRelative } from "../lib/time";

const LANE_W = 16; // 每条 lane 的像素宽
const ROW_H = 48; // 行高必须固定,否则跨行的连线对不齐
const NLANE = 8;
const laneColor = (c: number) => `var(--lane-${((c % NLANE) + NLANE) % NLANE})`;

export function CommitGraph({
  rows, branch, selectedId, onSelect, onLoadMore, loading, hasMore,
}: {
  rows: GraphRowDto[];
  branch: string | null;
  selectedId: string | null;
  onSelect: (c: CommitDto) => void;
  onLoadMore: () => void;
  loading: boolean;
  hasMore: boolean;
}) {
  // 首屏加载骨架
  if (loading && rows.length === 0) {
    return (
      <div className="overflow-hidden">
        {Array.from({ length: 8 }).map((_, i) => (
          <div key={i} className="flex items-center gap-2 px-3" style={{ height: ROW_H, opacity: 1 - i * 0.1 }}>
            <div className="skeleton h-2.5 w-2.5 shrink-0 rounded-full" />
            <div className="min-w-0 flex-1 space-y-1.5">
              <div className="skeleton h-3" style={{ width: `${70 - (i % 3) * 15}%` }} />
              <div className="skeleton h-2.5 w-2/5" />
            </div>
          </div>
        ))}
      </div>
    );
  }

  // gutter 宽度 = 所有行里出现过的最大列号 + 1
  let maxCol = 0;
  for (const r of rows) {
    maxCol = Math.max(maxCol, r.column);
    for (const s of r.top) maxCol = Math.max(maxCol, s.from, s.to);
    for (const s of r.bottom) maxCol = Math.max(maxCol, s.from, s.to);
  }
  const gutterW = (maxCol + 1) * LANE_W;
  const cx = (c: number) => c * LANE_W + LANE_W / 2;

  return (
    <div className="fade-in overflow-y-auto">
      {rows.map((r, i) => {
        const on = selectedId === r.commit.id;
        const head = i === 0;
        return (
          <div
            key={r.commit.id}
            onClick={() => onSelect(r.commit)}
            className={`flex cursor-pointer items-stretch border-l-2 transition-colors ${
              on ? "border-accent-emphasis bg-overlay" : "border-transparent hover:bg-elevated"
            }`}
            style={{ height: ROW_H }}
          >
            {/* 图谱泳道 */}
            <svg width={gutterW} height={ROW_H} className="shrink-0" style={{ minWidth: gutterW }}>
              {r.top.map((s, j) => (
                <line key={`t${j}`} x1={cx(s.from)} y1={0} x2={cx(s.to)} y2={ROW_H / 2}
                  stroke={laneColor(s.color)} strokeWidth={2} />
              ))}
              {r.bottom.map((s, j) => (
                <line key={`b${j}`} x1={cx(s.from)} y1={ROW_H / 2} x2={cx(s.to)} y2={ROW_H}
                  stroke={laneColor(s.color)} strokeWidth={2} />
              ))}
              <circle cx={cx(r.column)} cy={ROW_H / 2} r={4.5}
                fill={laneColor(r.color)}
                stroke={head ? "var(--color-accent)" : "transparent"}
                strokeWidth={head ? 2.5 : 0} />
            </svg>

            {/* 提交信息 */}
            <div className="flex min-w-0 flex-1 flex-col justify-center pr-3">
              <div className="truncate text-[13px] text-fg">{r.commit.summary}</div>
              <div className="flex items-center gap-1.5 truncate font-mono text-[11px] text-fg-muted">
                {head && (
                  <span className="rounded-full border border-success/40 bg-success/10 px-1 text-[10px] not-italic text-success">
                    HEAD{branch ? `→${branch}` : ""}
                  </span>
                )}
                <span className="truncate">{r.commit.short_id} · {formatRelative(r.commit.timestamp)}</span>
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
        rows.length > 0 && <div className="py-2.5 text-center text-[11px] text-fg-subtle">已到历史开端</div>
      )}
    </div>
  );
}
