import { useEffect, useRef, useState } from "react";
import {
  clearGitlabToken,
  hasGitlabToken,
  setGitlabToken,
  type IpcError,
} from "../ipc";
import { CloseIcon, SpinnerIcon } from "./icons";
import { useToast } from "./Toast";
import { useT } from "../lib/i18n";

export function GitLabTokenDialog({ onClose }: { onClose: () => void }) {
  const toast = useToast();
  const t = useT();
  const inputRef = useRef<HTMLInputElement>(null);
  const [token, setToken] = useState("");
  const [hasToken, setHasToken] = useState(false);
  const [busy, setBusy] = useState(true);

  useEffect(() => {
    inputRef.current?.focus();
    let alive = true;
    hasGitlabToken()
      .then((value) => {
        if (alive) setHasToken(value);
      })
      .catch((e) =>
        toast({ kind: "error", title: (e as IpcError).message ?? String(e) }),
      )
      .finally(() => {
        if (alive) setBusy(false);
      });
    return () => {
      alive = false;
    };
  }, [toast]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  async function save() {
    if (!token.trim() || busy) return;
    setBusy(true);
    try {
      await setGitlabToken(token);
      toast({ kind: "success", title: t("collabToken.gitlabSaved") });
      setToken("");
      setHasToken(true);
      onClose();
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
    }
  }

  async function clear() {
    if (busy) return;
    setBusy(true);
    try {
      await clearGitlabToken();
      toast({ kind: "success", title: t("collabToken.gitlabCleared") });
      setHasToken(false);
      setToken("");
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="overlay-in fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onClick={busy ? undefined : onClose}
    >
      <form
        role="dialog"
        aria-modal="true"
        aria-label="GitLab token"
        className="panel-in popover flex w-[440px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
        onClick={(e) => e.stopPropagation()}
        onSubmit={(e) => {
          e.preventDefault();
          save();
        }}
      >
        <div className="flex items-center gap-2 border-b border-line px-4 py-3">
          <h2 className="text-sm font-semibold text-fg">{t("collabToken.gitlabTitle")}</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label={t("collabToken.close")}
            className="ml-auto grid h-6 w-6 place-items-center rounded text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
          >
            <CloseIcon width={13} height={13} />
          </button>
        </div>

        <div className="flex flex-col gap-3 px-4 py-4">
          <div className="rounded border border-line bg-elevated px-3 py-2 text-xs text-fg-muted">
            {t("collabToken.currentStatus", {
              status: hasToken ? t("collabToken.statusSaved") : t("collabToken.statusUnset"),
            })}
          </div>
          <label className="flex flex-col gap-1">
            <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">
              {t("collabToken.label")}
            </span>
            <input
              ref={inputRef}
              type="password"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder={t("collabToken.gitlabPlaceholder")}
              className="rounded bg-canvas px-2.5 py-1.5 font-mono text-xs text-fg placeholder:text-fg-subtle field"
            />
          </label>
        </div>

        <div className="flex justify-between gap-2 border-t border-line px-4 py-3">
          <button
            type="button"
            onClick={clear}
            disabled={busy || !hasToken}
            className="rounded-md px-3 py-1.5 text-xs text-danger transition-colors hover:bg-danger/10 disabled:opacity-50"
          >
            {t("collabToken.clear")}
          </button>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={onClose}
              disabled={busy}
              className="rounded-md px-3 py-1.5 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-50"
            >
              {t("collabToken.cancel")}
            </button>
            <button
              type="submit"
              disabled={busy || !token.trim()}
              className="flex items-center gap-1.5 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-50"
            >
              {busy ? <SpinnerIcon width={13} height={13} /> : null}
              {t("collabToken.save")}
            </button>
          </div>
        </div>
      </form>
    </div>
  );
}
