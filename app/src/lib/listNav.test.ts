import { describe, it, expect } from "vitest";
import { navTarget } from "./listNav";

// 列表键盘导航纯逻辑回归。锁:方向键映射、边界停住、未选中先落 0、非导航键返回 null。

describe("navTarget", () => {
  it("j / ArrowDown 下移一格", () => {
    expect(navTarget("j", 2, 10)).toBe(3);
    expect(navTarget("ArrowDown", 2, 10)).toBe(3);
  });

  it("k / ArrowUp 上移一格", () => {
    expect(navTarget("k", 2, 10)).toBe(1);
    expect(navTarget("ArrowUp", 2, 10)).toBe(1);
  });

  it("到底/到顶停住(返回 null 表示不动)", () => {
    expect(navTarget("j", 9, 10)).toBeNull(); // 已在最后一项
    expect(navTarget("k", 0, 10)).toBeNull(); // 已在第一项
  });

  it("未选中(-1)时上下都先落到第 0 项", () => {
    expect(navTarget("j", -1, 10)).toBe(0);
    expect(navTarget("k", -1, 10)).toBe(0);
  });

  it("g/Home 到顶,G/End 到底", () => {
    expect(navTarget("g", 5, 10)).toBe(0);
    expect(navTarget("Home", 5, 10)).toBe(0);
    expect(navTarget("G", 5, 10)).toBe(9);
    expect(navTarget("End", 5, 10)).toBe(9);
  });

  it("已在目标位时返回 null(g 在顶、G 在底)", () => {
    expect(navTarget("g", 0, 10)).toBeNull();
    expect(navTarget("G", 9, 10)).toBeNull();
  });

  it("非导航键返回 null", () => {
    expect(navTarget("x", 2, 10)).toBeNull();
    expect(navTarget("Enter", 2, 10)).toBeNull();
  });

  it("空列表返回 null", () => {
    expect(navTarget("j", -1, 0)).toBeNull();
  });
});
