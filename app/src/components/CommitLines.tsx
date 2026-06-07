import { type CommitDto } from "../ipc";
import { formatRelative } from "../lib/time";

/** 提交的两行展示:首行 summary(可在前面插 badges),次行 `short_id · 作者 · 相对时间`。
 *  图谱行与搜索结果行共用,保证两处观感一致、改样式只改一处。 */
export function CommitLines({ commit, badges }: { commit: CommitDto; badges?: React.ReactNode }) {
  return (
    <>
      <div className="flex items-center gap-1 overflow-hidden">
        {badges}
        <span className="truncate text-[13px] text-fg" title={commit.summary}>{commit.summary}</span>
      </div>
      <div className="truncate font-mono text-[11px] text-fg-muted">
        {commit.short_id} · {commit.author_name} · {formatRelative(commit.timestamp)}
      </div>
    </>
  );
}
