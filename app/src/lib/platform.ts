/** 是否 Chromium 内核 UA。Chrome/Edge 的 UA 含 "Chrome/" 或 "Edg/";
 *  WKWebView/Safari 含 "Safari" 但无 "Chrome/",Firefox 含 "Firefox"。据此区分。 */
export function isChromiumUA(ua: string): boolean {
  return /\bChrome\/|\bEdg\//.test(ua);
}

/** 运行时:当前 webview 是否 Chromium(真折射前提)。 */
export function supportsRefraction(): boolean {
  return typeof navigator !== "undefined" && isChromiumUA(navigator.userAgent);
}
