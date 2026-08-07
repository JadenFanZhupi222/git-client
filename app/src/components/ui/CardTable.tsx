import type { ReactNode } from "react";

/** 次级视图(子模块 / 工作树 / 稀疏检出)的杂志级页眉:
 *  34×34 朱红图标瓦片 + 衬线标题 + 其下 mono 计数。对齐 Strata 原型 viewSecondary 头。 */
export function SecondaryHeader({ icon, title, count }: { icon: ReactNode; title: string; count?: ReactNode }) {
  return (
    <div className="mb-5 flex items-center gap-3">
      <span className="grid h-[34px] w-[34px] shrink-0 place-items-center rounded-[9px] bg-accent/[0.14] text-accent-ink">{icon}</span>
      <div>
        <h2 className="serif text-[27px] font-normal leading-[1.1] text-fg">{title}</h2>
        {count != null && <p className="mt-0.5 font-mono text-[11.5px] text-fg-subtle">{count}</p>}
      </div>
    </div>
  );
}

/** 居中卡片表格外壳:圆角描边容器 + mono 小号大写列头。首列 flex-2,其余 flex-1。
 *  行用 {@link CardRow} + {@link Cell},与列头同样的 flex 权重 + gap 才能对齐。 */
export function CardTable({ cols, children }: { cols: string[]; children: ReactNode }) {
  return (
    <div className="overflow-hidden rounded-xl border border-line bg-elevated/40">
      <div className="flex items-center gap-3 border-b border-line bg-elevated/60 px-4 py-2.5">
        {cols.map((c, i) => (
          <span key={i} className={`${i === 0 ? "flex-[2]" : "flex-1"} text-[10.5px] font-semibold uppercase tracking-[0.05em] text-fg-subtle`}>
            {c}
          </span>
        ))}
      </div>
      {children}
    </div>
  );
}

/** 表格数据行。`accent` 给「当前」一类的行加极淡朱红底。`trailing` 渲染行尾动作(不占列)。 */
export function CardRow({ children, accent, trailing }: { children: ReactNode; accent?: boolean; trailing?: ReactNode }) {
  return (
    <div className={`flex items-center gap-3 border-b border-line px-4 py-2.5 transition-colors last:border-b-0 hover:bg-elevated/60 ${accent ? "bg-accent/[0.06]" : ""}`}>
      {children}
      {trailing}
    </div>
  );
}

/** 表格单元。`first` 占双倍宽 + mono + 主色;`last` mono;其余 fg-muted。 */
export function Cell({ children, first, last, className = "" }: { children: ReactNode; first?: boolean; last?: boolean; className?: string }) {
  return (
    <span
      className={`${first ? "flex-[2]" : "flex-1"} flex min-w-0 items-center gap-1.5 truncate text-[12.5px] ${first || last ? "font-mono" : ""} ${first ? "text-fg" : "text-fg-muted"} ${className}`}
    >
      {children}
    </span>
  );
}
