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

/** 暂存某未暂存 hunk 中的指定行(lines 为该 hunk 内行下标)。 */
export async function stageLines(repoPath: string, file: string, hunkIndex: number, lines: number[]): Promise<void> {
  await invoke("stage_lines", { repoPath, file, hunkIndex, lines });
}

export async function commit(repoPath: string, message: string): Promise<string> {
  return await invoke<string>("commit", { repoPath, message });
}

/** 修订最近一次提交。message 省略/空 → 保留原提交信息;否则替换。纳入当前暂存改动。 */
export async function amendCommit(repoPath: string, message?: string): Promise<string> {
  return await invoke<string>("amend_commit", { repoPath, message: message?.trim() ? message : null });
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

/** 某文件的提交历史(git log --follow,跟随重命名,新→旧)。 */
export async function getFileHistory(repoPath: string, file: string, limit: number): Promise<CommitDto[]> {
  return await invoke<CommitDto[]>("file_history", { repoPath, file, limit });
}

/** 行历史的一条:某提交 + 它对选中行范围的 diff。 */
export interface LineHistoryEntryDto {
  commit: CommitDto;
  diff: FileDiffDto;
}

/** 某文件第 start–end 行的演变史(git log -L,新→旧,每条带范围 diff)。行号 1 起、含两端。 */
export async function getLineHistory(repoPath: string, file: string, start: number, end: number): Promise<LineHistoryEntryDto[]> {
  return await invoke<LineHistoryEntryDto[]>("line_history", { repoPath, file, start, end });
}

export async function getCommitFiles(repoPath: string, commitId: string): Promise<FileChangeDto[]> {
  return await invoke<FileChangeDto[]>("get_commit_files", { repoPath, commitId });
}

/** 提交签名状态。status: "none" | "good" | "unverified" | "bad"。 */
export interface SignatureInfoDto {
  status: string;
  signer: string;
}

export async function getCommitSignature(repoPath: string, commitId: string): Promise<SignatureInfoDto> {
  return await invoke<SignatureInfoDto>("get_commit_signature", { repoPath, commitId });
}

export async function getCurrentBranch(repoPath: string): Promise<string | null> {
  return await invoke<string | null>("get_current_branch", { repoPath });
}

// ── 子模块(M4.4) ──
export type SubmoduleStatusStr = "uninitialized" | "up-to-date" | "modified" | "conflict";
export interface SubmoduleInfoDto {
  path: string;
  url: string;
  head_sha: string;
  short_sha: string;
  status: SubmoduleStatusStr;
  describe: string; // 末尾括号描述(heads/main、tag 等),可能为空
}

/** 列出子模块(读 .gitmodules + git submodule status);无子模块返回空数组。 */
export async function listSubmodules(repoPath: string): Promise<SubmoduleInfoDto[]> {
  return await invoke<SubmoduleInfoDto[]>("list_submodules", { repoPath });
}

/** 初始化并更新某子模块到超级项目记录的提交(git submodule update --init)。可能联网。 */
export async function updateSubmodule(repoPath: string, path: string): Promise<void> {
  await invoke("update_submodule", { repoPath, path });
}

// ── 工作树(M4.5) ──
export interface WorktreeInfoDto {
  path: string;
  head_sha: string;
  short_sha: string;
  branch: string; // 短分支名;分离头/裸仓库为空
  is_main: boolean;
  is_current: boolean;
  detached: boolean;
  locked: boolean;
  bare: boolean;
}

/** 列出工作树(主 + 链接);普通仓库只有一个(主工作树)。 */
export async function listWorktrees(repoPath: string): Promise<WorktreeInfoDto[]> {
  return await invoke<WorktreeInfoDto[]>("list_worktrees", { repoPath });
}

/** 稀疏检出范围规则;未开启稀疏检出返回空数组。 */
export async function sparseCheckoutPatterns(repoPath: string): Promise<string[]> {
  return await invoke<string[]>("sparse_checkout_patterns", { repoPath });
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

/** 列出所有引用(本地分支/远程跟踪/标签/HEAD)。 */
export async function listRefs(repoPath: string): Promise<RefDto[]> {
  return await invoke<RefDto[]>("list_refs", { repoPath });
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

export interface BranchDeleteImpactDto {
  unmerged_commits: number;     // 删后会丢的提交数(0=安全)
  sample_summaries: string[];   // 这些提交的摘要样本
}

/** 删某分支前的影响预览(会丢多少提交)。供二次确认。 */
export async function branchDeleteImpact(repoPath: string, name: string): Promise<BranchDeleteImpactDto> {
  return await invoke<BranchDeleteImpactDto>("branch_delete_impact", { repoPath, name });
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

/** 冲突文件三方内容(base/ours/theirs,某方缺失为 null)。供三栏合并编辑器。 */
export interface ConflictSidesDto {
  base: string | null;
  ours: string | null;
  theirs: string | null;
}

/** 读冲突文件三方内容(index stage 1/2/3 blob)。 */
export async function conflictSides(repoPath: string, file: string): Promise<ConflictSidesDto> {
  return await invoke<ConflictSidesDto>("conflict_sides", { repoPath, file });
}

/** 写入解决后的内容并标记已解决(写文件 + git add)。 */
export async function writeResolved(repoPath: string, file: string, content: string): Promise<void> {
  await invoke("write_resolved", { repoPath, file, content });
}

/** 把某提交拣选到当前分支。冲突抛 MERGE_CONFLICT(进入 cherry-pick 中)。 */
export async function cherryPick(repoPath: string, commitId: string): Promise<void> {
  await invoke("cherry_pick", { repoPath, commitId });
}

/** 回滚某提交(生成抵消其改动的新提交)。冲突抛 MERGE_CONFLICT(进入 reverting 中)。 */
export async function revert(repoPath: string, commitId: string): Promise<void> {
  await invoke("revert", { repoPath, commitId });
}

/** 在某提交上打标签。message 非空 → 附注标签,否则轻量标签。同名抛 TAG_EXISTS。 */
export async function createTag(repoPath: string, name: string, commitId: string, message?: string): Promise<void> {
  await invoke("create_tag", { repoPath, name, commitId, message: message?.trim() ? message : null });
}

/** 删除标签。 */
export async function deleteTag(repoPath: string, name: string): Promise<void> {
  await invoke("delete_tag", { repoPath, name });
}

export type ResetMode = "soft" | "mixed" | "hard";
/** 把当前分支 HEAD 重置到某提交。hard 会丢弃未提交改动(UI 须二次确认)。 */
export async function resetTo(repoPath: string, commitId: string, mode: ResetMode): Promise<void> {
  await invoke("reset", { repoPath, commitId, mode });
}

export type RebaseActionKind = "pick" | "reword" | "squash" | "fixup" | "drop";
export interface RebaseStepInput {
  sha: string;
  action: RebaseActionKind;
  message?: string; // reword/squash 用
}
/** 交互式 rebase。base=最旧提交的父 SHA(null→--root);steps 为 oldest→newest。
 *  冲突抛 MERGE_CONFLICT(进入 rebasing 中,去 Changes 页解决)。 */
export async function interactiveRebase(repoPath: string, base: string | null, steps: RebaseStepInput[]): Promise<void> {
  await invoke("interactive_rebase", { repoPath, base, steps });
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

export interface SegDto {
  text: string;
  changed: boolean;
}

export interface DiffLineDto {
  kind: string; // "context" | "add" | "del"
  old_lineno: number | null;
  new_lineno: number | null;
  content: string;
  emphasis?: SegDto[] | null;
}

export interface HunkDto {
  header: string;
  lines: DiffLineDto[];
}

export interface FileDiffDto {
  path: string;
  is_binary: boolean;
  too_large: boolean;
  is_lfs_pointer: boolean; // Git LFS 指针文件(内容是指针而非真实文件)
  lfs_size: string;        // LFS 指针记录的实际字节数(字符串,非 LFS 为空)
  hunks: HunkDto[];
}

export async function getCommitFileDiff(
  repoPath: string,
  commitId: string,
  file: string,
): Promise<FileDiffDto> {
  return await invoke<FileDiffDto>("get_commit_file_diff", { repoPath, commitId, file });
}

/** 任意两 revision(commit/分支/标签)间的改动文件(from→to)。 */
export async function compareFiles(repoPath: string, from: string, to: string): Promise<FileChangeDto[]> {
  return await invoke<FileChangeDto[]>("compare_files", { repoPath, from, to });
}

/** 两 revision 间单个文件的行级 diff(from→to)。 */
export async function compareFileDiff(repoPath: string, from: string, to: string, file: string): Promise<FileDiffDto> {
  return await invoke<FileDiffDto>("compare_file_diff", { repoPath, from, to, file });
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
  sync: "" | "outgoing" | "incoming"; // outgoing=已commit未push,incoming=已fetch未pull
}

/** 从 HEAD 取 limit 条提交并算好 lane 布局。 */
export async function getCommitGraph(repoPath: string, limit: number): Promise<GraphRowDto[]> {
  return await invoke<GraphRowDto[]>("get_commit_graph", { repoPath, limit });
}

/** 搜索提交(匹配 message/作者/SHA 前缀,大小写不敏感),扁平列表;空 query 返回空。 */
// ── reflog(HEAD 移动历史 / 后悔药)──
export interface ReflogEntryDto {
  index: number;
  selector: string;   // "HEAD@{0}"
  new_oid: string;    // 该步后 HEAD 指向的提交(reset 目标)
  new_short: string;
  message: string;    // "commit: ..." / "reset: moving to ..." / "checkout: ..."
  committer_name: string;
  timestamp: number;
}

/** HEAD 的 reflog,最近在前,最多 limit 条。用于找回被 reset/rebase 丢弃的提交。 */
export async function getReflog(repoPath: string, limit: number): Promise<ReflogEntryDto[]> {
  return await invoke<ReflogEntryDto[]>("get_reflog", { repoPath, limit });
}

// ── 多级 Undo/Redo(操作时间线 + 光标,reset --soft,永不丢工作区)──
export interface UndoStepDto {
  label: string;             // 操作中文名,如 "提交"、"重置(reset)"
  target_short: string;      // 这一步移动后 HEAD 的短 SHA
  worktree_restored: boolean; // true=还原了工作区(hard,撤销 reset 等);false=内容回暂存区(soft,撤销提交)
}

export interface UndoStateDto {
  can_undo: UndoStepDto | null; // 后退一步(撤销),null=不可
  can_redo: UndoStepDto | null; // 前进一步(重做),null=不可
}

/** 撤销/重做的当前可用性(只读),驱动顶栏按钮的显隐与文案。 */
export async function undoState(repoPath: string): Promise<UndoStateDto> {
  return await invoke<UndoStateDto>("undo_state", { repoPath });
}

/** 撤销一步:沿时间线后退,reset --soft。改动回暂存区,不丢工作区。 */
export async function undo(repoPath: string): Promise<UndoStepDto> {
  return await invoke<UndoStepDto>("undo", { repoPath });
}

/** 重做一步:沿时间线前进,reset --soft。 */
export async function redo(repoPath: string): Promise<UndoStepDto> {
  return await invoke<UndoStepDto>("redo", { repoPath });
}

// ── 操作日志(本会话写操作时间线)──
export interface OpLogEntryDto {
  label: string;        // 操作中文名;基点为 "起点"
  target_short: string; // 该操作后 HEAD 短 SHA
  timestamp: number;    // Unix 秒
}

export interface OpLogDto {
  entries: OpLogEntryDto[]; // oldest→newest
  current: number;          // 当前 HEAD 所在项下标
}

/** 操作日志:本工具本会话做过的写操作时间线 + 当前光标。 */
export async function opLog(repoPath: string): Promise<OpLogDto> {
  return await invoke<OpLogDto>("op_log", { repoPath });
}

/** 跳到操作日志第 index 项(reset --soft 过去)。 */
export async function opGoto(repoPath: string, index: number): Promise<UndoStepDto> {
  return await invoke<UndoStepDto>("op_goto", { repoPath, index });
}

export async function searchCommits(repoPath: string, query: string, limit: number): Promise<CommitDto[]> {
  return await invoke<CommitDto[]>("search_commits", { repoPath, query, limit });
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
