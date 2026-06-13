import { type DiffLineDto } from "../ipc";

/** 一行 diff + 它在所属 hunk.lines 里的原始下标。行级暂存的选中键 `${hi}:${li}` 依赖这个下标,
 *  所以折叠/配对过程中必须把它带着走,展开后键不变。 */
export type LineRef = { line: DiffLineDto; li: number };

/** 折叠分块:`lines` 段照常逐行渲染;`fold` 段默认折成一条「展开 N 行」,refs 为被折叠的行。 */
export type DiffChunk =
  | { kind: "lines"; refs: LineRef[] }
  | { kind: "fold"; refs: LineRef[] };

export type CollapseOpts = {
  /** 改动块上下各保留的 context 行数,默认 3(同 git 默认 diff context)。 */
  ctx?: number;
  /** 至少要隐藏这么多行才折叠(否则全显);默认 2,折 1 行净省 0 行没意义。 */
  minFold?: number;
};

function isChange(l: DiffLineDto): boolean {
  return l.kind === "add" || l.kind === "del";
}

/** 把每行配上原始下标。collapse / 配对前的统一入口。 */
export function toRefs(lines: DiffLineDto[]): LineRef[] {
  return lines.map((line, li) => ({ line, li }));
}

/**
 * 折叠一个 hunk 内的未改区:每个改动块上下保留 ctx 行 context,
 * 中间多出的连续 context 折成一个 fold 分块(仅当能隐藏 ≥ minFold 行,否则整段保留)。
 * - hunk 顶/尾的 context(一侧没有相邻改动)那一侧不保留(keepStart/keepEnd 为 0)。
 * - 保留每行原始下标,展开后行级暂存键不变。
 * 纯函数,可测。
 */
export function collapseContext(lines: DiffLineDto[], opts: CollapseOpts = {}): DiffChunk[] {
  const ctx = opts.ctx ?? 3;
  const minFold = opts.minFold ?? 2;
  const refs = toRefs(lines);
  const n = refs.length;
  const chunks: DiffChunk[] = [];

  // 累积「可见行」缓冲;遇到要 push fold(或走完)时先 flush 成一个 lines 分块。
  let buf: LineRef[] = [];
  const flush = () => {
    if (buf.length) {
      chunks.push({ kind: "lines", refs: buf });
      buf = [];
    }
  };

  let i = 0;
  while (i < n) {
    if (isChange(refs[i].line)) {
      buf.push(refs[i]);
      i++;
      continue;
    }
    // 一段极大 context run [start, end)。
    const start = i;
    while (i < n && !isChange(refs[i].line)) i++;
    const end = i;
    const keepStart = start > 0 ? ctx : 0; // 前面有改动 → 顶住 ctx 行
    const keepEnd = end < n ? ctx : 0; // 后面有改动 → 顶住 ctx 行
    const hidden = end - start - keepStart - keepEnd;
    if (hidden >= minFold) {
      for (let k = start; k < start + keepStart; k++) buf.push(refs[k]);
      flush();
      chunks.push({ kind: "fold", refs: refs.slice(start + keepStart, end - keepEnd) });
      for (let k = end - keepEnd; k < end; k++) buf.push(refs[k]);
    } else {
      for (let k = start; k < end; k++) buf.push(refs[k]);
    }
  }
  flush();
  return chunks;
}

export type SbsCell = LineRef | null;
export type SbsRow = { left: SbsCell; right: SbsCell };

/** 把行配对成并排行:context 两侧同行;连续删块 del[i] 配连续增块 add[i],多余行对侧留空。
 *  入参带原始下标 → 配对后选中键仍按原 li。纯函数,可测。 */
export function buildSbsRows(refs: LineRef[]): SbsRow[] {
  const rows: SbsRow[] = [];
  let i = 0;
  while (i < refs.length) {
    const l = refs[i].line;
    if (l.kind !== "del" && l.kind !== "add") {
      rows.push({ left: refs[i], right: refs[i] });
      i++;
      continue;
    }
    const dels: SbsCell[] = [];
    while (i < refs.length && refs[i].line.kind === "del") { dels.push(refs[i]); i++; }
    const adds: SbsCell[] = [];
    while (i < refs.length && refs[i].line.kind === "add") { adds.push(refs[i]); i++; }
    const max = Math.max(dels.length, adds.length);
    for (let k = 0; k < max; k++) rows.push({ left: dels[k] ?? null, right: adds[k] ?? null });
  }
  return rows;
}
