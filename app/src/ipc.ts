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
  body: string;
  author_name: string;
  author_email: string;
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

/** 暂存某文件第 hunkIndex 个未暂存改动块。 */
export async function stageHunk(repoPath: string, file: string, hunkIndex: number): Promise<void> {
  await invoke("stage_hunk", { repoPath, file, hunkIndex });
}

/** 取消暂存某文件第 hunkIndex 个已暂存改动块。 */
export async function unstageHunk(repoPath: string, file: string, hunkIndex: number): Promise<void> {
  await invoke("unstage_hunk", { repoPath, file, hunkIndex });
}

export async function commit(repoPath: string, message: string): Promise<string> {
  return await invoke<string>("commit", { repoPath, message });
}

export interface FileChangeDto {
  path: string;
  status: string; // added | modified | deleted | renamed
  additions: number;
  deletions: number;
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

// ── 分支管理(阶段 2a) ──
export interface BranchDto {
  name: string;
  is_head: boolean;
}

/** 列出本地分支(名字升序,当前分支 is_head=true)。 */
export async function listBranches(repoPath: string): Promise<BranchDto[]> {
  return await invoke<BranchDto[]>("list_branches", { repoPath });
}

export interface AheadBehindDto {
  ahead: number; // 本地领先上游(可 push)
  behind: number; // 本地落后上游(可 pull)
}

/** 当前分支相对上游的领先/落后;无上游返回 null。 */
export async function getAheadBehind(repoPath: string): Promise<AheadBehindDto | null> {
  return await invoke<AheadBehindDto | null>("get_ahead_behind", { repoPath });
}

/** 列出远程名(["origin", ...])。 */
export async function getRemotes(repoPath: string): Promise<string[]> {
  return await invoke<string[]>("get_remotes", { repoPath });
}

/** 把当前分支上游设为 upstream(形如 "origin/main")。 */
export async function setUpstream(repoPath: string, upstream: string): Promise<void> {
  await invoke("set_upstream", { repoPath, upstream });
}

/** 切换到已有本地分支。脏工作区冲突会抛 IpcError(code: CHECKOUT_CONFLICT)。 */
export async function checkoutBranch(repoPath: string, name: string): Promise<void> {
  await invoke("checkout_branch", { repoPath, name });
}

/** 在 HEAD 新建分支;checkout=true 时建完即切过去。同名抛 BRANCH_EXISTS。 */
export async function createBranch(repoPath: string, name: string, checkout: boolean): Promise<void> {
  await invoke("create_branch", { repoPath, name, checkout });
}

/** 删除本地分支。删当前分支抛 CANNOT_DELETE_CURRENT。 */
export async function deleteBranch(repoPath: string, name: string): Promise<void> {
  await invoke("delete_branch", { repoPath, name });
}

// ── 远程(阶段 2d-1) ──
export interface FetchResultDto {
  remote: string;
  summary: string;
}

/** 从默认远程 fetch(remote 省略 = git 默认远程)。 */
export async function fetchRemote(repoPath: string, remote?: string): Promise<FetchResultDto> {
  return await invoke<FetchResultDto>("fetch", { repoPath, remote: remote ?? null });
}

export interface PullResultDto {
  summary: string;
}

/** pull。rebase=true 走 fetch+rebase。冲突抛 MERGE_CONFLICT、无上游抛 NO_UPSTREAM。 */
export async function pullRemote(repoPath: string, rebase = false, remote?: string): Promise<PullResultDto> {
  return await invoke<PullResultDto>("pull", { repoPath, remote: remote ?? null, rebase });
}

export interface PushResultDto {
  summary: string;
  set_upstream: boolean; // 首次 push 自动建上游时为 true
}

/** push 当前分支。首次自动建上游;被拒(落后远程)抛 PUSH_REJECTED。 */
export async function pushRemote(repoPath: string, remote?: string): Promise<PushResultDto> {
  return await invoke<PushResultDto>("push", { repoPath, remote: remote ?? null });
}

// ── Blame(追溯) ──
export interface BlameLineDto {
  line_no: number;
  commit_id: string;
  short_id: string;
  author_name: string;
  timestamp: number;
  content: string;
}

/** 逐行 blame(file 为仓库根相对路径)。 */
export async function blame(repoPath: string, file: string): Promise<BlameLineDto[]> {
  return await invoke<BlameLineDto[]>("blame", { repoPath, file });
}

// ── 冲突 / 进行中操作 ──
export type RepoStateStr = "clean" | "merging" | "rebasing" | "cherry-picking" | "reverting" | "other";

/** 仓库状态:是否在合并/变基/cherry-pick 中。 */
export async function getRepoState(repoPath: string): Promise<RepoStateStr> {
  return await invoke<RepoStateStr>("get_repo_state", { repoPath });
}

/** 整文件采用我方并标记已解决。 */
export async function resolveOurs(repoPath: string, file: string): Promise<void> {
  await invoke("resolve_ours", { repoPath, file });
}

/** 整文件采用对方并标记已解决。 */
export async function resolveTheirs(repoPath: string, file: string): Promise<void> {
  await invoke("resolve_theirs", { repoPath, file });
}

/** 继续进行中的操作(合并提交 / rebase --continue 等);仍有冲突抛 MERGE_CONFLICT。 */
export async function continueOp(repoPath: string): Promise<void> {
  await invoke("continue_op", { repoPath });
}

/** 中止进行中的操作(merge/rebase/cherry-pick --abort)。 */
export async function abortOp(repoPath: string): Promise<void> {
  await invoke("abort_op", { repoPath });
}

/** 读工作区文件原文(冲突文件只读展示)。 */
export async function readWorkingFile(repoPath: string, file: string): Promise<string> {
  return await invoke<string>("read_working_file", { repoPath, file });
}

/** 把某提交拣选到当前分支。冲突抛 MERGE_CONFLICT(进入 cherry-pick 中)。 */
export async function cherryPick(repoPath: string, commitId: string): Promise<void> {
  await invoke("cherry_pick", { repoPath, commitId });
}

// ── 贮藏(stash) ──
export interface StashDto {
  index: number;
  message: string;
}

export async function stashList(repoPath: string): Promise<StashDto[]> {
  return await invoke<StashDto[]>("stash_list", { repoPath });
}

/** 贮藏当前工作区改动;无改动抛 NOTHING_TO_STASH。 */
export async function stashSave(repoPath: string, message?: string): Promise<void> {
  await invoke("stash_save", { repoPath, message: message ?? null });
}

/** 应用贮藏(不移除)。冲突抛 MERGE_CONFLICT。 */
export async function stashApply(repoPath: string, index: number): Promise<void> {
  await invoke("stash_apply", { repoPath, index });
}

/** 弹出贮藏(应用并移除)。冲突抛 MERGE_CONFLICT(此时不移除)。 */
export async function stashPop(repoPath: string, index: number): Promise<void> {
  await invoke("stash_pop", { repoPath, index });
}

/** 删除贮藏。 */
export async function stashDrop(repoPath: string, index: number): Promise<void> {
  await invoke("stash_drop", { repoPath, index });
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

/** 工作区文件 diff:staged=false 未暂存(index↔工作区)、true 已暂存(HEAD↔index)。 */
export async function getWorkingDiff(
  repoPath: string,
  file: string,
  staged: boolean,
): Promise<FileDiffDto> {
  return await invoke<FileDiffDto>("get_working_diff", { repoPath, file, staged });
}

// ── 提交图谱 ──
export interface GraphSegDto {
  from: number;
  to: number;
  color: number;
}

export interface RefDto {
  name: string;
  kind: "head" | "local" | "remote" | "tag";
}

export interface GraphRowDto {
  commit: CommitDto;
  column: number;
  color: number;
  top: GraphSegDto[];
  bottom: GraphSegDto[];
  refs: RefDto[]; // 指向本行提交的引用(分支/远程/HEAD),多数为空
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
