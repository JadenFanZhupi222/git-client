import { useWorktrees } from "../lib/queries";
import { WorktreeIcon, BranchIcon } from "../components/icons";
import { EmptyHint } from "../components/ui/EmptyHint";
import { SecondaryHeader, CardTable, CardRow, Cell } from "../components/ui/CardTable";
import type { IpcError, WorktreeInfoDto } from "../ipc";
import { useT } from "../lib/i18n";

export function WorktreesView({ repo }: { repo: string }) {
  const t = useT();
  const q = useWorktrees(repo);
  const wts = q.data ?? [];
  const queryErr = (q.error as IpcError | null)?.message ?? null;

  if (q.isLoading) return <Center>{t("common.loading")}</Center>;
  if (queryErr) return <Center>{queryErr}</Center>;

  return (
    <div className="fade-in h-full overflow-auto px-7 py-8">
      {/* 居中卡片表格(max 860) */}
      <div className="mx-auto max-w-[860px]">
        <SecondaryHeader
          icon={<WorktreeIcon width={17} height={17} />}
          title={t("title.worktrees")}
          count={wts.length > 0 ? `${wts.length} ${t("count.items")}` : undefined}
        />
        {wts.length === 0 ? (
          <EmptyHint icon={<WorktreeIcon width={24} height={24} />}>{t("worktrees.empty")}</EmptyHint>
        ) : (
          <CardTable cols={[t("col.path"), t("col.branch"), t("col.head"), t("col.status")]}>
            {wts.map((w) => (
              <WorktreeRow key={w.path} wt={w} />
            ))}
          </CardTable>
        )}
      </div>
    </div>
  );
}

function WorktreeRow({ wt }: { wt: WorktreeInfoDto }) {
  const t = useT();
  return (
    <CardRow accent={wt.is_current}>
      <Cell first>
        <span className="truncate" title={wt.path}>{wt.path}</span>
      </Cell>
      <Cell className="!font-sans">
        <BranchIcon width={11} height={11} className="shrink-0" />
        <span className="truncate">{wt.detached ? t("worktrees.detached") : wt.branch || "—"}</span>
      </Cell>
      <Cell last>
        <span title={wt.head_sha}>{wt.short_sha || "—"}</span>
      </Cell>
      <Cell className="!font-sans flex-wrap gap-1">
        {wt.is_current && <Badge cls="border-accent/40 bg-accent/10 text-accent">{t("worktrees.current")}</Badge>}
        {wt.is_main && <Badge cls="border-line-strong bg-overlay text-fg-muted">{t("worktrees.main")}</Badge>}
        {wt.locked && <Badge cls="border-warning/40 bg-warning/10 text-warning">{t("worktrees.locked")}</Badge>}
        {wt.bare && <Badge cls="border-line-strong bg-overlay text-fg-muted">{t("worktrees.bare")}</Badge>}
        {!wt.is_current && !wt.is_main && !wt.locked && !wt.bare && <span className="text-fg-subtle">—</span>}
      </Cell>
    </CardRow>
  );
}

function Badge({ cls, children }: { cls: string; children: React.ReactNode }) {
  return (
    <span className={`shrink-0 rounded-full border px-1.5 py-0.5 text-[10px] ${cls}`}>{children}</span>
  );
}

function Center({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 items-center justify-center p-4 text-center text-sm text-fg-subtle">
      {children}
    </div>
  );
}
