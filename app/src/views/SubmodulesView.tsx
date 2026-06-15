import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useSubmodules, qk, invalidateWorktree } from "../lib/queries";
import { updateSubmodule, type IpcError, type SubmoduleInfoDto, type SubmoduleStatusStr } from "../ipc";
import { SubmoduleIcon, SpinnerIcon, PullIcon, RefreshIcon } from "../components/icons";
import { Button } from "../components/ui/Button";
import { EmptyHint } from "../components/ui/EmptyHint";
import { useToast } from "../components/Toast";

/** 各状态的徽章样式 + 中文。颜色只用 @theme token,不硬编码 hex。 */
const STATUS_META: Record<SubmoduleStatusStr, { label: string; cls: string }> = {
  "up-to-date": { label: "已同步", cls: "border-success/40 bg-success/10 text-success" },
  modified: { label: "未同步", cls: "border-warning/40 bg-warning/10 text-warning" },
  uninitialized: { label: "未初始化", cls: "border-line-strong bg-elevated text-fg-subtle" },
  conflict: { label: "冲突", cls: "border-danger/40 bg-danger/10 text-danger" },
};

export function SubmodulesView({ repo }: { repo: string }) {
  const q = useSubmodules(repo);
  const subs = q.data ?? [];
  const queryErr = (q.error as IpcError | null)?.message ?? null;

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-1.5 text-xs text-fg-muted">
        <SubmoduleIcon width={13} height={13} />
        <span>子模块</span>
        {subs.length > 0 && <span className="text-fg-subtle">· {subs.length}</span>}
      </div>

      {q.isLoading ? (
        <Center>加载中…</Center>
      ) : queryErr ? (
        <Center>{queryErr}</Center>
      ) : subs.length === 0 ? (
        <EmptyHint icon={<SubmoduleIcon width={24} height={24} />}>该仓库没有子模块</EmptyHint>
      ) : (
        <div className="fade-in flex-1 overflow-auto p-3">
          <div className="flex flex-col gap-2">
            {subs.map((s) => (
              <SubmoduleRow key={s.path} repo={repo} sub={s} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function SubmoduleRow({ repo, sub }: { repo: string; sub: SubmoduleInfoDto }) {
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const qc = useQueryClient();
  const meta = STATUS_META[sub.status as SubmoduleStatusStr];
  // 未初始化 → 初始化检出;未同步 → 更新到记录版本。两者同一后端命令,文案/图标不同。
  const action =
    sub.status === "uninitialized"
      ? { label: "初始化", Icon: PullIcon }
      : sub.status === "modified"
        ? { label: "更新到记录版本", Icon: RefreshIcon }
        : null;

  async function doUpdate() {
    setBusy(true);
    try {
      await updateSubmodule(repo, sub.path);
      toast({ kind: "success", title: `已更新子模块 ${sub.path}` });
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
      qc.invalidateQueries({ queryKey: qk.submodules(repo) });
      invalidateWorktree(qc, repo); // 子模块工作区内容变了 → 刷新 status/diff
    }
  }

  return (
    <div className="flex items-center gap-3 rounded-md border border-line bg-elevated px-3 py-2">
      <SubmoduleIcon width={16} height={16} className="shrink-0 text-fg-subtle" />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate font-mono text-[13px] text-fg" title={sub.path}>
            {sub.path}
          </span>
          <span className={`shrink-0 rounded-full border px-1.5 py-0.5 text-[10px] ${meta.cls}`}>
            {meta.label}
          </span>
        </div>
        <div className="mt-0.5 flex items-center gap-2 font-mono text-[11px] text-fg-subtle">
          {sub.short_sha && <span title={sub.head_sha}>{sub.short_sha}</span>}
          {sub.describe && <span className="truncate">({sub.describe})</span>}
          {sub.url && (
            <span className="truncate text-fg-muted" title={sub.url}>
              {sub.url}
            </span>
          )}
        </div>
      </div>
      {action && (
        <Button variant="secondary" size="sm" onClick={doUpdate} disabled={busy} className="shrink-0">
          {busy ? <SpinnerIcon width={13} height={13} /> : <action.Icon width={13} height={13} />}
          {action.label}
        </Button>
      )}
    </div>
  );
}

function Center({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 items-center justify-center p-4 text-center text-sm text-fg-subtle">
      {children}
    </div>
  );
}
