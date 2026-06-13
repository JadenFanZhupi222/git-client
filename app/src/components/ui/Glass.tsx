import type { HTMLAttributes, ReactNode } from "react";
import { cx } from "./Button";

/** 液态玻璃容器。只用于外壳浮层(顶栏/侧栏/菜单/面板/Toast/弹层),
 *  禁止包 Diff/图谱/文件列表等密集内容。渲染档位由 <html data-glass> 决定。 */
export function Glass({
  as: Tag = "div",
  className,
  children,
  ...rest
}: {
  as?: "div" | "nav" | "aside" | "header" | "section";
  children?: ReactNode;
} & HTMLAttributes<HTMLElement>) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const T = Tag as any;
  return (
    <T className={cx("glass", className)} {...rest}>
      {children}
    </T>
  );
}
