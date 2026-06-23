import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useSubmodules, qk, invalidateWorktree } from "../lib/queries";
import { updateSubmodule, type IpcError, type SubmoduleInfoDto, type SubmoduleStatusStr } from "../ipc";
import { SubmoduleIcon, SpinnerIcon, PullIcon, RefreshIcon } from "../components/icons";
import { Button } from "../components/ui/Button";
import { EmptyHint } from "../components/ui/EmptyHint";
import { SecondaryHeader, CardTable, CardRow, Cell } from "../components/ui/CardTable";
import { useToast } from "../components/Toast";
import { useT } from "../lib/i18n";

/** 各状态的徽章 i18n key + 样式。颜色只用 @theme token,不硬编码 hex。 */
const STATUS_META: Record<SubmoduleStatusStr, { key: "submodules.statusUpToDate" | "submodules.statusModified" | "submodules.statusUninitialized" | "submodules.statusConflict"; cls: string }> = {
  "up-to-date": { key: "submodules.statusUpToDate", cls: "border-success/40 bg-success/10 text-success" },
  modified: { key: "submodules.statusModified", cls: "border-warning/40 bg-warning/10 text-warning" },
  uninitialized: { key: "submodules.statusUninitialized", cls: "border-line-strong bg-elevated text-fg-subtle" },
  conflict: { key: "submodules.statusConflict", cls: "border-danger/40 bg-danger/10 text-danger" },
};

export function SubmodulesView({ repo }: { repo: string }) {
  const t = useT();
  const q = useSubmodules(repo);
  const subs = q.data ?? [];
  const queryErr = (q.error as IpcError | null)?.message ?? null;

  if (q.isLoading) return <Center>{t("common.loading")}</Center>;
  if (queryErr) return <Center>{queryErr}</Center>;

  return (
    <div className="fade-in h-full overflow-auto px-7 py-8">
      {/* 居中卡片表格(max 860):杂志级版式 —— 图标瓦片 + 衬线标题 + mono 计数 + 列头表格。 */}
      <div className="mx-auto max-w-[860px]">
        <SecondaryHeader
          icon={<SubmoduleIcon width={17} height={17} />}
          title={t("title.submodules")}
          count={subs.length > 0 ? `${subs.length} ${t("count.items")}` : undefined}
        />
        {subs.length === 0 ? (
          <EmptyHint icon={<SubmoduleIcon width={24} height={24} />}>{t("submodules.empty")}</EmptyHint>
        ) : (
          <CardTable cols={[t("col.path"), t("col.url"), t("col.status"), t("col.commit")]}>
            {subs.map((s) => (
              <SubmoduleRow key={s.path} repo={repo} sub={s} />
            ))}
          </CardTable>
        )}
      </div>
    </div>
  );
}

function SubmoduleRow({ repo, sub }: { repo: string; sub: SubmoduleInfoDto }) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const qc = useQueryClient();
  const meta = STATUS_META[sub.status as SubmoduleStatusStr];
  // 未初始化 → 初始化检出;未同步 → 更新到记录版本。两者同一后端命令,文案/图标不同。
  const action =
    sub.status === "uninitialized"
      ? { label: t("submodules.init"), Icon: PullIcon }
      : sub.status === "modified"
        ? { label: t("submodules.update"), Icon: RefreshIcon }
        : null;

  async function doUpdate() {
    setBusy(true);
    try {
      await updateSubmodule(repo, sub.path);
      toast({ kind: "success", title: t("submodules.updated", { path: sub.path }) });
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
      qc.invalidateQueries({ queryKey: qk.submodules(repo) });
      invalidateWorktree(qc, repo); // 子模块工作区内容变了 → 刷新 status/diff
    }
  }

  return (
    <CardRow
      trailing={action ? (
        <Button variant="secondary" size="sm" onClick={doUpdate} disabled={busy} className="ml-2 shrink-0">
          {busy ? <SpinnerIcon width={13} height={13} /> : <action.Icon width={13} height={13} />}
          {action.label}
        </Button>
      ) : undefined}
    >
      <Cell first className="text-fg" >
        <span className="truncate" title={sub.path}>{sub.path}</span>
      </Cell>
      <Cell className="!font-sans" >
        <span className="truncate" title={sub.url || undefined}>{sub.url || "—"}</span>
      </Cell>
      <Cell>
        <span className={`shrink-0 rounded-full border px-1.5 py-0.5 text-[10px] ${meta.cls}`}>{t(meta.key)}</span>
      </Cell>
      <Cell last>
        <span className="truncate" title={sub.describe ? `${sub.head_sha} (${sub.describe})` : sub.head_sha}>{sub.short_sha || "—"}</span>
      </Cell>
    </CardRow>
  );
}

function Center({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 items-center justify-center p-4 text-center text-sm text-fg-subtle">
      {children}
    </div>
  );
}
