export type Tab = "changes" | "history";

export function TabBar({ active, onChange }: { active: Tab; onChange: (t: Tab) => void }) {
  const cls = (t: Tab) =>
    `px-4 py-2 text-sm cursor-pointer ${
      active === t ? "text-[#e6edf3] border-b-2 border-[#3b82f6] font-semibold" : "text-[#8b949e]"
    }`;
  return (
    <div className="flex gap-1 border-b border-[#21262d] px-2">
      <div className={cls("changes")} onClick={() => onChange("changes")}>更改</div>
      <div className={cls("history")} onClick={() => onChange("history")}>历史</div>
    </div>
  );
}
