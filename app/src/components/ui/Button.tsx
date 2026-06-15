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

// primary/commit 用主题色的柔和投影抬升,做出「可按的实体」高级感;
// 全局 button:active 已加按压回弹,这里只补静止态的标高。
const VARIANTS: Record<ButtonVariant, string> = {
  primary: "bg-accent text-white shadow-[0_1px_2px_rgba(8,15,26,0.10),0_8px_22px_-10px_color-mix(in_oklab,var(--color-accent)_60%,transparent)] hover:opacity-95",
  commit: "bg-done text-white shadow-[0_1px_2px_rgba(8,15,26,0.10),0_8px_22px_-10px_color-mix(in_oklab,var(--color-done)_60%,transparent)] hover:opacity-95",
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
