import type { ButtonHTMLAttributes, ReactNode } from "react";
import { cx } from "./Button";

export type IconButtonTone = "default" | "danger";

/** tone 决定 hover 配色,集中在原语里(别在调用处用 className 覆盖,Tailwind v4 胜负不靠顺序)。 */
const TONES: Record<IconButtonTone, string> = {
  default: "text-fg-subtle hover:bg-overlay hover:text-fg",
  danger: "text-fg-subtle hover:bg-danger/10 hover:text-danger",
};

/** 仅含图标的小按钮(关闭 X、行内动作图标等)。
 *  统一 hover 反馈,无边框。aria-label 必填以保证可达性。tone="danger" 用于删除等危险动作。 */
export function IconButton({
  className,
  children,
  tone = "default",
  "aria-label": ariaLabel,
  ...rest
}: {
  "aria-label": string;
  tone?: IconButtonTone;
  children: ReactNode;
} & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      aria-label={ariaLabel}
      className={cx(
        "inline-flex items-center justify-center rounded p-0.5 transition-colors disabled:cursor-not-allowed disabled:opacity-40",
        TONES[tone],
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}
