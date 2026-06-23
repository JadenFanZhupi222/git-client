import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { continueOp, abortOp, type RepoStateStr, type IpcError } from "../ipc";
import { invalidateWorktree, invalidateHistory, qk } from "../lib/queries";
import { useToast } from "./Toast";
import { Button } from "./ui/Button";
import { AlertIcon } from "./icons";
import { useT } from "../lib/i18n";
import type { MessageKey } from "../lib/locales/zh";

const LABEL: Record<RepoStateStr, MessageKey | ""> = {
  clean: "",
  merging: "repoState.merging",
  rebasing: "repoState.rebasing",
  "cherry-picking": "repoState.cherryPicking",
  reverting: "repoState.reverting",
  other: "repoState.other",
};

/** 进行中操作横幅:显示状态 + 冲突数 + 继续/中止。conflicts>0 时「继续」禁用。 */
export function ConflictBanner({ repo, state, conflicts }: { repo: string; state: RepoStateStr; conflicts: number }) {
  const t = useT();
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
      <span className="font-semibold text-warning">{LABEL[state] ? t(LABEL[state] as MessageKey) : ""}</span>
      <span className="text-fg-muted">
        {conflicts > 0 ? t("conflict.nFiles", { n: conflicts }) : t("conflict.resolved")}
      </span>
      <div className="ml-auto flex items-center gap-1.5">
        <Button
          variant="secondary"
          size="sm"
          onClick={() => run(continueOp.bind(null, repo), t("conflict.continued"))}
          disabled={busy || conflicts > 0}
          title={conflicts > 0 ? t("conflict.continueBlockedTitle") : t("conflict.continueTitle")}
        >
          {t("conflict.continue")}
        </Button>
        <Button
          variant="danger"
          size="sm"
          onClick={() => run(abortOp.bind(null, repo), t("conflict.aborted"))}
          disabled={busy}
          title={t("conflict.abortTitle")}
        >
          {t("conflict.abort")}
        </Button>
      </div>
    </div>
  );
}
