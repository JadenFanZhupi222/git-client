// 集中定义所有 IPC「读」的 React Query hooks + query key + 失效辅助。
// 组件不再各自手写 useEffect 拉数据 / loading-error 布尔;写操作后调
// invalidate* 触发重取,外部文件变化由 useRepoWatch 一处监听失效。

import { useQuery, useQueryClient, keepPreviousData, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import {
  getStatus, getWorkingDiff, getCommitGraph, searchCommits, getReflog, getCommitFiles, getCommitFileDiff,
  getCommitSignature, compareFiles, compareFileDiff,
  getCurrentBranch, getAheadBehind, getRemotes, listBranches, listRefs, stashList,
  getRepoState, readWorkingFile, blame, conflictSides, undoState, opLog,
  watchRepo, onRepoChanged,
} from "../ipc";

// ---- query keys(数组前缀便于部分匹配失效)----
export const qk = {
  status: (repo: string) => ["status", repo] as const,
  workingDiff: (repo: string) => ["workingDiff", repo] as const,
  graph: (repo: string) => ["graph", repo] as const,
  commitFiles: (repo: string) => ["commitFiles", repo] as const,
  commitDiff: (repo: string) => ["commitDiff", repo] as const,
  commitSignature: (repo: string) => ["commitSignature", repo] as const,
  currentBranch: (repo: string) => ["currentBranch", repo] as const,
  aheadBehind: (repo: string) => ["aheadBehind", repo] as const,
  remotes: (repo: string) => ["remotes", repo] as const,
  branches: (repo: string) => ["branches", repo] as const,
  stashes: (repo: string) => ["stashes", repo] as const,
  repoState: (repo: string) => ["repoState", repo] as const,
  fileText: (repo: string) => ["fileText", repo] as const,
  blame: (repo: string) => ["blame", repo] as const,
  conflictSides: (repo: string) => ["conflictSides", repo] as const,
  search: (repo: string) => ["search", repo] as const,
  reflog: (repo: string) => ["reflog", repo] as const,
  compareFiles: (repo: string) => ["compareFiles", repo] as const,
  compareDiff: (repo: string) => ["compareDiff", repo] as const,
  refs: (repo: string) => ["refs", repo] as const,
  undoState: (repo: string) => ["undoState", repo] as const,
  opLog: (repo: string) => ["opLog", repo] as const,
};

// ---- 读 hooks ----
export function useStatus(repo: string) {
  return useQuery({ queryKey: qk.status(repo), queryFn: () => getStatus(repo) });
}

export function useWorkingDiff(repo: string, file: string | null, staged: boolean) {
  return useQuery({
    queryKey: [...qk.workingDiff(repo), file ?? "", staged],
    queryFn: () => getWorkingDiff(repo, file!, staged),
    enabled: !!file,
  });
}

export function useGraph(repo: string, limit: number) {
  return useQuery({
    queryKey: [...qk.graph(repo), limit],
    queryFn: () => getCommitGraph(repo, limit),
    placeholderData: keepPreviousData, // 「加载更多」时保留旧行,不闪空
  });
}

/** 提交搜索:query 为空时不发请求(enabled=false),结果为扁平列表。 */
export function useCommitSearch(repo: string, query: string, limit: number) {
  const q = query.trim();
  return useQuery({
    queryKey: [...qk.search(repo), q, limit],
    queryFn: () => searchCommits(repo, q, limit),
    enabled: !!repo && q.length > 0,
    placeholderData: keepPreviousData, // 连续输入时不闪空
  });
}

/** HEAD 的 reflog;enabled 控制按需取(仅面板打开时拉)。 */
export function useReflog(repo: string, limit: number, enabled: boolean) {
  return useQuery({
    queryKey: [...qk.reflog(repo), limit],
    queryFn: () => getReflog(repo, limit),
    enabled: enabled && !!repo,
  });
}

/** 两 revision 间改动文件;from/to 任一为空则不查。 */
export function useCompareFiles(repo: string, from: string, to: string) {
  return useQuery({
    queryKey: [...qk.compareFiles(repo), from, to],
    queryFn: () => compareFiles(repo, from, to),
    enabled: !!repo && !!from && !!to,
  });
}

/** 两 revision 间单文件 diff;需 from/to/file 都有。 */
export function useCompareDiff(repo: string, from: string, to: string, file: string | null) {
  return useQuery({
    queryKey: [...qk.compareDiff(repo), from, to, file ?? ""],
    queryFn: () => compareFileDiff(repo, from, to, file!),
    enabled: !!repo && !!from && !!to && !!file,
  });
}

export function useCommitFiles(repo: string, commitId: string | null) {
  return useQuery({
    queryKey: [...qk.commitFiles(repo), commitId ?? ""],
    queryFn: () => getCommitFiles(repo, commitId!),
    enabled: !!commitId,
  });
}

export function useCommitSignature(repo: string, commitId: string | null) {
  return useQuery({
    queryKey: [...qk.commitSignature(repo), commitId ?? ""],
    queryFn: () => getCommitSignature(repo, commitId!),
    enabled: !!repo && !!commitId,
  });
}

export function useCommitDiff(repo: string, commitId: string | null, file: string | null) {
  return useQuery({
    queryKey: [...qk.commitDiff(repo), commitId ?? "", file ?? ""],
    queryFn: () => getCommitFileDiff(repo, commitId!, file!),
    enabled: !!commitId && !!file,
  });
}

export function useCurrentBranch(repo: string) {
  return useQuery({ queryKey: qk.currentBranch(repo), queryFn: () => getCurrentBranch(repo), enabled: !!repo });
}

/** 撤销/重做的当前可用性。随历史变化失效;驱动顶栏「撤销」「重做」按钮。 */
export function useUndoState(repo: string) {
  return useQuery({ queryKey: qk.undoState(repo), queryFn: () => undoState(repo), enabled: !!repo });
}

/** 操作日志(本会话写操作时间线)。随历史变化失效。`enabled` 控制是否拉(面板打开才拉)。 */
export function useOpLog(repo: string, enabled: boolean) {
  return useQuery({ queryKey: qk.opLog(repo), queryFn: () => opLog(repo), enabled: enabled && !!repo });
}

export function useAheadBehind(repo: string) {
  return useQuery({ queryKey: qk.aheadBehind(repo), queryFn: () => getAheadBehind(repo), enabled: !!repo });
}

export function useRemotes(repo: string) {
  return useQuery({ queryKey: qk.remotes(repo), queryFn: () => getRemotes(repo), enabled: !!repo });
}

export function useBranches(repo: string, enabled: boolean) {
  return useQuery({ queryKey: qk.branches(repo), queryFn: () => listBranches(repo), enabled });
}

export function useRefs(repo: string, enabled: boolean) {
  return useQuery({ queryKey: qk.refs(repo), queryFn: () => listRefs(repo), enabled: enabled && !!repo });
}

export function useStashList(repo: string) {
  return useQuery({ queryKey: qk.stashes(repo), queryFn: () => stashList(repo), enabled: !!repo });
}

export function useRepoState(repo: string) {
  return useQuery({ queryKey: qk.repoState(repo), queryFn: () => getRepoState(repo), enabled: !!repo });
}

/** 逐行 blame;file 为仓库根相对路径,null 时不取。 */
export function useBlame(repo: string, file: string | null) {
  return useQuery({
    queryKey: [...qk.blame(repo), file ?? ""],
    queryFn: () => blame(repo, file!),
    enabled: !!repo && !!file,
  });
}

/** 冲突文件三方内容(base/ours/theirs);供三栏合并编辑器。 */
export function useConflictSides(repo: string, file: string | null) {
  return useQuery({
    queryKey: [...qk.conflictSides(repo), file ?? ""],
    queryFn: () => conflictSides(repo, file!),
    enabled: !!repo && !!file,
  });
}

/** 读工作区文件原文(冲突文件只读展示)。enabled 控制按需取。 */
export function useFileText(repo: string, file: string | null, enabled: boolean) {
  return useQuery({
    queryKey: [...qk.fileText(repo), file ?? ""],
    queryFn: () => readWorkingFile(repo, file!),
    enabled: enabled && !!file,
  });
}

// ---- 失效辅助 ----
/** 工作区相关(status + 工作区 diff):暂存/提交等写操作后调用。 */
export function invalidateWorktree(qc: QueryClient, repo: string) {
  qc.invalidateQueries({ queryKey: qk.status(repo) });
  qc.invalidateQueries({ queryKey: qk.workingDiff(repo) });
}

/** 历史/分支/远程同步(提交、切分支、fetch/pull/push 后)。 */
export function invalidateHistory(qc: QueryClient, repo: string) {
  qc.invalidateQueries({ queryKey: qk.graph(repo) });
  qc.invalidateQueries({ queryKey: qk.branches(repo) });
  qc.invalidateQueries({ queryKey: qk.currentBranch(repo) });
  qc.invalidateQueries({ queryKey: qk.aheadBehind(repo) });
  qc.invalidateQueries({ queryKey: qk.undoState(repo) }); // HEAD 动了 → 重算可否撤销/重做
  qc.invalidateQueries({ queryKey: qk.opLog(repo) }); // 操作日志(光标/新条目)也刷新
}

// ---- 一处监听文件变化 → 失效对应查询(取代各 view 的订阅+重载)----
export function useRepoWatch(repo: string | null) {
  const qc = useQueryClient();
  useEffect(() => {
    if (!repo) return;
    watchRepo(repo).catch(() => {});
    let un: (() => void) | undefined;
    onRepoChanged((kind) => {
      invalidateWorktree(qc, repo);
      qc.invalidateQueries({ queryKey: qk.repoState(repo) }); // 合并/变基状态可能变
      qc.invalidateQueries({ queryKey: qk.fileText(repo) }); // 冲突文件内容可能变
      if (kind === "ref") invalidateHistory(qc, repo);
    }).then((u) => { un = u; });
    return () => un?.();
  }, [repo, qc]);
}
