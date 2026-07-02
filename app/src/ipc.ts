// app/src/ipc.ts
// 所有对后端的调用集中在这一层,组件不直接 invoke。
// 后端契约变了,只改这里。

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

// DTO 类型由 ts-rs 从 crates/ipc-types 自动生成(M6.4),统一从 bindings 出口再导出。
// 后端改字段 → 重新生成 → 前端编译期报错。生成命令见 docs/HANDOFF.md。
export type {
  AheadBehindDto,
  BlameLineDto,
  BranchDeleteImpactDto,
  BranchDto,
  CommitDto,
  ConflictSidesDto,
  DiffLineDto,
  FetchResultDto,
  FileChangeDto,
  FileDiffDto,
  FileEntryDto,
  GraphRowDto,
  GraphSegDto,
  HunkDto,
  ImageRefDto,
  IpcError,
  LineHistoryEntryDto,
  MergeResultDto,
  OpLogDto,
  OpLogEntryDto,
  PullResultDto,
  PushResultDto,
  RefDto,
  ReflogEntryDto,
  RemoteInfoDto,
  SegDto,
  SignatureInfoDto,
  StashDto,
  StatusDto,
  SubmoduleInfoDto,
  UndoStateDto,
  UndoStepDto,
  WorktreeInfoDto,
} from "./bindings";

import type {
  CommitDto,
  StatusDto,
  FileChangeDto,
  LineHistoryEntryDto,
  SignatureInfoDto,
  SubmoduleInfoDto,
  WorktreeInfoDto,
  BranchDto,
  AheadBehindDto,
  BranchDeleteImpactDto,
  FetchResultDto,
  MergeResultDto,
  PullResultDto,
  PushResultDto,
  BlameLineDto,
  ConflictSidesDto,
  StashDto,
  FileDiffDto,
  GraphRowDto,
  RefDto,
  ReflogEntryDto,
  RemoteInfoDto,
  UndoStateDto,
  UndoStepDto,
  OpLogDto,
} from "./bindings";

/** onboarding:在 path 处新建空仓库(git init,尊重 init.defaultBranch)。 */
export async function initRepo(path: string): Promise<void> {
  await invoke("init_repo", { path });
}

/** onboarding:把 url 克隆进 parentDir(子目录名从 url 推导),返回克隆出的仓库根路径。
 *  认证失败抛 AUTH_FAILED、网络抛 NETWORK_ERROR、地址无效抛 INVALID_URL、目标非空抛 DESTINATION_NOT_EMPTY。 */
export async function cloneRepo(
  url: string,
  parentDir: string,
): Promise<string> {
  return await invoke<string>("clone_repo", { url, parentDir });
}

export async function getHeadCommit(repoPath: string): Promise<CommitDto> {
  // invoke 的参数名要和 Rust 命令的参数名一致(repo_path → repoPath,Tauri 自动转驼峰)
  return await invoke<CommitDto>("get_head_commit", { repoPath });
}

export async function getStatus(repoPath: string): Promise<StatusDto> {
  return await invoke<StatusDto>("get_status", { repoPath });
}

export async function stageFile(
  repoPath: string,
  filePath: string,
): Promise<void> {
  await invoke("stage_file", { repoPath, filePath });
}

export async function unstageFile(
  repoPath: string,
  filePath: string,
): Promise<void> {
  await invoke("unstage_file", { repoPath, filePath });
}

/** 暂存某文件第 hunkIndex 个未暂存改动块。 */
export async function stageHunk(
  repoPath: string,
  file: string,
  hunkIndex: number,
): Promise<void> {
  await invoke("stage_hunk", { repoPath, file, hunkIndex });
}

/** 取消暂存某文件第 hunkIndex 个已暂存改动块。 */
export async function unstageHunk(
  repoPath: string,
  file: string,
  hunkIndex: number,
): Promise<void> {
  await invoke("unstage_hunk", { repoPath, file, hunkIndex });
}

/** 暂存某未暂存 hunk 中的指定行(lines 为该 hunk 内行下标)。 */
export async function stageLines(
  repoPath: string,
  file: string,
  hunkIndex: number,
  lines: number[],
): Promise<void> {
  await invoke("stage_lines", { repoPath, file, hunkIndex, lines });
}

export async function commit(
  repoPath: string,
  message: string,
): Promise<string> {
  return await invoke<string>("commit", { repoPath, message });
}

/** 修订最近一次提交。message 省略/空 → 保留原提交信息;否则替换。纳入当前暂存改动。 */
export async function amendCommit(
  repoPath: string,
  message?: string,
): Promise<string> {
  return await invoke<string>("amend_commit", {
    repoPath,
    message: message?.trim() ? message : null,
  });
}

export async function getLog(
  repoPath: string,
  limit: number,
  skip: number,
): Promise<CommitDto[]> {
  return await invoke<CommitDto[]>("get_log", { repoPath, limit, skip });
}

/** 某文件的提交历史(git log --follow,跟随重命名,新→旧)。 */
export async function getFileHistory(
  repoPath: string,
  file: string,
  limit: number,
): Promise<CommitDto[]> {
  return await invoke<CommitDto[]>("file_history", { repoPath, file, limit });
}

/** 某文件第 start–end 行的演变史(git log -L,新→旧,每条带范围 diff)。行号 1 起、含两端。 */
export async function getLineHistory(
  repoPath: string,
  file: string,
  start: number,
  end: number,
): Promise<LineHistoryEntryDto[]> {
  return await invoke<LineHistoryEntryDto[]>("line_history", {
    repoPath,
    file,
    start,
    end,
  });
}

/** pickaxe:按 diff 内容搜提交。regex=false → git log -S(出现次数变化);regex=true → git log -G(改动行匹配正则)。 */
export async function pickaxe(
  repoPath: string,
  query: string,
  regex: boolean,
  limit: number,
): Promise<CommitDto[]> {
  return await invoke<CommitDto[]>("pickaxe", {
    repoPath,
    query,
    regex,
    limit,
  });
}

export async function getCommitFiles(
  repoPath: string,
  commitId: string,
): Promise<FileChangeDto[]> {
  return await invoke<FileChangeDto[]>("get_commit_files", {
    repoPath,
    commitId,
  });
}

export async function getCommitSignature(
  repoPath: string,
  commitId: string,
): Promise<SignatureInfoDto> {
  return await invoke<SignatureInfoDto>("get_commit_signature", {
    repoPath,
    commitId,
  });
}

export async function getCurrentBranch(
  repoPath: string,
): Promise<string | null> {
  return await invoke<string | null>("get_current_branch", { repoPath });
}

// ── 子模块(M4.4) ──
export type SubmoduleStatusStr =
  | "uninitialized"
  | "up-to-date"
  | "modified"
  | "conflict";

/** 列出子模块(读 .gitmodules + git submodule status);无子模块返回空数组。 */
export async function listSubmodules(
  repoPath: string,
): Promise<SubmoduleInfoDto[]> {
  return await invoke<SubmoduleInfoDto[]>("list_submodules", { repoPath });
}

/** 初始化并更新某子模块到超级项目记录的提交(git submodule update --init)。可能联网。 */
export async function updateSubmodule(
  repoPath: string,
  path: string,
): Promise<void> {
  await invoke("update_submodule", { repoPath, path });
}

// ── 工作树(M4.5) ──
/** 列出工作树(主 + 链接);普通仓库只有一个(主工作树)。 */
export async function listWorktrees(
  repoPath: string,
): Promise<WorktreeInfoDto[]> {
  return await invoke<WorktreeInfoDto[]>("list_worktrees", { repoPath });
}

/** 稀疏检出范围规则;未开启稀疏检出返回空数组。 */
export async function sparseCheckoutPatterns(
  repoPath: string,
): Promise<string[]> {
  return await invoke<string[]>("sparse_checkout_patterns", { repoPath });
}

// ── 分支管理(阶段 2a) ──
/** 列出本地分支(名字升序,当前分支 is_head=true)。 */
export async function listBranches(repoPath: string): Promise<BranchDto[]> {
  return await invoke<BranchDto[]>("list_branches", { repoPath });
}

/** 当前分支相对上游的领先/落后;无上游返回 null。 */
export async function getAheadBehind(
  repoPath: string,
): Promise<AheadBehindDto | null> {
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

/** 列出远程及其 fetch URL(供「管理远程」面板)。 */
export async function remoteList(repoPath: string): Promise<RemoteInfoDto[]> {
  return await invoke<RemoteInfoDto[]>("remote_list", { repoPath });
}

/** 新增远程。同名抛 REMOTE_EXISTS;名/URL 空抛 INVALID_REMOTE_NAME。 */
export async function addRemote(
  repoPath: string,
  name: string,
  url: string,
): Promise<void> {
  await invoke("add_remote", { repoPath, name, url });
}

/** 删除远程(连同其远程跟踪引用)。不存在抛 REMOTE_NOT_FOUND。 */
export async function removeRemote(
  repoPath: string,
  name: string,
): Promise<void> {
  await invoke("remove_remote", { repoPath, name });
}

/** 重命名远程。旧名不存在抛 REMOTE_NOT_FOUND;新名占用抛 REMOTE_EXISTS。 */
export async function renameRemote(
  repoPath: string,
  oldName: string,
  newName: string,
): Promise<void> {
  await invoke("rename_remote", { repoPath, old: oldName, new: newName });
}

/** 把当前分支上游设为 upstream(形如 "origin/main")。 */
export async function setUpstream(
  repoPath: string,
  upstream: string,
): Promise<void> {
  await invoke("set_upstream", { repoPath, upstream });
}

/** 切换到已有本地分支。脏工作区冲突会抛 IpcError(code: CHECKOUT_CONFLICT)。 */
export async function checkoutBranch(
  repoPath: string,
  name: string,
): Promise<void> {
  await invoke("checkout_branch", { repoPath, name });
}

/** 在 HEAD 新建分支;checkout=true 时建完即切过去。同名抛 BRANCH_EXISTS。 */
export async function createBranch(
  repoPath: string,
  name: string,
  checkout: boolean,
): Promise<void> {
  await invoke("create_branch", { repoPath, name, checkout });
}

/** 删除本地分支。删当前分支抛 CANNOT_DELETE_CURRENT。 */
export async function deleteBranch(
  repoPath: string,
  name: string,
): Promise<void> {
  await invoke("delete_branch", { repoPath, name });
}

/** 删某分支前的影响预览(会丢多少提交)。供二次确认。 */
export async function branchDeleteImpact(
  repoPath: string,
  name: string,
): Promise<BranchDeleteImpactDto> {
  return await invoke<BranchDeleteImpactDto>("branch_delete_impact", {
    repoPath,
    name,
  });
}

/** 把某分支合并进当前分支。冲突抛 MERGE_CONFLICT(进入 merging 中,去更改页解决)。 */
export async function mergeBranch(
  repoPath: string,
  name: string,
): Promise<MergeResultDto> {
  return await invoke<MergeResultDto>("merge_branch", { repoPath, name });
}

// ── 远程(阶段 2d-1) ──
/** 从默认远程 fetch(remote 省略 = git 默认远程)。 */
export async function fetchRemote(
  repoPath: string,
  remote?: string,
): Promise<FetchResultDto> {
  return await invoke<FetchResultDto>("fetch", {
    repoPath,
    remote: remote ?? null,
  });
}

/** pull。rebase=true 走 fetch+rebase。冲突抛 MERGE_CONFLICT、无上游抛 NO_UPSTREAM。 */
export async function pullRemote(
  repoPath: string,
  rebase = false,
  remote?: string,
): Promise<PullResultDto> {
  return await invoke<PullResultDto>("pull", {
    repoPath,
    remote: remote ?? null,
    rebase,
  });
}

/** push 当前分支。首次自动建上游;被拒(落后远程)抛 PUSH_REJECTED。 */
export async function pushRemote(
  repoPath: string,
  remote?: string,
): Promise<PushResultDto> {
  return await invoke<PushResultDto>("push", {
    repoPath,
    remote: remote ?? null,
  });
}

export async function setGithubToken(token: string): Promise<void> {
  await invoke("set_github_token", { token });
}

export async function hasGithubToken(): Promise<boolean> {
  return await invoke<boolean>("has_github_token");
}

export async function getGithubToken(): Promise<string> {
  return await invoke<string>("get_github_token");
}

export async function clearGithubToken(): Promise<void> {
  await invoke("clear_github_token");
}

export async function setGitlabToken(token: string): Promise<void> {
  await invoke("set_gitlab_token", { token });
}

export async function hasGitlabToken(): Promise<boolean> {
  return await invoke<boolean>("has_gitlab_token");
}

export async function getGitlabToken(): Promise<string> {
  return await invoke<string>("get_gitlab_token");
}

export async function clearGitlabToken(): Promise<void> {
  await invoke("clear_gitlab_token");
}

// ── Blame(追溯) ──
/** 逐行 blame(file 为仓库根相对路径)。 */
export async function blame(
  repoPath: string,
  file: string,
): Promise<BlameLineDto[]> {
  return await invoke<BlameLineDto[]>("blame", { repoPath, file });
}

// ── 冲突 / 进行中操作 ──
export type RepoStateStr =
  | "clean"
  | "merging"
  | "rebasing"
  | "cherry-picking"
  | "reverting"
  | "other";

/** 仓库状态:是否在合并/变基/cherry-pick 中。 */
export async function getRepoState(repoPath: string): Promise<RepoStateStr> {
  return await invoke<RepoStateStr>("get_repo_state", { repoPath });
}

/** 整文件采用我方并标记已解决。 */
export async function resolveOurs(
  repoPath: string,
  file: string,
): Promise<void> {
  await invoke("resolve_ours", { repoPath, file });
}

/** 整文件采用对方并标记已解决。 */
export async function resolveTheirs(
  repoPath: string,
  file: string,
): Promise<void> {
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
export async function readWorkingFile(
  repoPath: string,
  file: string,
): Promise<string> {
  return await invoke<string>("read_working_file", { repoPath, file });
}

/** 读冲突文件三方内容(index stage 1/2/3 blob)。 */
export async function conflictSides(
  repoPath: string,
  file: string,
): Promise<ConflictSidesDto> {
  return await invoke<ConflictSidesDto>("conflict_sides", { repoPath, file });
}

/** 写入解决后的内容并标记已解决(写文件 + git add)。 */
export async function writeResolved(
  repoPath: string,
  file: string,
  content: string,
): Promise<void> {
  await invoke("write_resolved", { repoPath, file, content });
}

/** 把某提交拣选到当前分支。冲突抛 MERGE_CONFLICT(进入 cherry-pick 中)。 */
export async function cherryPick(
  repoPath: string,
  commitId: string,
): Promise<void> {
  await invoke("cherry_pick", { repoPath, commitId });
}

/** 回滚某提交(生成抵消其改动的新提交)。冲突抛 MERGE_CONFLICT(进入 reverting 中)。 */
export async function revert(
  repoPath: string,
  commitId: string,
): Promise<void> {
  await invoke("revert", { repoPath, commitId });
}

/** 在某提交上打标签。message 非空 → 附注标签,否则轻量标签。同名抛 TAG_EXISTS。 */
export async function createTag(
  repoPath: string,
  name: string,
  commitId: string,
  message?: string,
): Promise<void> {
  await invoke("create_tag", {
    repoPath,
    name,
    commitId,
    message: message?.trim() ? message : null,
  });
}

/** 删除标签。 */
export async function deleteTag(repoPath: string, name: string): Promise<void> {
  await invoke("delete_tag", { repoPath, name });
}

export type ResetMode = "soft" | "mixed" | "hard";
/** 把当前分支 HEAD 重置到某提交。hard 会丢弃未提交改动(UI 须二次确认)。 */
export async function resetTo(
  repoPath: string,
  commitId: string,
  mode: ResetMode,
): Promise<void> {
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
export async function interactiveRebase(
  repoPath: string,
  base: string | null,
  steps: RebaseStepInput[],
): Promise<void> {
  await invoke("interactive_rebase", { repoPath, base, steps });
}

// ── 贮藏(stash) ──
export async function stashList(repoPath: string): Promise<StashDto[]> {
  return await invoke<StashDto[]>("stash_list", { repoPath });
}

/** 贮藏当前工作区改动;无改动抛 NOTHING_TO_STASH。 */
export async function stashSave(
  repoPath: string,
  message?: string,
): Promise<void> {
  await invoke("stash_save", { repoPath, message: message ?? null });
}

/** 应用贮藏(不移除)。冲突抛 MERGE_CONFLICT。 */
export async function stashApply(
  repoPath: string,
  index: number,
): Promise<void> {
  await invoke("stash_apply", { repoPath, index });
}

/** 弹出贮藏(应用并移除)。冲突抛 MERGE_CONFLICT(此时不移除)。 */
export async function stashPop(repoPath: string, index: number): Promise<void> {
  await invoke("stash_pop", { repoPath, index });
}

/** 删除贮藏。 */
export async function stashDrop(
  repoPath: string,
  index: number,
): Promise<void> {
  await invoke("stash_drop", { repoPath, index });
}

/** 取一侧图片的原始字节(M6.2:不再走 base64-in-JSON)。返回 ArrayBuffer,前端转 Blob URL。
 *  oid 非空 → 对象库 blob;空串 → 读工作区文件 file。 */
export async function readImage(
  repoPath: string,
  oid: string,
  file: string,
): Promise<ArrayBuffer> {
  return await invoke<ArrayBuffer>("read_image", { repoPath, oid, path: file });
}

export async function getCommitFileDiff(
  repoPath: string,
  commitId: string,
  file: string,
): Promise<FileDiffDto> {
  return await invoke<FileDiffDto>("get_commit_file_diff", {
    repoPath,
    commitId,
    file,
  });
}

/** 任意两 revision(commit/分支/标签)间的改动文件(from→to)。 */
export async function compareFiles(
  repoPath: string,
  from: string,
  to: string,
): Promise<FileChangeDto[]> {
  return await invoke<FileChangeDto[]>("compare_files", { repoPath, from, to });
}

/** 两 revision 间单个文件的行级 diff(from→to)。 */
export async function compareFileDiff(
  repoPath: string,
  from: string,
  to: string,
  file: string,
): Promise<FileDiffDto> {
  return await invoke<FileDiffDto>("compare_file_diff", {
    repoPath,
    from,
    to,
    file,
  });
}

/** 工作区文件 diff:staged=false 未暂存(index↔工作区)、true 已暂存(HEAD↔index)。 */
export async function getWorkingDiff(
  repoPath: string,
  file: string,
  staged: boolean,
): Promise<FileDiffDto> {
  return await invoke<FileDiffDto>("get_working_diff", {
    repoPath,
    file,
    staged,
  });
}

/** 从 HEAD 取 limit 条提交并算好 lane 布局。 */
export async function getCommitGraph(
  repoPath: string,
  limit: number,
): Promise<GraphRowDto[]> {
  return await invoke<GraphRowDto[]>("get_commit_graph", { repoPath, limit });
}

/** HEAD 的 reflog,最近在前,最多 limit 条。用于找回被 reset/rebase 丢弃的提交。 */
export async function getReflog(
  repoPath: string,
  limit: number,
): Promise<ReflogEntryDto[]> {
  return await invoke<ReflogEntryDto[]>("get_reflog", { repoPath, limit });
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

/** 操作日志:本工具本会话做过的写操作时间线 + 当前光标。 */
export async function opLog(repoPath: string): Promise<OpLogDto> {
  return await invoke<OpLogDto>("op_log", { repoPath });
}

/** 跳到操作日志第 index 项(reset --soft 过去)。 */
export async function opGoto(
  repoPath: string,
  index: number,
): Promise<UndoStepDto> {
  return await invoke<UndoStepDto>("op_goto", { repoPath, index });
}

/** 搜索提交(匹配 message/作者/SHA 前缀,大小写不敏感),扁平列表;空 query 返回空。 */
export async function searchCommits(
  repoPath: string,
  query: string,
  limit: number,
): Promise<CommitDto[]> {
  return await invoke<CommitDto[]>("search_commits", {
    repoPath,
    query,
    limit,
  });
}

// ── 文件监听 ──
export type RepoChangeKind = "worktree" | "index" | "ref";

/** 开始监听该仓库;切仓库再调一次会自动替换旧监听。 */
export async function watchRepo(repoPath: string): Promise<void> {
  await invoke("watch_repo", { repoPath });
}

/** 订阅仓库变化事件。返回取消订阅函数(在 cleanup 里调用)。 */
export function onRepoChanged(
  cb: (kind: RepoChangeKind) => void,
): Promise<() => void> {
  return listen<RepoChangeKind>("repo-changed", (e) => cb(e.payload));
}
