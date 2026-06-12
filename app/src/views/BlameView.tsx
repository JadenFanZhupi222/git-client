import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useBlame } from "../lib/queries";
import { formatRelative } from "../lib/time";
import { FileDiffIcon, HistoryIcon, CloseIcon } from "../components/icons";
import { Button } from "../components/ui/Button";
import { IconButton } from "../components/ui/IconButton";
import { LineHistoryPanel } from "../components/LineHistoryPanel";
import type { IpcError } from "../ipc";

/** 把绝对路径转成仓库根相对路径(正斜杠);不在仓库内返回 null。 */
function toRepoRelative(repo: string, abs: string): string | null {
  const norm = (s: string) => s.replace(/\\/g, "/").replace(/\/+$/, "");
  const r = norm(repo);
  const a = norm(abs);
  if (a.toLowerCase().startsWith(r.toLowerCase() + "/")) return a.slice(r.length + 1);
  return null;
}

export function BlameView({ repo }: { repo: string }) {
  const [file, setFile] = useState<string | null>(null);
  const [pickError, setPickError] = useState<string | null>(null);
  // 行选择:anchor 是起点、focus 是当前端点(shift-点扩范围);范围 = min..max。
  const [sel, setSel] = useState<{ anchor: number; focus: number } | null>(null);
  const [historyRange, setHistoryRange] = useState<{ start: number; end: number } | null>(null);
  const q = useBlame(repo, file);

  // 切仓库清空选择
  useEffect(() => { setFile(null); setPickError(null); }, [repo]);
  // 切文件清空行选择
  useEffect(() => { setSel(null); }, [file]);

  // 点行:普通点 → 单行选中;shift-点 → 从 anchor 扩到该行。
  function clickLine(lineNo: number, shift: boolean) {
    setSel((prev) => (shift && prev ? { anchor: prev.anchor, focus: lineNo } : { anchor: lineNo, focus: lineNo }));
  }
  const selRange = sel ? { start: Math.min(sel.anchor, sel.focus), end: Math.max(sel.anchor, sel.focus) } : null;

  async function pick() {
    setPickError(null);
    const picked = await open({ multiple: false, directory: false, defaultPath: repo, title: "选择要追溯的文件" });
    if (typeof picked !== "string") return;
    const rel = toRepoRelative(repo, picked);
    if (!rel) { setPickError("请选择当前仓库内的文件"); return; }
    setFile(rel);
  }

  const lines = q.data ?? [];
  const queryErr = (q.error as IpcError | null)?.message ?? null;

  return (
    <div className="flex h-full flex-col">
      {/* 工具栏 */}
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-1.5">
        <Button variant="ghost" size="sm" onClick={pick}>
          <FileDiffIcon width={13} height={13} /> 选择文件
        </Button>
        {file && <span className="truncate font-mono text-xs text-fg" title={file}>{file}</span>}
        {selRange && (
          <div className="ml-auto flex shrink-0 items-center gap-1.5">
            <Button
              variant="secondary"
              size="chip"
              onClick={() => setHistoryRange(selRange)}
              title="查看选中行的演变史(git log -L)"
            >
              <HistoryIcon width={12} height={12} />
              {selRange.start === selRange.end ? `第 ${selRange.start} 行历史` : `第 ${selRange.start}–${selRange.end} 行历史`}
            </Button>
            <IconButton aria-label="清除选择" title="清除选择" onClick={() => setSel(null)}>
              <CloseIcon width={13} height={13} />
            </IconButton>
          </div>
        )}
      </div>

      {pickError && <p className="border-b border-line px-3 py-1.5 text-xs text-danger">{pickError}</p>}

      {/* 内容 */}
      {!file ? (
        <Center>选择一个文件查看逐行追溯(blame)</Center>
      ) : q.isLoading ? (
        <Center>加载中…</Center>
      ) : queryErr ? (
        // 超大 / 二进制文件等:后端返回友好错误,居中显示(而不是误报「空文件」)
        <Center>{queryErr}</Center>
      ) : lines.length === 0 ? (
        <Center>空文件或无追溯信息</Center>
      ) : (
        <div className="fade-in flex-1 overflow-auto font-mono text-[12px] leading-5">
          {lines.map((l, i) => {
            const prev = lines[i - 1];
            const newGroup = !prev || prev.commit_id !== l.commit_id;
            const uncommitted = l.commit_id === "";
            const selected = !!selRange && l.line_no >= selRange.start && l.line_no <= selRange.end;
            return (
              <div
                key={i}
                onClick={(e) => clickLine(l.line_no, e.shiftKey)}
                className={`flex cursor-pointer select-none items-stretch ${selected ? "bg-accent/15" : "hover:bg-elevated"}`}
              >
                {/* 提交信息 gutter:同一提交的连续行只在首行显示。
                    flex 布局让「作者名」成为唯一可截断项,sha 与时间 shrink-0 始终可见
                    —— 否则整体 truncate 会把末尾的时间切掉(仿 JetBrains 始终留时间)。 */}
                <div
                  className={`flex w-56 shrink-0 items-center gap-1.5 overflow-hidden border-r border-line px-2 ${newGroup ? "" : "opacity-0"} ${uncommitted ? "text-fg-subtle" : "text-fg-muted"}`}
                  title={uncommitted ? "未提交" : `${l.commit_id}\n${l.author_name} · ${formatRelative(l.timestamp)}`}
                >
                  {newGroup &&
                    (uncommitted ? (
                      <span>· 未提交</span>
                    ) : (
                      <>
                        <span className="shrink-0 text-accent">{l.short_id}</span>
                        <span className="min-w-0 flex-1 truncate">{l.author_name}</span>
                        <span className="shrink-0 text-fg-subtle">{formatRelative(l.timestamp)}</span>
                      </>
                    ))}
                </div>
                <span className="w-10 shrink-0 select-none px-1.5 text-right text-fg-subtle">{l.line_no}</span>
                <span className="flex-1 whitespace-pre pr-3 text-fg">{l.content || " "}</span>
              </div>
            );
          })}
        </div>
      )}

      {historyRange && file && (
        <LineHistoryPanel repo={repo} file={file} range={historyRange} onClose={() => setHistoryRange(null)} />
      )}
    </div>
  );
}

function Center({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 items-center justify-center p-4 text-center text-sm text-fg-subtle">
      {children}
    </div>
  );
}
