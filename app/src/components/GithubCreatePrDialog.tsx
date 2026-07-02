import { useEffect, useMemo, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getGithubToken, hasGithubToken, type IpcError } from "../ipc";
import {
  createGithubPullRequest,
  type GithubPullRequestSummary,
} from "../lib/github";
import {
  detectHostingRemote,
  type HostingRemote,
  type RemoteLike,
} from "../lib/hosting";
import { CloseIcon, SpinnerIcon } from "./icons";
import { useToast } from "./Toast";

export function GithubCreatePrDialog({
  remotes,
  branch,
  preferredRemote,
  onClose,
  onCreated,
  onConfigureToken,
}: {
  remotes: RemoteLike[];
  branch: string | null;
  preferredRemote: string | null;
  onClose: () => void;
  onCreated?: (pr: GithubPullRequestSummary) => void;
  onConfigureToken: () => void;
}) {
  const toast = useToast();
  const titleRef = useRef<HTMLInputElement>(null);
  const remote = useMemo(
    () => findGithubRemote(remotes, preferredRemote),
    [remotes, preferredRemote],
  );
  const [title, setTitle] = useState(branch ? `${branch}` : "");
  const [body, setBody] = useState("");
  const [head, setHead] = useState(branch ?? "");
  const [base, setBase] = useState("main");
  const [draft, setDraft] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    titleRef.current?.focus();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  async function submit() {
    if (!remote || busy) return;
    setBusy(true);
    try {
      const token = (await hasGithubToken()) ? await getGithubToken() : null;
      if (!token) {
        toast({ kind: "error", title: "请先设置 GitHub token" });
        onConfigureToken();
        return;
      }
      const pr = await createGithubPullRequest(
        remote,
        { title, body, head, base, draft },
        token,
      );
      toast({ kind: "success", title: `已创建 PR #${pr.number}` });
      onCreated?.(pr);
      await openUrl(pr.url);
      onClose();
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
    }
  }

  const disabled =
    busy || !remote || !title.trim() || !head.trim() || !base.trim();

  return (
    <div
      className="overlay-in fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onClick={busy ? undefined : onClose}
    >
      <form
        role="dialog"
        aria-modal="true"
        aria-label="Create GitHub pull request"
        className="panel-in popover flex w-[520px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
        onClick={(e) => e.stopPropagation()}
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <div className="flex items-center gap-2 border-b border-line px-4 py-3">
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-fg">Create GitHub PR</h2>
            <p className="truncate text-[11px] text-fg-subtle">
              {remote ? `${remote.owner}/${remote.repo}` : "No GitHub remote"}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="ml-auto grid h-6 w-6 place-items-center rounded text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
          >
            <CloseIcon width={13} height={13} />
          </button>
        </div>

        <div className="flex flex-col gap-3 px-4 py-4">
          {!remote && (
            <div className="rounded border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
              当前仓库没有可识别的 GitHub 远程地址
            </div>
          )}
          <label className="flex flex-col gap-1">
            <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
              Title
            </span>
            <input
              ref={titleRef}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              className="rounded bg-canvas px-2.5 py-1.5 text-xs text-fg field"
            />
          </label>
          <div className="grid grid-cols-2 gap-2">
            <label className="flex flex-col gap-1">
              <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
                Head
              </span>
              <input
                value={head}
                onChange={(e) => setHead(e.target.value)}
                className="rounded bg-canvas px-2.5 py-1.5 font-mono text-xs text-fg field"
              />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
                Base
              </span>
              <input
                value={base}
                onChange={(e) => setBase(e.target.value)}
                className="rounded bg-canvas px-2.5 py-1.5 font-mono text-xs text-fg field"
              />
            </label>
          </div>
          <label className="flex flex-col gap-1">
            <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
              Body
            </span>
            <textarea
              value={body}
              onChange={(e) => setBody(e.target.value)}
              rows={5}
              className="resize-none rounded bg-canvas px-2.5 py-1.5 text-xs text-fg field"
            />
          </label>
          <label className="flex items-center gap-2 text-xs text-fg-muted">
            <input
              type="checkbox"
              checked={draft}
              onChange={(e) => setDraft(e.target.checked)}
              className="h-3.5 w-3.5"
            />
            Draft pull request
          </label>
        </div>

        <div className="flex justify-between gap-2 border-t border-line px-4 py-3">
          <button
            type="button"
            onClick={onConfigureToken}
            className="rounded-md px-3 py-1.5 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
          >
            Token
          </button>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={onClose}
              disabled={busy}
              className="rounded-md px-3 py-1.5 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={disabled}
              className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-50"
            >
              {busy ? <SpinnerIcon width={13} height={13} /> : null}
              Create
            </button>
          </div>
        </div>
      </form>
    </div>
  );
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
