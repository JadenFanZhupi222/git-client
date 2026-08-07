import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  getGithubIssueContext,
  listGithubIssues,
  type IpcError,
} from "../ipc";
import type { IssueContextDto, IssueSummaryDto } from "../bindings";
import type { RemoteLike } from "../lib/hosting";
import { useLang, useT } from "../lib/i18n";
import { findGithubRemote } from "./GithubPrPanel";
import { IssueIcon, SearchIcon } from "./icons";
import { IssueTriageWorkspace } from "./IssueTriageWorkspace";

type CredentialKind = "deepseek" | "github";

export function IssuesView({
  remotes,
  preferredRemote,
  onConfigureCredential,
}: {
  remotes: RemoteLike[];
  preferredRemote: string | null;
  onConfigureCredential: (kind: CredentialKind) => void;
}) {
  const t = useT();
  const lang = useLang();
  const remote = useMemo(
    () => findGithubRemote(remotes, preferredRemote),
    [remotes, preferredRemote],
  );
  const [issues, setIssues] = useState<IssueSummaryDto[]>([]);
  const [selectedNumber, setSelectedNumber] = useState<number | null>(null);
  const [context, setContext] = useState<IssueContextDto | null>(null);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [error, setError] = useState<IpcError | null>(null);
  const [detailError, setDetailError] = useState<IpcError | null>(null);
  const [triageOpen, setTriageOpen] = useState(false);
  const [compactDetailOpen, setCompactDetailOpen] = useState(false);
  const selectedNumberRef = useRef<number | null>(null);

  const visibleIssues = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return issues;
    return issues.filter((issue) => (
      `${issue.number} ${issue.title} ${issue.author ?? ""} ${issue.labels.map((label) => label.name).join(" ")}`
        .toLocaleLowerCase()
        .includes(normalized)
    ));
  }, [issues, query]);

  useEffect(() => {
    let alive = true;
    void loadIssues(() => alive);
    return () => { alive = false; };
  }, [remote]);

  useEffect(() => {
    if (!visibleIssues.some((issue) => issue.number === selectedNumber)) {
      setSelectedNumber(visibleIssues[0]?.number ?? null);
    }
  }, [selectedNumber, visibleIssues]);

  useEffect(() => {
    selectedNumberRef.current = selectedNumber;
  }, [selectedNumber]);

  useEffect(() => {
    let alive = true;
    if (!remote || selectedNumber === null) {
      setContext(null);
      return;
    }
    setContext(null);
    setDetailError(null);
    setDetailLoading(true);
    void getGithubIssueContext({
      owner: remote.owner,
      repo: remote.repo,
      issue_number: selectedNumber,
    })
      .then((next) => { if (alive) setContext(next); })
      .catch((reason) => { if (alive) setDetailError(asIpcError(reason)); })
      .finally(() => { if (alive) setDetailLoading(false); });
    return () => { alive = false; };
  }, [remote, selectedNumber]);

  async function loadIssues(isAlive: () => boolean = () => true) {
    setLoading(true);
    setError(null);
    try {
      if (!remote) {
        setIssues([]);
        setSelectedNumber(null);
        setError({ code: "REMOTE_MISSING", message: t("issueWorkspace.remoteMissing"), recoverable: true });
        return;
      }
      const next = await listGithubIssues({ owner: remote.owner, repo: remote.repo });
      if (!isAlive()) return;
      setIssues(next);
      setSelectedNumber((current) => (
        next.some((issue) => issue.number === current) ? current : (next[0]?.number ?? null)
      ));
    } catch (reason) {
      if (isAlive()) setError(asIpcError(reason));
    } finally {
      if (isAlive()) setLoading(false);
    }
  }

  async function openTriage() {
    if (!remote || !context || detailLoading) return;
    const issueNumber = context.issue.number;
    setDetailLoading(true);
    setDetailError(null);
    try {
      // Re-read the issue before accepting a cached triage result. The detail
      // view may have been open for a while, so its snapshot is not sufficient
      // to prove that a local result is still current.
      const next = await getGithubIssueContext({
        owner: remote.owner,
        repo: remote.repo,
        issue_number: issueNumber,
      });
      if (selectedNumberRef.current !== issueNumber) return;
      setContext(next);
      setTriageOpen(true);
    } catch (reason) {
      if (selectedNumberRef.current === issueNumber) setDetailError(asIpcError(reason));
    } finally {
      if (selectedNumberRef.current === issueNumber) setDetailLoading(false);
    }
  }

  const configureAction = error?.code === "GITHUB_TOKEN_MISSING"
    ? () => onConfigureCredential("github")
    : undefined;

  return (
    <section className="flex h-full min-h-0 flex-col bg-canvas" aria-label={t("issueWorkspace.aria")}>
      <header className="flex shrink-0 items-center gap-3 border-b border-line px-4 py-3">
        <div className="min-w-0">
          <h1 className="text-sm font-semibold text-fg">{t("issueWorkspace.title")}</h1>
          <p className="truncate text-[11px] text-fg-subtle">
            {remote ? `${remote.owner}/${remote.repo}` : t("issueWorkspace.noRemote")}
          </p>
        </div>
        <button
          onClick={() => void loadIssues()}
          disabled={loading}
          className="ml-auto rounded-md border border-line-strong px-2.5 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg disabled:opacity-50"
        >
          {loading ? t("issueWorkspace.refreshing") : t("issueWorkspace.refresh")}
        </button>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className={`w-[340px] shrink-0 flex-col border-r border-line bg-elevated/35 ${compactDetailOpen ? "flex max-md:hidden" : "flex max-md:w-full"}`}>
          <div className="border-b border-line p-3">
            <label className="flex items-center gap-2 rounded-md border border-line bg-canvas px-2.5 py-1.5 focus-within:border-accent">
              <SearchIcon width={13} height={13} className="shrink-0 text-fg-subtle" />
              <span className="sr-only">{t("issueWorkspace.search")}</span>
              <input
                value={query}
                onChange={(event) => setQuery(event.currentTarget.value)}
                placeholder={t("issueWorkspace.search")}
                className="min-w-0 flex-1 bg-transparent text-xs text-fg outline-none placeholder:text-fg-subtle"
              />
              <span className="font-mono text-[10px] text-fg-subtle">{visibleIssues.length}</span>
            </label>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto p-2">
            {loading ? (
              <div className="grid gap-2 p-1" aria-label={t("issueWorkspace.loading")}>
                {[0, 1, 2].map((item) => <div key={item} className="skeleton h-20 rounded-md" />)}
              </div>
            ) : error ? (
              <EmptyState
                title={t("issueWorkspace.loadError")}
                detail={displayError(error, t)}
                action={configureAction ? t("issueWorkspace.configureGithub") : t("issueWorkspace.retry")}
                onAction={configureAction ?? (() => void loadIssues())}
              />
            ) : visibleIssues.length === 0 ? (
              <EmptyState title={t("issueWorkspace.noOpen")} detail={t("issueWorkspace.noOpenDetail")} />
            ) : (
              <ul className="grid gap-1">
                {visibleIssues.map((issue) => {
                  const selected = issue.number === selectedNumber;
                  return (
                    <li key={issue.number}>
                      <button
                        onClick={() => { selectedNumberRef.current = issue.number; setSelectedNumber(issue.number); setCompactDetailOpen(true); }}
                        className={`w-full rounded-md px-3 py-2.5 text-left transition-colors ${selected ? "bg-accent/12 text-fg" : "text-fg-muted hover:bg-overlay hover:text-fg"}`}
                      >
                        <span className="flex items-start gap-2">
                          <IssueIcon width={14} height={14} className={selected ? "mt-0.5 shrink-0 text-accent" : "mt-0.5 shrink-0 text-fg-subtle"} />
                          <span className="min-w-0 flex-1">
                            <span className="block text-xs font-medium leading-5">{issue.title}</span>
                            <span className="mt-1 flex flex-wrap items-center gap-1.5 text-[10px] text-fg-subtle">
                              <span className="font-mono">#{issue.number}</span>
                              <span>{issue.author ?? t("issueWorkspace.unknownAuthor")}</span>
                              <span>{t("issueWorkspace.commentCount", { count: issue.comments })}</span>
                            </span>
                            {issue.labels.length > 0 && <LabelList labels={issue.labels} compact />}
                          </span>
                        </span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        </aside>

        <main className={`min-h-0 min-w-0 flex-1 overflow-y-auto ${compactDetailOpen ? "max-md:block" : "max-md:hidden"}`}>
          {detailLoading ? (
            <div className="grid gap-3 p-5" aria-label={t("issueWorkspace.loadingDetail")}>
              <div className="skeleton h-7 w-2/3 rounded" />
              <div className="skeleton h-28 rounded-md" />
              <div className="skeleton h-40 rounded-md" />
            </div>
          ) : detailError ? (
            <div className="mx-auto max-w-3xl p-6">
              <EmptyState
                title={t("issueWorkspace.detailError")}
                detail={displayError(detailError, t)}
                action={detailError.code === "GITHUB_TOKEN_MISSING" ? t("issueWorkspace.configureGithub") : undefined}
                onAction={detailError.code === "GITHUB_TOKEN_MISSING" ? () => onConfigureCredential("github") : undefined}
              />
            </div>
          ) : context ? (
            <article className="mx-auto max-w-4xl px-5 py-4">
              <button onClick={() => setCompactDetailOpen(false)} className="mb-3 hidden rounded-md border border-line-strong px-2.5 py-1.5 text-xs text-fg-muted hover:bg-overlay max-md:inline-flex">
                ← {t("issueWorkspace.title")}
              </button>
              <div className="flex items-start gap-4">
                <div className="min-w-0 flex-1">
                  <h2 className="text-base font-semibold text-fg">{context.issue.title}</h2>
                  <p className="mt-1 text-xs text-fg-subtle">
                    #{context.issue.number} · {context.issue.author ?? t("issueWorkspace.unknownAuthor")} · {formatDate(context.issue.updated_at, lang)}
                  </p>
                  <LabelList labels={context.issue.labels} />
                </div>
                <div className="flex shrink-0 gap-2">
                  <button onClick={() => void openUrl(context.issue.url)} className="rounded-md border border-line-strong px-2.5 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg">
                    {t("issueWorkspace.openGithub")}
                  </button>
                  <button onClick={() => void openTriage()} className="rounded-md bg-accent px-3 py-1.5 text-xs font-semibold text-on-accent">
                    {t("issueWorkspace.aiTriage")}
                  </button>
                </div>
              </div>

              <section className="mt-5 rounded-md border border-line bg-elevated/35 p-4">
                <h3 className="text-xs font-semibold text-fg">{t("issueWorkspace.description")}</h3>
                <p className="mt-2 whitespace-pre-wrap break-words text-xs leading-5 text-fg-muted">
                  {context.body.trim() || t("issueWorkspace.noDescription")}
                </p>
              </section>

              <section className="mt-5">
                <h3 className="text-sm font-semibold text-fg">{t("issueWorkspace.comments", { count: context.comments.length })}</h3>
                {context.comments_truncated && <p className="mt-1 text-[11px] text-warning">{t("issueWorkspace.commentsTruncated")}</p>}
                {context.comments.length === 0 ? (
                  <p className="mt-3 text-xs text-fg-subtle">{t("issueWorkspace.noComments")}</p>
                ) : (
                  <div className="mt-3 grid gap-2">
                    {context.comments.map((comment, index) => (
                      <article key={`${comment.created_at}-${index}`} className="rounded-md border border-line p-3">
                        <div className="flex items-center gap-2 text-[11px] text-fg-subtle">
                          <span className="font-medium text-fg">{comment.author ?? t("issueWorkspace.unknownAuthor")}</span>
                          <span>{formatDate(comment.created_at, lang)}</span>
                        </div>
                        <p className="mt-2 whitespace-pre-wrap break-words text-xs leading-5 text-fg-muted">{comment.body}</p>
                      </article>
                    ))}
                  </div>
                )}
              </section>
            </article>
          ) : (
            <div className="grid h-full place-items-center p-8 text-center">
              <div>
                <IssueIcon width={28} height={28} className="mx-auto text-fg-subtle" />
                <h2 className="mt-3 text-sm font-medium text-fg">{t("issueWorkspace.select")}</h2>
                <p className="mt-1 text-xs text-fg-subtle">{t("issueWorkspace.selectDetail")}</p>
              </div>
            </div>
          )}
        </main>
      </div>

      {triageOpen && remote && context && createPortal(
        <IssueTriageWorkspace
          target={{ owner: remote.owner, repo: remote.repo, issue_number: context.issue.number }}
          context={context}
          onClose={() => setTriageOpen(false)}
          onConfigureCredential={(kind) => { setTriageOpen(false); onConfigureCredential(kind); }}
        />,
        document.body,
      )}
    </section>
  );
}

function LabelList({ labels, compact = false }: { labels: IssueSummaryDto["labels"]; compact?: boolean }) {
  if (labels.length === 0) return null;
  return (
    <div className={`${compact ? "mt-1.5" : "mt-3"} flex flex-wrap gap-1`}>
      {labels.map((label) => (
        <span
          key={label.name}
          className="rounded-full border px-1.5 py-0.5 text-[10px] text-fg-muted"
          style={{ borderColor: `#${label.color}` }}
        >
          {label.name}
        </span>
      ))}
    </div>
  );
}

function EmptyState({ title, detail, action, onAction }: { title: string; detail: string; action?: string; onAction?: () => void }) {
  return (
    <div className="px-5 py-10 text-center">
      <IssueIcon width={22} height={22} className="mx-auto text-fg-subtle" />
      <h3 className="mt-3 text-xs font-medium text-fg">{title}</h3>
      <p className="mt-1 text-[11px] leading-5 text-fg-subtle">{detail}</p>
      {action && onAction && <button onClick={onAction} className="mt-3 rounded-md border border-line-strong px-2.5 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg">{action}</button>}
    </div>
  );
}

function displayError(error: IpcError, t: ReturnType<typeof useT>) {
  if (error.code === "GITHUB_TOKEN_MISSING") return t("issueTriage.error.GITHUB_TOKEN_MISSING");
  if (error.code === "AUTH_FAILED") return t("issueTriage.error.AUTH_FAILED");
  if (error.code === "RATE_LIMITED") return t("issueTriage.error.RATE_LIMITED");
  if (error.code === "NETWORK_ERROR") return t("issueTriage.error.NETWORK_ERROR");
  return error.message;
}

function asIpcError(reason: unknown): IpcError {
  const candidate = reason as Partial<IpcError> | null;
  return {
    code: candidate?.code ?? "UNKNOWN",
    message: candidate?.message ?? String(reason),
    recoverable: candidate?.recoverable ?? true,
  };
}

function formatDate(value: string, lang: "zh" | "en") {
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return value;
  return new Intl.DateTimeFormat(lang === "zh" ? "zh-CN" : "en-US", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}
