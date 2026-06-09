import { useState } from "react";
import { opGoto, type IpcError } from "../ipc";
import { useOpLog } from "../lib/queries";
import { formatRelative } from "../lib/time";
import { useToast } from "./Toast";
import { IconButton } from "./ui/IconButton";
import { CloseIcon, UndoIcon, RedoIcon } from "./icons";

/**
 * 操作日志面板:本工具本会话做过的写操作时间线(commit/reset/cherry-pick…)。
 * 当前所在项高亮;点其它项 = 沿时间线 reset --soft 跳过去(撤销/重做的泛化,永不丢工作区)。
 * 数据来自 RepoContext 的 UndoNav(与顶栏撤销/重做同一条线)。
 */
export function OpLogPanel({
  repo,
  onClose,
  onJumped,
}: {
  repo: string;
  onClose: () => void;
  onJumped: () => void;
}) {
  const toast = useToast();
  const q = useOpLog(repo, true);
  const log = q.data;
  const [busy, setBusy] = useState(false);

  async function jump(index: number, label: string, dir: "back" | "fwd") {
    setBusy(true);
    try {
      const info = await opGoto(repo, index);
      toast({
        kind: "success",
        title: `${dir === "back" ? "已回到" : "已前进到"}:${label}`,
        detail: `HEAD → ${info.target_short},${info.worktree_restored ? "工作区已还原" : "改动回暂存区"}`,
      });
      onJumped();
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
    }
  }

  // 时间线是 oldest→newest;面板按最新在上展示。
  const rows = log ? log.entries.map((e, i) => ({ e, i })).reverse() : [];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        className="flex max-h-[80vh] w-[560px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex shrink-0 items-center justify-between border-b border-line px-4 py-3">
          <h2 className="text-sm font-semibold text-fg">操作日志 · 本会话</h2>
          <IconButton aria-label="关闭" onClick={onClose}><CloseIcon width={15} height={15} /></IconButton>
        </div>

        <p className="shrink-0 border-b border-line bg-accent/10 px-4 py-2 text-xs text-fg-muted">
          本工具做过的写操作。点任意一项跳过去：撤销提交→改动回暂存区；撤销 reset/cherry-pick 等→忠实还原工作区(有未提交改动会先拦下，不丢活)。
        </p>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {q.isLoading ? (
            <div className="p-4 text-xs text-fg-subtle">加载中…</div>
          ) : rows.length === 0 ? (
            <div className="p-4 text-xs text-fg-subtle">本会话还没有可记录的操作</div>
          ) : (
            rows.map(({ e, i }) => {
              const current = i === log!.current;
              const dir: "back" | "fwd" = i < log!.current ? "back" : "fwd";
              return (
                <button
                  key={i}
                  disabled={busy || current}
                  onClick={() => jump(i, e.label, dir)}
                  className={`flex w-full items-center gap-3 border-b border-line/60 px-4 py-2 text-left transition-colors disabled:cursor-default ${
                    current ? "bg-accent/10" : "hover:bg-elevated"
                  }`}
                >
                  <span className="grid w-5 shrink-0 place-items-center text-fg-subtle">
                    {current ? (
                      <span className="h-2 w-2 rounded-full bg-accent" title="当前位置" />
                    ) : dir === "back" ? (
                      <UndoIcon width={13} height={13} />
                    ) : (
                      <RedoIcon width={13} height={13} />
                    )}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-[13px] text-fg" title={e.label}>
                    {e.label}
                    {current && <span className="ml-2 text-[11px] text-accent">当前</span>}
                  </span>
                  <span className="w-16 shrink-0 font-mono text-[11px] text-accent">{e.target_short}</span>
                  <span className="w-16 shrink-0 text-right text-[11px] text-fg-subtle">
                    {e.timestamp ? formatRelative(e.timestamp) : ""}
                  </span>
                </button>
              );
            })
          )}
        </div>
      </div>
    </div>
  );
}
