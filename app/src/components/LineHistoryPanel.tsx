import { useEffect, useRef, useState } from "react";
import { useLineHistory } from "../lib/queries";
import { useModalListNav } from "../lib/listNav";
import { formatRelative } from "../lib/time";
import { DiffView } from "./DiffView";
import { IconButton } from "./ui/IconButton";
import { CloseIcon } from "./icons";

/** 行历史面板:某文件第 start–end 行的演变史(git log -L)。
 *  左侧动过这几行的提交列表,选中 → 右侧显示那几行在该提交的 diff(仅范围 hunk,复用 DiffView)。 */
export function LineHistoryPanel({
  repo, file, range, onClose,
}: {
  repo: string;
  file: string;
  range: { start: number; end: number };
  onClose: () => void;
}) {
  const q = useLineHistory(repo, file, range);
  const entries = q.data ?? [];
  const [idx, setIdx] = useState(0);
  useEffect(() => { setIdx(0); }, [file, range.start, range.end]); // 换范围回到最新一条
  const safeIdx = Math.min(idx, Math.max(0, entries.length - 1));
  const selected = entries[safeIdx] ?? null;

  const { dialogRef, onKeyDown } = useModalListNav({
    count: entries.length,
    index: safeIdx,
    onSelect: setIdx,
    onClose,
  });
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>("[data-active='true']")?.scrollIntoView({ block: "nearest" });
  }, [safeIdx]);

  const name = file.slice(file.lastIndexOf("/") + 1);
  const rangeLabel = range.start === range.end ? `第 ${range.start} 行` : `第 ${range.start}–${range.end} 行`;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={`行历史 ${name} ${rangeLabel}`}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="flex h-[85vh] w-[90vw] max-w-[1200px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas shadow-2xl outline-none"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex shrink-0 items-center justify-between border-b border-line px-4 py-3">
          <h2 className="min-w-0 text-sm font-semibold text-fg">
            行历史 · <span className="font-mono text-accent" title={file}>{name}</span>
            <span className="ml-1.5 text-fg-muted">{rangeLabel}</span>
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
                <div className="p-4 text-xs text-danger">{(q.error as { message?: string }).message ?? "读取行历史失败"}</div>
              ) : entries.length === 0 ? (
                <div className="p-4 text-xs text-fg-subtle">这几行没有可追溯的历史</div>
              ) : (
                entries.map((e, i) => {
                  const c = e.commit;
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
            {entries.length > 0 && (
              <div className="shrink-0 border-t border-line px-3 py-1.5 text-[11px] text-fg-subtle">
                {entries.length} 次改动
              </div>
            )}
          </div>

          {/* 右:选中提交里这几行的 diff(来自 entry,无需另查) */}
          <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
            <DiffView diff={selected?.diff ?? null} loading={false} hasFile={!!selected} repo={repo} />
          </div>
        </div>
      </div>
    </div>
  );
}
