import { useState } from "react";
import { createTag, resetTo, type CommitDto, type ResetMode, type IpcError } from "../ipc";
import { useToast } from "./Toast";
import { Button } from "./ui/Button";

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
  const toast = useToast();
  const [view, setView] = useState<"main" | "reset" | "resetHard" | "tag">("main");
  const [tagName, setTagName] = useState("");
  const [busy, setBusy] = useState(false);

  async function doReset(mode: ResetMode) {
    setBusy(true);
    try {
      await resetTo(repo, commit.id, mode);
      onChanged();
      toast({ kind: "success", title: `已 ${mode} reset 到 ${commit.short_id}` });
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
      toast({ kind: "success", title: `已创建标签 ${tagName.trim()}` });
      onClose();
    } catch (e) {
      const err = e as IpcError;
      toast({ kind: "error", title: err.code === "TAG_EXISTS" ? "标签已存在" : (err.message ?? String(e)) });
    } finally {
      setBusy(false);
    }
  }

  async function copySha() {
    try {
      await navigator.clipboard.writeText(commit.id);
      toast({ kind: "success", title: "已复制完整 SHA" });
    } catch {
      toast({ kind: "error", title: "复制失败" });
    }
    onClose();
  }

  // 菜单项样式
  const item = "block w-full px-3 py-1.5 text-left text-xs text-fg transition-colors hover:bg-overlay disabled:opacity-40";
  const back = (
    <button className={`${item} text-fg-muted`} onClick={() => setView("main")}>← 返回</button>
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
            <button className={item} onClick={() => { onClose(); onCherryPick(); }}>Cherry-pick 到当前分支</button>
            <button className={item} onClick={() => { onClose(); onRevert(); }}>Revert 此提交</button>
            <button className={item} onClick={() => { onClose(); onRebase(); }}>从此交互式变基</button>
            {onCompareWithSelected && (
              <button className={item} onClick={() => { onClose(); onCompareWithSelected(); }}>
                与选中提交 {selectedShort} 比较
              </button>
            )}
            <button className={item} onClick={() => setView("reset")}>Reset 到此 ▸</button>
            <button className={item} onClick={() => setView("tag")}>打标签…</button>
            <div className="my-1 border-t border-line" />
            <button className={item} onClick={copySha}>复制完整 SHA</button>
          </>
        )}

        {view === "reset" && (
          <>
            {back}
            <button className={item} disabled={busy} onClick={() => doReset("soft")}>Soft(保留暂存+工作区)</button>
            <button className={item} disabled={busy} onClick={() => doReset("mixed")}>Mixed(保留工作区)</button>
            <button className={`${item} text-danger`} disabled={busy} onClick={() => setView("resetHard")}>Hard(丢弃改动)</button>
          </>
        )}

        {view === "resetHard" && (
          <div className="px-3 py-2 text-[11px]">
            <p className="mb-1.5 text-danger">Hard reset 会丢弃未提交改动,确认?</p>
            <div className="flex gap-2">
              <Button variant="danger" size="chip" disabled={busy} onClick={() => doReset("hard")}>确认</Button>
              <button onClick={() => setView("reset")} className="text-fg-muted hover:underline">取消</button>
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
              placeholder="标签名"
              className="mt-1 w-full rounded border border-line-strong bg-canvas px-2 py-1 text-xs text-fg focus:border-accent focus:outline-none"
            />
            <Button variant="primary" size="sm" disabled={busy || !tagName.trim()} onClick={doTag} className="mt-1 w-full">创建标签</Button>
          </div>
        )}
      </div>
    </>
  );
}
