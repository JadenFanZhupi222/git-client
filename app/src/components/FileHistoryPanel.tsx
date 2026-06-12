import { useEffect, useState } from "react";
import { type CommitDto } from "../ipc";
import { useFileHistory, useCommitDiff } from "../lib/queries";
import { formatRelative } from "../lib/time";
import { DiffView } from "./DiffView";
import { IconButton } from "./ui/IconButton";
import { CloseIcon } from "./icons";

/** 文件历史面板:某文件的提交历史(git log --follow,跟随重命名)。
 *  左侧动过该文件的提交列表,选中 → 右侧显示该文件在那次提交的 diff(复用 DiffView,自带统一/并排)。
 *  只读:看历史 + diff,不做 per-commit 操作。 */
export function FileHistoryPanel({
  repo, file, onClose,
}: {
  repo: string;
  file: string;
  onClose: () => void;
}) {
  const q = useFileHistory(repo, file);
  const commits = q.data ?? [];
  const [selected, setSelected] = useState<CommitDto | null>(null);

  // 换文件 / 首次加载完成时,默认选中最新一条。
  useEffect(() => { setSelected(null); }, [file]);
  useEffect(() => {
    if (!selected && commits.length > 0) setSelected(commits[0]);
  }, [commits, selected]);

  const diffQ = useCommitDiff(repo, selected?.id ?? null, file);
  const name = file.slice(file.lastIndexOf("/") + 1);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        className="flex h-[85vh] w-[90vw] max-w-[1200px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex shrink-0 items-center justify-between border-b border-line px-4 py-3">
          <h2 className="min-w-0 text-sm font-semibold text-fg">
            文件历史 · <span className="font-mono text-accent" title={file}>{name}</span>
          </h2>
          <IconButton aria-label="关闭" onClick={onClose}><CloseIcon width={15} height={15} /></IconButton>
        </div>

        <div className="flex min-h-0 flex-1">
          {/* 左:提交列表 */}
          <div className="flex w-[340px] shrink-0 flex-col overflow-hidden border-r border-line">
            <div className="min-h-0 flex-1 overflow-y-auto">
              {q.isLoading ? (
                <div className="p-4 text-xs text-fg-subtle">加载中…</div>
              ) : q.error ? (
                <div className="p-4 text-xs text-danger">{(q.error as { message?: string }).message ?? "读取文件历史失败"}</div>
              ) : commits.length === 0 ? (
                <div className="p-4 text-xs text-fg-subtle">该文件没有提交历史</div>
              ) : (
                commits.map((c) => {
                  const on = selected?.id === c.id;
                  return (
                    <div
                      key={c.id}
                      onClick={() => setSelected(c)}
                      title={c.summary}
                      className={`cursor-pointer border-b border-line/60 px-3 py-2 transition-colors ${
                        on ? "bg-overlay" : "hover:bg-elevated"
                      }`}
                    >
                      <div className="truncate text-[13px] text-fg">{c.summary}</div>
                      <div className="mt-0.5 flex items-center gap-2 text-[11px] text-fg-subtle">
                        <span className="font-mono text-accent">{c.short_id}</span>
                        <span className="truncate">{c.author_name}</span>
                        <span className="ml-auto shrink-0" title={String(c.timestamp)}>{formatRelative(c.timestamp)}</span>
                      </div>
                    </div>
                  );
                })
              )}
            </div>
            {commits.length > 0 && (
              <div className="shrink-0 border-t border-line px-3 py-1.5 text-[11px] text-fg-subtle">
                {commits.length} 次提交
              </div>
            )}
          </div>

          {/* 右:选中提交里该文件的 diff */}
          <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
            <DiffView diff={diffQ.data ?? null} loading={diffQ.isLoading} hasFile={!!selected} />
          </div>
        </div>
      </div>
    </div>
  );
}
