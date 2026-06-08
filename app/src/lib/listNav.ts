import { useEffect } from "react";

/**
 * 列表的全局键盘导航(M3「Fluid」)——j/k 上下移动、g/G 跳首尾,选中即驱动详情/diff。
 *
 * 纯逻辑(navTarget / isTypingTarget)抽出来可单测;useListKeyboardNav 只负责挂/卸监听。
 */

/** 是否正处在文本输入态——此时不抢键(让用户正常打字/搜索)。 */
export function isTypingTarget(el: EventTarget | null): boolean {
  const t = el as HTMLElement | null;
  if (!t || !t.tagName) return false;
  const tag = t.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || t.isContentEditable;
}

/**
 * 给定按键 + 当前下标 + 总数,算出目标下标;不是导航键或无需移动则返回 null。
 * j/↓ 下移、k/↑ 上移(到边界停住),g/Home 到顶,G/End 到底。未选中(index<0)时
 * 上/下都先落到第 0 项。
 */
export function navTarget(key: string, index: number, count: number): number | null {
  if (count <= 0) return null;
  let next: number;
  switch (key) {
    case "j":
    case "ArrowDown":
      next = index < 0 ? 0 : Math.min(index + 1, count - 1);
      break;
    case "k":
    case "ArrowUp":
      next = index < 0 ? 0 : Math.max(index - 1, 0);
      break;
    case "g":
    case "Home":
      next = 0;
      break;
    case "G":
    case "End":
      next = count - 1;
      break;
    default:
      return null;
  }
  return next === index ? null : next;
}

export interface ListNavOptions {
  /** 列表项数量 */
  count: number;
  /** 当前选中下标(-1 = 未选) */
  index: number;
  /** 把选中移动到下标 i */
  onSelect: (i: number) => void;
  /** false 时不挂监听(如有弹层打开、或不在该视图) */
  enabled?: boolean;
}

/**
 * 列表全局键盘导航。在文本输入态(input/textarea/select/contenteditable)
 * 与组合键(⌘/Ctrl/Alt,留给命令面板等)下不拦截。
 */
export function useListKeyboardNav({ count, index, onSelect, enabled = true }: ListNavOptions) {
  useEffect(() => {
    if (!enabled || count === 0) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (isTypingTarget(document.activeElement)) return;
      const next = navTarget(e.key, index, count);
      if (next === null) return;
      e.preventDefault();
      onSelect(next);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [count, index, onSelect, enabled]);
}
