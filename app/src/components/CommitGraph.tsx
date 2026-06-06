import { type CommitDto, type GraphRowDto, type RefDto } from "../ipc";
import { formatRelative } from "../lib/time";

const LANE_W = 16; // 每条 lane 的像素宽
const ROW_H = 48; // 行高必须固定,否则跨行的连线对不齐
const NLANE = 8;
const laneColor = (c: number) => `var(--lane-${((c % NLANE) + NLANE) % NLANE})`;

/** 行内引用徽章:HEAD(绿)/ 本地分支(强调色)/ 远程跟踪(中性)。
 *  HEAD 指向的当前分支并入「HEAD → 名」一枚,避免与本地分支重复。 */
function RefBadges({ refs }: { refs: RefDto[] }) {
  if (!refs.length) return null;
  const head = refs.find((r) => r.kind === "head");
  const locals = refs.filter((r) => r.kind === "local" && r.name !== head?.name);
  const remotes = refs.filter((r) => r.kind === "remote");
  const pill = "shrink-0 rounded-full px-1.5 text-[10px] font-mono not-italic leading-[1.4]";
  return (
    <>
      {head && (
        <span className={`${pill} border border-success/40 bg-success/10 text-success`}>
          {head.name === "HEAD" ? "HEAD" : `HEAD → ${head.name}`}
        </span>
      )}
      {locals.map((r) => (
        <span key={`l-${r.name}`} className={`${pill} border border-accent/40 bg-accent/10 text-accent`}>
          {r.name}
        </span>
      ))}
      {remotes.map((r) => (
        <span key={`r-${r.name}`} className={`${pill} border border-line-strong bg-elevated text-fg-muted`}>
          {r.name}
        </span>
      ))}
    </>
  );
}

export function CommitGraph({
  rows, selectedId, onSelect, onLoadMore, loading, hasMore,
}: {
  rows: GraphRowDto[];
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
  const MID = ROW_H / 2;
  // 上半段:顶边 → 中点。直列时是竖线;换列时用三次 bezier,两端切线竖直 →
  // lane 在自己列里走直线,只在拐点柔和地弯,消除生硬的对角线/锯齿。
  const topPath = (from: number, to: number) => {
    const x1 = cx(from), x2 = cx(to);
    if (x1 === x2) return `M${x1},0 L${x1},${MID}`;
    return `M${x1},0 C${x1},${MID / 2} ${x2},${MID / 2} ${x2},${MID}`;
  };
  // 下半段:中点 → 底边。
  const botPath = (from: number, to: number) => {
    const x1 = cx(from), x2 = cx(to);
    if (x1 === x2) return `M${x1},${MID} L${x1},${ROW_H}`;
    return `M${x1},${MID} C${x1},${MID + MID / 2} ${x2},${MID + MID / 2} ${x2},${ROW_H}`;
  };

  return (
    <div className="fade-in overflow-y-auto">
      {rows.map((r) => {
        const on = selectedId === r.commit.id;
        const isHead = r.refs.some((x) => x.kind === "head");
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
                <path key={`t${j}`} d={topPath(s.from, s.to)} fill="none"
                  stroke={laneColor(s.color)} strokeWidth={2} strokeLinecap="round" />
              ))}
              {r.bottom.map((s, j) => (
                <path key={`b${j}`} d={botPath(s.from, s.to)} fill="none"
                  stroke={laneColor(s.color)} strokeWidth={2} strokeLinecap="round" />
              ))}
              {/* 光晕:用画布色描边把节点背后的泳道线「挖空」,圆点更干净 */}
              <circle cx={cx(r.column)} cy={ROW_H / 2} r={6.5} fill="var(--color-canvas)" />
              <circle cx={cx(r.column)} cy={ROW_H / 2} r={4.5}
                fill={laneColor(r.color)}
                stroke={isHead ? "var(--color-accent)" : "transparent"}
                strokeWidth={isHead ? 2.5 : 0} />
            </svg>

            {/* 提交信息 */}
            <div className="flex min-w-0 flex-1 flex-col justify-center pr-3">
              <div className="flex items-center gap-1 overflow-hidden">
                <RefBadges refs={r.refs} />
                <span className="truncate text-[13px] text-fg">{r.commit.summary}</span>
              </div>
              <div className="truncate font-mono text-[11px] text-fg-muted">
                {r.commit.short_id} · {r.commit.author_name} · {formatRelative(r.commit.timestamp)}
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
