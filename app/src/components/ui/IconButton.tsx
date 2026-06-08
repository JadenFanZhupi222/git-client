import type { ButtonHTMLAttributes, ReactNode } from "react";
import { cx } from "./Button";

/** 仅含图标的小按钮(关闭 X、行内动作图标等)。
 *  统一 hover 反馈(浅底 + 加深前景),无边框。aria-label 必填以保证可达性。 */
export function IconButton({
  className,
  children,
  "aria-label": ariaLabel,
  ...rest
}: {
  "aria-label": string;
  children: ReactNode;
} & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      aria-label={ariaLabel}
      className={cx(
        "inline-flex items-center justify-center rounded p-0.5 text-fg-subtle transition-colors hover:bg-overlay hover:text-fg disabled:cursor-not-allowed disabled:opacity-40",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}
