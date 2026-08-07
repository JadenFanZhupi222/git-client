/** 是否 Chromium 内核 UA。Chrome/Edge 的 UA 含 "Chrome/" 或 "Edg/";
 *  WKWebView/Safari 含 "Safari" 但无 "Chrome/",Firefox 含 "Firefox"。据此区分。 */
export function isChromiumUA(ua: string): boolean {
  return /\bChrome\/|\bEdg\//.test(ua);
}

/** 运行时:当前 webview 是否 Chromium(真折射前提)。 */
export function supportsRefraction(): boolean {
  return typeof navigator !== "undefined" && isChromiumUA(navigator.userAgent);
}

export type DesktopPlatform = "macos" | "windows" | "linux" | "unknown";

/**
 * Detect the desktop shell without depending on an additional Tauri plugin.
 * Prefer `navigator.platform`, then fall back to the user agent for webviews
 * that expose an empty or generic platform string.
 */
export function detectDesktopPlatform(
  platform = typeof navigator === "undefined" ? "" : navigator.platform || navigator.userAgent,
): DesktopPlatform {
  const source = platform.toLowerCase();
  if (source.includes("mac")) return "macos";
  if (source.includes("win")) return "windows";
  if (source.includes("linux") || source.includes("x11")) return "linux";
  return "unknown";
}

export function getDesktopPlatform(): DesktopPlatform {
  return detectDesktopPlatform();
}
