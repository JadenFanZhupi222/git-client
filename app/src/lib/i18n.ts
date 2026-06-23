import { useSyncExternalStore } from "react";
import { zh, type MessageKey } from "./locales/zh";
import { en } from "./locales/en";

/**
 * 轻量国际化:中 / English 切换。
 *
 * 设计与 lib/theme.ts 同构 —— 一个模块级外部 store(getLang/setLang + 订阅),
 * 组件用 useT() / useLang() 通过 useSyncExternalStore 订阅,语言一变就重渲染,
 * 不需要 Context 包裹、不需要 props 逐层透传。
 *
 * 文案按语言拆成独立文件(locales/zh.ts、locales/en.ts),key 在类型上强制对齐。
 * 带参数的文案用 `{name}` 占位,t(key, params) 负责插值 —— 不在组件里写
 * `lang === "zh" ? … : …` 这种散落三元。真实数据(提交/作者/路径/SHA)不进字典。
 */

export type Lang = "zh" | "en";

const KEY = "lang";

const messages: Record<Lang, Record<MessageKey, string>> = { zh, en };

/** 读偏好:localStorage 优先,否则跟随 navigator.language,默认中文。 */
function detectDefault(): Lang {
  const stored = localStorage.getItem(KEY);
  if (stored === "zh" || stored === "en") return stored;
  return typeof navigator !== "undefined" && navigator.language?.toLowerCase().startsWith("zh") ? "zh" : "en";
}

let current: Lang = detectDefault();
const listeners = new Set<() => void>();

// 首屏把 <html lang> 同步好(无障碍 + 字体回退正确)
if (typeof document !== "undefined") document.documentElement.lang = current;

export function getLang(): Lang {
  return current;
}

/** 切到指定语言:写 DOM + 持久化 + 通知所有订阅者重渲染。 */
export function setLang(lang: Lang): void {
  if (lang === current) return;
  current = lang;
  localStorage.setItem(KEY, lang);
  if (typeof document !== "undefined") document.documentElement.lang = lang;
  listeners.forEach((fn) => fn());
}

export function toggleLang(): void {
  setLang(current === "zh" ? "en" : "zh");
}

/** 语言切换键的显示文案:显示「将切换到的那个语言」。 */
export function nextLangLabel(lang: Lang): string {
  return lang === "zh" ? "EN" : "中";
}

function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

/** 订阅语言:语言变化时触发组件重渲染。 */
export function useLang(): Lang {
  return useSyncExternalStore(subscribe, getLang, getLang);
}

export type TParams = Record<string, string | number>;

export function translate(lang: Lang, key: MessageKey, params?: TParams): string {
  let s: string = messages[lang][key] ?? messages.zh[key] ?? key;
  if (params) {
    for (const [k, v] of Object.entries(params)) s = s.split(`{${k}}`).join(String(v));
  }
  return s;
}

/**
 * 翻译 hook:订阅语言 + 返回绑定当前语言的 t()。
 * 用法:`const t = useT(); …t("nav.history")` / `t("cmd.goToView", { name })`
 */
export function useT(): (key: MessageKey, params?: TParams) => string {
  const lang = useLang();
  return (key, params) => translate(lang, key, params);
}
