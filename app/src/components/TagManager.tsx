import { useState } from "react";
import { createTag, deleteTag, type CommitDto, type IpcError } from "../ipc";
import { useToast } from "./Toast";
import { CloseIcon } from "./icons";

/** 选中提交的标签管理:展示已有标签(可删,二次确认)+ 新建标签(名 + 可选附注信息)。
 *  挂在「提交详情」头。onChanged 让上层失效历史/图谱(refs 变了)。 */
export function TagManager({
  repo, commit, tags, onChanged,
}: {
  repo: string;
  commit: CommitDto;
  tags: string[];
  onChanged: () => void;
}) {
  const toast = useToast();
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmDel, setConfirmDel] = useState<string | null>(null);

  async function doCreate() {
    if (!name.trim() || busy) return;
    setBusy(true);
    try {
      await createTag(repo, name.trim(), commit.id, msg);
      onChanged();
      toast({ kind: "success", title: `已创建标签 ${name.trim()}` });
      setName(""); setMsg(""); setAdding(false);
    } catch (e) {
      const err = e as IpcError;
      toast({ kind: "error", title: err.code === "TAG_EXISTS" ? "标签已存在" : (err.message ?? String(e)) });
    } finally {
      setBusy(false);
    }
  }

  async function doDelete(n: string) {
    setBusy(true);
    try {
      await deleteTag(repo, n);
      onChanged();
      toast({ kind: "success", title: `已删除标签 ${n}` });
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
      setConfirmDel(null);
    }
  }

  const pill = "inline-flex items-center gap-1 rounded-full border border-warning/40 bg-warning/10 px-1.5 text-[10px] font-mono leading-[1.5] text-warning";

  return (
    <div className="flex flex-wrap items-center gap-1">
      {tags.map((t) =>
        confirmDel === t ? (
          <span key={t} className={pill}>
            删除 {t}?
            <button onClick={() => doDelete(t)} disabled={busy} className="text-danger hover:underline">✓</button>
            <button onClick={() => setConfirmDel(null)} className="text-fg-muted hover:underline">✗</button>
          </span>
        ) : (
          <span key={t} className={pill}>
            ⌖ {t}
            <button
              onClick={() => setConfirmDel(t)}
              title="删除此标签"
              className="text-warning/70 hover:text-danger"
            >
              <CloseIcon width={9} height={9} />
            </button>
          </span>
        )
      )}

      {adding ? (
        <span className="flex items-center gap-1">
          <input
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") doCreate(); if (e.key === "Escape") setAdding(false); }}
            placeholder="标签名"
            className="w-24 rounded border border-line-strong bg-canvas px-1.5 py-0.5 text-[11px] normal-case tracking-normal text-fg focus:border-accent focus:outline-none"
          />
          <input
            value={msg}
            onChange={(e) => setMsg(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") doCreate(); if (e.key === "Escape") setAdding(false); }}
            placeholder="附注(可选)"
            className="w-28 rounded border border-line-strong bg-canvas px-1.5 py-0.5 text-[11px] normal-case tracking-normal text-fg focus:border-accent focus:outline-none"
          />
          <button onClick={doCreate} disabled={busy || !name.trim()} className="text-[11px] text-accent hover:underline disabled:opacity-40">创建</button>
          <button onClick={() => setAdding(false)} className="text-[11px] text-fg-muted hover:underline">取消</button>
        </span>
      ) : (
        <button
          onClick={() => setAdding(true)}
          title="在此提交上打标签"
          className="rounded border border-line-strong bg-elevated px-1.5 py-0.5 text-[11px] normal-case tracking-normal text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
        >
          + Tag
        </button>
      )}
    </div>
  );
}
