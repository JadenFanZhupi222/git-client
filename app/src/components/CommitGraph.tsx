import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { type CommitDto, type GraphRowDto, type RefDto } from "../ipc";
import { CommitLines } from "./CommitLines";
import { ROW_H, cx, gutterWidth, topPath, botPath } from "../lib/graphGeometry";

const NLANE = 8;
const laneColor = (c: number) => `var(--lane-${((c % NLANE) + NLANE) % NLANE})`;

/** 行内引用徽章:HEAD(绿)/ 本地分支(强调色)/ 远程跟踪(中性)。
 *  HEAD 指向的当前分支并入「HEAD → 名」一枚,避免与本地分支重复。 */
function RefBadges({ refs }: { refs: RefDto[] }) {
  if (!refs.length) return null;
  const head = refs.find((r) => r.kind === "head");
  const locals = refs.filter((r) => r.kind === "local" && r.name !== head?.name);
  const remotes = refs.filter((r) => r.kind === "remote");
  const tags = refs.filter((r) => r.kind === "tag");
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
      {tags.map((r) => (
        <span key={`t-${r.name}`} className={`${pill} border border-warning/40 bg-warning/10 text-warning`}>
          ⌖ {r.name}
        </span>
      ))}
    </>
  );
}

export function CommitGraph({
  rows, selectedId, compareId, onSelect, onContext, onLoadMore, loading, hasMore, scrollToId, topInset = 0,
}: {
  rows: GraphRowDto[];
  selectedId: string | null;
  /** 比较模式下的第二个提交(对比目标),与 selectedId 一起高亮。 */
  compareId?: string | null;
  /** 键盘导航选中的提交 id:变化时把对应行滚进可视区(align auto:已可见则不动,不抖)。 */
  scrollToId?: string | null;
  /** opts.compare=true 表示按下了 Cmd/Ctrl(请求与已选提交比较)。 */
  onSelect: (c: CommitDto, opts?: { compare?: boolean }) => void;
  onContext?: (c: CommitDto, x: number, y: number) => void;
  onLoadMore: () => void;
  loading: boolean;
  hasMore: boolean;
  /** 顶部留白(px):液态玻璃工具栏浮在滚动体之上,这里预留出栏高,
   *  让首条提交落在玻璃栏下方,而往上滚的提交从玻璃栏底下穿过(满汉折射效果)。 */
  topInset?: number;
}) {
  // 滚动容器:虚拟化以它为测量基准。必须在所有 hook 之后再分支返回,故 ref/虚拟器
  // 始终调用(React hooks 规则:不能在条件后才调 hook)。
  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_H, // 行高固定,估计=实际,无需动态测量
    overscan: 12, // 视口外多渲染几行,快速滚动不露白
    paddingStart: topInset, // 顶部为浮动玻璃工具栏预留;首条提交落其下,上滚穿过栏底
  });

  // 键盘选中变化 → 把该行滚进可视区。align "auto" 只在行不可见时滚动,鼠标点选/已可见不抖。
  useEffect(() => {
    if (!scrollToId) return;
    const idx = rows.findIndex((r) => r.commit.id === scrollToId);
    if (idx >= 0) virtualizer.scrollToIndex(idx, { align: "auto" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scrollToId]);

  // 「活的图谱」新提交入场:记住见过的提交 id;只有「从未见过 + 落在顶部少数行」的提交
  // 才播入场动画(= 真正的新提交,如 commit/pull 后的 HEAD;翻页追加的旧提交在底部,不动)。
  // 首屏(isFirst)整体走容器 fade-in,不逐行闪。
  const seenIds = useRef<Set<string>>(new Set());
  const isFirst = useRef(true);
  const isNewCommit = (id: string, index: number) =>
    !isFirst.current && index < 8 && !seenIds.current.has(id);
  useEffect(() => {
    rows.forEach((r) => seenIds.current.add(r.commit.id));
    isFirst.current = false;
  }, [rows]);

  // hover 高亮:记住鼠标所在提交的泳道色,渲染时把同色泳道/节点点亮、其余淡下。
  // 设同值是 no-op(React 自动 bail),跨行移动只在颜色变化时重渲;离开整列才清空(避免行间闪烁)。
  const [hoverColor, setHoverColor] = useState<number | null>(null);

  // 首屏加载骨架(无数据时):不进虚拟化路径。
  if (loading && rows.length === 0) {
    return (
      <div className="h-full overflow-hidden" style={{ paddingTop: topInset }}>
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

  const gutterW = gutterWidth(rows);
  // 选中行下标 → 滑动高亮条的位置(只在选中变化时过渡,滚动不触发)。
  const selIdx = selectedId ? rows.findIndex((r) => r.commit.id === selectedId) : -1;

  return (
    <div ref={parentRef} className="fade-in h-full overflow-y-auto" onMouseLeave={() => setHoverColor(null)}>
      {/* 撑出全量高度的占位层;只有可见窗口内的行被真正渲染并绝对定位到各自位置。
          10 万提交也只挂十几个 DOM 节点,滚动恒定开销。 */}
      <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
        {/* 滑动选中高亮条:单个元素,选中变化时 translateY 平滑滑到目标行(macOS 列表式)。
            画在行之前 → 行内容绘于其上,文字/泳道清晰可读;pointer-events-none 不挡点击。
            偏移 = topInset(paddingStart) + 下标×ROW_H,与虚拟器的行起点口径一致。 */}
        {selIdx >= 0 && (
          <div
            aria-hidden
            className="pointer-events-none absolute left-0 top-0 w-full border-l-2 border-accent-emphasis bg-overlay"
            style={{ height: ROW_H, transform: `translateY(${topInset + selIdx * ROW_H}px)`, transition: "transform 0.18s cubic-bezier(0.22,1,0.36,1)" }}
          />
        )}
        {virtualizer.getVirtualItems().map((vrow) => {
          const r = rows[vrow.index];
          const isNew = isNewCommit(r.commit.id, vrow.index);
          const nodeDim = hoverColor !== null && r.color !== hoverColor;
          const on = selectedId === r.commit.id;
          const cmp = !on && compareId === r.commit.id;
          const isHead = r.refs.some((x) => x.kind === "head");
          // 同步状态:未 push=绿 / 未 pull=蓝(与状态栏 SyncBadge 的 ↑绿↓蓝 一致)。
          const syncColor =
            r.sync === "outgoing" ? "var(--color-success)"
            : r.sync === "incoming" ? "var(--color-accent)"
            : null;
          const syncTip =
            r.sync === "outgoing" ? "已提交，尚未 push 到远程"
            : r.sync === "incoming" ? "已 fetch，尚未 pull/合并到本地"
            : undefined;
          return (
            <div
              key={r.commit.id}
              onMouseEnter={() => setHoverColor(r.color)}
              onClick={(e) => onSelect(r.commit, { compare: e.metaKey || e.ctrlKey })}
              onContextMenu={(e) => { if (onContext) { e.preventDefault(); onSelect(r.commit); onContext(r.commit, e.clientX, e.clientY); } }}
              title={syncTip}
              className={`flex cursor-pointer items-stretch border-l-2 transition-colors ${isNew ? "commit-enter" : ""} ${
                on ? "border-transparent"
                : cmp ? "border-accent bg-accent/10"
                : "border-transparent hover:bg-elevated"
              }`}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                height: ROW_H,
                transform: `translateY(${vrow.start}px)`,
              }}
            >
              {/* 左侧同步色条:未 push/未 pull 的提交在行首画一条细竖条,一眼成组识别 */}
              <div
                className="w-[3px] shrink-0 self-stretch"
                style={{ background: syncColor ?? "transparent" }}
              />
              {/* 图谱泳道 */}
              <svg width={gutterW} height={ROW_H} className="shrink-0" style={{ minWidth: gutterW }}>
                {r.top.map((s, j) => {
                  const hl = hoverColor === null || s.color === hoverColor;
                  return (
                    <path key={`t${j}`} className="lane-path" d={topPath(s.from, s.to)} fill="none"
                      stroke={laneColor(s.color)} strokeWidth={hl ? 2.4 : 2} strokeOpacity={hl ? 1 : 0.2} strokeLinecap="round" />
                  );
                })}
                {r.bottom.map((s, j) => {
                  const hl = hoverColor === null || s.color === hoverColor;
                  return (
                    <path key={`b${j}`} className="lane-path" d={botPath(s.from, s.to)} fill="none"
                      stroke={laneColor(s.color)} strokeWidth={hl ? 2.4 : 2} strokeOpacity={hl ? 1 : 0.2} strokeLinecap="round" />
                  );
                })}
                {/* 光晕:用画布色描边把节点背后的泳道线「挖空」,圆点更干净 */}
                <circle cx={cx(r.column)} cy={ROW_H / 2} r={6.5} fill="var(--color-canvas)" />
                {/* 节点:已同步=实心(泳道色);未 push/未 pull=空心环(同步色),仿 JetBrains。
                    新提交时 node-pop 弹入(SVG scale,transform-box:fill-box 以圆心为原点)。 */}
                <circle cx={cx(r.column)} cy={ROW_H / 2} r={4.5}
                  className={isNew ? "node-pop" : undefined}
                  style={{ opacity: nodeDim ? 0.3 : 1, transition: "opacity 0.16s ease" }}
                  fill={syncColor ? "var(--color-canvas)" : laneColor(r.color)}
                  stroke={syncColor ?? (isHead ? "var(--color-accent)" : "transparent")}
                  strokeWidth={syncColor ? 2.5 : isHead ? 2.5 : 0} />
              </svg>

              {/* 提交信息 */}
              <div className="flex min-w-0 flex-1 flex-col justify-center pr-3">
                <CommitLines commit={r.commit} badges={<RefBadges refs={r.refs} />} />
              </div>
            </div>
          );
        })}
      </div>

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
