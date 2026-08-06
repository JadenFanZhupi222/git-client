import { useEffect, useMemo, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getGithubToken, hasGithubToken, type IpcError } from "../ipc";
import {
  createGithubPullRequestComment,
  fetchGithubPullRequestDetails,
  fetchGithubPullRequests,
  mergeGithubPullRequest,
  type GithubPullMergeMethod,
  type GithubPullRequestDetails,
  type GithubPullRequestSummary,
} from "../lib/github";
import type { RemoteLike } from "../lib/hosting";
import { useT } from "../lib/i18n";
import { useToast } from "./Toast";
import { CloudIcon, PlusIcon, SearchIcon } from "./icons";
import { PrReviewWorkspace } from "./PrReviewWorkspace";
import { findGithubRemote, PullRequestDetailsView } from "./GithubPrPanel";

type PullFilter = "all" | "current";

export function PullRequestsView({
  remotes,
  branch,
  preferredRemote,
  onCreatePullRequest,
  onConfigureToken,
  onConfigureCredential,
}: {
  remotes: RemoteLike[];
  branch: string | null;
  preferredRemote: string | null;
  onCreatePullRequest: () => void;
  onConfigureToken: () => void;
  onConfigureCredential?: (kind: "deepseek" | "github") => void;
}) {
  const toast = useToast();
  const t = useT();
  const remote = useMemo(
    () => findGithubRemote(remotes, preferredRemote),
    [remotes, preferredRemote],
  );
  const [pulls, setPulls] = useState<GithubPullRequestSummary[]>([]);
  const [selectedNumber, setSelectedNumber] = useState<number | null>(null);
  const [detail, setDetail] = useState<GithubPullRequestDetails | null>(null);
  const [filter, setFilter] = useState<PullFilter>("all");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [creatingComment, setCreatingComment] = useState(false);
  const [merging, setMerging] = useState(false);
  const [reviewTarget, setReviewTarget] = useState<{
    owner: string;
    repo: string;
    pull_number: number;
  } | null>(null);
  const [compactDetailOpen, setCompactDetailOpen] = useState(false);

  const visiblePulls = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return pulls.filter((pull) => {
      if (filter === "current" && pull.headRef !== branch) return false;
      if (!normalized) return true;
      return `${pull.number} ${pull.title} ${pull.author ?? ""} ${pull.headRef}`
        .toLocaleLowerCase()
        .includes(normalized);
    });
  }, [branch, filter, pulls, query]);

  useEffect(() => {
    let alive = true;
    void loadPulls(() => alive);
    return () => {
      alive = false;
    };
  }, [remote]);

  useEffect(() => {
    if (!visiblePulls.some((pull) => pull.number === selectedNumber)) {
      setSelectedNumber(visiblePulls[0]?.number ?? null);
    }
  }, [selectedNumber, visiblePulls]);

  useEffect(() => {
    let alive = true;
    const selected = pulls.find((pull) => pull.number === selectedNumber);
    if (!remote || !selected) {
      setDetail(null);
      return;
    }
    setDetail(null);
    setDetailLoading(true);
    void githubToken()
      .then((token) => fetchGithubPullRequestDetails(remote, selected.number, token))
      .then((next) => {
        if (alive) setDetail(next);
      })
      .catch((reason) => {
        if (alive) toast({ kind: "error", title: errorMessage(reason) });
      })
      .finally(() => {
        if (alive) setDetailLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [remote, selectedNumber]);

  async function loadPulls(isAlive: () => boolean = () => true) {
    setLoading(true);
    setError(null);
    try {
      if (!remote) {
        setPulls([]);
        setSelectedNumber(null);
        setError(t("prWorkspace.remoteMissing"));
        return;
      }
      const next = await fetchGithubPullRequests(remote, null, await githubToken());
      if (!isAlive()) return;
      setPulls(next);
      setSelectedNumber((current) => {
        if (next.some((pull) => pull.number === current)) return current;
        return next.find((pull) => pull.headRef === branch)?.number ?? next[0]?.number ?? null;
      });
    } catch (reason) {
      if (isAlive()) setError(errorMessage(reason));
    } finally {
      if (isAlive()) setLoading(false);
    }
  }

  async function createComment(
    current: GithubPullRequestDetails,
    body: string,
  ): Promise<boolean> {
    if (!remote || creatingComment) return false;
    setCreatingComment(true);
    try {
      const token = await githubToken();
      if (!token) {
        onConfigureToken();
        return false;
      }
      const comment = await createGithubPullRequestComment(
        remote,
        current.number,
        body,
        token,
      );
      setDetail({
        ...current,
        comments: current.comments + 1,
        recentComments: [...current.recentComments, comment].slice(-20),
      });
      toast({ kind: "success", title: `Commented on PR #${current.number}` });
      return true;
    } catch (reason) {
      toast({ kind: "error", title: errorMessage(reason) });
      return false;
    } finally {
      setCreatingComment(false);
    }
  }

  async function mergePull(
    current: GithubPullRequestDetails,
    method: GithubPullMergeMethod,
  ) {
    if (!remote || merging) return;
    setMerging(true);
    try {
      const token = await githubToken();
      if (!token) {
        onConfigureToken();
        return;
      }
      const result = await mergeGithubPullRequest(
        remote,
        current.number,
        { method, headSha: current.headSha },
        token,
      );
      const next = pulls.filter((pull) => pull.number !== current.number);
      setPulls(next);
      setSelectedNumber(next[0]?.number ?? null);
      toast({ kind: "success", title: result.message || `Merged PR #${current.number}` });
    } catch (reason) {
      toast({ kind: "error", title: errorMessage(reason) });
    } finally {
      setMerging(false);
    }
  }

  return (
    <section className="flex h-full min-h-0 flex-col bg-canvas" aria-label={t("prWorkspace.aria")}>
      <header className="flex shrink-0 items-center gap-3 border-b border-line px-4 py-3">
        <div className="min-w-0">
          <h1 className="text-sm font-semibold text-fg">{t("prWorkspace.title")}</h1>
          <p className="truncate text-[11px] text-fg-subtle">
            {remote ? `${remote.owner}/${remote.repo}` : t("prWorkspace.noRemote")}
          </p>
        </div>
        <div className="ml-auto flex items-center gap-2">
          <button
            onClick={() => void loadPulls()}
            disabled={loading}
            className="rounded-md border border-line-strong px-2.5 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg disabled:opacity-50"
          >
            {loading ? t("prWorkspace.refreshing") : t("prWorkspace.refresh")}
          </button>
          <button
            onClick={onCreatePullRequest}
            className="flex items-center gap-1.5 rounded-md bg-accent px-2.5 py-1.5 text-xs font-semibold text-on-accent"
          >
            <PlusIcon width={13} height={13} /> {t("prWorkspace.new")}
          </button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className={`w-[340px] shrink-0 flex-col border-r border-line bg-elevated/35 ${compactDetailOpen ? "flex max-md:hidden" : "flex max-md:w-full"}`}>
          <div className="grid gap-2 border-b border-line p-3">
            <label className="flex items-center gap-2 rounded-md border border-line bg-canvas px-2.5 py-1.5 focus-within:border-accent">
              <SearchIcon width={13} height={13} className="shrink-0 text-fg-subtle" />
              <span className="sr-only">{t("prWorkspace.search")}</span>
              <input
                value={query}
                onChange={(event) => setQuery(event.currentTarget.value)}
                placeholder={t("prWorkspace.search")}
                className="min-w-0 flex-1 bg-transparent text-xs text-fg outline-none placeholder:text-fg-subtle"
              />
            </label>
            <div className="flex gap-1" aria-label={t("prWorkspace.filters")}>
              <FilterButton active={filter === "all"} onClick={() => setFilter("all")}>
                {t("prWorkspace.allOpen")} <span className="font-mono text-[10px]">{pulls.length}</span>
              </FilterButton>
              <FilterButton active={filter === "current"} onClick={() => setFilter("current")} disabled={!branch}>
                {t("prWorkspace.currentBranch")}
              </FilterButton>
            </div>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto p-2">
            {loading ? (
              <div className="grid gap-2 p-1" aria-label={t("prWorkspace.loading")}>
                {[0, 1, 2].map((item) => <div key={item} className="skeleton h-16 rounded-md" />)}
              </div>
            ) : error ? (
              <EmptyState title={t("prWorkspace.loadError")} detail={error} action={t("prWorkspace.configureGithub")} onAction={onConfigureToken} />
            ) : visiblePulls.length === 0 ? (
              <EmptyState
                title={filter === "current" ? t("prWorkspace.noCurrent") : t("prWorkspace.noOpen")}
                detail={filter === "current" ? t("prWorkspace.noCurrentDetail") : t("prWorkspace.noOpenDetail")}
                action={t("prWorkspace.new")}
                onAction={onCreatePullRequest}
              />
            ) : (
              <ul className="grid gap-1">
                {visiblePulls.map((pull) => {
                  const selected = pull.number === selectedNumber;
                  const current = pull.headRef === branch;
                  return (
                    <li key={pull.number}>
                      <button
                        onClick={() => {
                          setSelectedNumber(pull.number);
                          setCompactDetailOpen(true);
                        }}
                        className={`w-full rounded-md px-3 py-2.5 text-left transition-colors ${selected ? "bg-accent/12 text-fg" : "text-fg-muted hover:bg-overlay hover:text-fg"}`}
                      >
                        <span className="flex items-start gap-2">
                          <CloudIcon width={14} height={14} className={selected ? "mt-0.5 shrink-0 text-accent" : "mt-0.5 shrink-0 text-fg-subtle"} />
                          <span className="min-w-0 flex-1">
                            <span className="block truncate text-xs font-medium">{pull.title}</span>
                            <span className="mt-1 flex min-w-0 items-center gap-1.5 text-[10px] text-fg-subtle">
                              <span className="font-mono">#{pull.number}</span>
                              <span className="truncate">{pull.headRef} → {pull.baseRef}</span>
                              {current && <span className="shrink-0 rounded-full bg-success/12 px-1.5 py-0.5 text-success">{t("prWorkspace.currentBranch")}</span>}
                            </span>
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
            <div className="grid gap-3 p-5" aria-label={t("prWorkspace.loadingDetail")}>
              <div className="skeleton h-7 w-2/3 rounded" />
              <div className="skeleton h-24 rounded-md" />
              <div className="skeleton h-48 rounded-md" />
            </div>
          ) : detail ? (
            <article className="mx-auto max-w-4xl px-5 py-4">
              <button onClick={() => setCompactDetailOpen(false)} className="mb-3 hidden rounded-md border border-line-strong px-2.5 py-1.5 text-xs text-fg-muted hover:bg-overlay max-md:inline-flex">← {t("prWorkspace.title")}</button>
              <div className="flex items-start gap-4">
                <div className="min-w-0 flex-1">
                  <h2 className="text-base font-semibold text-fg">{detail.title}</h2>
                  <p className="mt-1 text-xs text-fg-subtle">
                    #{detail.number} {t("prWorkspace.openedBy", { author: detail.author ?? "unknown" })} · <span className="font-mono">{detail.headRef} → {detail.baseRef}</span>
                  </p>
                </div>
                <button onClick={() => void openUrl(detail.url)} className="shrink-0 rounded-md border border-line-strong px-2.5 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg">
                  {t("prWorkspace.openGithub")}
                </button>
              </div>
              <PullRequestDetailsView
                detail={detail}
                creatingComment={creatingComment}
                onCreateComment={createComment}
                mergingPull={merging}
                onMerge={mergePull}
                onAiReview={() => remote && setReviewTarget({ owner: remote.owner, repo: remote.repo, pull_number: detail.number })}
              />
            </article>
          ) : (
            <div className="grid h-full place-items-center p-8 text-center">
              <div>
                <CloudIcon width={28} height={28} className="mx-auto text-fg-subtle" />
                <h2 className="mt-3 text-sm font-medium text-fg">{t("prWorkspace.select")}</h2>
                <p className="mt-1 text-xs text-fg-subtle">{t("prWorkspace.selectDetail")}</p>
              </div>
            </div>
          )}
        </main>
      </div>

      {reviewTarget && createPortal(
        <PrReviewWorkspace
          target={reviewTarget}
          onClose={() => setReviewTarget(null)}
          onConfigureCredential={(kind) => (onConfigureCredential ?? ((next) => next === "github" && onConfigureToken()))(kind)}
        />,
        document.body,
      )}
    </section>
  );
}

function FilterButton({ active, disabled, onClick, children }: { active: boolean; disabled?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`flex items-center gap-1.5 rounded-md px-2 py-1 text-[11px] transition-colors disabled:opacity-40 ${active ? "bg-overlay font-medium text-fg" : "text-fg-muted hover:bg-overlay hover:text-fg"}`}
    >
      {children}
    </button>
  );
}

function EmptyState({ title, detail, action, onAction }: { title: string; detail: string; action: string; onAction: () => void }) {
  return (
    <div className="px-5 py-10 text-center">
      <CloudIcon width={22} height={22} className="mx-auto text-fg-subtle" />
      <h3 className="mt-3 text-xs font-medium text-fg">{title}</h3>
      <p className="mt-1 text-[11px] leading-5 text-fg-subtle">{detail}</p>
      <button onClick={onAction} className="mt-3 rounded-md border border-line-strong px-2.5 py-1.5 text-xs text-fg-muted hover:bg-overlay hover:text-fg">{action}</button>
    </div>
  );
}

async function githubToken(): Promise<string | null> {
  return (await hasGithubToken()) ? getGithubToken() : null;
}

function errorMessage(reason: unknown): string {
  return (reason as IpcError)?.message ?? String(reason);
}
