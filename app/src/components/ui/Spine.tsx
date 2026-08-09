/** 选中态招牌「发光左脊」:2px 朱红竖条 + 外发光(`boxShadow:0 0 8px accent`)。
 *  配合所在行 `relative` + `bg-accent/10` 淡底使用(见 VersionArc 原型各 fileRow/commitRow)。
 *  脊顶/底各留 7px,贴左边、右上右下圆角,与 JetBrains 的整段高亮区分,更轻更精致。 */
export function Spine() {
  return (
    <span
      aria-hidden
      className="pointer-events-none absolute bottom-[7px] left-0 top-[7px] w-0.5 rounded-r-sm"
      style={{ background: "var(--color-accent)", boxShadow: "0 0 8px var(--color-accent)" }}
    />
  );
}
