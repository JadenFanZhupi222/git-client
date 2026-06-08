import type { ButtonHTMLAttributes, ReactNode } from "react";

/** 极简 className 合并:拼接真值串。无需引第三方 clsx。 */
export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

export type ButtonVariant = "primary" | "commit" | "secondary" | "danger" | "ghost";
export type ButtonSize = "chip" | "sm" | "md";

const BASE =
  "inline-flex items-center justify-center gap-1.5 font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-40";

const SIZES: Record<ButtonSize, string> = {
  chip: "rounded px-1.5 py-0.5 text-[11px]",
  sm: "rounded-md px-2 py-1 text-xs",
  md: "rounded-md px-3.5 py-1.5 text-sm",
};

const VARIANTS: Record<ButtonVariant, string> = {
  primary: "bg-accent text-white hover:opacity-90",
  commit: "bg-done text-white hover:opacity-90",
  secondary: "border border-line-strong bg-elevated text-fg-muted hover:bg-overlay hover:text-fg",
  danger: "border border-danger/50 text-danger hover:bg-danger/15",
  ghost: "text-fg-muted hover:bg-overlay hover:text-fg",
};

/** 全项目统一的按钮原语。颜色/尺寸只认 @theme token,样式集中在此一处。
 *  额外布局类(w-full、ml-auto、绝对定位等)通过 className 传入并追加。 */
export function Button({
  variant = "secondary",
  size = "sm",
  className,
  children,
  ...rest
}: {
  variant?: ButtonVariant;
  size?: ButtonSize;
  children?: ReactNode;
} & ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button className={cx(BASE, SIZES[size], VARIANTS[variant], className)} {...rest}>
      {children}
    </button>
  );
}
