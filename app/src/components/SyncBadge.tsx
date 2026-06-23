import { type AheadBehindDto } from "../ipc";
import { useT } from "../lib/i18n";

/** 状态栏同步角标:↓落后(可 Pull,蓝)/ ↑领先(可 Push,绿)/ 已同步。
 *  无上游(sync=null)时不渲染。仿 JetBrains/VSCode 的分支同步指示。 */
export function SyncBadge({ sync }: { sync: AheadBehindDto | null }) {
  const t = useT();
  if (!sync) return null;
  const { ahead, behind } = sync;
  if (ahead === 0 && behind === 0) {
    return <span className="text-fg-subtle" title={t("sync.syncedTitle")}>✓ {t("sync.synced")}</span>;
  }
  const tip = [
    behind > 0 ? t("sync.behind", { n: behind }) : "",
    ahead > 0 ? t("sync.ahead", { n: ahead }) : "",
  ].filter(Boolean).join("、");
  return (
    <span className="flex items-center gap-1.5 font-mono" title={tip}>
      {behind > 0 && <span className="text-accent">↓{behind}</span>}
      {ahead > 0 && <span className="text-success">↑{ahead}</span>}
    </span>
  );
}
