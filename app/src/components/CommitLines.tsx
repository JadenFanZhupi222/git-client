import { type CommitDto } from "../ipc";
import { formatRelative } from "../lib/time";

/** 提交行的统一版式(图谱行与搜索结果行共用):
 *  左 = 消息列(refs 徽章 + summary,选中加粗;合并提交 summary 用 fg-muted)+ 作者(mono);
 *  右 = 相对时间(62px 右对齐)与短 SHA(54px 右对齐,选中染 accent)。
 *  自身是 flex-1 的横向行,丢进 items-center 容器即可。 */
export function CommitLines({ commit, badges, selected }: { commit: CommitDto; badges?: React.ReactNode; selected?: boolean }) {
  const isMerge = commit.parents.length > 1;
  return (
    <div className="flex min-w-0 flex-1 items-center gap-3">
      <div className="flex min-w-0 flex-1 flex-col gap-[3px]">
        <div className="flex items-center gap-1.5 overflow-hidden">
          {badges}
          <span
            data-testid="commit-subject"
            className={`truncate text-[13.5px] ${selected ? "font-semibold" : "font-normal"} ${isMerge ? "text-fg-muted" : "text-fg"}`}
            title={commit.summary}
          >
            {commit.summary}
          </span>
        </div>
        <div className="truncate font-mono text-[11px] text-fg-subtle">{commit.author_name}</div>
      </div>
      <span className="shrink-0 text-right text-[11px] text-fg-subtle" style={{ width: 62 }}>{formatRelative(commit.timestamp)}</span>
      <span className={`shrink-0 text-right font-mono text-[11.5px] ${selected ? "text-accent" : "text-fg-subtle"}`} style={{ width: 54 }}>
        {commit.short_id}
      </span>
    </div>
  );
}
