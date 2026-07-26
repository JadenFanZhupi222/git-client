import type { ReactNode } from "react";
import type { Tab } from "./TabBar";
import { Glass } from "./ui/Glass";
import { cx } from "./ui/Button";
import {
  FileDiffIcon,
  HistoryIcon,
  SubmoduleIcon,
  WorktreeIcon,
  FolderIcon,
  ChevronDownIcon,
} from "./icons";
import { useT } from "../lib/i18n";

type Item = { id: Tab; label: string; icon: ReactNode };

export function Sidebar({
  active,
  onChange,
  collapsed,
  onToggleCollapse,
  hasSubmodules = false,
  hasWorktrees = false,
  hasSparse = false,
}: {
  active: Tab;
  onChange: (t: Tab) => void;
  collapsed: boolean;
  onToggleCollapse: () => void;
  hasSubmodules?: boolean;
  hasWorktrees?: boolean;
  hasSparse?: boolean;
}) {
  const t = useT();
  const items: Item[] = [
    { id: "changes", label: t("nav.changes"), icon: <FileDiffIcon width={16} height={16} /> },
    { id: "history", label: t("nav.history"), icon: <HistoryIcon width={16} height={16} /> },
    { id: "compare", label: t("nav.compare"), icon: <FileDiffIcon width={16} height={16} /> },
    { id: "blame", label: t("nav.blame"), icon: <HistoryIcon width={16} height={16} /> },
  ];
  if (hasSubmodules) items.push({ id: "submodules", label: t("nav.submodules"), icon: <SubmoduleIcon width={16} height={16} /> });
  if (hasWorktrees) items.push({ id: "worktrees", label: t("nav.worktrees"), icon: <WorktreeIcon width={16} height={16} /> });
  if (hasSparse) items.push({ id: "sparse", label: t("nav.sparse"), icon: <FolderIcon width={16} height={16} /> });

  return (
    <Glass
      as="aside"
      className={cx(
        "flex shrink-0 flex-col gap-0.5 border-r border-line p-2 transition-[width]",
        collapsed ? "w-12 items-center" : "w-44",
      )}
    >
      {items.map((it) => {
        const on = active === it.id;
        return (
          <button
            key={it.id}
            data-testid={`nav-${it.id}`}
            onClick={() => onChange(it.id)}
            title={collapsed ? it.label : undefined}
            className={cx(
              "relative flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm transition-colors",
              "before:absolute before:left-0 before:top-1/2 before:h-0 before:w-[3px] before:-translate-y-1/2 before:rounded-full before:bg-accent before:transition-all before:duration-300 before:ease-[cubic-bezier(0.32,0.72,0,1)]",
              collapsed ? "w-9 justify-center" : "w-full",
              on
                ? "bg-accent/12 text-accent before:h-4"
                : "text-fg-muted hover:bg-overlay hover:text-fg",
            )}
          >
            <span className="shrink-0">{it.icon}</span>
            {!collapsed && <span className="truncate">{it.label}</span>}
          </button>
        );
      })}

      {/* Collapse toggle — pinned to the bottom */}
      <button
        onClick={onToggleCollapse}
        title={collapsed ? t("nav.expandSidebar") : t("nav.collapseSidebar")}
        className={cx(
          "mt-auto flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm text-fg-subtle transition-colors hover:bg-overlay hover:text-fg",
          collapsed ? "w-9 justify-center" : "w-full",
        )}
      >
        {/* ChevronDownIcon rotated 90° CW = pointing right (expand); 90° CCW = pointing left (collapse) */}
        <span
          className={cx(
            "shrink-0 transition-transform",
            collapsed ? "-rotate-90" : "rotate-90",
          )}
        >
          <ChevronDownIcon width={16} height={16} />
        </span>
        {!collapsed && <span>{t("nav.collapse")}</span>}
      </button>
    </Glass>
  );
}
