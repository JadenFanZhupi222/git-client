import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  checkoutBranch,
  createBranch,
  deleteBranch,
  branchDeleteImpact,
  mergeBranch,
  type BranchDeleteImpactDto,
  type IpcError,
} from "../ipc";
import { useBranches, invalidateHistory, invalidateWorktree } from "../lib/queries";
import { Button } from "./ui/Button";
import { IconButton } from "./ui/IconButton";
import { ConfirmDialog } from "./ConfirmDialog";
import { useToast } from "./Toast";
import { BranchIcon, CheckIcon, PlusIcon, TrashIcon, MergeIcon } from "./icons";
import { useT } from "../lib/i18n";

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
  const t = useT();
  const qc = useQueryClient();
  const toast = useToast();
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

  // 把某分支合并进当前分支:成功 toast 摘要;冲突时失效并提示去「更改」页解决;
  // 其它错误内联显示。合并改工作区 + 历史,两域都失效。
  async function doMerge(name: string) {
    setBusy(name);
    setError(null);
    try {
      const res = await mergeBranch(repo, name);
      invalidate();
      invalidateWorktree(qc, repo);
      toast({
        kind: "success",
        title: t("branch.merged", { name }),
        detail: res.fast_forward ? t("branch.ff") : res.summary?.split("\n")[0],
      });
      close();
    } catch (e) {
      const err = e as IpcError;
      if (err.code === "MERGE_CONFLICT") {
        // 进入 merging 状态:失效让冲突横幅/状态显形,引导去「更改」页解决。
        invalidate();
        invalidateWorktree(qc, repo);
        qc.invalidateQueries({ queryKey: ["repoState", repo] });
        toast({ kind: "error", title: t("branch.mergeConflict"), detail: t("branch.mergeConflictDetail") });
        close();
      } else {
        setError(err.message ?? String(e));
      }
    } finally {
      setBusy(null);
    }
  }

  const shown = branches.filter((b) => b.name.toLowerCase().includes(filter.toLowerCase()));

  return (
    <div className="relative">
      <button
        onClick={() => (open ? close() : setOpen(true))}
        title={t("branch.switch")}
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
                placeholder={t("branch.filter")}
                className="w-full rounded bg-canvas px-2 py-1 text-xs text-fg placeholder:text-fg-subtle field"
              />
            </div>

            {error && (
              <p className="border-b border-line px-2.5 py-1.5 text-[11px] text-danger">{error}</p>
            )}

            <ul className="max-h-64 overflow-y-auto py-1">
              {loading ? (
                <li className="px-2.5 py-1.5 text-xs text-fg-subtle">{t("common.loading")}</li>
              ) : shown.length === 0 ? (
                <li className="px-2.5 py-1.5 text-xs text-fg-subtle">
                  {branches.length === 0 ? t("branch.none") : t("branch.noMatch")}
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

                      {/* 非当前分支:合并进当前分支 + 删除(查影响→确认),悬浮显现 */}
                      {!current && (
                        <IconButton
                          aria-label={t("branch.mergeAria", { name: b.name })}
                          onClick={() => doMerge(b.name)}
                          disabled={busy !== null || checkingDelete !== null}
                          title={t("branch.mergeTitle", { name: b.name, cur: branch ? `(${branch})` : "" })}
                          className="shrink-0 p-1 opacity-0 transition-opacity group-hover:opacity-100"
                        >
                          <MergeIcon width={12} height={12} />
                        </IconButton>
                      )}
                      {!current && (
                        <IconButton
                          aria-label={t("branch.deleteAria", { name: b.name })}
                          tone="danger"
                          onClick={() => requestDelete(b.name)}
                          disabled={busy !== null || checkingDelete !== null}
                          title={t("branch.deleteTitle")}
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
                    placeholder={t("branch.newPlaceholder")}
                    className="min-w-0 flex-1 rounded bg-canvas px-2 py-1 font-mono text-xs text-fg placeholder:text-fg-subtle field"
                  />
                  <Button type="submit" variant="commit" size="sm" disabled={!newName.trim() || busy !== null} className="shrink-0">
                    {t("branch.create")}
                  </Button>
                </form>
              ) : (
                <button
                  onClick={() => setCreating(true)}
                  className="flex w-full items-center gap-1.5 rounded px-1.5 py-1 text-xs text-fg-muted transition-colors hover:bg-overlay hover:text-fg"
                >
                  <PlusIcon width={12} height={12} />
                  {t("branch.new")}
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
            title={t("branch.deleteTitleQ", { name })}
            message={
              impact === null
                ? t("branch.deleteUnknown")
                : unmerged > 0
                  ? undefined
                  : t("branch.deleteSafe")
            }
            impactNote={unmerged > 0 ? t("branch.deleteImpact", { n: unmerged }) : undefined}
            items={unmerged > 0 ? impact!.sample_summaries : undefined}
            confirmLabel={danger ? t("branch.deleteForce") : t("branch.delete")}
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
