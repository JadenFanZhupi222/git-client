// 三方合并的「对齐模型」(纯逻辑,便于测试)。
//
// 以「结果(result)」为锚点,分别和「我方(ours)」「对方(theirs)」做行级 diff,
// 再把两个 diff 在 result 轴上合并成若干「区域(region)」。每个区域里三栏各自占多少行
// 可能不同,我们算出每栏需要补几行「空白占位(spacer)」,让三栏逐行对齐
// (就像 JetBrains 的三栏合并:对应内容横向对齐)。
//
// 这是 CodeMirror 没有内置的部分(它只做两栏对齐),所以这里自己算。

import { Text } from "@codemirror/state";
import { Chunk } from "@codemirror/merge";

/** 一个变更区域。所有行号都是 0-based,区间为 [from, to)(左闭右开)。 */
export interface MergeRegion {
  rFrom: number;
  rTo: number; // 结果栏行区间
  oFrom: number;
  oTo: number; // 我方栏行区间
  tFrom: number;
  tTo: number; // 对方栏行区间
  oursChanged: boolean; // 我方在此区域与结果不同
  theirsChanged: boolean; // 对方在此区域与结果不同
  conflict: boolean; // 两边都改 = 真冲突
}

/** 在某栏第 afterLine 行(0-based)之后补 count 个空白占位行;afterLine = -1 表示补在最顶部。 */
export interface Spacer {
  afterLine: number;
  count: number;
}

export interface MergeModel {
  regions: MergeRegion[];
  spacersOurs: Spacer[];
  spacersResult: Spacer[];
  spacersTheirs: Spacer[];
}

/** 把 chunk 的字符区间 [from,to) 换算成行区间 [startLine,endLine)(0-based)。空 chunk → 零宽。 */
function lineRange(doc: Text, from: number, to: number): [number, number] {
  const len = doc.length;
  const f = Math.max(0, Math.min(from, len));
  if (to <= from) {
    const l = doc.lineAt(f).number - 1;
    return [l, l];
  }
  const s = doc.lineAt(f).number - 1;
  const e = doc.lineAt(Math.min(to - 1, len)).number - 1;
  return [s, e + 1];
}

/** result 行 R(在某栏起始处、稳定边界)对应到源栏的行号:R + 所有「起点严格在 R 之前」的 chunk 的行数增量。 */
function mapStart(deltas: { rStart: number; delta: number }[], r: number): number {
  let acc = r;
  for (const d of deltas) {
    if (d.rStart < r) acc += d.delta;
    else break;
  }
  return acc;
}

/** 某栏相对 base 的「变更行区间」(在该栏自身行轴上,0-based [from,to))。base 为 null → 返回 null(无法判断)。 */
function changedVsBase(base: string | null | undefined, side: string): [number, number][] | null {
  if (base == null) return null;
  const baseDoc = Text.of(base.split("\n"));
  const sideDoc = Text.of(side.split("\n"));
  return Chunk.build(baseDoc, sideDoc).map((c) => lineRange(sideDoc, c.fromB, c.toB));
}

/** 区间 [aFrom,aTo) 与任一变更区间相交或相邻(含零宽边界接触)。null(无 base)→ 保守判为 true。 */
function touched(ranges: [number, number][] | null, from: number, to: number): boolean {
  if (ranges == null) return true;
  return ranges.some(([f, t]) => f <= to && from <= t);
}

export function buildMergeModel(ours: string, result: string, theirs: string, base?: string | null): MergeModel {
  const oursDoc = Text.of(ours.split("\n"));
  const resDoc = Text.of(result.split("\n"));
  const theirsDoc = Text.of(theirs.split("\n"));
  const chunksO = Chunk.build(oursDoc, resDoc);
  const chunksT = Chunk.build(theirsDoc, resDoc);

  // 真冲突判定独立于当前 result:看「我方」「对方」是否都相对共同祖先(base)改了同一处。
  const oursVsBase = changedVsBase(base, ours);
  const theirsVsBase = changedVsBase(base, theirs);

  // 每个 chunk 在 result 轴上的区间 + 它带来的「源行数 - 结果行数」增量。
  type Iv = { rf: number; rt: number; from: "o" | "t"; delta: number };
  const ivs: Iv[] = [];
  const deltasO: { rStart: number; delta: number }[] = [];
  const deltasT: { rStart: number; delta: number }[] = [];

  for (const c of chunksO) {
    const [rf, rt] = lineRange(resDoc, c.fromB, c.toB);
    const [sf, st] = lineRange(oursDoc, c.fromA, c.toA);
    const delta = st - sf - (rt - rf);
    ivs.push({ rf, rt, from: "o", delta });
    deltasO.push({ rStart: rf, delta });
  }
  for (const c of chunksT) {
    const [rf, rt] = lineRange(resDoc, c.fromB, c.toB);
    const [sf, st] = lineRange(theirsDoc, c.fromA, c.toA);
    const delta = st - sf - (rt - rf);
    ivs.push({ rf, rt, from: "t", delta });
    deltasT.push({ rStart: rf, delta });
  }
  ivs.sort((a, b) => a.rf - b.rf || a.rt - b.rt);
  deltasO.sort((a, b) => a.rStart - b.rStart);
  deltasT.sort((a, b) => a.rStart - b.rStart);

  const regions: MergeRegion[] = [];
  const spacersOurs: Spacer[] = [];
  const spacersResult: Spacer[] = [];
  const spacersTheirs: Spacer[] = [];

  let i = 0;
  while (i < ivs.length) {
    let rf = ivs[i].rf;
    let rt = ivs[i].rt;
    let sumO = 0;
    let sumT = 0;
    let oursChanged = false;
    let theirsChanged = false;
    const consume = (iv: Iv) => {
      if (iv.from === "o") {
        sumO += iv.delta;
        oursChanged = true;
      } else {
        sumT += iv.delta;
        theirsChanged = true;
      }
    };
    consume(ivs[i]);
    let j = i + 1;
    // 合并 result 轴上重叠/相邻的区间(零宽插入点也算相邻)。
    while (j < ivs.length && ivs[j].rf <= rt) {
      rt = Math.max(rt, ivs[j].rt);
      consume(ivs[j]);
      j++;
    }

    const rLines = rt - rf;
    const oLines = rLines + sumO;
    const tLines = rLines + sumT;
    const oFrom = mapStart(deltasO, rf);
    const oTo = oFrom + oLines;
    const tFrom = mapStart(deltasT, rf);
    const tTo = tFrom + tLines;
    const h = Math.max(rLines, oLines, tLines);

    if (h - rLines > 0) spacersResult.push({ afterLine: rLines > 0 ? rt - 1 : rf - 1, count: h - rLines });
    if (h - oLines > 0) spacersOurs.push({ afterLine: oLines > 0 ? oTo - 1 : oFrom - 1, count: h - oLines });
    if (h - tLines > 0) spacersTheirs.push({ afterLine: tLines > 0 ? tTo - 1 : tFrom - 1, count: h - tLines });

    // 真冲突:我方与对方都在此处(各自相对 base)做了改动。
    const conflict = touched(oursVsBase, oFrom, oTo) && touched(theirsVsBase, tFrom, tTo);
    regions.push({ rFrom: rf, rTo: rt, oFrom, oTo, tFrom, tTo, oursChanged, theirsChanged, conflict });
    i = j;
  }

  return { regions, spacersOurs, spacersResult, spacersTheirs };
}
