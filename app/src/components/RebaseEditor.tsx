import { Fragment, useRef, useState } from "react";
import { interactiveRebase, type CommitDto, type RebaseActionKind, type RebaseStepInput, type IpcError } from "../ipc";
import { moveItem } from "../lib/listNav";
import { useToast } from "./Toast";
import { Button } from "./ui/Button";
import { IconButton } from "./ui/IconButton";
import { CloseIcon, GripIcon } from "./icons";
import { useT } from "../lib/i18n";
import type { MessageKey } from "../lib/locales/zh";

type Row = { sha: string; short: string; summary: string; action: RebaseActionKind; message: string };

const ACTIONS: { value: RebaseActionKind; labelKey: MessageKey }[] = [
  { value: "pick", labelKey: "rebase.actPick" },
  { value: "reword", labelKey: "rebase.actReword" },
  { value: "squash", labelKey: "rebase.actSquash" },
  { value: "fixup", labelKey: "rebase.actFixup" },
  { value: "drop", labelKey: "rebase.actDrop" },
];

/** 交互式 rebase 编辑器。commits 按 oldest→newest;base=最旧提交的父 SHA(null→--root)。 */
export function RebaseEditor({
  repo, commits, base, onClose, onConflict, onDone,
}: {
  repo: string;
  commits: CommitDto[];
  base: string | null;
  onClose: () => void;
  onConflict: () => void;
  onDone: () => void;
}) {
  const t = useT();
  const toast = useToast();
  const [rows, setRows] = useState<Row[]>(
    commits.map((c) => ({ sha: c.id, short: c.short_id, summary: c.summary, action: "pick", message: c.summary })),
  );
  const [busy, setBusy] = useState(false);
  // 指针拖拽重排(不用 HTML5 DnD——Tauri/WebView 的系统级拖放会吞掉那些事件)。
  const [fromIndex, setFromIndex] = useState<number | null>(null); // 正在拖的行(用于变淡)
  const [overIndex, setOverIndex] = useState<number | null>(null); // 插入位置 0..length(用于插入线)
  const [preview, setPreview] = useState<{ x: number; y: number; short: string; summary: string } | null>(null);
  const overRef = useRef<number | null>(null); // onUp 读最新插入位置,避免闭包陈旧
  const listRef = useRef<HTMLDivElement>(null);

  function move(i: number, dir: -1 | 1) {
    setRows((rs) => moveItem(rs, i, i + dir));
  }

  // 从拖拽手柄按下:挂 window 指针监听,实时算插入位置 + 跟随预览,松手落定。
  function startDrag(i: number, e: React.PointerEvent) {
    e.preventDefault();
    setFromIndex(i);
    setPreview({ x: e.clientX, y: e.clientY, short: rows[i].short, summary: rows[i].summary });

    const onMove = (ev: PointerEvent) => {
      const items = listRef.current?.querySelectorAll<HTMLElement>("[data-row-index]");
      if (items) {
        let target = items.length; // 默认落到末尾
        for (let k = 0; k < items.length; k++) {
          const rect = items[k].getBoundingClientRect();
          if (ev.clientY < rect.top + rect.height / 2) {
            target = k;
            break;
          }
        }
        overRef.current = target;
        setOverIndex(target);
      }
      setPreview((p) => (p ? { ...p, x: ev.clientX, y: ev.clientY } : p));
    };
    const onUp = () => {
      const to = overRef.current;
      if (to !== null) {
        // to 是「插入到第 to 项之前」;移除 from 后下标左移,故 from<to 时目标 -1。
        const finalTo = i < to ? to - 1 : to;
        setRows((rs) => moveItem(rs, i, finalTo));
      }
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      overRef.current = null;
      setFromIndex(null);
      setOverIndex(null);
      setPreview(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }
  function setAction(i: number, action: RebaseActionKind) {
    setRows((rs) => rs.map((r, k) => (k === i ? { ...r, action } : r)));
  }
  function setMessage(i: number, message: string) {
    setRows((rs) => rs.map((r, k) => (k === i ? { ...r, message } : r)));
  }

  const firstKept = rows.find((r) => r.action !== "drop");
  const firstKeptInvalid = firstKept && (firstKept.action === "fixup" || firstKept.action === "squash");
  const allDropped = !firstKept;
  const canStart = !busy && !firstKeptInvalid && !allDropped;

  async function start() {
    setBusy(true);
    const steps: RebaseStepInput[] = rows.map((r) => ({
      sha: r.sha,
      action: r.action,
      message: r.action === "reword" || r.action === "squash" ? r.message : undefined,
    }));
    try {
      await interactiveRebase(repo, base, steps);
      toast({ kind: "success", title: t("rebase.done") });
      onDone();
    } catch (e) {
      const err = e as IpcError;
      if (err.code === "MERGE_CONFLICT") {
        toast({ kind: "error", title: t("rebase.conflict") });
        onConflict();
      } else {
        toast({ kind: "error", title: err.message ?? String(e) });
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="overlay-in fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6" onClick={onClose}>
      <div
        className="panel-in popover flex max-h-[80vh] w-[640px] flex-col overflow-hidden rounded-lg border border-line-strong bg-canvas"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex shrink-0 items-center justify-between border-b border-line px-4 py-3">
          <h2 className="text-sm font-semibold text-fg">{t("rebase.title", { n: rows.length })}</h2>
          <IconButton aria-label={t("toast.close")} onClick={onClose}><CloseIcon width={15} height={15} /></IconButton>
        </div>

        <p className="shrink-0 border-b border-line bg-warning/10 px-4 py-2 text-xs text-warning">
          {t("rebase.warn")}
        </p>

        <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto p-2">
          {rows.map((r, i) => (
            <Fragment key={r.sha}>
              <InsertLine active={overIndex === i} />
              <div
                data-row-index={i}
                className={`rounded px-2 py-1.5 transition-opacity ${fromIndex === i ? "opacity-40" : "hover:bg-elevated"}`}
              >
                <div className="flex items-center gap-2">
                  {/* 拖拽手柄:从这里按下可拖动整行重排(指针拖拽,自带预览) */}
                  <span
                    onPointerDown={(e) => startDrag(i, e)}
                    title={t("rebase.dragReorder")}
                    className="shrink-0 cursor-grab touch-none text-fg-subtle hover:text-fg active:cursor-grabbing"
                  >
                    <GripIcon width={14} height={14} />
                  </span>
                  <div className="flex shrink-0 flex-col">
                    <button onClick={() => move(i, -1)} disabled={i === 0} className="leading-none text-fg-subtle hover:text-fg disabled:opacity-30">▲</button>
                    <button onClick={() => move(i, 1)} disabled={i === rows.length - 1} className="leading-none text-fg-subtle hover:text-fg disabled:opacity-30">▼</button>
                  </div>
                  <select
                    value={r.action}
                    onChange={(e) => setAction(i, e.target.value as RebaseActionKind)}
                    className="shrink-0 rounded border border-line-strong bg-canvas px-1.5 py-1 text-xs text-fg field"
                  >
                    {ACTIONS.map((a) => <option key={a.value} value={a.value}>{t(a.labelKey)}</option>)}
                  </select>
                  <span className="shrink-0 font-mono text-[11px] text-accent">{r.short}</span>
                  <span className={`min-w-0 flex-1 truncate text-[13px] ${r.action === "drop" ? "text-fg-subtle line-through" : "text-fg"}`} title={r.summary}>
                    {r.summary}
                  </span>
                </div>
                {(r.action === "reword" || r.action === "squash") && (
                  <input
                    value={r.message}
                    onChange={(e) => setMessage(i, e.target.value)}
                    placeholder={t("rebase.newMessage")}
                    className="mt-1 ml-7 w-[calc(100%-1.75rem)] rounded border border-line-strong bg-canvas px-2 py-1 text-xs text-fg field"
                  />
                )}
              </div>
            </Fragment>
          ))}
          {/* 末尾插入位 */}
          <InsertLine active={overIndex === rows.length} />
        </div>

        <div className="flex shrink-0 items-center justify-between border-t border-line px-4 py-3">
          <span className="text-xs text-danger">
            {firstKeptInvalid ? t("rebase.errFirstSquash") : allDropped ? t("rebase.errAllDropped") : ""}
          </span>
          <div className="flex gap-2">
            <Button variant="secondary" size="md" onClick={onClose}>{t("confirm.cancel")}</Button>
            <Button variant="primary" size="md" onClick={start} disabled={!canStart}>
              {t("rebase.start")}
            </Button>
          </div>
        </div>
      </div>

      {/* 拖拽预览:跟随指针的浮层(fixed 不被 modal overflow 裁剪) */}
      {preview && (
        <div
          className="pointer-events-none fixed z-[60] flex max-w-[20rem] items-center gap-2 rounded border border-accent/50 bg-elevated px-2 py-1 text-[13px] popover"
          style={{ left: preview.x + 12, top: preview.y + 8 }}
        >
          <span className="shrink-0 font-mono text-[11px] text-accent">{preview.short}</span>
          <span className="min-w-0 truncate text-fg">{preview.summary}</span>
        </div>
      )}
    </div>
  );
}

/** 行间插入线:拖拽时高亮目标插入位。透明占位避免布局抖动。 */
function InsertLine({ active }: { active: boolean }) {
  return <div className={`mx-2 h-0.5 rounded transition-colors ${active ? "bg-accent" : "bg-transparent"}`} />;
}
