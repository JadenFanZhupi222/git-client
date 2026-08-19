import { useEffect, useMemo, useState } from "react";
import { createPortal, flushSync } from "react-dom";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { CredentialKindDto } from "../bindings";
import {
  type GithubPullRequestDetails,
  type GithubPullMergeMethod,
} from "../lib/github";
import {
  useGithubPullCommentMutation,
  useGithubPullMergeMutation,
  useGithubPullRequestDetails,
  useGithubPullRequests,
} from "../lib/hostingQueries";
import {
  detectHostingRemote,
  type RemoteLike,
  type HostingRemote,
} from "../lib/hosting";
import { CloseIcon, SpinnerIcon } from "./icons";
import { useToast } from "./Toast";
import { useT } from "../lib/i18n";
import { PrReviewWorkspace } from "./PrReviewWorkspace";
import { errorMessage, formatDate } from "../lib/uiShared";
import { DetailMetric } from "./DetailMetric";

export function GithubPrPanel({
  remotes,
  branch,
  preferredRemote,
  onClose,
  onConfigureToken,
  onConfigureCredential,
}: {
  remotes: RemoteLike[];
  branch: string | null;
  preferredRemote: string | null;
  onClose: () => void;
  onConfigureToken: () => void;
  onConfigureCredential?: (kind: CredentialKindDto) => void;
}) {
  const toast = useToast();
  const t = useT();
  const [detailNumber, setDetailNumber] = useState<number | null>(null);
  const [reviewTarget, setReviewTarget] = useState<{ owner: string; repo: string; pull_number: number } | null>(null);
  const [reviewOwnsFocus, setReviewOwnsFocus] = useState(false);

  const remote = useMemo(
    () => findGithubRemote(remotes, preferredRemote),
    [remotes, preferredRemote],
  );
  const pullsQuery = useGithubPullRequests(remote, branch);
  const detailQuery = useGithubPullRequestDetails(remote, detailNumber);
  const commentMutation = useGithubPullCommentMutation(remote, branch);
  const mergeMutation = useGithubPullMergeMutation(remote, branch);
  const pulls = pullsQuery.data ?? [];
  const loading = !!remote && !!branch && pullsQuery.isPending;
  const error = !branch
    ? t("githubPr.errNoBranch")
    : !remote
      ? t("githubPr.errNoRemote")
      : pullsQuery.error
        ? errorMessage(pullsQuery.error)
        : null;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !reviewTarget) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, reviewTarget]);

  useEffect(() => setDetailNumber(null), [remote, branch]);

  useEffect(() => {
    if (detailQuery.error) {
      toast({ kind: "error", title: errorMessage(detailQuery.error) });
    }
  }, [detailQuery.error, toast]);

  async function openPull(url: string) {
    try {
      await openUrl(url);
    } catch (e) {
      toast({ kind: "error", title: (e as Error).message ?? String(e) });
    }
  }

  function loadDetail(number: number) {
    if (!remote) return;
    if (detailNumber === number && detailQuery.isError) {
      void detailQuery.refetch();
      return;
    }
    setDetailNumber(number);
  }

  async function createPullComment(
    detail: GithubPullRequestDetails,
    body: string,
  ): Promise<boolean> {
    if (!remote || commentMutation.isPending) return false;
    try {
      await commentMutation.mutateAsync({ detail, body });
      toast({ kind: "success", title: `Commented on PR #${detail.number}` });
      return true;
    } catch (e) {
      const message = errorMessage(e);
      toast({ kind: "error", title: message });
      if (message === "GitHub token is required") onConfigureToken();
      return false;
    }
  }

  async function mergePull(
    detail: GithubPullRequestDetails,
    method: GithubPullMergeMethod,
  ) {
    if (!remote || mergeMutation.isPending) return;
    try {
      const { result } = await mergeMutation.mutateAsync({ detail, method });
      setDetailNumber(null);
      toast({
        kind: "success",
        title: result.message || `Merged PR #${detail.number}`,
      });
    } catch (e) {
      const message = errorMessage(e);
      toast({ kind: "error", title: message });
      if (message === "GitHub token is required") onConfigureToken();
    }
  }

  return (
    <div
      className="overlay-in fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("githubPr.dialog")}
        aria-hidden={reviewOwnsFocus ? true : undefined}
        inert={reviewOwnsFocus ? true : undefined}
        className="panel-in popover flex max-h-[78vh] w-[560px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-line px-4 py-3">
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-fg">{t("githubPr.title")}</h2>
            <p className="truncate text-[11px] text-fg-subtle">
              {remote ? `${remote.owner}/${remote.repo}` : t("githubPr.unknownRemote")}{" "}
              · {branch ?? t("githubPr.noBranch")}
            </p>
          </div>
          <button
            onClick={onClose}
            aria-label="关闭"
            className="ml-auto grid h-6 w-6 place-items-center rounded text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
          >
            <CloseIcon width={13} height={13} />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          {loading ? (
            <div className="flex items-center gap-2 py-8 text-xs text-fg-subtle">
              <SpinnerIcon width={13} height={13} /> {t("githubPr.loading")}
            </div>
          ) : error ? (
            <div className="rounded border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
              {error}
            </div>
          ) : pulls.length === 0 ? (
            <p className="py-8 text-center text-xs text-fg-subtle">
              {t("githubPr.empty")}
            </p>
          ) : (
            <ul className="flex flex-col gap-2">
              {pulls.map((pr) => (
                <li
                  key={pr.number}
                  className="rounded-md border border-line bg-elevated/50 px-3 py-2"
                >
                  <div className="flex min-w-0 items-start gap-3">
                    <div className="min-w-0 flex-1">
                      <button
                        onClick={() => openPull(pr.url)}
                        className="block max-w-full truncate text-left text-sm font-medium text-fg transition-colors hover:text-accent"
                      >
                        #{pr.number} {pr.title}
                      </button>
                      <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-fg-subtle">
                        {pr.draft && (
                          <span className="rounded bg-overlay px-1.5 py-0.5">
                            Draft
                          </span>
                        )}
                        <span>{pr.author ?? "unknown"}</span>
                        <span className="font-mono">
                          {pr.headRef} → {pr.baseRef}
                        </span>
                      </div>
                    </div>
                    <button
                      onClick={() => openPull(pr.url)}
                      className="shrink-0 rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
                    >
                      {t("githubPr.open")}
                    </button>
                    <button
                      onClick={() => loadDetail(pr.number)}
                      disabled={detailNumber === pr.number && detailQuery.isFetching}
                      className="shrink-0 rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
                    >
                      {detailNumber === pr.number && detailQuery.isFetching ? t("githubPr.detailsLoading") : t("githubPr.details")}
                    </button>
                  </div>
                  {detailNumber === pr.number && detailQuery.data && (
                    <PullRequestDetailsView
                      detail={detailQuery.data}
                      creatingComment={commentMutation.isPending}
                      onCreateComment={createPullComment}
                      mergingPull={mergeMutation.isPending}
                      onMerge={mergePull}
                      onAiReview={() => {
                        if (!remote) return;
                        setReviewOwnsFocus(false);
                        setReviewTarget({ owner: remote.owner, repo: remote.repo, pull_number: pr.number });
                      }}
                    />
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="flex justify-between gap-2 border-t border-line px-4 py-3">
          <div className="flex gap-2">
            <button
              onClick={onConfigureToken}
              className="rounded-md px-3 py-1.5 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
            >
              {t("githubPr.setToken")}
            </button>
            <button
              onClick={() => void pullsQuery.refetch()}
              disabled={loading}
              className="rounded-md px-3 py-1.5 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
            >
              {t("githubPr.refresh")}
            </button>
          </div>
          <button
            onClick={onClose}
            className="rounded-md px-3 py-1.5 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
          >
            {t("githubPr.close")}
          </button>
        </div>
      </div>
      {reviewTarget && createPortal(
        <PrReviewWorkspace
          target={reviewTarget}
          onClose={() => {
            flushSync(() => setReviewOwnsFocus(false));
            setReviewTarget(null);
          }}
          onConfigureCredential={(kind) => {
            flushSync(() => setReviewOwnsFocus(false));
            setReviewTarget(null);
            (onConfigureCredential ?? ((next) => { if (next === "github") onConfigureToken(); }))(kind);
          }}
          onFocusReady={() => setReviewOwnsFocus(true)}
        />,
        document.body,
      )}
    </div>
  );
}

export function PullRequestDetailsView({
  detail,
  creatingComment,
  onCreateComment,
  mergingPull,
  onMerge,
  onAiReview,
}: {
  detail: GithubPullRequestDetails;
  creatingComment: boolean;
  onCreateComment: (
    detail: GithubPullRequestDetails,
    body: string,
  ) => Promise<boolean>;
  mergingPull: boolean;
  onMerge: (
    detail: GithubPullRequestDetails,
    method: GithubPullMergeMethod,
  ) => void;
  onAiReview: () => void;
}) {
  const t = useT();
  const [commentBody, setCommentBody] = useState("");
  const [mergeMethod, setMergeMethod] =
    useState<GithubPullMergeMethod>("merge");
  const trimmedCommentBody = commentBody.trim();
  const reviewCounts = detail.reviews.reduce<Record<string, number>>(
    (counts, review) => ({
      ...counts,
      [review.state]: (counts[review.state] ?? 0) + 1,
    }),
    {},
  );

  async function submitComment() {
    if (!trimmedCommentBody || creatingComment) return;
    const created = await onCreateComment(detail, trimmedCommentBody);
    if (created) setCommentBody("");
  }

  const mergeBlockedReason = githubMergeBlockedReason(detail);

  return (
    <div className="mt-3 grid gap-2 rounded-md border border-line bg-canvas/60 p-3 text-xs">
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <DetailMetric label={t("githubPrDetail.metricMerge")} value={mergeableLabel(detail)} />
        <DetailMetric
          label={t("githubPrDetail.metricStatus")}
          value={detail.combinedStatus?.state ?? "unknown"}
        />
        <DetailMetric label={t("githubPrDetail.metricReviews")} value={reviewSummary(reviewCounts, t)} />
        <DetailMetric label={t("githubPrDetail.metricChanges")} value={t("githubPrDetail.files", { count: detail.changedFiles })} />
      </div>
      <div className="flex flex-wrap gap-2 text-[11px] text-fg-subtle">
        <span>{t("githubPrDetail.commits", { count: detail.commits })}</span>
        <span>+{detail.additions}</span>
        <span>-{detail.deletions}</span>
        <span>{t("githubPrDetail.comments", { count: detail.comments })}</span>
        <span>{t("githubPrDetail.reviewComments", { count: detail.reviewComments })}</span>
      </div>
      {detail.combinedStatus && detail.combinedStatus.statuses.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {detail.combinedStatus.statuses.slice(0, 6).map((status) => (
            <span
              key={`${status.context}-${status.state}`}
              className="rounded border border-line bg-elevated px-1.5 py-0.5 font-mono text-[10px] text-fg-muted"
            >
              {status.context}: {status.state}
            </span>
          ))}
        </div>
      )}
      {detail.checkRuns.length > 0 && (
        <div className="grid gap-1.5 border-t border-line pt-2">
          <div className="text-[10px] uppercase tracking-wide text-fg-subtle">
            {t("githubPrDetail.checkRuns")}
          </div>
          <div className="grid gap-1.5">
            {detail.checkRuns.slice(0, 6).map((run) => (
              <a
                key={run.id}
                href={run.url}
                onClick={(event) => {
                  event.preventDefault();
                  if (run.url) void openUrl(run.url);
                }}
                className="rounded border border-line bg-elevated/60 px-2 py-1.5 text-left transition-colors hover:border-line-strong hover:bg-overlay"
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate font-medium text-fg">
                    {run.name || t("githubPrDetail.unnamedCheck")}
                  </span>
                  <span className="shrink-0 font-mono text-[10px] text-fg-muted">
                    {run.conclusion ?? run.status}
                  </span>
                </div>
                <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-fg-subtle">
                  {run.app && <span>{run.app}</span>}
                  <span>{run.status}</span>
                </div>
              </a>
            ))}
          </div>
        </div>
      )}
      {detail.recentComments.length > 0 && (
        <div className="grid gap-1.5 border-t border-line pt-2">
          <div className="text-[10px] uppercase tracking-wide text-fg-subtle">
            {t("githubPrDetail.recentComments")}
          </div>
          <div className="grid gap-1.5">
            {detail.recentComments.slice(-3).map((comment) => (
              <a
                key={comment.id}
                href={comment.url}
                onClick={(event) => {
                  event.preventDefault();
                  void openUrl(comment.url);
                }}
                className="rounded border border-line bg-elevated/60 px-2 py-1.5 text-left transition-colors hover:border-line-strong hover:bg-overlay"
              >
                <div className="flex items-center justify-between gap-2 text-[11px] text-fg-subtle">
                  <span className="truncate">{comment.author ?? "unknown"}</span>
                  <span className="shrink-0 font-mono">
                    {formatCommentTime(comment.createdAt)}
                  </span>
                </div>
                <p className="mt-1 line-clamp-3 whitespace-pre-wrap break-words text-xs text-fg">
                  {comment.body || t("githubPrDetail.emptyComment")}
                </p>
              </a>
            ))}
          </div>
        </div>
      )}
      {detail.reviewThreads.length > 0 && (
        <div className="grid gap-1.5 border-t border-line pt-2">
          <div className="text-[10px] uppercase tracking-wide text-fg-subtle">
            {t("githubPrDetail.reviewThreads")}
          </div>
          <div className="grid gap-1.5">
            {detail.reviewThreads.slice(-3).map((thread) => (
              <a
                key={thread.id}
                href={thread.url}
                onClick={(event) => {
                  event.preventDefault();
                  void openUrl(thread.url);
                }}
                className="rounded border border-line bg-elevated/60 px-2 py-1.5 text-left transition-colors hover:border-line-strong hover:bg-overlay"
              >
                <div className="flex items-center justify-between gap-2 text-[11px] text-fg-subtle">
                  <span className="truncate font-mono">
                    {reviewThreadLocation(thread)}
                  </span>
                  <span className="shrink-0">{thread.author ?? "unknown"}</span>
                </div>
                <p className="mt-1 line-clamp-3 whitespace-pre-wrap break-words text-xs text-fg">
                  {thread.body || t("githubPrDetail.emptyReviewComment")}
                </p>
              </a>
            ))}
          </div>
        </div>
      )}
      <div className="grid gap-1.5 border-t border-line pt-2">
        <div className="flex justify-end">
          <button onClick={onAiReview} className="rounded-md bg-accent px-3 py-1.5 text-xs font-semibold text-on-accent">
            {t("githubPrDetail.aiReview")}
          </button>
        </div>
      </div>
      <div className="grid gap-1.5 border-t border-line pt-2">
        <div className="flex flex-wrap items-center gap-2">
          <label
            className="sr-only"
            htmlFor={`github-pr-merge-method-${detail.number}`}
          >
            {t("githubPrDetail.mergeMethod")}
          </label>
          <select
            id={`github-pr-merge-method-${detail.number}`}
            value={mergeMethod}
            onChange={(event) =>
              setMergeMethod(event.currentTarget.value as GithubPullMergeMethod)
            }
            className="h-7 rounded-md border border-line bg-elevated px-2 text-xs text-fg outline-none transition-colors focus:border-accent"
          >
            <option value="merge">{t("githubPrDetail.methodMergeCommit")}</option>
            <option value="squash">{t("githubPrDetail.methodSquash")}</option>
            <option value="rebase">{t("githubPrDetail.methodRebase")}</option>
          </select>
          <button
            onClick={() => onMerge(detail, mergeMethod)}
            disabled={Boolean(mergeBlockedReason) || mergingPull}
            className="rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
          >
            {mergingPull ? t("githubPrDetail.merging") : t("githubPrDetail.merge")}
          </button>
          {mergeBlockedReason && (
            <span className="text-[11px] text-fg-subtle">
              {mergeBlockedReason}
            </span>
          )}
        </div>
      </div>
      <div className="grid gap-1.5 border-t border-line pt-2">
        <label className="sr-only" htmlFor={`github-pr-comment-${detail.number}`}>
          {t("githubPrDetail.newComment")}
        </label>
        <textarea
          id={`github-pr-comment-${detail.number}`}
          value={commentBody}
          onChange={(event) => setCommentBody(event.currentTarget.value)}
          rows={2}
          className="min-h-14 resize-y rounded-md border border-line bg-elevated px-2 py-1.5 text-xs text-fg outline-none transition-colors placeholder:text-fg-subtle focus:border-accent"
          placeholder={t("githubPrDetail.commentPlaceholder")}
        />
        <div className="flex justify-end">
          <button
            onClick={submitComment}
            disabled={!trimmedCommentBody || creatingComment}
            className="rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
          >
            {creatingComment ? t("githubPrDetail.commenting") : t("githubPrDetail.comment")}
          </button>
        </div>
      </div>
    </div>
  );
}

function mergeableLabel(detail: GithubPullRequestDetails): string {
  if (detail.mergeable === true) return "mergeable";
  if (detail.mergeable === false) return detail.mergeableState ?? "blocked";
  return detail.mergeableState ?? "unknown";
}

function reviewSummary(
  counts: Record<string, number>,
  t: ReturnType<typeof useT>,
): string {
  const approved = counts.APPROVED ?? 0;
  const changes = counts.CHANGES_REQUESTED ?? 0;
  if (approved || changes) {
    return t("githubPrDetail.reviewSummaryChanges", { approved, changes });
  }
  const total = Object.values(counts).reduce((sum, count) => sum + count, 0);
  return total ? t("githubPrDetail.reviewSummaryTotal", { count: total }) : t("githubPrDetail.reviewSummaryNone");
}

function githubMergeBlockedReason(detail: GithubPullRequestDetails): string | null {
  if (!detail.headSha) return "missing head SHA";
  if (detail.mergeable !== true) return mergeableLabel(detail);
  if (detail.combinedStatus && detail.combinedStatus.state !== "success") {
    return `status ${detail.combinedStatus.state}`;
  }
  return null;
}

function formatCommentTime(value: string): string {
  return formatDate(value, "short");
}

function reviewThreadLocation(
  thread: GithubPullRequestDetails["reviewThreads"][number],
): string {
  const line = thread.line ?? thread.originalLine;
  return line ? `${thread.path}:${line}` : thread.path;
}

export function findGithubRemote(
  remotes: RemoteLike[],
  preferredRemote: string | null,
): HostingRemote | null {
  const ordered = [
    ...remotes.filter((remote) => remote.name === preferredRemote),
    ...remotes.filter((remote) => remote.name !== preferredRemote),
  ];
  for (const remote of ordered) {
    const hosting = detectHostingRemote(remote.url);
    if (hosting?.provider === "github") return hosting;
  }
  return null;
}
