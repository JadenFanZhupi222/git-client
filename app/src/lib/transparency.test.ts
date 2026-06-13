import { describe, it, expect } from "vitest";
import { resolveGlassMode } from "./transparency";

describe("resolveGlassMode", () => {
  it("用户显式关闭透明 → solid", () => {
    expect(resolveGlassMode({ pref: "reduced", systemReduce: false, chromium: true })).toBe("solid");
  });
  it("系统要求降低透明 → solid", () => {
    expect(resolveGlassMode({ pref: "auto", systemReduce: true, chromium: true })).toBe("solid");
  });
  it("auto + Chromium + 不降透明 → refract", () => {
    expect(resolveGlassMode({ pref: "auto", systemReduce: false, chromium: true })).toBe("refract");
  });
  it("auto + 非 Chromium + 不降透明 → blur", () => {
    expect(resolveGlassMode({ pref: "auto", systemReduce: false, chromium: false })).toBe("blur");
  });
});
