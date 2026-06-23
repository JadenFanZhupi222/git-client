import type { ReactNode } from "react";
import { Glass } from "./Glass";
import { cx } from "./Button";

/** 浮动磨砂玻璃工具栏:绝对定位悬于所在列顶部,内容从其下方滚过显折射(招牌材质)。
 *
 *  关键(仓库铁律):定位必须放在外层普通 div —— `.glass` 自带 `position:relative`
 *  是 unlayered 规则,会压过 Tailwind 的 `absolute` 工具类,直接给 <Glass> 加 absolute
 *  会沉到底部失效。
 *
 *  单行栏总高 ≈ 58px(外层 padding 9·2 + glass minHeight 40)。滚动体顶部留 {@link FLOAT_BAR_INSET}
 *  让首行从栏底穿过,而不是被遮住。多行内容(如历史搜索 + 模式)请改用 ResizeObserver 实测高度。 */
export const FLOAT_BAR_INSET = 58;

export function FloatBar({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className="absolute inset-x-0 top-0 z-10 px-3.5 py-[9px]">
      <Glass className={cx("flex min-h-10 items-center gap-2.5 rounded-xl px-2.5 py-[7px]", className)}>
        {children}
      </Glass>
    </div>
  );
}
