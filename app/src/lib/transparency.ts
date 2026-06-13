export type GlassPref = "auto" | "reduced";
export type GlassMode = "refract" | "blur" | "solid";

/** 纯逻辑:由偏好 + 系统设置 + 是否 Chromium 决定玻璃渲染档位。 */
export function resolveGlassMode(input: {
  pref: GlassPref;
  systemReduce: boolean;
  chromium: boolean;
}): GlassMode {
  if (input.pref === "reduced" || input.systemReduce) return "solid";
  return input.chromium ? "refract" : "blur";
}

const KEY = "glass.pref";

export function getStoredGlassPref(): GlassPref {
  return localStorage.getItem(KEY) === "reduced" ? "reduced" : "auto";
}

export function setStoredGlassPref(pref: GlassPref): void {
  localStorage.setItem(KEY, pref);
}

/** 系统是否要求降低透明度(SSR/jsdom 无 matchMedia 时安全回退 false)。 */
export function systemReducesTransparency(): boolean {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-reduced-transparency: reduce)").matches
    : false;
}
