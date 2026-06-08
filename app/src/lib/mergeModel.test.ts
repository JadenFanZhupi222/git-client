import { describe, it, expect } from "vitest";
import { buildMergeModel } from "./mergeModel";

// 三方对齐模型的纯逻辑回归。锁住三件最易回归的事:
// ① 单边改 ≠ 冲突;② 双边改同处 = 冲突;③ 行数不等时补 spacer 对齐三栏。

describe("buildMergeModel", () => {
  it("三栏完全相同 → 无区域、无 spacer", () => {
    const m = buildMergeModel("a\nb\nc", "a\nb\nc", "a\nb\nc", "a\nb\nc");
    expect(m.regions).toHaveLength(0);
    expect(m.spacersOurs).toHaveLength(0);
    expect(m.spacersResult).toHaveLength(0);
    expect(m.spacersTheirs).toHaveLength(0);
  });

  it("仅我方相对 base 改动 → 单区域、非冲突", () => {
    const base = "a\nb\nc";
    const m = buildMergeModel(/*ours*/ "a\nB\nc", /*result*/ "a\nb\nc", /*theirs*/ "a\nb\nc", base);
    expect(m.regions).toHaveLength(1);
    expect(m.regions[0].oursChanged).toBe(true);
    expect(m.regions[0].theirsChanged).toBe(false);
    expect(m.regions[0].conflict).toBe(false);
  });

  it("我方与对方都相对 base 改同一处 → 真冲突", () => {
    const base = "a\nb\nc";
    const m = buildMergeModel(/*ours*/ "a\nX\nc", /*result*/ "a\nb\nc", /*theirs*/ "a\nY\nc", base);
    expect(m.regions).toHaveLength(1);
    expect(m.regions[0].oursChanged).toBe(true);
    expect(m.regions[0].theirsChanged).toBe(true);
    expect(m.regions[0].conflict).toBe(true);
  });

  it("无 base 时保守判为冲突(无法区分两边是否动了同处)", () => {
    const m = buildMergeModel("a\nB\nc", "a\nb\nc", "a\nb\nc");
    expect(m.regions[0].conflict).toBe(true);
  });

  it("我方比 result 多 2 行 → result 栏补 2 行 spacer 对齐", () => {
    // result 2 行,ours 在中间插入 X、Y 共 4 行;对方 = result。
    const m = buildMergeModel(/*ours*/ "a\nX\nY\nc", /*result*/ "a\nc", /*theirs*/ "a\nc", "a\nc");
    const resultSpacerLines = m.spacersResult.reduce((n, s) => n + s.count, 0);
    expect(resultSpacerLines).toBe(2);
    expect(m.spacersOurs).toHaveLength(0); // 最长的一栏不需要补
  });
});
