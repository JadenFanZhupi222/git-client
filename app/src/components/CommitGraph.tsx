import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { type CommitDto, type GraphRowDto, type RefDto } from "../ipc";
import { CommitLines } from "./CommitLines";
import { Spine } from "./ui/Spine";
import { ROW_H, cx, gutterWidth, topPath, botPath } from "../lib/graphGeometry";
import { useT } from "../lib/i18n";

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
  // 徽章圆角 5(对齐原型,非全圆);HEAD/本地 = 朱红家族,远程 = 中性,标签 = warning + tag 图标。
  const pill = "inline-flex shrink-0 items-center gap-1 rounded-[5px] px-1.5 text-[10px] font-mono font-semibold not-italic leading-[1.4]";
  return (
    <>
      {head && (
        <span className={`${pill} bg-accent/[0.16] text-accent-ink`}>
          {head.name === "HEAD" ? "HEAD" : `HEAD → ${head.name}`}
        </span>
      )}
      {locals.map((r) => (
        <span key={`l-${r.name}`} className={`${pill} bg-accent/10 text-accent-ink`}>
          {r.name}
        </span>
      ))}
      {remotes.map((r) => (
        <span key={`r-${r.name}`} className={`${pill} bg-fg/[0.08] text-fg-muted`}>
          {r.name}
        </span>
      ))}
      {tags.map((r) => (
        <span key={`t-${r.name}`} className={`${pill} bg-warning/[0.16] text-warning`}>
          <svg width={9} height={9} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.8}>
            <path d="M2 7 7 2h5v5l-5 5z" />
            <circle cx={9.5} cy={5.5} r={0.6} fill="currentColor" />
          </svg>
          {r.name}
        </span>
      ))}
    </>
  );
}

export function CommitGraph({
  rows, selectedId, compareId, onSelect, onContext, onCherryPick, onLoadMore, loading, hasMore, scrollToId, topInset = 0,
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
  /** 拖一个提交丢到 HEAD 行 → 把它 cherry-pick 到当前分支(直接操作)。提供时行变可拖。 */
  onCherryPick?: (c: CommitDto) => void;
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

  const t = useT();
  // hover 高亮:记住鼠标所在提交的泳道色,渲染时把同色泳道/节点点亮、其余淡下。
  // 设同值是 no-op(React 自动 bail),跨行移动只在颜色变化时重渲;离开整列才清空(避免行间闪烁)。
  const [hoverColor, setHoverColor] = useState<number | null>(null);
  // 正在拖拽的提交 id(拖放 cherry-pick)。state 驱动视觉;ref 给 dragover/drop 同步读
  // (避免 setState 落后于 dragover 那一帧 → 判定取到 null)。
  const [dragId, setDragId] = useState<string | null>(null);
  const dragIdRef = useRef<string | null>(null);

  // 智能护栏:从 HEAD 沿 parents BFS 遍历已加载行,得到「已在当前分支」的提交集合。
  // 把这类提交拖到 HEAD → cherry-pick 只会得到空提交,故它们不接受投放(no-drop 光标)。
  // 未加载到的远祖会漏判(false-negative),那种情况退回后端报错 + toast 兜底,不会误拦正常拣选。
  const headId = useMemo(
    () => rows.find((r) => r.refs.some((x) => x.kind === "head"))?.commit.id ?? null,
    [rows],
  );
  const reachableFromHead = useMemo(() => {
    const set = new Set<string>();
    if (!headId) return set;
    const parentsById = new Map(rows.map((r) => [r.commit.id, r.commit.parents] as const));
    const stack = [headId];
    while (stack.length) {
      const id = stack.pop()!;
      if (set.has(id)) continue;
      set.add(id);
      for (const p of parentsById.get(id) ?? []) if (!set.has(p)) stack.push(p);
    }
    return set;
  }, [rows, headId]);

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
  // 当前被拖的提交是否已在当前分支(HEAD 可达)→ 投放无效,不把 HEAD 行点亮成投放区。
  const draggedInBranch = dragId !== null && reachableFromHead.has(dragId);

  // 拖放进行中且被拖提交是有效拣选目标 → 图谱顶部显示常驻投放区(不依赖 HEAD 行是否在视口内)。
  const showDropZone = dragId !== null && !draggedInBranch && !!onCherryPick;

  return (
    <div className="relative h-full">
    <div
      ref={parentRef}
      role="listbox"
      aria-label={t("history.commitHistory")}
      className="fade-in h-full overflow-y-auto"
      onMouseLeave={() => setHoverColor(null)}
    >
      {/* 撑出全量高度的占位层;只有可见窗口内的行被真正渲染并绝对定位到各自位置。
          10 万提交也只挂十几个 DOM 节点,滚动恒定开销。 */}
      <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
        {/* 滑动选中高亮条:单个元素,选中变化时 translateY 平滑滑到目标行(macOS 列表式)。
            画在行之前 → 行内容绘于其上,文字/泳道清晰可读;pointer-events-none 不挡点击。
            偏移 = topInset(paddingStart) + 下标×ROW_H,与虚拟器的行起点口径一致。 */}
        {selIdx >= 0 && (
          <div
            aria-hidden
            className="pointer-events-none absolute left-0 top-0 w-full bg-accent/10"
            style={{ height: ROW_H, transform: `translateY(${topInset + selIdx * ROW_H}px)`, transition: "transform 0.18s cubic-bezier(0.22,1,0.36,1)" }}
          >
            <Spine />
          </div>
        )}
        {virtualizer.getVirtualItems().map((vrow) => {
          const r = rows[vrow.index];
          const isNew = isNewCommit(r.commit.id, vrow.index);
          const nodeDim = hoverColor !== null && r.color !== hoverColor;
          const on = selectedId === r.commit.id;
          const cmp = !on && compareId === r.commit.id;
          const isHead = r.refs.some((x) => x.kind === "head");
          const isMerge = r.commit.parents.length > 1;
          // 节点半径:选中放大到 6,合并 4.5,普通 5(对齐原型)。
          const nodeR = on ? 6 : isMerge ? 4.5 : 5;
          // 拖放 cherry-pick:被拖的行半透明;HEAD 行(非被拖那条)在拖拽时变投放区。
          const isDragged = dragId === r.commit.id;
          const isDropTarget = dragId !== null && isHead && !isDragged && !!onCherryPick && !draggedInBranch;
          // 同步状态:未 push=绿 / 未 pull=蓝(与状态栏 SyncBadge 的 ↑绿↓蓝 一致)。
          const syncColor =
            r.sync === "outgoing" ? "var(--color-success)"
            : r.sync === "incoming" ? "var(--color-accent)"
            : null;
          const syncTip =
            r.sync === "outgoing" ? t("graph.syncOutgoing")
            : r.sync === "incoming" ? t("graph.syncIncoming")
            : undefined;
          return (
            <div
              key={r.commit.id}
              role="option"
              aria-selected={on}
              aria-label={`${r.commit.summary}, ${r.commit.author_name}, ${r.commit.short_id}`}
              aria-posinset={vrow.index + 1}
              aria-setsize={rows.length}
              tabIndex={on ? 0 : -1}
              draggable={!!onCherryPick}
              onDragStart={onCherryPick ? (e) => { dragIdRef.current = r.commit.id; setDragId(r.commit.id); e.dataTransfer.effectAllowed = "copy"; e.dataTransfer.setData("text/plain", r.commit.id); } : undefined}
              onDragEnd={() => { dragIdRef.current = null; setDragId(null); }}
              // 接受投放只看静态的 isHead(不依赖 dragId 状态,避免 setState 赶不上 dragover → 全程禁用光标)。
              // 被拖提交从 dragIdRef 同步读;已在当前分支(HEAD 可达)→ dropEffect=none(no-drop 光标),否则 copy(「+」)。
              onDragOver={isHead && onCherryPick ? (e) => { e.preventDefault(); const id = dragIdRef.current; e.dataTransfer.dropEffect = id && reachableFromHead.has(id) ? "none" : "copy"; } : undefined}
              onDrop={isHead && onCherryPick ? (e) => { e.preventDefault(); const id = e.dataTransfer.getData("text/plain"); dragIdRef.current = null; setDragId(null); if (!id || reachableFromHead.has(id)) return; const c = rows.find((x) => x.commit.id === id)?.commit; if (c && c.id !== r.commit.id) onCherryPick(c); } : undefined}
              onMouseEnter={() => setHoverColor(r.color)}
              onClick={(e) => onSelect(r.commit, { compare: e.metaKey || e.ctrlKey })}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onSelect(r.commit, { compare: e.metaKey || e.ctrlKey });
                }
              }}
              onContextMenu={(e) => { if (onContext) { e.preventDefault(); onSelect(r.commit); onContext(r.commit, e.clientX, e.clientY); } }}
              title={syncTip}
              className={`flex items-stretch transition-colors ${onCherryPick ? "cursor-grab active:cursor-grabbing" : "cursor-pointer"} ${isNew ? "commit-enter" : ""} ${isDragged ? "opacity-40" : ""} ${
                isDropTarget ? "bg-accent/10 ring-2 ring-inset ring-accent"
                : cmp ? "border-l-2 border-accent bg-accent/10"
                : "hover:bg-elevated"
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
                      stroke={laneColor(s.color)} strokeWidth={hl ? 2.4 : 1.8} strokeOpacity={hl ? 1 : 0.6} strokeLinecap="round" />
                  );
                })}
                {r.bottom.map((s, j) => {
                  const hl = hoverColor === null || s.color === hoverColor;
                  return (
                    <path key={`b${j}`} className="lane-path" d={botPath(s.from, s.to)} fill="none"
                      stroke={laneColor(s.color)} strokeWidth={hl ? 2.4 : 1.8} strokeOpacity={hl ? 1 : 0.6} strokeLinecap="round" />
                  );
                })}
                {/* 节点(对齐 Strata 原型):合并=空心环(画布填充 + 泳道描边);普通=实心泳道色 +
                    画布描边把背后泳道挖干净;选中放大到 r6 + 泳道色辉光;HEAD 外加一圈淡环。
                    同步状态(未 push/pull)不再改节点形态,改由行首竖色条表达,避免与「合并空心」语义撞。
                    新提交时 node-pop 弹入(transform-box:fill-box 以圆心为原点)。 */}
                {isMerge ? (
                  <circle cx={cx(r.column)} cy={ROW_H / 2} r={nodeR}
                    className={isNew ? "node-pop" : undefined}
                    style={{ opacity: nodeDim ? 0.3 : 1, transition: "opacity 0.16s ease" }}
                    fill="var(--color-canvas)" stroke={laneColor(r.color)} strokeWidth={2} />
                ) : (
                  <circle cx={cx(r.column)} cy={ROW_H / 2} r={nodeR}
                    className={isNew ? "node-pop" : undefined}
                    style={{ opacity: nodeDim ? 0.3 : 1, transition: "opacity 0.16s ease", filter: on ? `drop-shadow(0 0 6px ${laneColor(r.color)})` : undefined }}
                    fill={laneColor(r.color)} stroke="var(--color-canvas)" strokeWidth={2} />
                )}
                {isHead && (
                  <circle cx={cx(r.column)} cy={ROW_H / 2} r={nodeR + 3}
                    fill="none" stroke={laneColor(r.color)} strokeWidth={1.4} strokeOpacity={0.5}
                    style={{ opacity: nodeDim ? 0.3 : 1 }} />
                )}
              </svg>

              {/* 提交信息 */}
              <div className="flex min-w-0 flex-1 items-center pr-3">
                <CommitLines commit={r.commit} badges={<RefBadges refs={r.refs} />} selected={on} />
              </div>

              {/* 投放区提示:拖到当前分支(HEAD)行时显示「松开 → 拣选」 */}
              {isDropTarget && (
                <span className="popover pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 shrink-0 rounded-full bg-accent px-2 py-0.5 text-[10px] font-medium text-white">
                  {t("graph.dropToBranch")}
                </span>
              )}
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
          {loading ? t("common.loading") : t("graph.loadMore")}
        </button>
      ) : (
        rows.length > 0 && <div className="py-2.5 text-center text-[11px] text-fg-muted">{t("graph.end")}</div>
      )}
      </div>

      {/* 常驻投放区:拖拽进行中浮在图谱顶部(玻璃栏下方),HEAD 行滚出视口也能投放。
          只在被拖提交是有效拣选目标时出现(已在当前分支的不显)。 */}
      {showDropZone && (
        <div
          onDragOver={(e) => { e.preventDefault(); e.dataTransfer.dropEffect = "copy"; }}
          onDrop={(e) => { e.preventDefault(); const id = e.dataTransfer.getData("text/plain"); dragIdRef.current = null; setDragId(null); if (!id || reachableFromHead.has(id)) return; const c = rows.find((x) => x.commit.id === id)?.commit; if (c && onCherryPick) onCherryPick(c); }}
          className="popover absolute inset-x-2 z-20 flex items-center justify-center gap-1.5 rounded-lg border border-dashed border-accent bg-accent/10 py-2 text-xs font-medium text-accent-ink backdrop-blur-sm"
          style={{ top: topInset + 8 }}
        >
          {t("graph.dropToCurrent")}
        </div>
      )}
    </div>
  );
}
