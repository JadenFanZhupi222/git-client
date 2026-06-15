import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  checkoutBranch,
  createBranch,
  deleteBranch,
  branchDeleteImpact,
  type BranchDeleteImpactDto,
  type IpcError,
} from "../ipc";
import { useBranches, invalidateHistory } from "../lib/queries";
import { Button } from "./ui/Button";
import { IconButton } from "./ui/IconButton";
import { ConfirmDialog } from "./ConfirmDialog";
import { BranchIcon, CheckIcon, PlusIcon, TrashIcon } from "./icons";

/**
 * 底栏分支切换器(VSCode 状态栏式):点当前分支名 → 向上弹出本地分支列表。
 * 支持:点选 checkout、新建分支(建完即切)、删除分支(统一 ConfirmDialog 确认,
 * 删前查影响:未合并分支强警告并列出将丢失的提交)。
 * 分支列表走 useBranches(打开时拉);切换/新建/删除后失效查询,各视图自动重载。
 */
export function BranchSwitcher({
  repo,
  branch,
  direction = "up",
}: {
  repo: string;
  branch: string | null;
  /** 下拉弹出方向:底栏用 "up"(向上),顶栏用 "down"(向下)。 */
  direction?: "up" | "down";
}) {
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);
  const branchesQ = useBranches(repo, open);
  const branches = branchesQ.data ?? [];
  const loading = branchesQ.isLoading;
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null); // 正在操作的分支名
  const [filter, setFilter] = useState("");
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [checkingDelete, setCheckingDelete] = useState<string | null>(null); // 正在查影响的分支
  const [pendingDelete, setPendingDelete] = useState<{ name: string; impact: BranchDeleteImpactDto | null } | null>(null);
  const newInputRef = useRef<HTMLInputElement>(null);

  // checkout/create/delete 后失效:分支列表 + 历史/当前分支/同步状态。
  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ["branches", repo] });
    invalidateHistory(qc, repo);
  };

  // Esc 关闭
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // 进入新建态时聚焦输入框
  useEffect(() => {
    if (creating) newInputRef.current?.focus();
  }, [creating]);

  function close() {
    setOpen(false);
    setFilter("");
    setError(null);
    setCreating(false);
    setNewName("");
    setPendingDelete(null);
    setCheckingDelete(null);
  }

  async function select(name: string, isCurrent: boolean) {
    // 已是当前分支(branch prop 可能短暂滞后,故一并看 is_head)→ 不重复 checkout
    if (isCurrent) return close();
    setBusy(name);
    setError(null);
    try {
      await checkoutBranch(repo, name);
      invalidate();
      close();
    } catch (e) {
      setError((e as IpcError).message ?? String(e));
    } finally {
      setBusy(null);
    }
  }

  async function create(e: React.FormEvent) {
    e.preventDefault();
    const name = newName.trim();
    if (!name) return;
    setBusy(name);
    setError(null);
    try {
      await createBranch(repo, name, true); // 建完即切
      invalidate();
      close();
    } catch (e) {
      // create+checkout 非原子:可能「已建未切」(如脏工作区切换失败)。
      // 失效让已创建的分支显形,配合错误信息,用户能看清真实状态。
      setError((e as IpcError).message ?? String(e));
      invalidate();
    } finally {
      setBusy(null);
    }
  }

  // 点删除图标:先查影响(会丢多少提交),再开统一确认弹窗。
  async function requestDelete(name: string) {
    setCheckingDelete(name);
    setError(null);
    try {
      const impact = await branchDeleteImpact(repo, name);
      setPendingDelete({ name, impact });
    } catch {
      // 查影响失败(如分支已被外部删)→ 仍给一个保守确认,不直接删。
      setPendingDelete({ name, impact: null });
    } finally {
      setCheckingDelete(null);
    }
  }

  async function doDelete(name: string) {
    setBusy(name);
    setError(null);
    try {
      await deleteBranch(repo, name);
      setPendingDelete(null);
      invalidate();
    } catch (e) {
      setError((e as IpcError).message ?? String(e));
      setPendingDelete(null);
    } finally {
      setBusy(null);
    }
  }

  const shown = branches.filter((b) => b.name.toLowerCase().includes(filter.toLowerCase()));

  return (
    <div className="relative">
      <button
        onClick={() => (open ? close() : setOpen(true))}
        title="切换分支"
        className="flex items-center gap-1 rounded px-1 text-accent transition-colors hover:bg-overlay"
      >
        <BranchIcon width={12} height={12} />
        <span className="max-w-48 truncate">{branch ?? "—"}</span>
        <Caret open={open} />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={close} />
          <div className={`absolute left-0 z-50 w-64 overflow-hidden rounded-md border border-line-strong bg-elevated menu-in popover ${direction === "down" ? "top-full mt-1.5" : "bottom-full mb-1.5"}`}>
            <div className="border-b border-line p-1.5">
              <input
                autoFocus
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="筛选分支…"
                className="w-full rounded bg-canvas px-2 py-1 text-xs text-fg placeholder:text-fg-subtle field"
              />
            </div>

            {error && (
              <p className="border-b border-line px-2.5 py-1.5 text-[11px] text-danger">{error}</p>
            )}

            <ul className="max-h-64 overflow-y-auto py-1">
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
                    <li key={b.name} className="group flex items-center pr-1.5">
                      <button
                        onClick={() => select(b.name, current)}
                        disabled={busy !== null}
                        className={`flex min-w-0 flex-1 items-center gap-2 px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-overlay disabled:opacity-50 ${
                          current ? "text-fg" : "text-fg-muted"
                        }`}
                      >
                        <span className="grid w-3.5 shrink-0 place-items-center text-accent">
                          {current ? <CheckIcon width={12} height={12} /> : null}
                        </span>
                        <span className="truncate font-mono">{b.name}</span>
                      </button>

                      {/* 当前分支不可删;点删除 → 查影响 → 统一确认弹窗 */}
                      {!current && (
                        <IconButton
                          aria-label={`删除分支 ${b.name}`}
                          tone="danger"
                          onClick={() => requestDelete(b.name)}
                          disabled={busy !== null || checkingDelete !== null}
                          title="删除分支"
                          className="shrink-0 p-1 opacity-0 transition-opacity group-hover:opacity-100"
                        >
                          <TrashIcon width={12} height={12} />
                        </IconButton>
                      )}
                    </li>
                  );
                })
              )}
            </ul>

            {/* 新建分支 */}
            <div className="border-t border-line p-1.5">
              {creating ? (
                <form onSubmit={create} className="flex items-center gap-1">
                  <input
                    ref={newInputRef}
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    placeholder="新分支名(基于当前 HEAD)…"
                    className="min-w-0 flex-1 rounded bg-canvas px-2 py-1 font-mono text-xs text-fg placeholder:text-fg-subtle field"
                  />
                  <Button type="submit" variant="commit" size="sm" disabled={!newName.trim() || busy !== null} className="shrink-0">
                    创建
                  </Button>
                </form>
              ) : (
                <button
                  onClick={() => setCreating(true)}
                  className="flex w-full items-center gap-1.5 rounded px-1.5 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
                >
                  <PlusIcon width={12} height={12} />
                  新建分支
                </button>
              )}
            </div>
          </div>
        </>
      )}

      {pendingDelete && (() => {
        const { name, impact } = pendingDelete;
        const unmerged = impact?.unmerged_commits ?? 0;
        const danger = impact === null || unmerged > 0;
        return (
          <ConfirmDialog
            open
            title={`删除分支 “${name}”?`}
            message={
              impact === null
                ? "无法确认该分支是否已并入别处，请谨慎删除。"
                : unmerged > 0
                  ? undefined
                  : "该分支的提交已并入别处，删除不会丢失工作。"
            }
            impactNote={
              unmerged > 0
                ? `该分支有 ${unmerged} 个提交未合并到任何其它分支，删除后将丢失。`
                : undefined
            }
            items={unmerged > 0 ? impact!.sample_summaries : undefined}
            confirmLabel={danger ? "仍要删除" : "删除"}
            busy={busy === name}
            onConfirm={() => doDelete(name)}
            onCancel={() => setPendingDelete(null)}
          />
        );
      })()}
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
