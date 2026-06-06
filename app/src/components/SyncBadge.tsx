import { type AheadBehindDto } from "../ipc";

/** 状态栏同步角标:↓落后(可 Pull,蓝)/ ↑领先(可 Push,绿)/ 已同步。
 *  无上游(sync=null)时不渲染。仿 JetBrains/VSCode 的分支同步指示。 */
export function SyncBadge({ sync }: { sync: AheadBehindDto | null }) {
  if (!sync) return null;
  const { ahead, behind } = sync;
  if (ahead === 0 && behind === 0) {
    return <span className="text-fg-subtle" title="已与上游同步">✓ 已同步</span>;
  }
  const tip = [
    behind > 0 ? `落后 ${behind} 个(可 Pull)` : "",
    ahead > 0 ? `领先 ${ahead} 个(可 Push)` : "",
  ].filter(Boolean).join("、");
  return (
    <span className="flex items-center gap-1.5 font-mono" title={tip}>
      {behind > 0 && <span className="text-accent">↓{behind}</span>}
      {ahead > 0 && <span className="text-success">↑{ahead}</span>}
    </span>
  );
}
