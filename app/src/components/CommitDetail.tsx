import { type CommitDto } from "../ipc";
import { formatAbsolute } from "../lib/time";

/** 选中提交的完整信息:标题(完整换行)+ 作者/时间/SHA + 正文。
 *  正文用 pre-wrap 保留换行;整体可滚动,长 message 不会撑垮文件列表。 */
export function CommitDetail({ commit }: { commit: CommitDto | null }) {
  if (!commit) return <div className="p-3 text-xs text-fg-subtle">选择一个提交查看详情</div>;
  return (
    <div className="h-full overflow-y-auto px-3 py-2.5">
      {/* 标题:完整显示、可换行 */}
      <div className="text-[13px] font-semibold leading-snug text-fg">{commit.summary}</div>

      {/* 元信息 */}
      <div className="mt-1.5 space-y-0.5 font-mono text-[11px] text-fg-muted">
        <div title={commit.author_email}>
          {commit.author_name}
          {commit.author_email && <span className="text-fg-subtle"> &lt;{commit.author_email}&gt;</span>}
        </div>
        <div>{formatAbsolute(commit.timestamp)}</div>
        <div className="text-fg-subtle" title={commit.id}>{commit.short_id}</div>
      </div>

      {/* 正文 */}
      {commit.body.trim() && (
        <pre className="mt-2.5 whitespace-pre-wrap break-words border-t border-line pt-2.5 font-sans text-[12px] leading-relaxed text-fg">
          {commit.body.trim()}
        </pre>
      )}
    </div>
  );
}
