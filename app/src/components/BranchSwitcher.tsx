import { useEffect, useRef, useState } from "react";
import { listBranches, checkoutBranch, type BranchDto, type IpcError } from "../ipc";
import { BranchIcon, CheckIcon } from "./icons";

/**
 * 底栏分支切换器(VSCode 状态栏式):点当前分支名 → 向上弹出本地分支列表 →
 * 点选即 checkout。脏工作区冲突等错误就地提示,不打断。
 *
 * 切换成功后:① 立刻回调更新外层分支名(手感跟手);
 * ② 工作区/HEAD 变化会触发文件监听的 repo-changed,各视图自动重载。
 */
export function BranchSwitcher({
  repo,
  branch,
  onSwitched,
}: {
  repo: string;
  branch: string | null;
  onSwitched: (name: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [branches, setBranches] = useState<BranchDto[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null); // 正在 checkout 的分支名
  const [filter, setFilter] = useState("");
  const rootRef = useRef<HTMLDivElement>(null);

  // 打开时拉取分支列表
  useEffect(() => {
    if (!open) return;
    setLoading(true);
    setError(null);
    listBranches(repo)
      .then(setBranches)
      .catch((e) => setError((e as IpcError).message ?? String(e)))
      .finally(() => setLoading(false));
  }, [open, repo]);

  // Esc 关闭 + 打开时聚焦过滤框
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  function close() {
    setOpen(false);
    setFilter("");
    setError(null);
  }

  async function select(name: string) {
    if (name === branch) {
      close();
      return;
    }
    setBusy(name);
    setError(null);
    try {
      await checkoutBranch(repo, name);
      onSwitched(name);
      close();
    } catch (e) {
      setError((e as IpcError).message ?? String(e));
    } finally {
      setBusy(null);
    }
  }

  const shown = branches.filter((b) => b.name.toLowerCase().includes(filter.toLowerCase()));

  return (
    <div ref={rootRef} className="relative">
      <button
        onClick={() => (open ? close() : setOpen(true))}
        title="切换分支"
        className="flex items-center gap-1 rounded px-1 text-accent transition-colors hover:bg-overlay"
      >
        <BranchIcon width={12} height={12} />
        <span className="max-w-[12rem] truncate">{branch ?? "—"}</span>
        <Caret open={open} />
      </button>

      {open && (
        <>
          {/* 点击空白处关闭 */}
          <div className="fixed inset-0 z-40" onClick={close} />
          <div className="absolute bottom-full left-0 z-50 mb-1.5 w-64 overflow-hidden rounded-md border border-line-strong bg-elevated shadow-lg">
            <div className="border-b border-line p-1.5">
              <input
                autoFocus
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="筛选分支…"
                className="w-full rounded bg-canvas px-2 py-1 text-xs text-fg placeholder:text-fg-subtle focus:outline-none"
              />
            </div>

            {error && (
              <p className="border-b border-line px-2.5 py-1.5 text-[11px] text-danger">{error}</p>
            )}

            <ul className="max-h-72 overflow-y-auto py-1">
              {loading ? (
                <li className="px-2.5 py-1.5 text-xs text-fg-subtle">加载中…</li>
              ) : shown.length === 0 ? (
                <li className="px-2.5 py-1.5 text-xs text-fg-subtle">
                  {branches.length === 0 ? "没有本地分支" : "无匹配分支"}
                </li>
              ) : (
                shown.map((b) => {
                  const current = b.name === branch || b.is_head;
                  return (
                    <li key={b.name}>
                      <button
                        onClick={() => select(b.name)}
                        disabled={busy !== null}
                        className={`flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-overlay disabled:opacity-50 ${
                          current ? "text-fg" : "text-fg-muted"
                        }`}
                      >
                        <span className="grid w-3.5 shrink-0 place-items-center text-accent">
                          {current ? <CheckIcon width={12} height={12} /> : null}
                        </span>
                        <span className="truncate font-mono">{b.name}</span>
                        {busy === b.name && (
                          <span className="ml-auto text-[10px] text-fg-subtle">切换中…</span>
                        )}
                      </button>
                    </li>
                  );
                })
              )}
            </ul>
          </div>
        </>
      )}
    </div>
  );
}

function Caret({ open }: { open: boolean }) {
  return (
    <svg
      width={9}
      height={9}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`transition-transform ${open ? "rotate-180" : ""}`}
    >
      <path d="M4 6l4 4 4-4" />
    </svg>
  );
}
