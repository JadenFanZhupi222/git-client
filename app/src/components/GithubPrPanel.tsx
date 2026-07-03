import { useEffect, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getGithubToken, hasGithubToken, type IpcError } from "../ipc";
import {
  createGithubPullRequestComment,
  fetchGithubPullRequestDetails,
  fetchGithubPullRequests,
  type GithubPullRequestDetails,
  type GithubPullRequestSummary,
} from "../lib/github";
import {
  detectHostingRemote,
  type RemoteLike,
  type HostingRemote,
} from "../lib/hosting";
import { CloseIcon, SpinnerIcon } from "./icons";
import { useToast } from "./Toast";

export function GithubPrPanel({
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
  const [pulls, setPulls] = useState<GithubPullRequestSummary[]>([]);
  const [detailByNumber, setDetailByNumber] = useState<
    Record<number, GithubPullRequestDetails>
  >({});
  const [detailLoading, setDetailLoading] = useState<number | null>(null);
  const [creatingComment, setCreatingComment] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const remote = useMemo(
    () => findGithubRemote(remotes, preferredRemote),
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
        setPulls([]);
        setDetailByNumber({});
        setError(
          branch ? "当前仓库没有 GitHub 远程地址" : "当前仓库还没有本地分支",
        );
        return;
      }
      const token = (await hasGithubToken()) ? await getGithubToken() : null;
      const next = await fetchGithubPullRequests(remote, branch, token);
      if (isAlive()) {
        setPulls(next);
        setDetailByNumber({});
      }
    } catch (e) {
      if (isAlive()) setError((e as IpcError).message ?? String(e));
    } finally {
      if (isAlive()) setLoading(false);
    }
  }

  async function openPull(url: string) {
    try {
      await openUrl(url);
    } catch (e) {
      toast({ kind: "error", title: (e as Error).message ?? String(e) });
    }
  }

  async function loadDetail(pr: GithubPullRequestSummary) {
    if (!remote || detailByNumber[pr.number] || detailLoading === pr.number) {
      return;
    }
    setDetailLoading(pr.number);
    try {
      const token = (await hasGithubToken()) ? await getGithubToken() : null;
      const detail = await fetchGithubPullRequestDetails(
        remote,
        pr.number,
        token,
      );
      setDetailByNumber((current) => ({
        ...current,
        [pr.number]: detail,
      }));
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setDetailLoading(null);
    }
  }

  async function createPullComment(
    detail: GithubPullRequestDetails,
    body: string,
  ): Promise<boolean> {
    if (!remote || creatingComment === detail.number) return false;
    setCreatingComment(detail.number);
    try {
      const token = (await hasGithubToken()) ? await getGithubToken() : null;
      if (!token?.trim()) {
        toast({ kind: "error", title: "GitHub token is required" });
        onConfigureToken();
        return false;
      }
      await createGithubPullRequestComment(
        remote,
        detail.number,
        body,
        token,
      );
      setDetailByNumber((current) => ({
        ...current,
        [detail.number]: {
          ...detail,
          comments: detail.comments + 1,
        },
      }));
      toast({ kind: "success", title: `Commented on PR #${detail.number}` });
      return true;
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
      return false;
    } finally {
      setCreatingComment(null);
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
        aria-label="GitHub pull requests"
        className="panel-in popover flex max-h-[78vh] w-[560px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-line px-4 py-3">
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-fg">GitHub PR</h2>
            <p className="truncate text-[11px] text-fg-subtle">
              {remote ? `${remote.owner}/${remote.repo}` : "未识别 GitHub 远程"}{" "}
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
              <SpinnerIcon width={13} height={13} /> 正在读取 GitHub PR
            </div>
          ) : error ? (
            <div className="rounded border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
              {error}
            </div>
          ) : pulls.length === 0 ? (
            <p className="py-8 text-center text-xs text-fg-subtle">
              当前分支没有 open PR
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
                      打开
                    </button>
                    <button
                      onClick={() => loadDetail(pr)}
                      disabled={detailLoading === pr.number}
                      className="shrink-0 rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
                    >
                      {detailLoading === pr.number ? "Loading" : "Details"}
                    </button>
                  </div>
                  {detailByNumber[pr.number] && (
                    <PullRequestDetailsView
                      detail={detailByNumber[pr.number]}
                      creatingComment={creatingComment === pr.number}
                      onCreateComment={createPullComment}
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

function PullRequestDetailsView({
  detail,
  creatingComment,
  onCreateComment,
}: {
  detail: GithubPullRequestDetails;
  creatingComment: boolean;
  onCreateComment: (
    detail: GithubPullRequestDetails,
    body: string,
  ) => Promise<boolean>;
}) {
  const [commentBody, setCommentBody] = useState("");
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

  return (
    <div className="mt-3 grid gap-2 rounded-md border border-line bg-canvas/60 p-3 text-xs">
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <DetailMetric label="Merge" value={mergeableLabel(detail)} />
        <DetailMetric
          label="Status"
          value={detail.combinedStatus?.state ?? "unknown"}
        />
        <DetailMetric label="Reviews" value={reviewSummary(reviewCounts)} />
        <DetailMetric label="Changes" value={`${detail.changedFiles} files`} />
      </div>
      <div className="flex flex-wrap gap-2 text-[11px] text-fg-subtle">
        <span>{detail.commits} commits</span>
        <span>+{detail.additions}</span>
        <span>-{detail.deletions}</span>
        <span>{detail.comments} comments</span>
        <span>{detail.reviewComments} review comments</span>
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
      <div className="grid gap-1.5 border-t border-line pt-2">
        <label className="sr-only" htmlFor={`github-pr-comment-${detail.number}`}>
          New pull request comment
        </label>
        <textarea
          id={`github-pr-comment-${detail.number}`}
          value={commentBody}
          onChange={(event) => setCommentBody(event.currentTarget.value)}
          rows={2}
          className="min-h-14 resize-y rounded-md border border-line bg-elevated px-2 py-1.5 text-xs text-fg outline-none transition-colors placeholder:text-fg-subtle focus:border-accent"
          placeholder="Write a comment"
        />
        <div className="flex justify-end">
          <button
            onClick={submitComment}
            disabled={!trimmedCommentBody || creatingComment}
            className="rounded-md border border-line-strong px-2 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
          >
            {creatingComment ? "Commenting" : "Comment"}
          </button>
        </div>
      </div>
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

function mergeableLabel(detail: GithubPullRequestDetails): string {
  if (detail.mergeable === true) return "mergeable";
  if (detail.mergeable === false) return detail.mergeableState ?? "blocked";
  return detail.mergeableState ?? "unknown";
}

function reviewSummary(counts: Record<string, number>): string {
  const approved = counts.APPROVED ?? 0;
  const changes = counts.CHANGES_REQUESTED ?? 0;
  if (approved || changes) return `${approved} approved / ${changes} changes`;
  const total = Object.values(counts).reduce((sum, count) => sum + count, 0);
  return total ? `${total} reviews` : "none";
}

function findGithubRemote(
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
