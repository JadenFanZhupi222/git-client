import { useEffect, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getGitlabToken, hasGitlabToken, type IpcError } from "../ipc";
import {
  approveGitlabMergeRequest,
  createGitlabMergeRequestNote,
  fetchGitlabMergeRequestDetails,
  fetchGitlabMergeRequests,
  type GitlabMergeRequestDetails,
  type GitlabMergeRequestSummary,
  unapproveGitlabMergeRequest,
} from "../lib/gitlab";
import {
  detectHostingRemote,
  type HostingRemote,
  type RemoteLike,
} from "../lib/hosting";
import { CloseIcon, SpinnerIcon } from "./icons";
import { useToast } from "./Toast";

export function GitlabMrPanel({
  remotes,
  branch,
  preferredRemote,
  onClose,
  onConfigureToken,
}: {
  remotes: RemoteLike[];
  branch: string | null;
  preferredRemote: string | null;
  onClose: () => void;
  onConfigureToken: () => void;
}) {
  const toast = useToast();
  const [mrs, setMrs] = useState<GitlabMergeRequestSummary[]>([]);
  const [detailByIid, setDetailByIid] = useState<
    Record<number, GitlabMergeRequestDetails>
  >({});
  const [detailLoading, setDetailLoading] = useState<number | null>(null);
  const [approvalAction, setApprovalAction] = useState<{
    iid: number;
    type: "approve" | "unapprove";
  } | null>(null);
  const [creatingNote, setCreatingNote] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const remote = useMemo(
    () => findGitlabRemote(remotes, preferredRemote),
    [remotes, preferredRemote],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  useEffect(() => {
    let alive = true;
    loadList(() => alive);
    return () => {
      alive = false;
    };
  }, [remote, branch]);

  async function loadList(isAlive: () => boolean = () => true) {
    setLoading(true);
    setError(null);
    try {
      if (!remote || !branch) {
        setMrs([]);
        setDetailByIid({});
        setError(
          branch ? "当前仓库没有 GitLab 远程地址" : "当前仓库还没有本地分支",
        );
        return;
      }
      const token = (await hasGitlabToken()) ? await getGitlabToken() : null;
      const next = await fetchGitlabMergeRequests(remote, branch, token);
      if (isAlive()) {
        setMrs(next);
        setDetailByIid({});
      }
    } catch (e) {
      if (isAlive()) setError((e as IpcError).message ?? String(e));
    } finally {
      if (isAlive()) setLoading(false);
    }
  }

  async function openMr(url: string) {
    try {
      await openUrl(url);
    } catch (e) {
      toast({ kind: "error", title: (e as Error).message ?? String(e) });
    }
  }

  async function loadDetail(mr: GitlabMergeRequestSummary) {
    if (!remote || detailByIid[mr.iid] || detailLoading === mr.iid) {
      return;
    }
    setDetailLoading(mr.iid);
    try {
      const token = (await hasGitlabToken()) ? await getGitlabToken() : null;
      const detail = await fetchGitlabMergeRequestDetails(
        remote,
        mr.iid,
        token,
      );
      setDetailByIid((current) => ({
        ...current,
        [mr.iid]: detail,
      }));
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setDetailLoading(null);
    }
  }

  async function approveMr(detail: GitlabMergeRequestDetails) {
    if (!remote || approvalAction?.iid === detail.iid) return;
    setApprovalAction({ iid: detail.iid, type: "approve" });
    try {
      const token = (await hasGitlabToken()) ? await getGitlabToken() : null;
      if (!token?.trim()) {
        toast({ kind: "error", title: "GitLab token is required" });
        onConfigureToken();
        return;
      }
      const approvals = await approveGitlabMergeRequest(
        remote,
        detail.iid,
        token,
      );
      setDetailByIid((current) => ({
        ...current,
        [detail.iid]: {
          ...detail,
          approvals,
        },
      }));
      toast({ kind: "success", title: `Approved MR !${detail.iid}` });
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setApprovalAction(null);
    }
  }

  async function unapproveMr(detail: GitlabMergeRequestDetails) {
    if (!remote || approvalAction?.iid === detail.iid) return;
    setApprovalAction({ iid: detail.iid, type: "unapprove" });
    try {
      const token = (await hasGitlabToken()) ? await getGitlabToken() : null;
      if (!token?.trim()) {
        toast({ kind: "error", title: "GitLab token is required" });
        onConfigureToken();
        return;
      }
      const approvals = await unapproveGitlabMergeRequest(
        remote,
        detail.iid,
        token,
      );
      setDetailByIid((current) => ({
        ...current,
        [detail.iid]: {
          ...detail,
          approvals,
        },
      }));
      toast({ kind: "success", title: `Unapproved MR !${detail.iid}` });
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setApprovalAction(null);
    }
  }

  async function createMrNote(
    detail: GitlabMergeRequestDetails,
    body: string,
  ): Promise<boolean> {
    if (!remote || creatingNote === detail.iid) return false;
    setCreatingNote(detail.iid);
    try {
      const token = (await hasGitlabToken()) ? await getGitlabToken() : null;
      if (!token?.trim()) {
        toast({ kind: "error", title: "GitLab token is required" });
        onConfigureToken();
        return false;
      }
      const note = await createGitlabMergeRequestNote(
        remote,
        detail.iid,
        body,
        token,
      );
      setDetailByIid((current) => ({
        ...current,
        [detail.iid]: {
          ...detail,
          userNotesCount: detail.userNotesCount + 1,
          notes: [note, ...detail.notes],
        },
      }));
      toast({ kind: "success", title: `Commented on MR !${detail.iid}` });
      return true;
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
      return false;
    } finally {
      setCreatingNote(null);
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
        aria-label="GitLab merge requests"
        className="panel-in popover flex max-h-[78vh] w-[560px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-line px-4 py-3">
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-fg">GitLab MR</h2>
            <p className="truncate text-[11px] text-fg-subtle">
              {remote ? `${remote.owner}/${remote.repo}` : "未识别 GitLab 远程"}{" "}
              · {branch ?? "无分支"}
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
              <SpinnerIcon width={13} height={13} /> 正在读取 GitLab MR
            </div>
          ) : error ? (
            <div className="rounded border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
              {error}
            </div>
          ) : mrs.length === 0 ? (
            <p className="py-8 text-center text-xs text-fg-subtle">
              当前分支没有 open MR
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
                            Draft
                          </span>
                        )}
                        <span>{mr.author ?? "unknown"}</span>
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
                      打开
                    </button>
                    <button
                      onClick={() => loadDetail(mr)}
                      disabled={detailLoading === mr.iid}
                      className="shrink-0 rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
                    >
                      {detailLoading === mr.iid ? "Loading" : "Details"}
                    </button>
                  </div>
                  {detailByIid[mr.iid] && (
                    <MergeRequestDetailsView
                      detail={detailByIid[mr.iid]}
                      approvalAction={
                        approvalAction?.iid === mr.iid
                          ? approvalAction.type
                          : null
                      }
                      onApprove={approveMr}
                      onUnapprove={unapproveMr}
                      creatingNote={creatingNote === mr.iid}
                      onCreateNote={createMrNote}
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
              设置 token
            </button>
            <button
              onClick={() => loadList()}
              disabled={loading}
              className="rounded-md px-3 py-1.5 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
            >
              Refresh
            </button>
          </div>
          <button
            onClick={onClose}
            className="rounded-md px-3 py-1.5 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
          >
            关闭
          </button>
        </div>
      </div>
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
}) {
  const [noteBody, setNoteBody] = useState("");
  const trimmedNoteBody = noteBody.trim();

  async function submitNote() {
    if (!trimmedNoteBody || creatingNote) return;
    const created = await onCreateNote(detail, trimmedNoteBody);
    if (created) setNoteBody("");
  }

  return (
    <div className="mt-3 grid gap-2 rounded-md border border-line bg-canvas/60 p-3 text-xs">
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-5">
        <DetailMetric label="Merge" value={mergeLabel(detail)} />
        <DetailMetric
          label="Pipeline"
          value={detail.latestPipeline?.status ?? "unknown"}
        />
        <DetailMetric label="Changes" value={`${detail.changesCount || "0"} changes`} />
        <DetailMetric label="Notes" value={`${detail.userNotesCount} notes`} />
        <DetailMetric label="Approvals" value={approvalLabel(detail)} />
      </div>
      <div className="flex flex-wrap gap-2 text-[11px] text-fg-subtle">
        <span>{detail.hasConflicts ? "conflicts" : "no conflicts"}</span>
        <span>
          discussions {detail.blockingDiscussionsResolved === false ? "blocked" : "resolved"}
        </span>
        <span>+{detail.upvotes}</span>
        <span>-{detail.downvotes}</span>
        {detail.approvals && detail.approvals.approvedBy.length > 0 && (
          <span>{detail.approvals.approvedBy.join(", ")}</span>
        )}
        {detail.approvals?.userCanApprove && !detail.approvals.userHasApproved && (
          <span>you can approve</span>
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
              {approvalAction === "unapprove" ? "Unapproving" : "Unapprove"}
            </button>
          ) : detail.approvals.userCanApprove ? (
            <button
              onClick={() => onApprove(detail)}
              disabled={approvalAction !== null}
              className="rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
            >
              {approvalAction === "approve" ? "Approving" : "Approve"}
            </button>
          ) : null}
        </div>
      )}
      {detail.notes.length > 0 && (
        <div className="grid gap-1.5 border-t border-line pt-2">
          <div className="text-[10px] uppercase tracking-wide text-fg-subtle">
            Notes
          </div>
          {detail.notes.map((note) => (
            <div
              key={note.id}
              className="rounded border border-line bg-elevated/50 px-2 py-1.5"
            >
              <div className="flex min-w-0 items-center gap-2 text-[10px] text-fg-subtle">
                <span className="font-medium text-fg-muted">
                  {note.author ?? "unknown"}
                </span>
                {note.system && <span>system</span>}
                {note.internal && <span>internal</span>}
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
            Discussions
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
                <span>{discussion.resolved ? "resolved" : "unresolved"}</span>
                <span className="ml-auto truncate">
                  {discussion.author ?? "unknown"}
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
          New merge request note
        </label>
        <textarea
          id={`gitlab-mr-note-${detail.iid}`}
          value={noteBody}
          onChange={(event) => setNoteBody(event.currentTarget.value)}
          rows={2}
          className="min-h-14 resize-y rounded-md border border-line bg-elevated px-2 py-1.5 text-xs text-fg outline-none transition-colors placeholder:text-fg-subtle focus:border-accent"
          placeholder="Write a comment"
        />
        <div className="flex justify-end">
          <button
            onClick={submitNote}
            disabled={!trimmedNoteBody || creatingNote}
            className="rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
          >
            {creatingNote ? "Commenting" : "Comment"}
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

function DetailMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-line bg-elevated/60 px-2 py-1.5">
      <div className="text-[10px] uppercase tracking-wide text-fg-subtle">
        {label}
      </div>
      <div className="mt-0.5 truncate font-medium text-fg" title={value}>
        {value}
      </div>
    </div>
  );
}

function mergeLabel(detail: GitlabMergeRequestDetails): string {
  return detail.detailedMergeStatus || detail.mergeStatus || "unknown";
}

function approvalLabel(detail: GitlabMergeRequestDetails): string {
  const approvals = detail.approvals;
  if (!approvals) return "unknown";
  const approvedCount = Math.max(
    approvals.approvalsRequired - approvals.approvalsLeft,
    0,
  );
  return `${approvedCount}/${approvals.approvalsRequired} approved`;
}

function formatGitlabDate(value: string): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
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
