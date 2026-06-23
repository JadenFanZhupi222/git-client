import type { ReactNode } from "react";
import { Button } from "./ui/Button";
import { AlertIcon } from "./icons";
import { useT } from "../lib/i18n";

/**
 * 统一的破坏性操作二次确认弹窗(M2.2)。
 *
 * 所有「会丢工作」的操作(hard reset、删未合并分支、丢弃改动…)都走它,体验一致:
 * 标题 + 说明 + 可选「将影响 N 项」预览(列出受影响项)+ 危险确认按钮。
 * 居中模态;点遮罩 / 取消关闭;`busy` 时禁用按钮防重复提交。
 */
export function ConfirmDialog({
  open,
  title,
  message,
  items,
  impactNote,
  confirmLabel,
  busy = false,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  title: string;
  message?: ReactNode;
  /** 受影响项预览(如将丢失的提交摘要);为空则不显清单。 */
  items?: string[];
  /** 「将影响 N 项」一行提示文案;无影响信息时省略。 */
  impactNote?: string;
  confirmLabel?: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  if (!open) return null;
  return (
    <div
      className="overlay-in fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onClick={busy ? undefined : onCancel}
    >
      <div
        role="alertdialog"
        aria-label={title}
        className="panel-in popover flex max-h-[80vh] w-[420px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start gap-2.5 px-4 pt-4">
          <span className="mt-0.5 shrink-0 text-danger">
            <AlertIcon width={18} height={18} />
          </span>
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-fg">{title}</h2>
            {message && <div className="mt-1 text-xs text-fg-muted">{message}</div>}
          </div>
        </div>

        {impactNote && (
          <div className="mx-4 mt-3 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs text-danger">
            {impactNote}
          </div>
        )}

        {items && items.length > 0 && (
          <ul className="mx-4 mt-2 max-h-40 overflow-y-auto rounded-md border border-line bg-elevated px-3 py-1.5 text-[11px] text-fg-muted">
            {items.map((it, i) => (
              <li key={i} className="truncate py-0.5 font-mono" title={it}>
                {it}
              </li>
            ))}
          </ul>
        )}

        <div className="mt-4 flex justify-end gap-2 px-4 pb-4">
          <Button variant="ghost" size="md" disabled={busy} onClick={onCancel}>
            {t("confirm.cancel")}
          </Button>
          <Button variant="danger" size="md" disabled={busy} onClick={onConfirm}>
            {busy ? t("confirm.busy") : confirmLabel ?? t("confirm.ok")}
          </Button>
        </div>
      </div>
    </div>
  );
}
