import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useBlame } from "../lib/queries";
import { formatRelative } from "../lib/time";
import { FileDiffIcon } from "../components/icons";
import { Button } from "../components/ui/Button";
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
  const q = useBlame(repo, file);

  // 切仓库清空选择
  useEffect(() => { setFile(null); setPickError(null); }, [repo]);

  async function pick() {
    setPickError(null);
    const picked = await open({ multiple: false, directory: false, defaultPath: repo, title: "选择要追溯的文件" });
    if (typeof picked !== "string") return;
    const rel = toRepoRelative(repo, picked);
    if (!rel) { setPickError("请选择当前仓库内的文件"); return; }
    setFile(rel);
  }

  const lines = q.data ?? [];
  const err = pickError ?? (q.error as IpcError | null)?.message ?? null;

  return (
    <div className="flex h-full flex-col">
      {/* 工具栏 */}
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-1.5">
        <Button variant="ghost" size="sm" onClick={pick}>
          <FileDiffIcon width={13} height={13} /> 选择文件
        </Button>
        {file && <span className="truncate font-mono text-xs text-fg" title={file}>{file}</span>}
      </div>

      {err && <p className="border-b border-line px-3 py-1.5 text-xs text-danger">{err}</p>}

      {/* 内容 */}
      {!file ? (
        <Center>选择一个文件查看逐行追溯(blame)</Center>
      ) : q.isLoading ? (
        <Center>加载中…</Center>
      ) : lines.length === 0 ? (
        <Center>空文件或无追溯信息</Center>
      ) : (
        <div className="fade-in flex-1 overflow-auto font-mono text-[12px] leading-5">
          {lines.map((l, i) => {
            const prev = lines[i - 1];
            const newGroup = !prev || prev.commit_id !== l.commit_id;
            const uncommitted = l.commit_id === "";
            return (
              <div key={i} className="flex items-stretch hover:bg-elevated">
                {/* 提交信息 gutter:同一提交的连续行只在首行显示 */}
                <div
                  className={`w-48 shrink-0 truncate border-r border-line px-2 ${newGroup ? "" : "opacity-0"} ${uncommitted ? "text-fg-subtle" : "text-fg-muted"}`}
                  title={uncommitted ? "未提交" : `${l.commit_id}\n${l.author_name}`}
                >
                  {newGroup && (
                    uncommitted ? "· 未提交"
                      : <><span className="text-accent">{l.short_id}</span> {l.author_name} · {formatRelative(l.timestamp)}</>
                  )}
                </div>
                <span className="w-10 shrink-0 select-none px-1.5 text-right text-fg-subtle">{l.line_no}</span>
                <span className="flex-1 whitespace-pre pr-3 text-fg">{l.content || " "}</span>
              </div>
            );
          })}
        </div>
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
