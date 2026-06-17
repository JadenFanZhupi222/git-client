import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { addRemote, removeRemote, renameRemote, type IpcError } from "../ipc";
import { useRemoteList, invalidateHistory, qk } from "../lib/queries";
import { useToast } from "./Toast";
import { Button } from "./ui/Button";
import { CloseIcon, CloudIcon, TrashIcon, SpinnerIcon } from "./icons";

/** 管理远程(add / remove / rename)。居中模态:列出远程(名 + URL,可改名/删除)+ 新增表单。
 *  写操作后失效 remoteList + remotes 选择器 + 历史(远程跟踪引用变了)。 */
export function RemoteManager({ repo, onClose }: { repo: string; onClose: () => void }) {
  const qc = useQueryClient();
  const toast = useToast();
  const listQ = useRemoteList(repo, true);
  const remotes = listQ.data ?? [];

  const [busy, setBusy] = useState(false);
  const [newName, setNewName] = useState("");
  const [newUrl, setNewUrl] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null); // 正在改名的远程
  const [renameTo, setRenameTo] = useState("");
  const [confirmDel, setConfirmDel] = useState<string | null>(null);

  // 写操作后:刷新本面板列表 + 顶栏远程选择器 + 历史(remove/rename 会动远程跟踪引用)。
  function invalidate() {
    qc.invalidateQueries({ queryKey: qk.remoteList(repo) });
    qc.invalidateQueries({ queryKey: qk.remotes(repo) });
    invalidateHistory(qc, repo);
    qc.invalidateQueries({ queryKey: qk.refs(repo) });
  }

  // Esc 关闭
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  function errText(e: unknown): string {
    const err = e as IpcError;
    switch (err.code) {
      case "REMOTE_EXISTS": return "已存在同名远程";
      case "REMOTE_NOT_FOUND": return "远程不存在(可能已被外部删除)";
      case "INVALID_REMOTE_NAME": return "远程名 / URL 无效";
      default: return err.message ?? String(e);
    }
  }

  async function doAdd() {
    const name = newName.trim();
    const url = newUrl.trim();
    if (!name || !url || busy) return;
    setBusy(true);
    try {
      await addRemote(repo, name, url);
      invalidate();
      toast({ kind: "success", title: `已添加远程 ${name}` });
      setNewName(""); setNewUrl("");
    } catch (e) {
      toast({ kind: "error", title: errText(e) });
    } finally {
      setBusy(false);
    }
  }

  async function doRename(oldName: string) {
    const to = renameTo.trim();
    if (!to || to === oldName) { setRenaming(null); return; }
    setBusy(true);
    try {
      await renameRemote(repo, oldName, to);
      invalidate();
      toast({ kind: "success", title: `已重命名为 ${to}` });
      setRenaming(null);
    } catch (e) {
      toast({ kind: "error", title: errText(e) });
    } finally {
      setBusy(false);
    }
  }

  async function doDelete(name: string) {
    setBusy(true);
    try {
      await removeRemote(repo, name);
      invalidate();
      toast({ kind: "success", title: `已删除远程 ${name}` });
    } catch (e) {
      toast({ kind: "error", title: errText(e) });
    } finally {
      setBusy(false);
      setConfirmDel(null);
    }
  }

  return (
    <div
      className="overlay-in fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onClick={busy ? undefined : onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="管理远程"
        className="panel-in popover flex max-h-[80vh] w-[480px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 头 */}
        <div className="flex items-center gap-2 border-b border-line px-4 py-3">
          <span className="text-accent"><CloudIcon width={16} height={16} /></span>
          <h2 className="text-sm font-semibold text-fg">管理远程</h2>
          <button
            onClick={onClose}
            aria-label="关闭"
            className="ml-auto grid h-6 w-6 place-items-center rounded text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
          >
            <CloseIcon width={13} height={13} />
          </button>
        </div>

        {/* 列表 */}
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
          {listQ.isLoading ? (
            <div className="flex items-center gap-2 py-6 text-xs text-fg-subtle">
              <SpinnerIcon width={13} height={13} /> 加载中…
            </div>
          ) : remotes.length === 0 ? (
            <p className="py-6 text-center text-xs text-fg-subtle">还没有远程,在下方添加一个。</p>
          ) : (
            <ul className="flex flex-col gap-1.5">
              {remotes.map((r) => (
                <li key={r.name} className="group rounded-md border border-line bg-elevated/50 px-3 py-2">
                  {renaming === r.name ? (
                    <form
                      onSubmit={(e) => { e.preventDefault(); doRename(r.name); }}
                      className="flex items-center gap-1.5"
                    >
                      <input
                        autoFocus
                        value={renameTo}
                        onChange={(e) => setRenameTo(e.target.value)}
                        onKeyDown={(e) => { if (e.key === "Escape") setRenaming(null); }}
                        placeholder="新名字"
                        className="min-w-0 flex-1 rounded bg-canvas px-2 py-1 font-mono text-xs text-fg field"
                      />
                      <Button type="submit" variant="commit" size="sm" disabled={busy || !renameTo.trim()}>保存</Button>
                      <Button type="button" variant="ghost" size="sm" onClick={() => setRenaming(null)}>取消</Button>
                    </form>
                  ) : confirmDel === r.name ? (
                    <div className="flex items-center gap-2 text-xs">
                      <span className="min-w-0 flex-1 truncate text-danger">删除远程「{r.name}」?其远程跟踪引用一并移除。</span>
                      <Button variant="danger" size="sm" disabled={busy} onClick={() => doDelete(r.name)}>删除</Button>
                      <Button variant="ghost" size="sm" onClick={() => setConfirmDel(null)}>取消</Button>
                    </div>
                  ) : (
                    <div className="flex items-center gap-2">
                      <div className="min-w-0 flex-1">
                        <div className="truncate font-mono text-xs font-medium text-fg">{r.name}</div>
                        <div className="truncate text-[11px] text-fg-subtle" title={r.url}>{r.url || "（无 URL）"}</div>
                      </div>
                      <button
                        onClick={() => { setRenaming(r.name); setRenameTo(r.name); }}
                        disabled={busy}
                        className="shrink-0 rounded px-1.5 py-0.5 text-[11px] text-fg-muted opacity-0 transition-opacity hover:bg-overlay hover:text-fg group-hover:opacity-100 disabled:opacity-40"
                      >
                        改名
                      </button>
                      <button
                        onClick={() => setConfirmDel(r.name)}
                        disabled={busy}
                        aria-label={`删除远程 ${r.name}`}
                        title="删除远程"
                        className="shrink-0 grid h-6 w-6 place-items-center rounded text-fg-muted opacity-0 transition-opacity hover:bg-danger/15 hover:text-danger group-hover:opacity-100 disabled:opacity-40"
                      >
                        <TrashIcon width={12} height={12} />
                      </button>
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>

        {/* 新增 */}
        <form onSubmit={(e) => { e.preventDefault(); doAdd(); }} className="flex flex-col gap-2 border-t border-line px-4 py-3">
          <div className="text-[11px] font-medium uppercase tracking-wide text-fg-subtle">添加远程</div>
          <div className="flex gap-2">
            <input
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="名字(如 origin)"
              className="w-32 rounded bg-canvas px-2 py-1.5 font-mono text-xs text-fg placeholder:text-fg-subtle field"
            />
            <input
              value={newUrl}
              onChange={(e) => setNewUrl(e.target.value)}
              placeholder="URL(https:// 或 git@…)"
              className="min-w-0 flex-1 rounded bg-canvas px-2 py-1.5 font-mono text-xs text-fg placeholder:text-fg-subtle field"
            />
            <Button type="submit" variant="commit" size="sm" disabled={busy || !newName.trim() || !newUrl.trim()} className="shrink-0">
              添加
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
}
