export type Tab = "changes" | "history" | "compare" | "blame" | "submodules";

const TABS: { id: Tab; label: string }[] = [
  { id: "changes", label: "更改" },
  { id: "history", label: "历史" },
  { id: "compare", label: "比较" },
  { id: "blame", label: "追溯" },
];

export function TabBar({
  active,
  onChange,
  hasSubmodules = false,
}: {
  active: Tab;
  onChange: (t: Tab) => void;
  hasSubmodules?: boolean;
}) {
  // 「子模块」标签仅在仓库确有子模块时出现,避免对绝大多数仓库徒增噪音。
  const tabs = hasSubmodules ? [...TABS, { id: "submodules" as Tab, label: "子模块" }] : TABS;
  return (
    <div className="flex shrink-0 items-center gap-1 border-b border-line px-2">
      {tabs.map((t) => {
        const on = active === t.id;
        return (
          <button
            key={t.id}
            onClick={() => onChange(t.id)}
            className={`relative px-3 py-2 text-sm transition-colors ${
              on ? "font-medium text-fg" : "text-fg-muted hover:text-fg"
            }`}
          >
            {t.label}
            {/* 下划线高亮:active 时显示,贴住底边 */}
            <span
              className={`absolute inset-x-2 -bottom-px h-0.5 rounded-full bg-accent transition-opacity ${
                on ? "opacity-100" : "opacity-0"
              }`}
            />
          </button>
        );
      })}
    </div>
  );
}
