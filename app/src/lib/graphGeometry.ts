// 提交图谱泳道的「纯几何」:列号 → 像素坐标、连线 SVG path、gutter 宽度。
// 从 CommitGraph 组件抽出,既能单测,也为 M1.5「增量泳道」复用同一套坐标系。

import type { GraphSegDto } from "../ipc";

export const LANE_W = 16; // 每条 lane 的像素宽
export const ROW_H = 48; // 行高必须固定,否则跨行的连线对不齐
const MID = ROW_H / 2;

/** 列号 → 该列圆点的水平中心像素。 */
export const cx = (c: number) => c * LANE_W + LANE_W / 2;

/** gutter 宽度 = 所有行里出现过的最大列号 + 1 列。空输入 → 至少 1 列宽。 */
export function gutterWidth(rows: { column: number; top: GraphSegDto[]; bottom: GraphSegDto[] }[]): number {
  let maxCol = 0;
  for (const r of rows) {
    maxCol = Math.max(maxCol, r.column);
    for (const s of r.top) maxCol = Math.max(maxCol, s.from, s.to);
    for (const s of r.bottom) maxCol = Math.max(maxCol, s.from, s.to);
  }
  return (maxCol + 1) * LANE_W;
}

// 上半段:顶边 → 中点。直列时是竖线(L);换列时用三次 bezier(C),两端切线竖直 →
// lane 在自己列里走直线,只在拐点柔和地弯,消除生硬的对角线/锯齿。
export const topPath = (from: number, to: number) => {
  const x1 = cx(from), x2 = cx(to);
  if (x1 === x2) return `M${x1},0 L${x1},${MID}`;
  return `M${x1},0 C${x1},${MID / 2} ${x2},${MID / 2} ${x2},${MID}`;
};

// 下半段:中点 → 底边。
export const botPath = (from: number, to: number) => {
  const x1 = cx(from), x2 = cx(to);
  if (x1 === x2) return `M${x1},${MID} L${x1},${ROW_H}`;
  return `M${x1},${MID} C${x1},${MID + MID / 2} ${x2},${MID + MID / 2} ${x2},${ROW_H}`;
};
