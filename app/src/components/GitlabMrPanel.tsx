import { useEffect, useMemo, useState } from "react";
import { createPortal, flushSync } from "react-dom";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { CredentialKindDto } from "../bindings";
import {
  type GitlabMergeRequestDetails,
  type GitlabPipelineJobSummary,
} from "../lib/gitlab";
import {
  useGitlabApprovalMutation,
  useGitlabMergeMutation,
  useGitlabMergeRequestDetails,
  useGitlabMergeRequests,
  useGitlabNoteMutation,
  useGitlabRetryJobMutation,
} from "../lib/hostingQueries";
import {
  detectHostingRemote,
  type HostingRemote,
  type RemoteLike,
} from "../lib/hosting";
import { CloseIcon, SpinnerIcon } from "./icons";
import { useToast } from "./Toast";
import { useT } from "../lib/i18n";
import { PrReviewWorkspace } from "./PrReviewWorkspace";
import { errorMessage, formatDate } from "../lib/uiShared";
import { DetailMetric } from "./DetailMetric";

export function GitlabMrPanel({
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
  const [detailIid, setDetailIid] = useState<number | null>(null);
  const [reviewTarget, setReviewTarget] = useState<{ owner: string; repo: string; pull_number: number } | null>(null);
  const [reviewOwnsFocus, setReviewOwnsFocus] = useState(false);

  const remote = useMemo(
    () => findGitlabRemote(remotes, preferredRemote),
    [remotes, preferredRemote],
  );
  const mrsQuery = useGitlabMergeRequests(remote, branch);
  const detailQuery = useGitlabMergeRequestDetails(remote, detailIid);
  const approveMutation = useGitlabApprovalMutation(remote, branch, "approve");
  const unapproveMutation = useGitlabApprovalMutation(remote, branch, "unapprove");
  const noteMutation = useGitlabNoteMutation(remote, branch);
  const mergeMutation = useGitlabMergeMutation(remote, branch);
  const retryMutation = useGitlabRetryJobMutation(remote, branch);
  const mrs = mrsQuery.data ?? [];
  const loading = !!remote && !!branch && mrsQuery.isPending;
  const error = !branch
    ? t("gitlabMr.errNoBranch")
    : !remote
      ? t("gitlabMr.errNoRemote")
      : mrsQuery.error
        ? errorMessage(mrsQuery.error)
        : null;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !reviewTarget) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, reviewTarget]);

  useEffect(() => setDetailIid(null), [remote, branch]);

  useEffect(() => {
    if (detailQuery.error) {
      toast({ kind: "error", title: errorMessage(detailQuery.error) });
    }
  }, [detailQuery.error, toast]);

  async function openMr(url: string) {
    try {
      await openUrl(url);
    } catch (e) {
      toast({ kind: "error", title: (e as Error).message ?? String(e) });
    }
  }

  function loadDetail(iid: number) {
    if (!remote) return;
    if (detailIid === iid && detailQuery.isError) {
      void detailQuery.refetch();
      return;
    }
    setDetailIid(iid);
  }

  async function approveMr(detail: GitlabMergeRequestDetails) {
    if (!remote || approveMutation.isPending || unapproveMutation.isPending) return;
    try {
      await approveMutation.mutateAsync({ iid: detail.iid });
      toast({ kind: "success", title: t("gitlabMrDetail.approvedToast", { iid: detail.iid }) });
    } catch (e) {
      handleMutationError(e);
    }
  }

  async function unapproveMr(detail: GitlabMergeRequestDetails) {
    if (!remote || approveMutation.isPending || unapproveMutation.isPending) return;
    try {
      await unapproveMutation.mutateAsync({ iid: detail.iid });
      toast({ kind: "success", title: t("gitlabMrDetail.unapprovedToast", { iid: detail.iid }) });
    } catch (e) {
      handleMutationError(e);
    }
  }

  async function createMrNote(
    detail: GitlabMergeRequestDetails,
    body: string,
  ): Promise<boolean> {
    if (!remote || noteMutation.isPending) return false;
    try {
      await noteMutation.mutateAsync({ iid: detail.iid, body });
      toast({ kind: "success", title: t("gitlabMrDetail.commentedToast", { iid: detail.iid }) });
      return true;
    } catch (e) {
      handleMutationError(e);
      return false;
    }
  }

  async function mergeMr(detail: GitlabMergeRequestDetails, squash: boolean) {
    if (!remote || mergeMutation.isPending) return;
    try {
      const merged = await mergeMutation.mutateAsync({ detail, squash });
      setDetailIid(null);
      toast({ kind: "success", title: t("gitlabMrDetail.mergedToast", { iid: merged.iid }) });
    } catch (e) {
      handleMutationError(e);
    }
  }

  async function retryJob(
    detail: GitlabMergeRequestDetails,
    job: GitlabPipelineJobSummary,
  ) {
    if (!remote || retryMutation.isPending) return;
    try {
      await retryMutation.mutateAsync({ iid: detail.iid, job });
      toast({ kind: "success", title: t("gitlabMrDetail.retriedToast", { name: job.name }) });
    } catch (e) {
      handleMutationError(e);
    }
  }

  function handleMutationError(error: unknown) {
    const message = errorMessage(error);
    toast({
      kind: "error",
      title: message === "GitLab token is required"
        ? t("gitlabMrDetail.tokenRequired")
        : message,
    });
    if (message === "GitLab token is required") onConfigureToken();
  }

  return (
    <div
      className="overlay-in fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("gitlabMr.dialog")}
        aria-hidden={reviewOwnsFocus ? true : undefined}
        inert={reviewOwnsFocus ? true : undefined}
        className="panel-in popover flex max-h-[78vh] w-[560px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-line px-4 py-3">
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-fg">{t("gitlabMr.title")}</h2>
            <p className="truncate text-[11px] text-fg-subtle">
              {remote ? `${remote.owner}/${remote.repo}` : t("gitlabMr.unknownRemote")}{" "}
              · {branch ?? t("gitlabMr.noBranch")}
            </p>
          </div>
          <button
            onClick={onClose}
            aria-label={t("gitlabMr.close")}
            className="ml-auto grid h-6 w-6 place-items-center rounded text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
          >
            <CloseIcon width={13} height={13} />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          {loading ? (
            <div className="flex items-center gap-2 py-8 text-xs text-fg-subtle">
              <SpinnerIcon width={13} height={13} /> {t("gitlabMr.loading")}
            </div>
          ) : error ? (
            <div className="rounded border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
              {error}
            </div>
          ) : mrs.length === 0 ? (
            <p className="py-8 text-center text-xs text-fg-subtle">
              {t("gitlabMr.empty")}
            </p>
          ) : (
            <ul className="flex flex-col gap-2">
              {mrs.map((mr) => (
                <li
                  key={mr.iid}
                  className="rounded-md border border-line bg-elevated/50 px-3 py-2"
                >
                  <div className="flex min-w-0 items-start gap-3">
                    <div className="min-w-0 flex-1">
                      <button
                        onClick={() => openMr(mr.url)}
                        className="block max-w-full truncate text-left text-sm font-medium text-fg transition-colors hover:text-accent"
                      >
                        !{mr.iid} {mr.title}
                      </button>
                      <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-fg-subtle">
                        {mr.draft && (
                          <span className="rounded bg-overlay px-1.5 py-0.5">
                            {t("gitlabMr.draft")}
                          </span>
                        )}
                        <span>{mr.author ?? t("gitlabMr.unknownAuthor")}</span>
                        <span className="font-mono">
                          {mr.sourceBranch} → {mr.targetBranch}
                        </span>
                        {mr.detailedMergeStatus && (
                          <span>{mr.detailedMergeStatus}</span>
                        )}
                      </div>
                    </div>
                    <button
                      onClick={() => openMr(mr.url)}
                      className="shrink-0 rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
                    >
                      {t("gitlabMr.open")}
                    </button>
                    <button
                      onClick={() => loadDetail(mr.iid)}
                      disabled={detailIid === mr.iid && detailQuery.isFetching}
                      className="shrink-0 rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
                    >
                      {detailIid === mr.iid && detailQuery.isFetching ? t("gitlabMr.detailsLoading") : t("gitlabMr.details")}
                    </button>
                  </div>
                  {detailIid === mr.iid && detailQuery.data && (
                    <MergeRequestDetailsView
                      detail={detailQuery.data}
                      approvalAction={
                        approveMutation.isPending
                          ? "approve"
                          : unapproveMutation.isPending
                            ? "unapprove"
                            : null
                      }
                      onApprove={approveMr}
                      onUnapprove={unapproveMr}
                      creatingNote={noteMutation.isPending}
                      onCreateNote={createMrNote}
                      mergingMr={mergeMutation.isPending}
                      onMerge={mergeMr}
                      retryingJobId={retryMutation.isPending ? retryMutation.variables?.job.id ?? null : null}
                      onRetryJob={retryJob}
                      onAiReview={() => {
                        if (!remote) return;
                        setReviewOwnsFocus(false);
                        setReviewTarget({ owner: remote.owner, repo: remote.repo, pull_number: mr.iid });
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
              {t("gitlabMr.setToken")}
            </button>
            <button
              onClick={() => void mrsQuery.refetch()}
              disabled={loading}
              className="rounded-md px-3 py-1.5 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
            >
              {t("gitlabMr.refresh")}
            </button>
          </div>
          <button
            onClick={onClose}
            className="rounded-md px-3 py-1.5 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
          >
            {t("gitlabMr.close")}
          </button>
        </div>
      </div>
      {reviewTarget && createPortal(
        <PrReviewWorkspace
          platform="gitlab"
          target={reviewTarget}
          onClose={() => {
            flushSync(() => setReviewOwnsFocus(false));
            setReviewTarget(null);
          }}
          onConfigureCredential={(kind) => {
            flushSync(() => setReviewOwnsFocus(false));
            setReviewTarget(null);
            (onConfigureCredential ?? ((next) => { if (next === "gitlab") onConfigureToken(); }))(kind);
          }}
          onFocusReady={() => setReviewOwnsFocus(true)}
        />,
        document.body,
      )}
    </div>
  );
}

function MergeRequestDetailsView({
  detail,
  approvalAction,
  onApprove,
  onUnapprove,
  creatingNote,
  onCreateNote,
  mergingMr,
  onMerge,
  retryingJobId,
  onRetryJob,
  onAiReview,
}: {
  detail: GitlabMergeRequestDetails;
  approvalAction: "approve" | "unapprove" | null;
  onApprove: (detail: GitlabMergeRequestDetails) => void;
  onUnapprove: (detail: GitlabMergeRequestDetails) => void;
  creatingNote: boolean;
  onCreateNote: (
    detail: GitlabMergeRequestDetails,
    body: string,
  ) => Promise<boolean>;
  mergingMr: boolean;
  onMerge: (detail: GitlabMergeRequestDetails, squash: boolean) => void;
  retryingJobId: number | null;
  onRetryJob: (
    detail: GitlabMergeRequestDetails,
    job: GitlabPipelineJobSummary,
  ) => void;
  onAiReview: () => void;
}) {
  const t = useT();
  const [noteBody, setNoteBody] = useState("");
  const [squash, setSquash] = useState(false);
  const trimmedNoteBody = noteBody.trim();

  async function submitNote() {
    if (!trimmedNoteBody || creatingNote) return;
    const created = await onCreateNote(detail, trimmedNoteBody);
    if (created) setNoteBody("");
  }

  const mergeBlockedReason = gitlabMergeBlockedReason(detail, t);

  return (
    <div className="mt-3 grid gap-2 rounded-md border border-line bg-canvas/60 p-3 text-xs">
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-5">
        <DetailMetric label={t("gitlabMrDetail.metricMerge")} value={mergeLabel(detail, t)} />
        <DetailMetric
          label={t("gitlabMrDetail.metricPipeline")}
          value={detail.latestPipeline?.status ?? t("gitlabMrDetail.unknown")}
        />
        <DetailMetric label={t("gitlabMrDetail.metricChanges")} value={t("gitlabMrDetail.changesCount", { count: detail.changesCount || "0" })} />
        <DetailMetric label={t("gitlabMrDetail.metricNotes")} value={t("gitlabMrDetail.notesCount", { count: detail.userNotesCount })} />
        <DetailMetric label={t("gitlabMrDetail.metricApprovals")} value={approvalLabel(detail, t)} />
      </div>
      <div className="flex flex-wrap gap-2 text-[11px] text-fg-subtle">
        <span>{detail.hasConflicts ? t("gitlabMrDetail.conflicts") : t("gitlabMrDetail.noConflicts")}</span>
        <span>
          {detail.blockingDiscussionsResolved === false
            ? t("gitlabMrDetail.discussionsBlocked")
            : t("gitlabMrDetail.discussionsResolved")}
        </span>
        <span>+{detail.upvotes}</span>
        <span>-{detail.downvotes}</span>
        {detail.approvals && detail.approvals.approvedBy.length > 0 && (
          <span>{detail.approvals.approvedBy.join(", ")}</span>
        )}
        {detail.approvals?.userCanApprove && !detail.approvals.userHasApproved && (
          <span>{t("gitlabMrDetail.canApprove")}</span>
        )}
      </div>
      {detail.approvals && (
        <div className="flex justify-end">
          {detail.approvals.userHasApproved ? (
            <button
              onClick={() => onUnapprove(detail)}
              disabled={approvalAction !== null}
              className="rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
            >
              {approvalAction === "unapprove" ? t("gitlabMrDetail.unapproving") : t("gitlabMrDetail.unapprove")}
            </button>
          ) : detail.approvals.userCanApprove ? (
            <button
              onClick={() => onApprove(detail)}
              disabled={approvalAction !== null}
              className="rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
            >
              {approvalAction === "approve" ? t("gitlabMrDetail.approving") : t("gitlabMrDetail.approve")}
            </button>
          ) : null}
        </div>
      )}
      <div className="flex justify-end border-t border-line pt-2">
        <button
          onClick={onAiReview}
          className="rounded-md bg-accent px-3 py-1.5 text-xs font-semibold text-on-accent"
        >
          {t("gitlabMrDetail.aiReview")}
        </button>
      </div>
      {detail.pipelineJobs.length > 0 && (
        <div className="grid gap-1.5 border-t border-line pt-2">
          <div className="text-[10px] uppercase tracking-wide text-fg-subtle">
            {t("gitlabMrDetail.pipelineJobs")}
          </div>
          <div className="grid gap-1.5">
            {detail.pipelineJobs.slice(0, 6).map((job) => (
              <div
                key={job.id}
                className="rounded border border-line bg-elevated/60 px-2 py-1.5 text-left transition-colors hover:border-line-strong hover:bg-overlay"
              >
                <a
                  href={job.url ?? ""}
                  onClick={(event) => {
                    event.preventDefault();
                    if (job.url) void openUrl(job.url);
                  }}
                  className="flex items-center justify-between gap-2"
                >
                  <span className="truncate font-medium text-fg">
                    {job.name || t("gitlabMrDetail.unnamedJob")}
                  </span>
                  <span className="shrink-0 font-mono text-[10px] text-fg-muted">
                    {job.status}
                  </span>
                </a>
                <div className="mt-1 flex flex-wrap items-center gap-2 text-[11px] text-fg-subtle">
                  {job.stage && <span>{job.stage}</span>}
                  {job.duration !== null && (
                    <span>{formatJobDuration(job.duration)}</span>
                  )}
                  {isRetryableGitlabJob(job.status) && (
                    <button
                      onClick={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        onRetryJob(detail, job);
                      }}
                      aria-label={t("gitlabMrDetail.retryJob", { name: job.name || t("gitlabMrDetail.unnamedJob") })}
                      disabled={retryingJobId === job.id}
                      className="ml-auto rounded border border-line-strong px-1.5 py-0.5 text-[10px] text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
                    >
                      {retryingJobId === job.id ? t("gitlabMrDetail.retrying") : t("gitlabMrDetail.retry")}
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
      <div className="grid gap-1.5 border-t border-line pt-2">
        <div className="flex flex-wrap items-center gap-2">
          <label
            className="inline-flex items-center gap-1.5 text-xs text-fg-muted"
            htmlFor={`gitlab-mr-squash-${detail.iid}`}
          >
            <input
              id={`gitlab-mr-squash-${detail.iid}`}
              type="checkbox"
              checked={squash}
              onChange={(event) => setSquash(event.currentTarget.checked)}
              className="h-3.5 w-3.5 accent-accent"
            />
            {t("gitlabMrDetail.squash")}
          </label>
          <button
            onClick={() => onMerge(detail, squash)}
            disabled={Boolean(mergeBlockedReason) || mergingMr}
            className="rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
          >
            {mergingMr ? t("gitlabMrDetail.merging") : t("gitlabMrDetail.merge")}
          </button>
          {mergeBlockedReason && (
            <span className="text-[11px] text-fg-subtle">
              {mergeBlockedReason}
            </span>
          )}
        </div>
      </div>
      {detail.notes.length > 0 && (
        <div className="grid gap-1.5 border-t border-line pt-2">
          <div className="text-[10px] uppercase tracking-wide text-fg-subtle">
            {t("gitlabMrDetail.notes")}
          </div>
          {detail.notes.map((note) => (
            <div
              key={note.id}
              className="rounded border border-line bg-elevated/50 px-2 py-1.5"
            >
              <div className="flex min-w-0 items-center gap-2 text-[10px] text-fg-subtle">
                <span className="font-medium text-fg-muted">
                  {note.author ?? t("gitlabMrDetail.unknown")}
                </span>
                {note.system && <span>{t("gitlabMrDetail.system")}</span>}
                {note.internal && <span>{t("gitlabMrDetail.internal")}</span>}
                <span className="ml-auto truncate">{formatGitlabDate(note.updatedAt)}</span>
              </div>
              <p className="mt-1 line-clamp-2 whitespace-pre-wrap text-[11px] leading-4 text-fg">
                {note.body}
              </p>
            </div>
          ))}
        </div>
      )}
      {detail.discussions.length > 0 && (
        <div className="grid gap-1.5 border-t border-line pt-2">
          <div className="text-[10px] uppercase tracking-wide text-fg-subtle">
            {t("gitlabMrDetail.discussions")}
          </div>
          {detail.discussions.slice(-3).map((discussion) => (
            <div
              key={discussion.id}
              className="rounded border border-line bg-elevated/50 px-2 py-1.5"
            >
              <div className="flex min-w-0 items-center gap-2 text-[10px] text-fg-subtle">
                <span className="truncate font-mono">
                  {gitlabDiscussionLocation(discussion)}
                </span>
                <span>{discussion.resolved ? t("gitlabMrDetail.resolved") : t("gitlabMrDetail.unresolved")}</span>
                <span className="ml-auto truncate">
                  {discussion.author ?? t("gitlabMrDetail.unknown")}
                </span>
              </div>
              <p className="mt-1 line-clamp-2 whitespace-pre-wrap text-[11px] leading-4 text-fg">
                {discussion.body}
              </p>
            </div>
          ))}
        </div>
      )}
      <div className="grid gap-1.5 border-t border-line pt-2">
        <label className="sr-only" htmlFor={`gitlab-mr-note-${detail.iid}`}>
          {t("gitlabMrDetail.newNote")}
        </label>
        <textarea
          id={`gitlab-mr-note-${detail.iid}`}
          value={noteBody}
          onChange={(event) => setNoteBody(event.currentTarget.value)}
          rows={2}
          className="min-h-14 resize-y rounded-md border border-line bg-elevated px-2 py-1.5 text-xs text-fg outline-none transition-colors placeholder:text-fg-subtle focus:border-accent"
          placeholder={t("gitlabMrDetail.commentPlaceholder")}
        />
        <div className="flex justify-end">
          <button
            onClick={submitNote}
            disabled={!trimmedNoteBody || creatingNote}
            className="rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
          >
            {creatingNote ? t("gitlabMrDetail.commenting") : t("gitlabMrDetail.comment")}
          </button>
        </div>
      </div>
      {detail.latestPipeline && (
        <div className="flex flex-wrap gap-1.5">
          <span className="rounded border border-line bg-elevated px-1.5 py-0.5 font-mono text-[10px] text-fg-muted">
            {detail.latestPipeline.ref}: {detail.latestPipeline.sha}
          </span>
        </div>
      )}
    </div>
  );
}

function mergeLabel(detail: GitlabMergeRequestDetails, t: ReturnType<typeof useT>): string {
  return detail.detailedMergeStatus || detail.mergeStatus || t("gitlabMrDetail.unknown");
}

function approvalLabel(detail: GitlabMergeRequestDetails, t: ReturnType<typeof useT>): string {
  const approvals = detail.approvals;
  if (!approvals) return t("gitlabMrDetail.unknown");
  const approvedCount = Math.max(
    approvals.approvalsRequired - approvals.approvalsLeft,
    0,
  );
  return t("gitlabMrDetail.approvalProgress", {
    approved: approvedCount,
    required: approvals.approvalsRequired,
  });
}

function gitlabMergeBlockedReason(
  detail: GitlabMergeRequestDetails,
  t: ReturnType<typeof useT>,
): string | null {
  if (!detail.headSha) return t("gitlabMrDetail.blockMissingSha");
  if (detail.hasConflicts) return t("gitlabMrDetail.blockConflicts");
  if (detail.blockingDiscussionsResolved === false) return t("gitlabMrDetail.blockDiscussions");
  if (detail.latestPipeline && detail.latestPipeline.status !== "success") {
    return t("gitlabMrDetail.blockPipeline", { status: detail.latestPipeline.status });
  }
  if (detail.approvals && detail.approvals.approvalsLeft > 0) {
    return t("gitlabMrDetail.blockApprovals", { count: detail.approvals.approvalsLeft });
  }
  const mergeStatus = detail.detailedMergeStatus || detail.mergeStatus;
  if (!["mergeable", "can_be_merged"].includes(mergeStatus)) {
    return mergeStatus || t("gitlabMrDetail.blockNotMergeable");
  }
  return null;
}

function formatGitlabDate(value: string): string {
  return formatDate(value);
}

function formatJobDuration(value: number): string {
  if (!Number.isFinite(value) || value < 0) return "";
  if (value < 60) return `${Math.round(value)}s`;
  const minutes = Math.floor(value / 60);
  const seconds = Math.round(value % 60);
  return seconds ? `${minutes}m ${seconds}s` : `${minutes}m`;
}

function isRetryableGitlabJob(status: string): boolean {
  return ["failed", "canceled"].includes(status);
}

function gitlabDiscussionLocation(
  discussion: GitlabMergeRequestDetails["discussions"][number],
): string {
  return discussion.line ? `${discussion.path}:${discussion.line}` : discussion.path;
}

function findGitlabRemote(
  remotes: RemoteLike[],
  preferredRemote: string | null,
): HostingRemote | null {
  const ordered = [
    ...remotes.filter((remote) => remote.name === preferredRemote),
    ...remotes.filter((remote) => remote.name !== preferredRemote),
  ];
  for (const remote of ordered) {
    const hosting = detectHostingRemote(remote.url);
    if (hosting?.provider === "gitlab") return hosting;
  }
  return null;
}
