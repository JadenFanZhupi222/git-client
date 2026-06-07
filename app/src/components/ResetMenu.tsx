import { useState } from "react";
import { resetTo, type CommitDto, type ResetMode, type IpcError } from "../ipc";
import { useToast } from "./Toast";

const MODES: { mode: ResetMode; label: string; desc: string; danger?: boolean }[] = [
  { mode: "soft", label: "Soft", desc: "保留暂存区与工作区(改动转为已暂存)" },
  { mode: "mixed", label: "Mixed", desc: "保留工作区,重置暂存区(改动转为未暂存)" },
  { mode: "hard", label: "Hard", desc: "丢弃所有未提交改动", danger: true },
];

/** 把当前分支重置到选中提交。下拉选模式;Hard 破坏性,内联二次确认。
 *  onDone 让上层失效工作区/历史/状态。 */
export function ResetMenu({
  repo, commit, onDone,
}: {
  repo: string;
  commit: CommitDto;
  onDone: () => void;
}) {
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirmHard, setConfirmHard] = useState(false);

  function close() {
    setOpen(false);
    setConfirmHard(false);
  }

  async function run(mode: ResetMode) {
    setBusy(true);
    try {
      await resetTo(repo, commit.id, mode);
      onDone();
      toast({ kind: "success", title: `已 ${mode} reset 到 ${commit.short_id}` });
      close();
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="relative">
      <button
        onClick={() => (open ? close() : setOpen(true))}
        disabled={busy}
        title="把当前分支重置到此提交"
        className="rounded border border-line-strong bg-elevated px-1.5 py-0.5 text-[11px] normal-case tracking-normal text-fg-muted transition-colors hover:bg-overlay hover:text-fg disabled:opacity-40"
      >
        Reset
      </button>
      {open && (
        <>
          {/* 点击外部关闭 */}
          <div className="fixed inset-0 z-10" onClick={close} />
          <div className="absolute right-0 z-20 mt-1 w-60 overflow-hidden rounded-md border border-line-strong bg-elevated shadow-lg">
            {MODES.map((m) => (
              <button
                key={m.mode}
                disabled={busy}
                onClick={() => (m.danger ? setConfirmHard(true) : run(m.mode))}
                className={`block w-full px-3 py-2 text-left transition-colors hover:bg-overlay disabled:opacity-40 ${
                  m.danger ? "text-danger" : "text-fg"
                }`}
              >
                <div className="text-[12px] font-medium normal-case tracking-normal">{m.label}</div>
                <div className="text-[11px] text-fg-muted">{m.desc}</div>
              </button>
            ))}
            {confirmHard && (
              <div className="border-t border-line bg-danger/10 px-3 py-2 text-[11px]">
                <p className="mb-1.5 text-danger">Hard reset 会永久丢弃未提交改动,确认?</p>
                <div className="flex gap-2">
                  <button
                    disabled={busy}
                    onClick={() => run("hard")}
                    className="rounded border border-danger/50 px-2 py-0.5 text-danger hover:bg-danger/15 disabled:opacity-40"
                  >
                    确认丢弃
                  </button>
                  <button onClick={() => setConfirmHard(false)} className="text-fg-muted hover:underline">
                    取消
                  </button>
                </div>
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
}
