// 集中定义所有 IPC「读」的 React Query hooks + query key + 失效辅助。
// 组件不再各自手写 useEffect 拉数据 / loading-error 布尔;写操作后调
// invalidate* 触发重取,外部文件变化由 useRepoWatch 一处监听失效。

import { useQuery, useQueryClient, keepPreviousData, type QueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import {
  getStatus, getWorkingDiff, getCommitGraph, getCommitFiles, getCommitFileDiff,
  getCurrentBranch, getAheadBehind, getRemotes, listBranches,
  watchRepo, onRepoChanged,
} from "../ipc";

// ---- query keys(数组前缀便于部分匹配失效)----
export const qk = {
  status: (repo: string) => ["status", repo] as const,
  workingDiff: (repo: string) => ["workingDiff", repo] as const,
  graph: (repo: string) => ["graph", repo] as const,
  commitFiles: (repo: string) => ["commitFiles", repo] as const,
  commitDiff: (repo: string) => ["commitDiff", repo] as const,
  currentBranch: (repo: string) => ["currentBranch", repo] as const,
  aheadBehind: (repo: string) => ["aheadBehind", repo] as const,
  remotes: (repo: string) => ["remotes", repo] as const,
  branches: (repo: string) => ["branches", repo] as const,
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

export function useCommitFiles(repo: string, commitId: string | null) {
  return useQuery({
    queryKey: [...qk.commitFiles(repo), commitId ?? ""],
    queryFn: () => getCommitFiles(repo, commitId!),
    enabled: !!commitId,
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

export function useAheadBehind(repo: string) {
  return useQuery({ queryKey: qk.aheadBehind(repo), queryFn: () => getAheadBehind(repo), enabled: !!repo });
}

export function useRemotes(repo: string) {
  return useQuery({ queryKey: qk.remotes(repo), queryFn: () => getRemotes(repo), enabled: !!repo });
}

export function useBranches(repo: string, enabled: boolean) {
  return useQuery({ queryKey: qk.branches(repo), queryFn: () => listBranches(repo), enabled });
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
      if (kind === "ref") invalidateHistory(qc, repo);
    }).then((u) => { un = u; });
    return () => un?.();
  }, [repo, qc]);
}
