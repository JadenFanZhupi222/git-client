import { useReflog } from "../lib/queries";
import { formatRelative } from "../lib/time";
import { ResetMenu } from "./ResetMenu";
import { IconButton } from "./ui/IconButton";
import { CloseIcon } from "./icons";
import { useT } from "../lib/i18n";

const REFLOG_LIMIT = 200;

/** reflog 面板:HEAD 的移动历史(后悔药)。
 *  每条记录可一键 reset 回去 —— 即便提交已不在任何分支上,只要还在 reflog 就能找回。
 *  onReset 让上层在 reset 成功后失效历史/工作区/状态。 */
export function ReflogPanel({
  repo, onClose, onReset,
}: {
  repo: string;
  onClose: () => void;
  onReset: () => void;
}) {
  const t = useT();
  const q = useReflog(repo, REFLOG_LIMIT, true);
  const entries = q.data ?? [];

  return (
    <div className="overlay-in fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        className="panel-in popover flex max-h-[80vh] w-[680px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex shrink-0 items-center justify-between border-b border-line px-4 py-3">
          <h2 className="text-sm font-semibold text-fg">{t("reflog.title")}</h2>
          <IconButton aria-label={t("toast.close")} onClick={onClose}><CloseIcon width={15} height={15} /></IconButton>
        </div>

        <p className="shrink-0 border-b border-line bg-accent/10 px-4 py-2 text-xs text-fg-muted">
          {t("reflog.desc")}
        </p>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {q.isLoading ? (
            <div className="p-4 text-xs text-fg-subtle">{t("common.loading")}</div>
          ) : q.error ? (
            <div className="p-4 text-xs text-danger">{(q.error as { message?: string }).message ?? t("reflog.readFailed")}</div>
          ) : entries.length === 0 ? (
            <div className="p-4 text-xs text-fg-subtle">{t("reflog.empty")}</div>
          ) : (
            entries.map((e) => (
              <div
                key={e.index}
                className="flex items-center gap-3 border-b border-line/60 px-4 py-2 hover:bg-elevated"
              >
                <span className="w-16 shrink-0 font-mono text-[11px] text-fg-subtle">{e.selector}</span>
                <span className="w-16 shrink-0 font-mono text-[11px] text-accent">{e.new_short}</span>
                <span className="min-w-0 flex-1 truncate text-[13px] text-fg" title={e.message}>{e.message}</span>
                <span className="w-20 shrink-0 text-right text-[11px] text-fg-subtle" title={String(e.timestamp)}>
                  {formatRelative(e.timestamp)}
                </span>
                <div className="shrink-0">
                  <ResetMenu repo={repo} commitId={e.new_oid} label={e.new_short} onDone={onReset} />
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
