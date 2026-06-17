import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { cloneRepo, type IpcError } from "../ipc";
import { Button } from "./ui/Button";
import { CloseIcon, CloudIcon, FolderIcon, SpinnerIcon } from "./icons";

/** 克隆远程仓库。居中模态:URL + 目标父目录(选择器)→ 克隆进 父目录/<推导名>。
 *  成功回调克隆出的仓库根路径(上层据此打开)。冲突/认证/网络给友好提示。 */
export function CloneDialog({ onCloned, onClose }: { onCloned: (repoPath: string) => void; onClose: () => void }) {
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
    const dir = await open({ directory: true, title: "选择克隆到哪个文件夹" });
    if (typeof dir === "string") setParent(dir);
  }

  function errText(e: unknown): string {
    const err = e as IpcError;
    switch (err.code) {
      case "INVALID_URL": return "仓库地址无效,或远程仓库不存在";
      case "AUTH_FAILED": return "认证失败,请检查凭据 / SSH key";
      case "NETWORK_ERROR": return "网络错误,无法访问远程";
      case "DESTINATION_NOT_EMPTY": return "目标目录已存在且非空";
      case "GIT_CLI_NOT_FOUND": return "未找到 git 命令,请确认已安装 git";
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
    const t = url.trim().replace(/\/+$/, "");
    if (!t) return "";
    const last = t.split(/[/:]/).pop() ?? "";
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
        aria-label="克隆仓库"
        className="panel-in popover flex w-[480px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 border-b border-line px-4 py-3">
          <span className="text-accent"><CloudIcon width={16} height={16} /></span>
          <h2 className="text-sm font-semibold text-fg">克隆仓库</h2>
          <button type="button" onClick={onClose} aria-label="关闭" className="ml-auto grid h-6 w-6 place-items-center rounded text-fg-muted transition-colors hover:bg-overlay hover:text-fg">
            <CloseIcon width={13} height={13} />
          </button>
        </div>

        <div className="flex flex-col gap-3 px-4 py-4">
          <label className="flex flex-col gap-1">
            <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">仓库地址</span>
            <input
              ref={urlRef}
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://github.com/user/repo.git 或 git@…"
              className="rounded bg-canvas px-2.5 py-1.5 font-mono text-xs text-fg placeholder:text-fg-subtle field"
            />
          </label>

          <label className="flex flex-col gap-1">
            <span className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">克隆到</span>
            <button
              type="button"
              onClick={pickParent}
              className="flex items-center gap-2 rounded border border-line-strong bg-elevated px-2.5 py-1.5 text-left text-xs text-fg transition-colors hover:bg-overlay"
            >
              <FolderIcon width={14} height={14} className="shrink-0 text-fg-subtle" />
              <span className={`truncate ${parent ? "font-mono text-fg" : "text-fg-subtle"}`}>
                {parent ?? "选择目标文件夹…"}
              </span>
            </button>
            {parent && previewName && (
              <span className="text-[11px] text-fg-subtle">
                将克隆到 <span className="font-mono text-fg-muted">{parent}/{previewName}</span>
              </span>
            )}
          </label>

          {error && <p className="rounded border border-danger/40 bg-danger/10 px-2.5 py-1.5 text-[11px] text-danger">{error}</p>}
        </div>

        <div className="flex justify-end gap-2 border-t border-line px-4 py-3">
          <Button type="button" variant="ghost" size="md" disabled={busy} onClick={onClose}>取消</Button>
          <Button type="submit" variant="commit" size="md" disabled={busy || !url.trim() || !parent}>
            {busy ? <span className="flex items-center gap-1.5"><SpinnerIcon width={13} height={13} /> 克隆中…</span> : "克隆"}
          </Button>
        </div>
      </form>
    </div>
  );
}
