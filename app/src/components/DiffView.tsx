import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { readImage, type DiffLineDto, type FileDiffDto, type ImageRefDto } from "../ipc";
import { buildDiffRows, maxContentCols, type DiffRow, type LineRef, type SbsRow } from "../lib/diffRows";
import { ChevronDownIcon, FileDiffIcon } from "./icons";
import { EmptyHint } from "./ui/EmptyHint";
import { useT } from "../lib/i18n";

type Side = "old" | "new";

type ViewMode = "unified" | "split";
/** 每行固定高度(px)。font-mono text-[12px] leading-5 → 20px;hunk 头/折叠条同高。
 *  固定高度让虚拟化「估计=实际」,无需动态测量。 */
const ROW_H = 20;
const VIEW_KEY = "diff-view-mode";
function getStoredView(): ViewMode {
  return localStorage.getItem(VIEW_KEY) === "split" ? "split" : "unified";
}

/** 单文件行级 diff 渲染。支持 unified(上下统一)与 split(左右并排)两种视图。 */
export function DiffView({
  diff,
  loading,
  hasFile,
  repo,
  hunkAction,
  lineStage,
}: {
  diff: FileDiffDto | null;
  loading: boolean;
  hasFile: boolean;
  /** 仓库路径:图片 diff 取字节(read_image)用;非图片可省。 */
  repo?: string;
  /** 可选:每个 hunk 头部显示一个动作按钮(Changes 视图的「暂存/取消暂存此块」)。 */
  hunkAction?: { label: string; onAct: (hunkIndex: number) => void; disabled?: boolean };
  /** 可选:开启行级选择暂存(仅未暂存改动)。点 +/- 行选中,hunk 头出现「暂存选中行」。 */
  lineStage?: { onStage: (hunkIndex: number, lines: number[]) => void; disabled?: boolean };
}) {
  // 选中的行:键 `${hunkIdx}:${lineIdx}`。diff 变化(切文件)时清空。
  const [selected, setSelected] = useState<Set<string>>(new Set());
  useEffect(() => { setSelected(new Set()); }, [diff]);
  // 已展开的折叠块:键 `${hunkIdx}:${首个被折叠行的 li}`。切文件时清空。
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  useEffect(() => { setExpanded(new Set()); }, [diff]);
  const expand = (key: string) => setExpanded((prev) => new Set(prev).add(key));
  const [view, setView] = useState<ViewMode>(getStoredView);
  const setViewPersist = (v: ViewMode) => { setView(v); localStorage.setItem(VIEW_KEY, v); };
  const t = useT();
  const toggle = (key: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(key) ? next.delete(key) : next.add(key);
      return next;
    });
  if (!hasFile) {
    return <EmptyHint icon={<FileDiffIcon width={24} height={24} />}>{t("diff.empty")}</EmptyHint>;
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
    return <Center>{t("diff.loadFailed")}</Center>;
  }
  if (diff.is_lfs_pointer) {
    const size = formatBytes(diff.lfs_size);
    return <Center>{t("diff.lfs", { size: size ? `(${size})` : "" })}</Center>;
  }
  if (diff.is_image) {
    // 图片是二进制的一种,但能并排预览新旧图(此分支须在 is_binary 之前)。
    if (!diff.old_image && !diff.new_image) {
      return <Center>{t("diff.imageSkipped")}</Center>;
    }
    if (!repo) {
      return <Center>{t("diff.imgPreviewFailed")}</Center>;
    }
    return <ImageDiff diff={diff} repo={repo} />;
  }
  if (diff.is_binary) {
    return <Center>{t("diff.binary")}</Center>;
  }
  if (diff.too_large) {
    return <Center>{t("diff.tooLarge")}</Center>;
  }
  if (diff.hunks.length === 0) {
    return <Center>{t("diff.noTextChange")}</Center>;
  }

  return (
    <div className="fade-in flex flex-1 flex-col overflow-hidden font-mono text-[12px] leading-5">
      <ViewToggle view={view} onChange={setViewPersist} />
      <VirtualDiff
        diff={diff}
        view={view}
        selected={selected}
        toggle={toggle}
        lineStage={lineStage}
        hunkAction={hunkAction}
        expanded={expanded}
        expand={expand}
      />
    </div>
  );
}

/** 统一/并排切换条。 */
function ViewToggle({ view, onChange }: { view: ViewMode; onChange: (v: ViewMode) => void }) {
  const t = useT();
  const btn = (v: ViewMode, label: string) => (
    <button
      onClick={() => onChange(v)}
      className={`rounded px-2.5 py-0.5 text-[11px] transition-colors ${
        view === v ? "bg-accent/15 text-accent" : "text-fg-muted hover:bg-overlay hover:text-fg"
      }`}
    >
      {label}
    </button>
  );
  return (
    <div className="flex shrink-0 select-none items-center justify-end border-b border-line bg-overlay px-3 py-1.5">
      <div className="flex items-center gap-0.5 rounded-md border border-line bg-elevated/60 p-0.5">
        {btn("unified", t("diff.unified"))}
        {btn("split", t("diff.split"))}
      </div>
    </div>
  );
}

type StageProps = {
  selected: Set<string>;
  toggle: (key: string) => void;
  lineStage?: { onStage: (hunkIndex: number, lines: number[]) => void; disabled?: boolean };
  hunkAction?: { label: string; onAct: (hunkIndex: number) => void; disabled?: boolean };
};

/** 折叠相关 props:已展开的折叠块键集合 + 展开回调。 */
type FoldProps = {
  expanded: Set<string>;
  expand: (key: string) => void;
};

/**
 * 虚拟化 diff 主体:把整个 FileDiff 拍平成一维渲染项(buildDiffRows),只挂可视窗口内的行,
 * 近 2 万行也恒定开销。统一/并排共用一个纵向滚动容器(纵向天然同步);横向宽度用等宽字体
 * 的 ch 单位由最长内容算出,短 diff 则铺满视口(minWidth 100%)。
 */
function VirtualDiff({
  diff,
  view,
  selected,
  toggle,
  lineStage,
  hunkAction,
  expanded,
  expand,
}: { diff: FileDiffDto; view: ViewMode } & StageProps & FoldProps) {
  const rows = useMemo(() => buildDiffRows(diff, expanded, view), [diff, expanded, view]);
  const hasSel = !!lineStage;
  const split = view === "split";
  // 统一视图:横向滚动体宽 = gutter(选择+旧/新行号+符号+右内边距)+ 最长内容 ch,外层容器双向滚动。
  // 并排视图:体宽 = 100%(铺满视口),左右各 50%,长行在「半栏内部自动换行」→ 不横滚、不超宽。
  const bodyWidth = useMemo<string>(() => {
    if (split) return "100%";
    const gutter = (hasSel ? 16 : 0) + 40 + 40 + 16 + 12;
    return `calc(${gutter}px + ${maxContentCols(diff)}ch)`;
  }, [diff, split, hasSel]);

  const parentRef = useRef<HTMLDivElement>(null);
  // 并排换行 → 行高可变,改用动态测量(measureElement + ResizeObserver):统一/折叠等固定行测到 ROW_H,
  // 并排里换了行的内容测到实际高度。估计值仍给 ROW_H,首帧不抖。
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_H,
    overscan: 16,
  });

  return (
    // 并排:外层只纵向滚动(横向靠换行,无横滚条)。统一:双向滚动,横向看长行。
    <div ref={parentRef} className={`flex-1 ${split ? "overflow-y-auto overflow-x-hidden" : "overflow-auto"}`}>
      {/* 撑出全量高/宽的占位层;可视窗口内的行绝对定位到各自 y。minWidth 100%:短 diff 铺满视口。 */}
      <div style={{ height: virtualizer.getTotalSize(), width: bodyWidth, minWidth: "100%", position: "relative" }}>
        {virtualizer.getVirtualItems().map((vi) => (
          <div
            key={vi.key}
            data-index={vi.index}
            ref={virtualizer.measureElement}
            style={{ position: "absolute", top: 0, left: 0, width: "100%", transform: `translateY(${vi.start}px)` }}
          >
            <DiffRowView
              r={rows[vi.index]}
              selected={selected}
              toggle={toggle}
              lineStage={lineStage}
              hunkAction={hunkAction}
              expand={expand}
            />
          </div>
        ))}
      </div>
    </div>
  );
}

/** 一条渲染项的分派:hunk 头 / 折叠条 / 统一行 / 并排配对行。 */
function DiffRowView({
  r,
  selected,
  toggle,
  lineStage,
  hunkAction,
  expand,
}: {
  r: DiffRow;
} & StageProps & Pick<FoldProps, "expand">) {
  switch (r.kind) {
    case "hunk": {
      const selCount = r.hunk.lines.filter((_, li) => selected.has(`${r.hi}:${li}`)).length;
      return <StageHeader h={r.hunk} hi={r.hi} selCount={selCount} selected={selected} lineStage={lineStage} hunkAction={hunkAction} />;
    }
    case "fold":
      return <FoldBar count={r.count} onExpand={() => expand(r.foldKey)} />;
    case "uline":
      return <UnifiedLine hi={r.hi} ref_={r.ref} selected={selected} toggle={toggle} lineStage={lineStage} />;
    case "pair":
      return <PairRow hi={r.hi} row={r.row} selected={selected} toggle={toggle} lineStage={lineStage} />;
  }
}

/** unified 视图的一行。 */
function UnifiedLine({ hi, ref_, selected, toggle, lineStage }: { hi: number; ref_: LineRef } & Pick<StageProps, "selected" | "toggle" | "lineStage">) {
  const l = ref_.line;
  const add = l.kind === "add";
  const del = l.kind === "del";
  const rowBg = add ? "bg-add-bg" : del ? "bg-del-bg" : "";
  const sign = add ? "+" : del ? "-" : " ";
  const signCls = add ? "text-success" : del ? "text-danger" : "text-fg-subtle";
  const selectable = !!lineStage && (add || del);
  const key = `${hi}:${ref_.li}`;
  const on = selected.has(key);
  return (
    <div
      onClick={selectable ? () => toggle(key) : undefined}
      // min-w-full:行宽 = 外层占位层体宽(= 最长行 ch),所有行齐平铺满;min-h-5 = 固定行高 ROW_H。
      className={`flex min-h-5 min-w-full items-center ${rowBg} ${selectable ? "cursor-pointer" : ""} ${on ? "ring-1 ring-inset ring-accent/60" : ""}`}
    >
      {lineStage && (
        <span className="w-4 shrink-0 select-none text-center text-[10px] text-accent">
          {selectable ? (on ? "✓" : "·") : ""}
        </span>
      )}
      <Gutter n={l.old_lineno} />
      <Gutter n={l.new_lineno} border />
      <span className={`w-4 shrink-0 select-none text-center ${signCls}`}>{sign}</span>
      <span className="flex-1 whitespace-pre pr-3 text-fg">
        <LineContent line={l} add={add} del={del} />
      </span>
    </div>
  );
}

/** 折叠条:替代一段被折叠的未改区,点击展开。统一/并排共用。标签左对齐(横向滚动时仍可见)。 */
function FoldBar({ count, onExpand }: { count: number; onExpand: () => void }) {
  const t = useT();
  return (
    <div
      onClick={onExpand}
      className="flex min-h-5 min-w-full cursor-pointer select-none items-center gap-1 border-y border-line/60 bg-overlay/40 pl-3 text-[11px] text-fg-subtle transition-colors hover:bg-overlay hover:text-accent"
    >
      <ChevronDownIcon className="h-3 w-3" />
      {t("diff.expand", { count })}
    </div>
  );
}

/** 并排配对行:左旧 | 右新 一行内并置,各占一半宽度(50/50)。长行在半栏内「自动换行」,不横滚、不超宽。
 *  items-stretch 让左右两半等高(取较高者),换行后两侧仍逐行对齐;竖向由外层虚拟化容器统一驱动。 */
function PairRow({ hi, row, selected, toggle, lineStage }: { hi: number; row: SbsRow } & Pick<StageProps, "selected" | "toggle" | "lineStage">) {
  return (
    <div className="flex w-full items-stretch">
      <HalfCell side="old" cell={row.left} hi={hi} selected={selected} toggle={toggle} lineStage={lineStage} />
      <HalfCell side="new" cell={row.right} hi={hi} selected={selected} toggle={toggle} lineStage={lineStage} border />
    </div>
  );
}

/** 并排某一侧的一格(取 row 的 left 或 right);对侧有内容本侧无则占位空行。
 *  行号/符号顶对齐(items-start),内容列自动换行(whitespace-pre-wrap + 任意处断词),GitHub 式。 */
function HalfCell({ hi, side, cell, border, selected, toggle, lineStage }: { hi: number; side: Side; cell: SbsRow["left"] } & Pick<StageProps, "selected" | "toggle" | "lineStage"> & { border?: boolean }) {
  const borderCls = border ? "border-l border-line" : "";
  if (!cell) {
    return <div className={`min-h-5 w-1/2 min-w-0 bg-overlay/40 ${borderCls}`}>&nbsp;</div>;
  }
  const l = cell.line;
  const add = l.kind === "add";
  const del = l.kind === "del";
  const rowBg = add ? "bg-add-bg" : del ? "bg-del-bg" : "";
  const sign = add ? "+" : del ? "-" : " ";
  const signCls = add ? "text-success" : del ? "text-danger" : "text-fg-subtle";
  const selectable = !!lineStage && (add || del);
  const key = `${hi}:${cell.li}`;
  const on = selected.has(key);
  const lineno = side === "old" ? l.old_lineno : l.new_lineno;
  return (
    <div
      onClick={selectable ? () => toggle(key) : undefined}
      className={`flex min-h-5 w-1/2 min-w-0 items-start ${borderCls} ${rowBg} ${selectable ? "cursor-pointer" : ""} ${on ? "ring-1 ring-inset ring-accent/60" : ""}`}
    >
      {lineStage && (
        <span className="w-4 shrink-0 select-none text-center text-[10px] text-accent">
          {selectable ? (on ? "✓" : "·") : ""}
        </span>
      )}
      <Gutter n={lineno} border />
      <span className={`w-4 shrink-0 select-none text-center ${signCls}`}>{sign}</span>
      <span className="min-w-0 flex-1 whitespace-pre-wrap pr-3 text-fg [overflow-wrap:anywhere]">
        <LineContent line={l} add={add} del={del} />
      </span>
    </div>
  );
}

/** hunk 头(带行级暂存按钮)。这里把「选中行下标」从 selected 里实算出来传给 onStage。 */
function StageHeader({
  h,
  hi,
  selCount,
  selected,
  lineStage,
  hunkAction,
}: {
  h: { header: string; lines: DiffLineDto[] };
  hi: number;
  selCount: number;
} & Pick<StageProps, "selected" | "lineStage" | "hunkAction">) {
  const t = useT();
  return (
    <div className="group flex min-h-5 min-w-full select-none items-center gap-2 bg-overlay px-3 text-[11px] text-accent/80">
      <span className="shrink-0">{h.header}</span>
      {((lineStage && selCount > 0) || hunkAction) && (
        <div className="flex shrink-0 items-center gap-2">
          {lineStage && selCount > 0 && (
            <button
              disabled={lineStage.disabled}
              onClick={() => {
                const lines = h.lines.map((_, li) => li).filter((li) => selected.has(`${hi}:${li}`));
                lineStage.onStage(hi, lines);
              }}
              className="shrink-0 rounded border border-accent bg-accent/15 px-1.5 py-px text-[10px] text-accent transition-colors hover:bg-accent/25 disabled:opacity-40"
            >
              {t("diff.stageSelected", { count: selCount })}
            </button>
          )}
          {hunkAction && (
            <button
              disabled={hunkAction.disabled}
              onClick={() => hunkAction.onAct(hi)}
              className="shrink-0 rounded border border-line-strong bg-elevated px-1.5 py-px text-[10px] text-fg-muted opacity-0 transition-colors hover:border-accent hover:bg-overlay hover:text-accent group-hover:opacity-100 disabled:opacity-40"
            >
              {hunkAction.label}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

/** 行内容渲染:有词级 emphasis 则逐段高亮(changed 段深一档底色),否则整行纯文本。两视图共用。 */
function LineContent({ line, add, del }: { line: DiffLineDto; add: boolean; del: boolean }) {
  if (line.emphasis && line.emphasis.length > 0) {
    return (
      <>
        {line.emphasis.map((s, si) =>
          s.changed ? (
            <span key={si} className={add ? "bg-success/30" : del ? "bg-danger/30" : ""}>
              {s.text}
            </span>
          ) : (
            <span key={si}>{s.text}</span>
          ),
        )}
      </>
    );
  }
  return <>{line.content || " "}</>;
}

type ImgMode = "side" | "swipe" | "onion";
const IMG_MODE_KEY = "img-diff-mode";
function getStoredImgMode(): ImgMode {
  const v = localStorage.getItem(IMG_MODE_KEY);
  return v === "swipe" || v === "onion" ? v : "side";
}

/** 一张图的字节加载状态:经 read_image 取 ArrayBuffer → Blob URL(随依赖变化/卸载时 revoke)。 */
type LoadedImage = { url: string | null; bytes: number; loading: boolean; error: boolean };
function useImageUrl(repo: string, ref: ImageRefDto | null, path: string): LoadedImage {
  const [state, setState] = useState<LoadedImage>({ url: null, bytes: 0, loading: !!ref, error: false });
  useEffect(() => {
    if (!ref) {
      setState({ url: null, bytes: 0, loading: false, error: false });
      return;
    }
    let url: string | null = null;
    let alive = true;
    setState({ url: null, bytes: 0, loading: true, error: false });
    readImage(repo, ref.oid, path)
      .then((buf) => {
        if (!alive) return;
        url = URL.createObjectURL(new Blob([buf], { type: ref.mime }));
        setState({ url, bytes: buf.byteLength, loading: false, error: false });
      })
      .catch(() => alive && setState({ url: null, bytes: 0, loading: false, error: true }));
    return () => {
      alive = false;
      if (url) URL.revokeObjectURL(url);
    };
  }, [repo, ref?.oid, ref?.mime, path]);
  return state;
}

/** 图片 diff:新旧两版预览。两版都在时给「并排 / 滑块 / 洋葱皮」三种对比模式;只有一侧时直接显该侧。 */
function ImageDiff({ diff, repo }: { diff: FileDiffDto; repo: string }) {
  const t = useT();
  const oldImg = useImageUrl(repo, diff.old_image, diff.path);
  const newImg = useImageUrl(repo, diff.new_image, diff.path);
  const both = !!diff.old_image && !!diff.new_image;
  const [mode, setMode] = useState<ImgMode>(getStoredImgMode);
  const setModePersist = (m: ImgMode) => { setMode(m); localStorage.setItem(IMG_MODE_KEY, m); };

  if (!both) {
    // 新增 → 单栏「新增」;删除 → 单栏「已删除」。
    return (
      <div className="fade-in flex flex-1 items-stretch gap-3 overflow-auto p-4">
        {diff.old_image && <ImagePane img={oldImg} label={t("diff.imgDeleted")} tone="danger" />}
        {diff.new_image && <ImagePane img={newImg} label={t("diff.imgAdded")} tone="success" />}
      </div>
    );
  }

  return (
    <div className="fade-in flex flex-1 flex-col overflow-hidden">
      <ImgModeBar mode={mode} onChange={setModePersist} />
      {mode === "side" ? (
        <div className="flex flex-1 items-stretch gap-3 overflow-auto p-4">
          <ImagePane img={oldImg} label={t("diff.imgOld")} tone="danger" />
          <ImagePane img={newImg} label={t("diff.imgNew")} tone="success" />
        </div>
      ) : mode === "swipe" ? (
        <SwipeCompare oldImg={oldImg} newImg={newImg} />
      ) : (
        <OnionCompare oldImg={oldImg} newImg={newImg} />
      )}
    </div>
  );
}

/** 对比模式切换条。 */
function ImgModeBar({ mode, onChange }: { mode: ImgMode; onChange: (m: ImgMode) => void }) {
  const t = useT();
  const btn = (m: ImgMode, label: string) => (
    <button
      onClick={() => onChange(m)}
      className={`rounded px-2 py-0.5 text-[11px] transition-colors ${
        mode === m ? "bg-accent/15 text-accent" : "text-fg-muted hover:text-fg"
      }`}
    >
      {label}
    </button>
  );
  return (
    <div className="flex shrink-0 select-none items-center justify-end gap-1 border-b border-line bg-overlay px-3 py-1">
      {btn("side", t("diff.modeSide"))}
      {btn("swipe", t("diff.modeSwipe"))}
      {btn("onion", t("diff.modeOnion"))}
    </div>
  );
}

/** 滑块对比:新图叠在旧图上,按滑块位置左右裁切,露出左侧新右侧旧。 */
function SwipeCompare({ oldImg, newImg }: { oldImg: LoadedImage; newImg: LoadedImage }) {
  const t = useT();
  const [pct, setPct] = useState(50);
  return (
    <div className="flex flex-1 flex-col gap-2 overflow-hidden p-4">
      <div className="checkerboard relative flex min-h-0 flex-1 items-center justify-center overflow-hidden rounded border border-line bg-overlay">
        {oldImg.url && <img src={oldImg.url} alt={t("diff.imgOld")} className="max-h-full max-w-full object-contain" />}
        {newImg.url && (
          <img
            src={newImg.url}
            alt={t("diff.imgNew")}
            className="absolute inset-0 m-auto max-h-full max-w-full object-contain"
            style={{ clipPath: `inset(0 ${100 - pct}% 0 0)` }}
          />
        )}
        {/* 分界竖线 */}
        <div className="pointer-events-none absolute inset-y-0 w-px bg-accent" style={{ left: `${pct}%` }} />
      </div>
      <div className="flex shrink-0 items-center gap-2 text-[11px] text-fg-subtle">
        <span className="text-danger">{t("diff.imgOld")}</span>
        <input type="range" min={0} max={100} value={pct} onChange={(e) => setPct(Number(e.target.value))} className="flex-1 accent-accent" aria-label={t("diff.swipePos")} />
        <span className="text-success">{t("diff.imgNew")}</span>
      </div>
    </div>
  );
}

/** 洋葱皮对比:新图以可调不透明度叠在旧图上。 */
function OnionCompare({ oldImg, newImg }: { oldImg: LoadedImage; newImg: LoadedImage }) {
  const t = useT();
  const [opacity, setOpacity] = useState(50);
  return (
    <div className="flex flex-1 flex-col gap-2 overflow-hidden p-4">
      <div className="checkerboard relative flex min-h-0 flex-1 items-center justify-center overflow-hidden rounded border border-line bg-overlay">
        {oldImg.url && <img src={oldImg.url} alt={t("diff.imgOld")} className="max-h-full max-w-full object-contain" />}
        {newImg.url && (
          <img
            src={newImg.url}
            alt={t("diff.imgNew")}
            className="absolute inset-0 m-auto max-h-full max-w-full object-contain"
            style={{ opacity: opacity / 100 }}
          />
        )}
      </div>
      <div className="flex shrink-0 items-center gap-2 text-[11px] text-fg-subtle">
        <span className="text-danger">{t("diff.imgOld")}</span>
        <input type="range" min={0} max={100} value={opacity} onChange={(e) => setOpacity(Number(e.target.value))} className="flex-1 accent-accent" aria-label={t("diff.onionOpacity")} />
        <span className="text-success">{t("diff.imgNew")}</span>
      </div>
    </div>
  );
}

/** 单侧图片面板:居中保持比例,棋盘格衬透明区,显尺寸 + 体积。 */
function ImagePane({ img, label, tone }: { img: LoadedImage; label: string; tone: "danger" | "success" }) {
  const t = useT();
  const [dims, setDims] = useState<{ w: number; h: number } | null>(null);
  const toneCls = tone === "danger" ? "text-danger" : "text-success";
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div className="mb-2 flex shrink-0 items-center gap-2 text-[11px]">
        <span className={`font-semibold ${toneCls}`}>{label}</span>
        <span className="text-fg-subtle">
          {dims ? `${dims.w}×${dims.h}` : ""}{dims && img.bytes ? " · " : ""}{img.bytes ? formatBytes(String(img.bytes)) : ""}
        </span>
      </div>
      <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto rounded border border-line bg-overlay p-3 checkerboard">
        {img.loading ? (
          <span className="text-[11px] text-fg-subtle">{t("common.loading")}</span>
        ) : img.error || !img.url ? (
          <span className="text-[11px] text-danger">{t("diff.imgLoadFailed")}</span>
        ) : (
          <img
            src={img.url}
            alt={label}
            onLoad={(e) => setDims({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })}
            className="max-h-full max-w-full object-contain"
            style={{ imageRendering: "auto" }}
          />
        )}
      </div>
    </div>
  );
}

function Gutter({ n, border }: { n: number | null; border?: boolean }) {
  return (
    <span
      // self-stretch:并排换行时行号列(及其右分隔线)随整格拉满高度,数字仍在顶部(行号对齐首行)。
      className={`w-10 shrink-0 select-none self-stretch px-1.5 text-right text-fg-subtle ${
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

/** 把字节字符串(如 "1048576")格式化成人类可读("1.0 MB");解析失败返回空串。 */
function formatBytes(raw: string): string {
  const n = Number(raw);
  if (!Number.isFinite(n) || n <= 0) return "";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${i === 0 ? v : v.toFixed(1)} ${units[i]}`;
}
