import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { type DiffLineDto, type FileDiffDto, type ImageDataDto } from "../ipc";
import { buildDiffRows, maxContentCols, maxSideCols, type DiffRow, type LineRef, type SbsRow } from "../lib/diffRows";
import { ChevronDownIcon } from "./icons";

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
  hunkAction,
  lineStage,
}: {
  diff: FileDiffDto | null;
  loading: boolean;
  hasFile: boolean;
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
  const toggle = (key: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(key) ? next.delete(key) : next.add(key);
      return next;
    });
  if (!hasFile) {
    return <Center>选择一个文件查看 diff</Center>;
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
    return <Center>无法加载 diff</Center>;
  }
  if (diff.is_lfs_pointer) {
    const size = formatBytes(diff.lfs_size);
    return <Center>Git LFS 文件{size ? `(${size})` : ""}，无法显示行级 diff</Center>;
  }
  if (diff.is_image) {
    // 图片是二进制的一种,但能并排预览新旧图(此分支须在 is_binary 之前)。
    if (!diff.old_image && !diff.new_image) {
      return <Center>图片过大或无法读取，已跳过预览</Center>;
    }
    return <ImageDiff diff={diff} />;
  }
  if (diff.is_binary) {
    return <Center>二进制文件，无法显示行级 diff</Center>;
  }
  if (diff.too_large) {
    return <Center>文件过大，已跳过逐行 diff(避免卡顿)</Center>;
  }
  if (diff.hunks.length === 0) {
    return <Center>该文件在此提交中无文本改动</Center>;
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
  const btn = (v: ViewMode, label: string) => (
    <button
      onClick={() => onChange(v)}
      className={`rounded px-2 py-0.5 text-[11px] transition-colors ${
        view === v ? "bg-accent/15 text-accent" : "text-fg-muted hover:text-fg"
      }`}
    >
      {label}
    </button>
  );
  return (
    <div className="flex shrink-0 select-none items-center justify-end gap-1 border-b border-line bg-overlay px-3 py-1">
      {btn("unified", "统一")}
      {btn("split", "并排")}
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
  // 一维渲染项 → 横向滚动体宽。统一:gutter(选择+旧/新行号+符号+右内边距)+ 最长内容 ch。
  // 并排:两个半栏 gutter + 左右各自最长内容 ch + 中缝 1px。
  const sideCols = useMemo(() => maxSideCols(diff), [diff]);
  const bodyWidth = useMemo<string>(() => {
    if (view === "unified") {
      const gutter = (hasSel ? 16 : 0) + 40 + 40 + 16 + 12;
      return `calc(${gutter}px + ${maxContentCols(diff)}ch)`;
    }
    const halfGutter = (hasSel ? 16 : 0) + 40 + 16 + 12;
    return `calc(${halfGutter * 2 + 1}px + ${sideCols.left + sideCols.right}ch)`;
  }, [diff, view, hasSel, sideCols]);

  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_H, // 行高固定,估计=实际
    overscan: 24,
  });

  return (
    <div ref={parentRef} className="flex-1 overflow-auto">
      {/* 撑出全量高/宽的占位层;可视窗口内的行绝对定位到各自 y。minWidth 100%:短 diff 铺满视口。 */}
      <div style={{ height: virtualizer.getTotalSize(), width: bodyWidth, minWidth: "100%", position: "relative" }}>
        {virtualizer.getVirtualItems().map((vi) => (
          <div
            key={vi.key}
            style={{ position: "absolute", top: 0, left: 0, width: "100%", height: ROW_H, transform: `translateY(${vi.start}px)` }}
          >
            <DiffRowView
              r={rows[vi.index]}
              sideCols={sideCols}
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
  sideCols,
  selected,
  toggle,
  lineStage,
  hunkAction,
  expand,
}: {
  r: DiffRow;
  sideCols: { left: number; right: number };
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
      return <PairRow hi={r.hi} row={r.row} cols={sideCols} selected={selected} toggle={toggle} lineStage={lineStage} />;
  }
}

/** unified 视图的一行。 */
function UnifiedLine({ hi, ref_, selected, toggle, lineStage }: { hi: number; ref_: LineRef } & Pick<StageProps, "selected" | "toggle" | "lineStage">) {
  const l = ref_.line;
  const add = l.kind === "add";
  const del = l.kind === "del";
  const rowBg = add ? "bg-success/10" : del ? "bg-danger/10" : "";
  const sign = add ? "+" : del ? "-" : " ";
  const signCls = add ? "text-success" : del ? "text-danger" : "text-fg-subtle";
  const selectable = !!lineStage && (add || del);
  const key = `${hi}:${ref_.li}`;
  const on = selected.has(key);
  return (
    <div
      onClick={selectable ? () => toggle(key) : undefined}
      // min-w-full:行宽 = 外层占位层体宽(= 最长行 ch),所有行齐平铺满;h-full 填满固定行高。
      className={`flex h-full min-w-full ${rowBg} ${selectable ? "cursor-pointer" : ""} ${on ? "ring-1 ring-inset ring-accent/60" : ""}`}
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
  return (
    <div
      onClick={onExpand}
      className="flex h-full min-w-full cursor-pointer select-none items-center gap-1 border-y border-line/60 bg-overlay/40 pl-3 text-[11px] text-fg-subtle transition-colors hover:bg-overlay hover:text-accent"
    >
      <ChevronDownIcon className="h-3 w-3" />
      展开 {count} 行
    </div>
  );
}

/** 并排配对行:左旧 | 右新 一行内并置。两侧宽度由各自最长内容(ch)定,共用一个纵向滚动容器
 *  → 纵横联动。左半固定宽,右半 flex-1 吸收余量(短 diff 铺满)。 */
function PairRow({ hi, row, cols, selected, toggle, lineStage }: { hi: number; row: SbsRow; cols: { left: number; right: number } } & Pick<StageProps, "selected" | "toggle" | "lineStage">) {
  const halfGutter = (lineStage ? 16 : 0) + 40 + 16 + 12;
  return (
    <div className="flex h-full min-w-full">
      <HalfCell side="old" cell={row.left} hi={hi} width={`calc(${halfGutter}px + ${cols.left}ch)`} selected={selected} toggle={toggle} lineStage={lineStage} />
      <HalfCell side="new" cell={row.right} hi={hi} width={`calc(${halfGutter}px + ${cols.right}ch)`} grow selected={selected} toggle={toggle} lineStage={lineStage} border />
    </div>
  );
}

/** 并排某一侧的一格(取 row 的 left 或 right);对侧有内容本侧无则占位空行。 */
function HalfCell({ hi, side, cell, width, grow, border, selected, toggle, lineStage }: { hi: number; side: "old" | "new"; cell: SbsRow["left"]; width: string; grow?: boolean } & Pick<StageProps, "selected" | "toggle" | "lineStage"> & { border?: boolean }) {
  const sizing = grow ? { flex: "1 0 auto", minWidth: width } : { width, flexShrink: 0 };
  const borderCls = border ? "border-l border-line" : "";
  if (!cell) {
    return <div className={`flex h-full bg-overlay/40 ${borderCls}`} style={sizing}>&nbsp;</div>;
  }
  const l = cell.line;
  const add = l.kind === "add";
  const del = l.kind === "del";
  const rowBg = add ? "bg-success/10" : del ? "bg-danger/10" : "";
  const sign = add ? "+" : del ? "-" : " ";
  const signCls = add ? "text-success" : del ? "text-danger" : "text-fg-subtle";
  const selectable = !!lineStage && (add || del);
  const key = `${hi}:${cell.li}`;
  const on = selected.has(key);
  const lineno = side === "old" ? l.old_lineno : l.new_lineno;
  return (
    <div
      onClick={selectable ? () => toggle(key) : undefined}
      style={sizing}
      className={`flex h-full ${borderCls} ${rowBg} ${selectable ? "cursor-pointer" : ""} ${on ? "ring-1 ring-inset ring-accent/60" : ""}`}
    >
      {lineStage && (
        <span className="w-4 shrink-0 select-none text-center text-[10px] text-accent">
          {selectable ? (on ? "✓" : "·") : ""}
        </span>
      )}
      <Gutter n={lineno} border />
      <span className={`w-4 shrink-0 select-none text-center ${signCls}`}>{sign}</span>
      <span className="flex-1 whitespace-pre pr-3 text-fg">
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
  return (
    <div className="group flex h-full min-w-full select-none items-center gap-2 bg-overlay px-3 text-[11px] text-accent/80">
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
              暂存选中行 ({selCount})
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

/** 图片 diff:并排预览新旧两版(新增只显新、删除只显旧)。 */
function ImageDiff({ diff }: { diff: FileDiffDto }) {
  const { old_image, new_image } = diff;
  // 新增(无旧)→ 单栏「新增」;删除(无新)→ 单栏「已删除」;否则旧 | 新两栏。
  const both = !!old_image && !!new_image;
  return (
    <div className="fade-in flex flex-1 items-stretch gap-3 overflow-auto p-4">
      {old_image && <ImagePane img={old_image} label={both ? "旧" : "已删除"} tone="danger" />}
      {new_image && <ImagePane img={new_image} label={both ? "新" : "新增"} tone="success" />}
    </div>
  );
}

function ImagePane({ img, label, tone }: { img: ImageDataDto; label: string; tone: "danger" | "success" }) {
  const [dims, setDims] = useState<{ w: number; h: number } | null>(null);
  const src = `data:${img.mime};base64,${img.base64}`;
  // base64 长度估算原始字节数(去掉 padding)。
  const bytes = Math.max(0, Math.floor((img.base64.length * 3) / 4) - (img.base64.endsWith("==") ? 2 : img.base64.endsWith("=") ? 1 : 0));
  const toneCls = tone === "danger" ? "text-danger" : "text-success";
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div className="mb-2 flex shrink-0 items-center gap-2 text-[11px]">
        <span className={`font-semibold ${toneCls}`}>{label}</span>
        <span className="text-fg-subtle">
          {dims ? `${dims.w}×${dims.h}` : ""}{dims ? " · " : ""}{formatBytes(String(bytes))}
        </span>
      </div>
      {/* 居中、保持比例;棋盘格底衬出透明区域 */}
      <div className="flex min-h-0 flex-1 items-center justify-center overflow-auto rounded border border-line bg-overlay p-3 checkerboard">
        <img
          src={src}
          alt={label}
          onLoad={(e) => setDims({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })}
          className="max-h-full max-w-full object-contain"
          style={{ imageRendering: "auto" }}
        />
      </div>
    </div>
  );
}

function Gutter({ n, border }: { n: number | null; border?: boolean }) {
  return (
    <span
      className={`w-10 shrink-0 select-none px-1.5 text-right text-fg-subtle ${
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
