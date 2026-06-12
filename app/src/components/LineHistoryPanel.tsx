import { useEffect, useState } from "react";
import { type LineHistoryEntryDto } from "../ipc";
import { useLineHistory } from "../lib/queries";
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
  const [selected, setSelected] = useState<LineHistoryEntryDto | null>(null);

  // 换范围 / 首次加载完成时默认选中最新一条。
  useEffect(() => { setSelected(null); }, [file, range.start, range.end]);
  useEffect(() => {
    if (!selected && entries.length > 0) setSelected(entries[0]);
  }, [entries, selected]);

  const name = file.slice(file.lastIndexOf("/") + 1);
  const rangeLabel = range.start === range.end ? `第 ${range.start} 行` : `第 ${range.start}–${range.end} 行`;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        className="flex h-[85vh] w-[90vw] max-w-[1200px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas shadow-2xl"
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
            <div className="min-h-0 flex-1 overflow-y-auto">
              {q.isLoading ? (
                <div className="p-4 text-xs text-fg-subtle">加载中…</div>
              ) : q.error ? (
                <div className="p-4 text-xs text-danger">{(q.error as { message?: string }).message ?? "读取行历史失败"}</div>
              ) : entries.length === 0 ? (
                <div className="p-4 text-xs text-fg-subtle">这几行没有可追溯的历史</div>
              ) : (
                entries.map((e) => {
                  const c = e.commit;
                  const on = selected?.commit.id === c.id;
                  return (
                    <div
                      key={c.id}
                      onClick={() => setSelected(e)}
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
            <DiffView diff={selected?.diff ?? null} loading={false} hasFile={!!selected} />
          </div>
        </div>
      </div>
    </div>
  );
}
