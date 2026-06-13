import { useEffect, useRef } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";

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

/**
 * 把数组里 from 处的元素移动到 to 处(标准列表重排语义,用于拖拽排序)。
 * 越界或原地返回原数组(引用不变,便于 setState 跳过无效更新)。
 */
export function moveItem<T>(arr: T[], from: number, to: number): T[] {
  if (from === to || from < 0 || to < 0 || from >= arr.length || to >= arr.length) return arr;
  const next = [...arr];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
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

/** 把焦点圈在容器内(模态焦点陷阱):Tab 到尾回头、Shift+Tab 到头回尾。 */
function trapFocus(container: HTMLElement | null, e: ReactKeyboardEvent) {
  if (!container) return;
  const focusable = container.querySelectorAll<HTMLElement>(
    'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
  );
  if (focusable.length === 0) {
    e.preventDefault(); // 无可聚焦元素:别让 Tab 跑到模态外
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const active = document.activeElement as HTMLElement | null;
  if (e.shiftKey) {
    if (active === first || active === container) {
      e.preventDefault();
      last.focus();
    }
  } else if (active === last) {
    e.preventDefault();
    first.focus();
  }
}

export interface ModalListNavOptions {
  /** 列表项数量 */
  count: number;
  /** 当前选中下标 */
  index: number;
  /** 把选中移动到下标 i */
  onSelect: (i: number) => void;
  /** Esc 关闭模态 */
  onClose: () => void;
}

/**
 * 模态弹层(file/line history 这类 overlay)的键盘 a11y:
 * - 挂载时把焦点移到弹层、卸载时还回原处(打开前聚焦的元素);
 * - Esc 关闭;↑↓/j/k/g/G/Home/End 在列表内移动选中(复用 [`navTarget`]);
 * - Tab 焦点陷阱(不逃出模态)。
 *
 * 返回 `{ dialogRef, onKeyDown }`:把 ref 挂到弹层根 div(需 `tabIndex={-1}` +
 * `role="dialog"` + `aria-modal`),onKeyDown 也挂在该 div 上。导航键 `stopPropagation`,
 * 避免背景列表的全局 [`useListKeyboardNav`] 也跟着动。
 */
export function useModalListNav({ count, index, onSelect, onClose }: ModalListNavOptions) {
  const dialogRef = useRef<HTMLDivElement>(null);

  // 打开即聚焦弹层;关闭还原焦点到打开前的元素(键盘用户不丢上下文)。
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    dialogRef.current?.focus();
    return () => prev?.focus?.();
  }, []);

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
      return;
    }
    if (e.key === "Tab") {
      trapFocus(dialogRef.current, e);
      return;
    }
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (isTypingTarget(e.target)) return;
    const next = navTarget(e.key, index, count);
    if (next === null) return;
    e.preventDefault();
    e.stopPropagation(); // 别让背景列表的全局监听也响应
    onSelect(next);
  };

  return { dialogRef, onKeyDown };
}
