import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { cloneRepo, type IpcError } from "../ipc";
import { Button } from "./ui/Button";
import { CloseIcon, CloudIcon, FolderIcon, SpinnerIcon } from "./icons";
import { useT } from "../lib/i18n";

/** 克隆远程仓库。居中模态:URL + 目标父目录(选择器)→ 克隆进 父目录/<推导名>。
 *  成功回调克隆出的仓库根路径(上层据此打开)。冲突/认证/网络给友好提示。 */
export function CloneDialog({ onCloned, onClose }: { onCloned: (repoPath: string) => void; onClose: () => void }) {
  const t = useT();
  const [url, setUrl] = useState("");
  const [parent, setParent] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const urlRef = useRef<HTMLInputElement>(null);

  useEffect(() => { urlRef.current?.focus(); }, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape" && !busy) onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  async function pickParent() {
    const dir = await open({ directory: true, title: t("clone.dialogTitle") });
    if (typeof dir === "string") setParent(dir);
  }

  function errText(e: unknown): string {
    const err = e as IpcError;
    switch (err.code) {
      case "INVALID_URL": return t("clone.errInvalidUrl");
      case "AUTH_FAILED": return t("clone.errAuth");
      case "NETWORK_ERROR": return t("clone.errNetwork");
      case "DESTINATION_NOT_EMPTY": return t("clone.errDestNotEmpty");
      case "GIT_CLI_NOT_FOUND": return t("clone.errNoGit");
      default: return err.message ?? String(e);
    }
  }

  async function doClone() {
    const u = url.trim();
    if (!u || !parent || busy) return;
    setBusy(true);
    setError(null);
    try {
      const repoPath = await cloneRepo(u, parent);
      onCloned(repoPath);
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  // 预览推导出的文件夹名(与后端 derive_repo_name 同口径)。
  const previewName = (() => {
    const raw = url.trim().replace(/\/+$/, "");
    if (!raw) return "";
    const last = raw.split(/[/:]/).pop() ?? "";
    return last.replace(/\.git$/, "") || "repo";
  })();

  return (
    <div
      className="overlay-in fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onClick={busy ? undefined : onClose}
    >
      <form
        onSubmit={(e) => { e.preventDefault(); doClone(); }}
        role="dialog"
        aria-modal="true"
        aria-label={t("clone.title")}
        className="panel-in popover flex w-[480px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-line px-4 py-3">
          <span className="text-accent"><CloudIcon width={16} height={16} /></span>
          <h2 className="text-sm font-semibold text-fg">{t("clone.title")}</h2>
          <button type="button" onClick={onClose} aria-label={t("toast.close")} className="ml-auto grid h-6 w-6 place-items-center rounded text-fg-muted transition-colors hover:bg-overlay hover:text-fg">
            <CloseIcon width={13} height={13} />
          </button>
        </div>

        <div className="flex flex-col gap-3 px-4 py-4">
          <label className="flex flex-col gap-1">
            <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">{t("clone.urlLabel")}</span>
            <input
              ref={urlRef}
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder={t("clone.urlPlaceholder")}
              className="rounded bg-canvas px-2.5 py-1.5 font-mono text-xs text-fg placeholder:text-fg-subtle field"
            />
          </label>

          <label className="flex flex-col gap-1">
            <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">{t("clone.toLabel")}</span>
            <button
              type="button"
              onClick={pickParent}
              className="flex items-center gap-2 rounded border border-line-strong bg-elevated px-2.5 py-1.5 text-left text-xs text-fg transition-colors hover:bg-overlay"
            >
              <FolderIcon width={14} height={14} className="shrink-0 text-fg-subtle" />
              <span className={`truncate ${parent ? "font-mono text-fg" : "text-fg-subtle"}`}>
                {parent ?? t("clone.pickFolder")}
              </span>
            </button>
            {parent && previewName && (
              <span className="text-[11px] text-fg-subtle">
                {t("clone.willCloneTo")} <span className="font-mono text-fg-muted">{parent}/{previewName}</span>
              </span>
            )}
          </label>

          {error && <p className="rounded border border-danger/40 bg-danger/10 px-2.5 py-1.5 text-[11px] text-danger">{error}</p>}
        </div>

        <div className="flex justify-end gap-2 border-t border-line px-4 py-3">
          <Button type="button" variant="ghost" size="md" disabled={busy} onClick={onClose}>{t("confirm.cancel")}</Button>
          <Button type="submit" variant="commit" size="md" disabled={busy || !url.trim() || !parent}>
            {busy ? <span className="flex items-center gap-1.5"><SpinnerIcon width={13} height={13} /> {t("clone.cloning")}</span> : t("clone.clone")}
          </Button>
        </div>
      </form>
    </div>
  );
}
