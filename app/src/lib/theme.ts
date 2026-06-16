import { resolveGlassMode, getStoredGlassPref, systemReducesTransparency } from "./transparency";
import { supportsRefraction } from "./platform";

/**
 * 主题切换:浅色(默认)/ 暗色。
 *
 * 实现完全靠 CSS 变量 —— 只在 <html> 上挂 data-theme="dark",
 * index.css 里的 :root[data-theme="dark"] 覆盖块就会接管所有 token。
 * 这里只负责「读偏好 / 写 DOM / 存 localStorage」,不碰任何颜色值。
 */

export type Theme = "light" | "dark";

const KEY = "theme";

/** 读取持久化偏好;没有则默认浅色。 */
export function getStoredTheme(): Theme {
  return localStorage.getItem(KEY) === "dark" ? "dark" : "light";
}

/** 应用到 <html> 并持久化。浅色不写 data-theme(回到 @theme 默认)。 */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement;
  if (theme === "dark") root.setAttribute("data-theme", "dark");
  else root.removeAttribute("data-theme");
  localStorage.setItem(KEY, theme);
}

/** 计算当前玻璃档位并写到 <html data-glass>。在首屏与偏好变更时调用。 */
export function applyGlassMode(): void {
  const mode = resolveGlassMode({
    pref: getStoredGlassPref(),
    systemReduce: systemReducesTransparency(),
    chromium: supportsRefraction(),
  });
  document.documentElement.setAttribute("data-glass", mode);
}
