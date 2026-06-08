import { type CommitDto } from "../ipc";
import { formatAbsolute } from "../lib/time";
import { useCommitSignature } from "../lib/queries";

/** 选中提交的完整信息:标题(完整换行)+ 作者/时间/SHA + 签名徽章 + 正文。
 *  正文用 pre-wrap 保留换行;整体可滚动,长 message 不会撑垮文件列表。 */
export function CommitDetail({ repo, commit }: { repo: string; commit: CommitDto | null }) {
  const sig = useCommitSignature(repo, commit?.id ?? null).data ?? null;
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
        {sig && sig.status !== "none" && <SignatureBadge status={sig.status} signer={sig.signer} />}
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

/** 签名徽章:good=绿「已验证」/ unverified=黄「已签名·未验证」/ bad=红「签名无效」。 */
function SignatureBadge({ status, signer }: { status: string; signer: string }) {
  const style =
    status === "good"
      ? { cls: "border-success/40 bg-success/10 text-success", label: "已验证签名" }
      : status === "bad"
        ? { cls: "border-danger/40 bg-danger/10 text-danger", label: "签名无效" }
        : { cls: "border-warning/40 bg-warning/10 text-warning", label: "已签名 · 未验证" };
  const tip = signer ? `${style.label}:${signer}` : style.label;
  return (
    <span
      title={tip}
      className={`mt-1 inline-flex max-w-full items-center gap-1 rounded-full border px-1.5 py-0.5 text-[10px] not-italic ${style.cls}`}
    >
      <ShieldIcon />
      <span className="truncate">{style.label}{signer && ` · ${signer}`}</span>
    </span>
  );
}

function ShieldIcon() {
  return (
    <svg width={10} height={10} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round" className="shrink-0">
      <path d="M8 1.5l5 2v4c0 3.2-2.1 5.4-5 6.5-2.9-1.1-5-3.3-5-6.5v-4l5-2Z" />
      <path d="M5.8 8l1.6 1.6L10.4 6.5" />
    </svg>
  );
}
