import { describe, it, expect } from "vitest";
import { detectDesktopPlatform, isChromiumUA } from "./platform";

describe("isChromiumUA", () => {
  it("Edge/Chrome 的 UA 判为 Chromium", () => {
    expect(isChromiumUA("Mozilla/5.0 ... Chrome/124.0 Safari/537.36")).toBe(true);
    expect(isChromiumUA("Mozilla/5.0 ... Edg/124.0")).toBe(true);
  });
  it("纯 Safari(含 WKWebView)与 Firefox 判为非 Chromium", () => {
    expect(isChromiumUA("Mozilla/5.0 ... Version/17.0 Safari/605.1.15")).toBe(false);
    expect(isChromiumUA("Mozilla/5.0 ... Firefox/126.0")).toBe(false);
  });
});

describe("detectDesktopPlatform", () => {
  it.each([
    ["MacIntel", "macos"],
    ["Win32", "windows"],
    ["Linux x86_64", "linux"],
    ["X11", "linux"],
    ["Mozilla/5.0 (Windows NT 10.0; Win64; x64)", "windows"],
    ["Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)", "macos"],
    ["", "unknown"],
  ] as const)("maps %s to %s", (platform, expected) => {
    expect(detectDesktopPlatform(platform)).toBe(expected);
  });
});
