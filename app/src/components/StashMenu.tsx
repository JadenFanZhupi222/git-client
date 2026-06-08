import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { stashSave, stashApply, stashPop, stashDrop, type IpcError } from "../ipc";
import { useStashList, qk, invalidateWorktree } from "../lib/queries";
import { useToast } from "./Toast";
import { Button } from "./ui/Button";
import { StashIcon, ChevronDownIcon, TrashIcon } from "./icons";

/** 顶栏贮藏下拉:贮藏当前改动(可填信息)+ 列出已有贮藏(应用/弹出/删除)。 */
export function StashMenu({ repo }: { repo: string }) {
  const qc = useQueryClient();
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);
  const stashes = useStashList(repo).data ?? [];

  function invalidate() {
    qc.invalidateQueries({ queryKey: qk.stashes(repo) });
    invalidateWorktree(qc, repo); // 贮藏/应用/弹出都改工作区
  }

  async function run(action: () => Promise<void>, okTitle: string) {
    setBusy(true);
    try {
      await action();
      invalidate();
      toast({ kind: "success", title: okTitle });
    } catch (e) {
      // 冲突(MERGE_CONFLICT)等:工作区可能留下冲突标记,到「更改」页查看。
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
    }
  }

  async function save() {
    const m = msg.trim();
    await run(() => stashSave(repo, m || undefined), "已贮藏改动");
    setMsg("");
  }

  return (
    <div className="relative">
      <Button
        variant="secondary"
        size="sm"
        onClick={() => setOpen((o) => !o)}
        disabled={busy}
        title="贮藏(stash)"
        className="hover:border-fg-subtle"
      >
        <StashIcon width={13} height={13} />
        贮藏
        {stashes.length > 0 && (
          <span className="rounded-full bg-accent/15 px-1 font-mono text-[10px] font-semibold text-accent">
            {stashes.length}
          </span>
        )}
        <ChevronDownIcon width={11} height={11} />
      </Button>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div className="absolute right-0 top-full z-50 mt-1 w-72 overflow-hidden rounded-md border border-line-strong bg-elevated text-xs shadow-lg">
            {/* 贮藏当前改动 */}
            <form
              onSubmit={(e) => { e.preventDefault(); save(); }}
              className="flex items-center gap-1 border-b border-line p-1.5"
            >
              <input
                value={msg}
                onChange={(e) => setMsg(e.target.value)}
                placeholder="贮藏信息(可选)…"
                className="min-w-0 flex-1 rounded bg-canvas px-2 py-1 text-fg placeholder:text-fg-subtle focus:outline-none"
              />
              <Button type="submit" variant="commit" size="sm" disabled={busy} className="shrink-0">
                贮藏改动
              </Button>
            </form>

            {/* 贮藏列表 */}
            <ul className="max-h-64 overflow-y-auto py-1">
              {stashes.length === 0 ? (
                <li className="px-2.5 py-2 text-fg-subtle">没有贮藏</li>
              ) : (
                stashes.map((s) => (
                  <li key={s.index} className="group flex items-center gap-1 px-2.5 py-1.5 hover:bg-overlay">
                    <span className="min-w-0 flex-1 truncate" title={s.message}>
                      <span className="font-mono text-fg-subtle">{`{${s.index}}`}</span>
                      <span className="ml-1.5 text-fg">{s.message}</span>
                    </span>
                    <button
                      onClick={() => run(() => stashApply(repo, s.index), "已应用贮藏")}
                      disabled={busy}
                      title="应用(保留贮藏)"
                      className="shrink-0 rounded px-1.5 py-0.5 text-accent opacity-0 transition-opacity hover:bg-overlay group-hover:opacity-100 disabled:opacity-40"
                    >
                      应用
                    </button>
                    <button
                      onClick={() => run(() => stashPop(repo, s.index), "已弹出贮藏")}
                      disabled={busy}
                      title="弹出(应用并删除)"
                      className="shrink-0 rounded px-1.5 py-0.5 text-accent opacity-0 transition-opacity hover:bg-overlay group-hover:opacity-100 disabled:opacity-40"
                    >
                      弹出
                    </button>
                    <button
                      onClick={() => run(() => stashDrop(repo, s.index), "已删除贮藏")}
                      disabled={busy}
                      title="删除"
                      className="shrink-0 rounded p-1 text-fg-subtle opacity-0 transition-opacity hover:bg-overlay hover:text-danger group-hover:opacity-100 disabled:opacity-40"
                    >
                      <TrashIcon width={12} height={12} />
                    </button>
                  </li>
                ))
              )}
            </ul>
          </div>
        </>
      )}
    </div>
  );
}
