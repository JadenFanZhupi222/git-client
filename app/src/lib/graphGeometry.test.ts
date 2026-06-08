import { describe, it, expect } from "vitest";
import { LANE_W, ROW_H, cx, gutterWidth, topPath, botPath } from "./graphGeometry";
import type { GraphSegDto } from "../ipc";

const seg = (from: number, to: number): GraphSegDto => ({ from, to, color: 0 });
const row = (column: number, top: GraphSegDto[] = [], bottom: GraphSegDto[] = []) => ({ column, top, bottom });

describe("graphGeometry", () => {
  it("cx 把列号映射到该列像素中心", () => {
    expect(cx(0)).toBe(LANE_W / 2);
    expect(cx(2)).toBe(2 * LANE_W + LANE_W / 2);
  });

  it("gutterWidth = (最大列号 + 1) 列宽,跨 column 与连线终点取最大", () => {
    // column 最大 1,但连线 to=3 更靠右 → gutter 要容到第 3 列。
    const rows = [row(0, [seg(0, 0)]), row(1, [seg(1, 3)], [seg(1, 1)])];
    expect(gutterWidth(rows)).toBe((3 + 1) * LANE_W);
  });

  it("gutterWidth 空输入至少 1 列宽(不返回 0)", () => {
    expect(gutterWidth([])).toBe(LANE_W);
  });

  it("同列连线走竖直 L,起止 x 一致", () => {
    const d = topPath(1, 1);
    expect(d).toContain("L"); // 直线段
    expect(d).not.toContain("C"); // 不应有 bezier
    expect(d).toBe(`M${cx(1)},0 L${cx(1)},${ROW_H / 2}`);
  });

  it("换列连线走三次 bezier C,从中点收到底边", () => {
    const d = botPath(0, 2);
    expect(d).toContain("C");
    expect(d.startsWith(`M${cx(0)},${ROW_H / 2}`)).toBe(true);
    expect(d.endsWith(`${cx(2)},${ROW_H}`)).toBe(true);
  });
});
