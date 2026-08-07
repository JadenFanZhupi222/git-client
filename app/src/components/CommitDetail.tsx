import { useState } from "react";
import { type CommitDto } from "../ipc";
import { formatAbsolute } from "../lib/time";
import { useCommitSignature } from "../lib/queries";
import { useT } from "../lib/i18n";

/** 作者姓名首字母(取前两个字符大写),用于头像方块。 */
function initials(name: string): string {
  return name.trim().slice(0, 2).toUpperCase() || "?";
}

/** 选中提交的完整信息:SHA 徽章 + 复制 → 衬线标题 → 作者头像行(含签名)→ 正文。
 *  正文用 pre-wrap 保留换行;整体可滚动,长 message 不会撑垮文件列表。 */
export function CommitDetail({ repo, commit }: { repo: string; commit: CommitDto | null }) {
  const t = useT();
  const sig = useCommitSignature(repo, commit?.id ?? null).data ?? null;
  const [copied, setCopied] = useState(false);
  if (!commit) return <div className="p-3 text-xs text-fg-muted">{t("commit.selectToView")}</div>;

  async function copySha() {
    try {
      await navigator.clipboard.writeText(commit!.id);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch { /* 剪贴板不可用:忽略 */ }
  }

  return (
    <div className="h-full overflow-y-auto px-4 py-4">
      {/* SHA 徽章 + 复制按钮 */}
      <div className="mb-3.5 flex items-center gap-2">
        <span className="rounded-md border border-line bg-canvas px-2 py-[3px] font-mono text-[12px] font-semibold text-accent" title={commit.id}>
          {commit.short_id}
        </span>
        <button
          onClick={copySha}
          title={t("commit.copySha")}
          aria-label={t("commit.copySha")}
          className="grid h-[26px] w-[26px] place-items-center rounded-md border border-line text-fg-subtle transition-colors hover:text-fg"
        >
          {copied ? <CheckMark /> : <CopyIcon />}
        </button>
      </div>

      {/* Keep diagnostic titles in the UI sans to avoid editorial drift in a data-heavy surface. */}
      <h2 className="mb-3.5 text-lg font-semibold leading-[1.3] text-fg" style={{ textWrap: "pretty" }}>{commit.summary}</h2>

      {/* 作者行:首字母方块头像(accent 底)+ 姓名 + 邮箱·时间 + 签名 */}
      <div className="flex items-center gap-2.5 border-b border-line pb-3.5">
        <div className="grid h-[30px] w-[30px] shrink-0 place-items-center rounded-lg bg-accent/[0.16] font-mono text-[11px] font-semibold text-accent-ink">
          {initials(commit.author_name)}
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] font-medium text-fg" title={commit.author_email}>{commit.author_name}</div>
          <div className="truncate font-mono text-[11px] text-fg-muted">
            {commit.author_email ? `${commit.author_email} · ` : ""}{formatAbsolute(commit.timestamp)}
          </div>
        </div>
        {sig && sig.status !== "none" && <SignatureBadge status={sig.status} signer={sig.signer} />}
      </div>

      {/* 正文 */}
      {commit.body.trim() && (
        <pre className="mt-3.5 whitespace-pre-wrap break-words font-sans text-[12px] leading-relaxed text-fg">
          {commit.body.trim()}
        </pre>
      )}
    </div>
  );
}

function CopyIcon() {
  return (
    <svg width={12} height={12} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round">
      <rect x={5.5} y={5.5} width={8} height={8} rx={1.5} />
      <path d="M3.5 10.5h-1v-8h8v1" />
    </svg>
  );
}

function CheckMark() {
  return (
    <svg width={12} height={12} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" className="text-success">
      <path d="M3.5 8.5 7 12l5.5-7" />
    </svg>
  );
}

/** 签名徽章:good=绿「已验证」/ unverified=黄「已签名·未验证」/ bad=红「签名无效」。 */
function SignatureBadge({ status, signer }: { status: string; signer: string }) {
  const t = useT();
  const style =
    status === "good"
      ? { cls: "border-success/40 bg-success/10 text-success", label: t("commit.sigGood") }
      : status === "bad"
        ? { cls: "border-danger/40 bg-danger/10 text-danger", label: t("commit.sigBad") }
        : { cls: "border-warning/40 bg-warning/10 text-warning", label: t("commit.sigUnverified") };
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
