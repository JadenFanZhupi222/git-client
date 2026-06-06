// app/src/ipc.ts
// 所有对后端的调用集中在这一层,组件不直接 invoke。
// 后端契约变了,只改这里。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// 这些类型生产环境应由 ipc-types(specta/ts-rs)自动生成。
// 阶段 0 先手写,和后端 DTO 保持一致。
export interface CommitDto {
  id: string;
  short_id: string;
  summary: string;
  author_name: string;
  timestamp: number;
  parents: string[];
}

export interface IpcError {
  code: string;
  message: string;
  recoverable: boolean;
}

export async function getHeadCommit(repoPath: string): Promise<CommitDto> {
  // invoke 的参数名要和 Rust 命令的参数名一致(repo_path → repoPath,Tauri 自动转驼峰)
  return await invoke<CommitDto>("get_head_commit", { repoPath });
}

export interface FileEntryDto {
  path: string;
  state: string; // modified | added | deleted | renamed | untracked | conflicted
  staged: boolean;
}

export interface StatusDto {
  entries: FileEntryDto[];
}

export async function getStatus(repoPath: string): Promise<StatusDto> {
  return await invoke<StatusDto>("get_status", { repoPath });
}

export async function stageFile(repoPath: string, filePath: string): Promise<void> {
  await invoke("stage_file", { repoPath, filePath });
}

export async function unstageFile(repoPath: string, filePath: string): Promise<void> {
  await invoke("unstage_file", { repoPath, filePath });
}

export async function commit(repoPath: string, message: string): Promise<string> {
  return await invoke<string>("commit", { repoPath, message });
}

export interface FileChangeDto {
  path: string;
  status: string; // added | modified | deleted | renamed
}

export async function getLog(repoPath: string, limit: number, skip: number): Promise<CommitDto[]> {
  return await invoke<CommitDto[]>("get_log", { repoPath, limit, skip });
}

export async function getCommitFiles(repoPath: string, commitId: string): Promise<FileChangeDto[]> {
  return await invoke<FileChangeDto[]>("get_commit_files", { repoPath, commitId });
}

export async function getCurrentBranch(repoPath: string): Promise<string | null> {
  return await invoke<string | null>("get_current_branch", { repoPath });
}

export interface DiffLineDto {
  kind: string; // "context" | "add" | "del"
  old_lineno: number | null;
  new_lineno: number | null;
  content: string;
}

export interface HunkDto {
  header: string;
  lines: DiffLineDto[];
}

export interface FileDiffDto {
  path: string;
  is_binary: boolean;
  hunks: HunkDto[];
}

export async function getCommitFileDiff(
  repoPath: string,
  commitId: string,
  file: string,
): Promise<FileDiffDto> {
  return await invoke<FileDiffDto>("get_commit_file_diff", { repoPath, commitId, file });
}

// ── 提交图谱 ──
export interface GraphSegDto {
  from: number;
  to: number;
  color: number;
}

export interface GraphRowDto {
  commit: CommitDto;
  column: number;
  color: number;
  top: GraphSegDto[];
  bottom: GraphSegDto[];
}

/** 从 HEAD 取 limit 条提交并算好 lane 布局。 */
export async function getCommitGraph(repoPath: string, limit: number): Promise<GraphRowDto[]> {
  return await invoke<GraphRowDto[]>("get_commit_graph", { repoPath, limit });
}

// ── 文件监听 ──
export type RepoChangeKind = "worktree" | "index" | "ref";

/** 开始监听该仓库;切仓库再调一次会自动替换旧监听。 */
export async function watchRepo(repoPath: string): Promise<void> {
  await invoke("watch_repo", { repoPath });
}

/** 订阅仓库变化事件。返回取消订阅函数(在 cleanup 里调用)。 */
export function onRepoChanged(cb: (kind: RepoChangeKind) => void): Promise<() => void> {
  return listen<RepoChangeKind>("repo-changed", (e) => cb(e.payload));
}
