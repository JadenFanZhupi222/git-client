import { useState } from "react";
import { createTag, resetTo, type CommitDto, type ResetMode, type IpcError } from "../ipc";
import { useToast } from "./Toast";
import { Button } from "./ui/Button";
import { useT } from "../lib/i18n";

/** 提交右键上下文菜单。cherry-pick/revert/变基 复用上层处理(带各自的失效/toast);
 *  reset/tag/复制 SHA 自包含。onChanged 让上层失效历史/工作区/状态。 */
export function CommitContextMenu({
  repo, commit, x, y, onClose, onCherryPick, onRevert, onRebase, onChanged,
  selectedShort, onCompareWithSelected,
}: {
  repo: string;
  commit: CommitDto;
  x: number;
  y: number;
  onClose: () => void;
  onCherryPick: () => void;
  onRevert: () => void;
  onRebase: () => void;
  onChanged: () => void;
  /** 已选中的另一提交短 SHA(用于「与选中提交比较」文案);无则不显示该项。 */
  selectedShort?: string;
  onCompareWithSelected?: () => void;
}) {
  const t = useT();
  const toast = useToast();
  const [view, setView] = useState<"main" | "reset" | "resetHard" | "tag">("main");
  const [tagName, setTagName] = useState("");
  const [busy, setBusy] = useState(false);

  async function doReset(mode: ResetMode) {
    setBusy(true);
    try {
      await resetTo(repo, commit.id, mode);
      onChanged();
      toast({ kind: "success", title: t("reset.done", { mode, label: commit.short_id }) });
      onClose();
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
    }
  }

  async function doTag() {
    if (!tagName.trim() || busy) return;
    setBusy(true);
    try {
      await createTag(repo, tagName.trim(), commit.id);
      onChanged();
      toast({ kind: "success", title: t("tag.created", { name: tagName.trim() }) });
      onClose();
    } catch (e) {
      const err = e as IpcError;
      toast({ kind: "error", title: err.code === "TAG_EXISTS" ? t("tag.exists") : (err.message ?? String(e)) });
    } finally {
      setBusy(false);
    }
  }

  async function copySha() {
    try {
      await navigator.clipboard.writeText(commit.id);
      toast({ kind: "success", title: t("ctx.shaCopied") });
    } catch {
      toast({ kind: "error", title: t("ctx.copyFailed") });
    }
    onClose();
  }

  // 菜单项样式
  const item = "block w-full px-3 py-1.5 text-left text-xs text-fg transition-colors hover:bg-overlay disabled:opacity-40";
  const back = (
    <button className={`${item} text-fg-muted`} onClick={() => setView("main")}>{t("ctx.back")}</button>
  );

  return (
    <>
      {/* 点击/右键外部关闭 */}
      <div className="fixed inset-0 z-50" onClick={onClose} onContextMenu={(e) => { e.preventDefault(); onClose(); }} />
      <div
        className="fixed z-50 w-44 overflow-hidden rounded-md border border-line-strong bg-elevated py-1 shadow-xl"
        style={{
          left: Math.min(x, window.innerWidth - 184),
          top: Math.min(y, window.innerHeight - 300),
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-3 py-1 font-mono text-[10px] text-fg-subtle">{commit.short_id}</div>
        <div className="my-1 border-t border-line" />

        {view === "main" && (
          <>
            <button className={item} onClick={() => { onClose(); onCherryPick(); }}>{t("ctx.cherryPick")}</button>
            <button className={item} onClick={() => { onClose(); onRevert(); }}>{t("ctx.revert")}</button>
            <button className={item} onClick={() => { onClose(); onRebase(); }}>{t("ctx.rebase")}</button>
            {onCompareWithSelected && (
              <button className={item} onClick={() => { onClose(); onCompareWithSelected(); }}>
                {t("ctx.compareWith", { short: selectedShort ?? "" })}
              </button>
            )}
            <button className={item} onClick={() => setView("reset")}>{t("ctx.resetTo")}</button>
            <button className={item} onClick={() => setView("tag")}>{t("ctx.tag")}</button>
            <div className="my-1 border-t border-line" />
            <button className={item} onClick={copySha}>{t("ctx.copySha")}</button>
          </>
        )}

        {view === "reset" && (
          <>
            {back}
            <button className={item} disabled={busy} onClick={() => doReset("soft")}>{t("ctx.resetSoft")}</button>
            <button className={item} disabled={busy} onClick={() => doReset("mixed")}>{t("ctx.resetMixed")}</button>
            <button className={`${item} text-danger`} disabled={busy} onClick={() => setView("resetHard")}>{t("ctx.resetHard")}</button>
          </>
        )}

        {view === "resetHard" && (
          <div className="px-3 py-2 text-[11px]">
            <p className="mb-1.5 text-danger">{t("ctx.resetHardConfirm")}</p>
            <div className="flex gap-2">
              <Button variant="danger" size="chip" disabled={busy} onClick={() => doReset("hard")}>{t("confirm.ok")}</Button>
              <button onClick={() => setView("reset")} className="text-fg-muted hover:underline">{t("confirm.cancel")}</button>
            </div>
          </div>
        )}

        {view === "tag" && (
          <div className="px-2 py-1.5">
            {back}
            <input
              autoFocus
              value={tagName}
              onChange={(e) => setTagName(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") doTag(); if (e.key === "Escape") setView("main"); }}
              placeholder={t("tag.namePlaceholder")}
              className="mt-1 w-full rounded border border-line-strong bg-canvas px-2 py-1 text-xs text-fg field"
            />
            <Button variant="primary" size="sm" disabled={busy || !tagName.trim()} onClick={doTag} className="mt-1 w-full">{t("ctx.createTag")}</Button>
          </div>
        )}
      </div>
    </>
  );
}
