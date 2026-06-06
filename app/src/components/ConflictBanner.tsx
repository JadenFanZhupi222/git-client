import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { continueOp, abortOp, type RepoStateStr, type IpcError } from "../ipc";
import { invalidateWorktree, invalidateHistory, qk } from "../lib/queries";
import { useToast } from "./Toast";
import { AlertIcon } from "./icons";

const LABEL: Record<RepoStateStr, string> = {
  clean: "",
  merging: "正在合并",
  rebasing: "正在变基",
  "cherry-picking": "正在 Cherry-pick",
  reverting: "正在 Revert",
  other: "操作进行中",
};

/** 进行中操作横幅:显示状态 + 冲突数 + 继续/中止。conflicts>0 时「继续」禁用。 */
export function ConflictBanner({ repo, state, conflicts }: { repo: string; state: RepoStateStr; conflicts: number }) {
  const qc = useQueryClient();
  const toast = useToast();
  const [busy, setBusy] = useState(false);

  async function run(action: () => Promise<void>, ok: string) {
    setBusy(true);
    try {
      await action();
      invalidateWorktree(qc, repo);
      invalidateHistory(qc, repo);
      qc.invalidateQueries({ queryKey: qk.repoState(repo) });
      toast({ kind: "success", title: ok });
    } catch (e) {
      // 继续时仍有冲突 → MERGE_CONFLICT:提示用户先解决剩余文件。
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex shrink-0 items-center gap-2 border-b border-warning/40 bg-warning/10 px-3 py-1.5 text-xs">
      <AlertIcon width={13} height={13} className="text-warning" />
      <span className="font-semibold text-warning">{LABEL[state]}</span>
      <span className="text-fg-muted">
        {conflicts > 0 ? `${conflicts} 个文件冲突待解决` : "冲突已解决,可继续"}
      </span>
      <div className="ml-auto flex items-center gap-1.5">
        <button
          onClick={() => run(continueOp.bind(null, repo), "已继续")}
          disabled={busy || conflicts > 0}
          title={conflicts > 0 ? "先解决所有冲突文件" : "继续(提交合并 / rebase --continue)"}
          className="rounded-md border border-line-strong bg-elevated px-2.5 py-1 text-fg transition-colors hover:bg-overlay hover:border-fg-subtle disabled:opacity-40"
        >
          继续
        </button>
        <button
          onClick={() => run(abortOp.bind(null, repo), "已中止")}
          disabled={busy}
          title="中止当前操作"
          className="rounded-md border border-line-strong bg-elevated px-2.5 py-1 text-danger transition-colors hover:bg-danger/10 disabled:opacity-40"
        >
          中止
        </button>
      </div>
    </div>
  );
}
