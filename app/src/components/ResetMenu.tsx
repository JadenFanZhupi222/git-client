import { useState } from "react";
import { resetTo, type ResetMode, type IpcError } from "../ipc";
import { useStatus } from "../lib/queries";
import { useToast } from "./Toast";
import { Button } from "./ui/Button";
import { ConfirmDialog } from "./ConfirmDialog";
import { useT } from "../lib/i18n";
import type { MessageKey } from "../lib/locales/zh";

const MODES: { mode: ResetMode; label: string; descKey: MessageKey; danger?: boolean }[] = [
  { mode: "soft", label: "Soft", descKey: "reset.softDesc" },
  { mode: "mixed", label: "Mixed", descKey: "reset.mixedDesc" },
  { mode: "hard", label: "Hard", descKey: "reset.hardDesc", danger: true },
];

/** 把当前分支重置到 commitId(提交 / reflog 条目皆可)。下拉选模式;
 *  Hard 破坏性,内联二次确认。label 为短 SHA,用于提示文案。
 *  onDone 让上层失效工作区/历史/状态。 */
export function ResetMenu({
  repo, commitId, label, onDone,
}: {
  repo: string;
  commitId: string;
  label: string;
  onDone: () => void;
}) {
  const t = useT();
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [confirmHard, setConfirmHard] = useState(false);
  // hard reset 会丢弃未提交改动:用工作区改动数做「将影响 N 项」预览。
  const dirty = useStatus(repo).data?.entries.length ?? 0;

  function close() {
    setOpen(false);
    setConfirmHard(false);
  }

  async function run(mode: ResetMode) {
    setBusy(true);
    try {
      await resetTo(repo, commitId, mode);
      onDone();
      toast({ kind: "success", title: t("reset.done", { mode, label }) });
      close();
    } catch (e) {
      toast({ kind: "error", title: (e as IpcError).message ?? String(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="relative">
      <Button
        variant="secondary"
        size="chip"
        onClick={() => (open ? close() : setOpen(true))}
        disabled={busy}
        title={t("reset.btnTitle")}
        className="normal-case tracking-normal"
      >
        {t("reset.label")}
      </Button>
      {open && (
        <>
          {/* 点击外部关闭 */}
          <div className="fixed inset-0 z-10" onClick={close} />
          <div className="absolute right-0 z-20 mt-1 w-60 overflow-hidden rounded-md border border-line-strong bg-elevated menu-in popover">
            {MODES.map((m) => (
              <button
                key={m.mode}
                disabled={busy}
                onClick={() => {
                  if (m.danger) {
                    setOpen(false);
                    setConfirmHard(true);
                  } else {
                    run(m.mode);
                  }
                }}
                className={`block w-full px-3 py-2 text-left transition-colors hover:bg-overlay disabled:opacity-40 ${
                  m.danger ? "text-danger" : "text-fg"
                }`}
              >
                <div className="text-[12px] font-medium normal-case tracking-normal">{m.label}</div>
                <div className="text-[11px] text-fg-muted">{t(m.descKey)}</div>
              </button>
            ))}
          </div>
        </>
      )}

      <ConfirmDialog
        open={confirmHard}
        title={t("reset.confirmTitle", { label })}
        message={t("reset.confirmMsg")}
        impactNote={dirty > 0 ? t("reset.impact", { n: dirty }) : undefined}
        confirmLabel={t("reset.confirmLabel")}
        busy={busy}
        onConfirm={() => run("hard")}
        onCancel={() => setConfirmHard(false)}
      />
    </div>
  );
}
