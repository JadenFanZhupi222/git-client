import { useEffect, useRef, useState } from "react";
import { useFileHistory, useCommitDiff } from "../lib/queries";
import { useModalListNav } from "../lib/listNav";
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
  // 下标驱动选中:键盘 ↑↓ 直接走 navTarget,免去 null+effect 的择一。
  const [idx, setIdx] = useState(0);
  useEffect(() => { setIdx(0); }, [file]); // 换文件回到最新一条
  const safeIdx = Math.min(idx, Math.max(0, commits.length - 1));
  const selected = commits[safeIdx] ?? null;

  const { dialogRef, onKeyDown } = useModalListNav({
    count: commits.length,
    index: safeIdx,
    onSelect: setIdx,
    onClose,
  });
  // 键盘移动时把选中行滚进可视区。
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>("[data-active='true']")?.scrollIntoView({ block: "nearest" });
  }, [safeIdx]);

  const diffQ = useCommitDiff(repo, selected?.id ?? null, file);
  const name = file.slice(file.lastIndexOf("/") + 1);

  return (
    <div className="overlay-in fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={`文件历史 ${name}`}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="panel-in popover flex h-[85vh] w-[90vw] max-w-[1200px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas outline-none"
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
            <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto">
              {q.isLoading ? (
                <div className="p-4 text-xs text-fg-subtle">加载中…</div>
              ) : q.error ? (
                <div className="p-4 text-xs text-danger">{(q.error as { message?: string }).message ?? "读取文件历史失败"}</div>
              ) : commits.length === 0 ? (
                <div className="p-4 text-xs text-fg-subtle">该文件没有提交历史</div>
              ) : (
                commits.map((c, i) => {
                  const on = i === safeIdx;
                  return (
                    <div
                      key={c.id}
                      data-active={on}
                      onClick={() => setIdx(i)}
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
            <DiffView diff={diffQ.data ?? null} loading={diffQ.isLoading} hasFile={!!selected} repo={repo} />
          </div>
        </div>
      </div>
    </div>
  );
}
