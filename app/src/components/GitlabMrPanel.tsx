import { useEffect, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getGitlabToken, hasGitlabToken, type IpcError } from "../ipc";
import {
  fetchGitlabMergeRequests,
  type GitlabMergeRequestSummary,
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
    async function load() {
      setLoading(true);
      setError(null);
      try {
        if (!remote || !branch) {
          setMrs([]);
          setError(
            branch ? "当前仓库没有 GitLab 远程地址" : "当前仓库还没有本地分支",
          );
          return;
        }
        const token = (await hasGitlabToken()) ? await getGitlabToken() : null;
        const next = await fetchGitlabMergeRequests(remote, branch, token);
        if (alive) setMrs(next);
      } catch (e) {
        if (alive) setError((e as IpcError).message ?? String(e));
      } finally {
        if (alive) setLoading(false);
      }
    }
    load();
    return () => {
      alive = false;
    };
  }, [remote, branch]);

  async function openMr(url: string) {
    try {
      await openUrl(url);
    } catch (e) {
      toast({ kind: "error", title: (e as Error).message ?? String(e) });
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
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="flex justify-between gap-2 border-t border-line px-4 py-3">
          <button
            onClick={onConfigureToken}
            className="rounded-md px-3 py-1.5 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
          >
            设置 token
          </button>
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
