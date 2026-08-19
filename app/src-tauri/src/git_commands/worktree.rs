use crate::*;

#[tauri::command]
pub(crate) async fn get_current_branch(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<Option<String>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.current_branch())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn list_branches(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<Vec<BranchDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.branches())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn get_ahead_behind(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<Option<AheadBehindDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.ahead_behind())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn get_remotes(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<Vec<String>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.remotes())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn list_refs(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<Vec<RefDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.refs())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn set_upstream(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    upstream: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.set_upstream(&upstream))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn remote_list(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<Vec<RemoteInfoDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.remote_list())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn add_remote(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    name: String,
    url: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.add_remote(&name, &url))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn remove_remote(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    name: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.remove_remote(&name))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn rename_remote(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    old: String,
    new: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.rename_remote(&old, &new))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn checkout_branch(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    name: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.checkout_branch(&name))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn create_branch(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    name: String,
    checkout: bool,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.create_branch(&name, checkout))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn delete_branch(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    name: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.delete_branch(&name))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn merge_branch(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    name: String,
) -> Result<MergeResultDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.merge_branch(&name))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 删某分支前的影响预览(只读):会丢多少提交 + 摘要样本。供二次确认。
#[tauri::command]
pub(crate) async fn branch_delete_impact(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    name: String,
) -> Result<BranchDeleteImpactDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.branch_delete_impact(&name))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn fetch(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    remote: Option<String>,
) -> Result<FetchResultDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.fetch(remote.as_deref()))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn pull(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    remote: Option<String>,
    rebase: bool,
) -> Result<PullResultDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.pull(remote.as_deref(), rebase))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn push(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    remote: Option<String>,
) -> Result<PushResultDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.push(remote.as_deref()))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn get_repo_state(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<String, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.repo_state())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn resolve_ours(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.resolve_ours(&file))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn resolve_theirs(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.resolve_theirs(&file))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn continue_op(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.continue_op())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn abort_op(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.abort_op())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn blame(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file: String,
) -> Result<Vec<BlameLineDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.blame(&file))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn cherry_pick(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    commit_id: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.cherry_pick(&commit_id))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn revert(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    commit_id: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.revert(&commit_id))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn create_tag(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    name: String,
    commit_id: String,
    message: Option<String>,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.create_tag(&name, &commit_id, message.as_deref()))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn delete_tag(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    name: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.delete_tag(&name))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 交互式 rebase 的单步入参(前端传来的 JSON)。
#[derive(serde::Deserialize)]
pub(crate) struct RebaseStepInput {
    sha: String,
    action: String,
    message: Option<String>,
}

#[tauri::command]
pub(crate) async fn interactive_rebase(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    base: Option<String>,
    steps: Vec<RebaseStepInput>,
) -> Result<(), IpcError> {
    use git_core::model::{RebaseAction, RebaseStep};
    let mut domain = Vec::with_capacity(steps.len());
    for s in steps {
        let action = match s.action.as_str() {
            "pick" => RebaseAction::Pick,
            "reword" => RebaseAction::Reword(s.message.unwrap_or_default()),
            "squash" => RebaseAction::Squash(s.message.unwrap_or_default()),
            "fixup" => RebaseAction::Fixup,
            "drop" => RebaseAction::Drop,
            other => {
                return Err(to_ipc(git_core::GitError::Backend(format!(
                    "未知 rebase 操作: {other}"
                ))));
            }
        };
        domain.push(RebaseStep { sha: s.sha, action });
    }
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.interactive_rebase(base.as_deref(), &domain))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn reset(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    commit_id: String,
    mode: String,
) -> Result<(), IpcError> {
    use git_core::model::ResetMode;
    let mode = match mode.as_str() {
        "soft" => ResetMode::Soft,
        "mixed" => ResetMode::Mixed,
        "hard" => ResetMode::Hard,
        other => {
            return Err(to_ipc(git_core::GitError::Backend(format!(
                "未知 reset 模式: {other}"
            ))));
        }
    };
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.reset(&commit_id, mode))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 撤销/重做的当前可用性(只读),驱动顶栏两个按钮的显隐。
#[tauri::command]
pub(crate) async fn undo_state(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<UndoStateDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.undo_state())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 撤销一步:沿操作时间线后退,reset --soft。改动回暂存区,不丢工作区。
#[tauri::command]
pub(crate) async fn undo(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<UndoStepDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.undo())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 重做一步:沿操作时间线前进,reset --soft。
#[tauri::command]
pub(crate) async fn redo(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<UndoStepDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.redo())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 操作日志(只读):本会话写操作时间线 + 当前光标。
#[tauri::command]
pub(crate) async fn op_log(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<OpLogDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.op_log())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 跳到操作日志第 index 项:reset --soft 过去。
#[tauri::command]
pub(crate) async fn op_goto(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    index: usize,
) -> Result<UndoStepDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.goto(index))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 写入冲突解决后的内容并标记已解决(写文件 + git add)。防目录穿越。
#[tauri::command]
pub(crate) async fn write_resolved(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file: String,
    content: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(&repo_path));
    tokio::task::spawn_blocking(move || -> Result<(), IpcError> {
        let write_err = |m: String| IpcError {
            code: "WRITE_FILE".into(),
            message: m,
            recoverable: true,
        };
        let repo = PathBuf::from(&repo_path);
        let target = repo.join(&file);
        let repo_c = repo.canonicalize().map_err(|e| write_err(e.to_string()))?;
        let target_c = target
            .canonicalize()
            .map_err(|e| write_err(e.to_string()))?;
        if !target_c.starts_with(&repo_c) {
            return Err(write_err("路径越界".into()));
        }
        std::fs::write(&target_c, content).map_err(|e| write_err(e.to_string()))?;
        // 写完即 git add 标记已解决
        ctx.stage(std::path::Path::new(&file)).map_err(to_ipc)
    })
    .await
    .map_err(join_panic)?
}

/// 读冲突文件三方内容(base/ours/theirs)供三栏合并编辑器渲染。
#[tauri::command]
pub(crate) async fn conflict_sides(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file: String,
) -> Result<ConflictSidesDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.conflict_sides(&file))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 读工作区某文件原文(用于冲突文件只读展示)。不经 git,直接 fs;防目录穿越。
#[tauri::command]
pub(crate) async fn read_working_file(repo_path: String, file: String) -> Result<String, IpcError> {
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let repo = PathBuf::from(&repo_path);
        let target = repo.join(&file);
        let repo_c = repo.canonicalize().map_err(|e| e.to_string())?;
        let target_c = target.canonicalize().map_err(|e| e.to_string())?;
        if !target_c.starts_with(&repo_c) {
            return Err("路径越界".into());
        }
        std::fs::read_to_string(&target_c).map_err(|e| e.to_string())
    })
    .await
    .map_err(join_panic)?
    .map_err(|message| IpcError {
        code: "READ_FILE".into(),
        message,
        recoverable: true,
    })
}

/// 读一侧图片的原始字节(M6.2:不再走 base64-in-JSON)。字节以 ArrayBuffer 直传前端,
/// 前端转 Blob URL 渲染。`oid` 非空 → 对象库 blob;为空 → 工作区文件(safe_join 防越权)。
#[tauri::command]
pub(crate) async fn read_image(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    oid: String,
    path: String,
) -> Result<tauri::ipc::Response, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    let bytes = tokio::task::spawn_blocking(move || ctx.read_image_bytes(&oid, &path))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub(crate) async fn stash_list(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<Vec<StashDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.stash_list())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn stash_save(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    message: Option<String>,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.stash_save(message.as_deref()))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn stash_apply(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    index: usize,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.stash_apply(index))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn stash_pop(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    index: usize,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.stash_pop(index))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn stash_drop(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    index: usize,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.stash_drop(index))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}
