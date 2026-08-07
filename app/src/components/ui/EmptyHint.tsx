import type { ReactNode } from "react";

/** 统一的「空 / 无数据」占位:软图标贴片 + 文案 + 可选操作。
 *  全 app 的空态走它,视觉一致(替换各处裸文字)。h-full + 默认 flex-shrink:
 *  在 flex-col(有栏头)里收缩填满剩余,在普通块父级里铺满父高,两种父级都居中。 */
export function EmptyHint({ icon, children, action }: { icon: ReactNode; children: ReactNode; action?: ReactNode }) {
  return (
    <div className="fade-in flex h-full w-full flex-col items-center justify-center gap-3 p-6 text-center">
      <div className="grid h-11 w-11 place-items-center rounded-xl border border-line bg-elevated/40 text-fg-muted">
        {icon}
      </div>
      <p className="max-w-[30ch] text-sm text-fg-muted">{children}</p>
      {action}
    </div>
  );
}
