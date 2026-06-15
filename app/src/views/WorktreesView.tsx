import { useWorktrees } from "../lib/queries";
import { WorktreeIcon, BranchIcon } from "../components/icons";
import { EmptyHint } from "../components/ui/EmptyHint";
import type { IpcError, WorktreeInfoDto } from "../ipc";

/** 路径尾段(目录名),完整路径放 title。 */
function baseName(p: string): string {
  return p.replace(/[/\\]+$/, "").split(/[/\\]/).pop() ?? p;
}

export function WorktreesView({ repo }: { repo: string }) {
  const q = useWorktrees(repo);
  const wts = q.data ?? [];
  const queryErr = (q.error as IpcError | null)?.message ?? null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-1.5 text-xs text-fg-muted">
        <WorktreeIcon width={13} height={13} />
        <span>工作树</span>
        {wts.length > 0 && <span className="text-fg-subtle">· {wts.length}</span>}
      </div>

      {q.isLoading ? (
        <Center>加载中…</Center>
      ) : queryErr ? (
        <Center>{queryErr}</Center>
      ) : wts.length === 0 ? (
        <EmptyHint icon={<WorktreeIcon width={24} height={24} />}>没有工作树信息</EmptyHint>
      ) : (
        <div className="fade-in flex-1 overflow-auto p-3">
          <div className="flex flex-col gap-2">
            {wts.map((w) => (
              <WorktreeRow key={w.path} wt={w} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function WorktreeRow({ wt }: { wt: WorktreeInfoDto }) {
  return (
    <div
      className={`flex items-center gap-3 rounded-md border bg-elevated px-3 py-2 ${
        wt.is_current ? "border-accent/60" : "border-line"
      }`}
    >
      <WorktreeIcon width={16} height={16} className="shrink-0 text-fg-subtle" />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-[13px] text-fg" title={wt.path}>
            {baseName(wt.path)}
          </span>
          {wt.is_current && <Badge cls="border-accent/40 bg-accent/10 text-accent">当前</Badge>}
          {wt.is_main && <Badge cls="border-line-strong bg-overlay text-fg-muted">主</Badge>}
          {wt.locked && <Badge cls="border-warning/40 bg-warning/10 text-warning">锁定</Badge>}
          {wt.bare && <Badge cls="border-line-strong bg-overlay text-fg-muted">裸仓库</Badge>}
        </div>
        <div className="mt-0.5 flex items-center gap-2 font-mono text-[11px] text-fg-subtle">
          <span className="inline-flex items-center gap-1 text-fg-muted">
            <BranchIcon width={11} height={11} />
            {wt.detached ? "分离头" : wt.branch || "—"}
          </span>
          {wt.short_sha && <span title={wt.head_sha}>{wt.short_sha}</span>}
          <span className="truncate" title={wt.path}>
            {wt.path}
          </span>
        </div>
      </div>
    </div>
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
