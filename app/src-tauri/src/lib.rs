use app_service::RepoService;
use app_service::watcher::{ChangeKind, RepoWatcher};
use git_engine::CompositeBackend; // 生产后端:git2(本地)+ cli(网络)组合
use ipc_types::{
    AheadBehindDto, BlameLineDto, BranchDto, CommitDto, ConflictSidesDto, FetchResultDto,
    FileChangeDto, FileDiffDto, GraphRowDto, IpcError, PullResultDto, PushResultDto, StashDto,
    StatusDto,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;

/// 持有当前仓库的文件监听器。切仓库时替换 → 旧 watcher 被 drop → 自动停止监听。
#[derive(Default)]
struct WatcherState(Mutex<Option<RepoWatcher>>);

/// 搜索代次:每次搜索领一个递增号;后来的搜索把全局号推进,使先前那次的
/// 闭包检测到 `当前 != 自己的号` → 返回 Cancelled,尽快放手不再扫历史。
#[derive(Default)]
struct SearchGen(Arc<AtomicU64>);

// 把领域错误翻译成给前端的结构化错误(带 code,前端可据此做分支)
fn to_ipc(e: git_core::GitError) -> IpcError {
    use git_core::GitError::*;
    let (code, recoverable) = match &e {
        RepoNotFound(_) => ("REPO_NOT_FOUND", false),
        NoHead => ("NO_HEAD", false),
        Cancelled => ("CANCELLED", true),
        NothingToCommit => ("NOTHING_TO_COMMIT", false),
        NothingToStash => ("NOTHING_TO_STASH", false),
        EmptyCommitMessage => ("EMPTY_COMMIT_MESSAGE", false),
        EmptySignature => ("EMPTY_SIGNATURE", false),
        BranchNotFound(_) => ("BRANCH_NOT_FOUND", false),
        BranchAlreadyExists(_) => ("BRANCH_EXISTS", false),
        InvalidBranchName => ("INVALID_BRANCH_NAME", false),
        CannotDeleteCurrentBranch => ("CANNOT_DELETE_CURRENT", false),
        TagAlreadyExists(_) => ("TAG_EXISTS", false),
        CheckoutConflict => ("CHECKOUT_CONFLICT", true),
        GitCliNotFound => ("GIT_CLI_NOT_FOUND", false),
        AuthFailed => ("AUTH_FAILED", true),
        NetworkError => ("NETWORK_ERROR", true),
        NoRemote => ("NO_REMOTE", false),
        NoUpstream => ("NO_UPSTREAM", false),
        PushRejected => ("PUSH_REJECTED", true),
        MergeConflict { .. } => ("MERGE_CONFLICT", true),
        Unsupported => ("UNSUPPORTED", false),
        Backend(_) => ("BACKEND", true),
    };
    IpcError {
        code: code.into(),
        message: e.to_string(),
        recoverable,
    }
}

/// 命令层:极薄。只做"接参数 → 丢阻塞线程池调 service → 返回"。
///
/// 关键铁律:git2 是同步阻塞的,绝不能在 async 命令里直接调,
/// 否则卡死整个 tokio 运行时、UI 冻结。一律 spawn_blocking。
#[tauri::command]
async fn get_head_commit(repo_path: String) -> Result<CommitDto, IpcError> {
    let result = tokio::task::spawn_blocking(move || {
        // 在阻塞线程里:注入真实后端,执行用例
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.head_commit(&PathBuf::from(repo_path))
    })
    .await
    // spawn_blocking 自身失败(线程 panic 等)→ 也转成可识别的错误,绝不让进程崩
    .map_err(|join_err| IpcError {
        code: "TASK_PANIC".into(),
        message: format!("后台任务异常: {join_err}"),
        recoverable: true,
    })?;

    result.map_err(to_ipc)
}

/// spawn_blocking 自身失败(线程 panic)→ 统一转可识别错误,绝不让进程崩。
fn join_panic(e: tokio::task::JoinError) -> IpcError {
    IpcError {
        code: "TASK_PANIC".into(),
        message: format!("后台任务异常: {e}"),
        recoverable: true,
    }
}

#[tauri::command]
async fn get_status(repo_path: String) -> Result<StatusDto, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.status(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn stage_file(repo_path: String, file_path: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.stage(&PathBuf::from(repo_path), &PathBuf::from(file_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn unstage_file(repo_path: String, file_path: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.unstage(&PathBuf::from(repo_path), &PathBuf::from(file_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn stage_hunk(repo_path: String, file: String, hunk_index: usize) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.stage_hunk(&PathBuf::from(repo_path), &file, hunk_index)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn stage_lines(
    repo_path: String,
    file: String,
    hunk_index: usize,
    lines: Vec<usize>,
) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.stage_lines(&PathBuf::from(repo_path), &file, hunk_index, &lines)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn unstage_hunk(repo_path: String, file: String, hunk_index: usize) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.unstage_hunk(&PathBuf::from(repo_path), &file, hunk_index)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn commit(repo_path: String, message: String) -> Result<String, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.commit(&PathBuf::from(repo_path), &message)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_log(repo_path: String, limit: usize, skip: usize) -> Result<Vec<CommitDto>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.log(&PathBuf::from(repo_path), limit, skip)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_commit_files(
    repo_path: String,
    commit_id: String,
) -> Result<Vec<FileChangeDto>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.commit_files(&PathBuf::from(repo_path), &commit_id)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_commit_file_diff(
    repo_path: String,
    commit_id: String,
    file: String,
) -> Result<FileDiffDto, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.commit_file_diff(&PathBuf::from(repo_path), &commit_id, &file)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_working_diff(
    repo_path: String,
    file: String,
    staged: bool,
) -> Result<FileDiffDto, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.working_diff(&PathBuf::from(repo_path), &file, staged)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_commit_graph(repo_path: String, limit: usize) -> Result<Vec<GraphRowDto>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.commit_graph(&PathBuf::from(repo_path), limit)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn search_commits(
    state: tauri::State<'_, SearchGen>,
    repo_path: String,
    query: String,
    limit: usize,
) -> Result<Vec<CommitDto>, IpcError> {
    // 领号:本次搜索的代次。后来的搜索 fetch_add 会推进全局号,使本次的闭包失效。
    let generation = state.0.clone();
    let mine = generation.fetch_add(1, Ordering::SeqCst) + 1;
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        let cancelled = || generation.load(Ordering::SeqCst) != mine;
        service.search_commits(&PathBuf::from(repo_path), &query, limit, &cancelled)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_current_branch(repo_path: String) -> Result<Option<String>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.current_branch(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn list_branches(repo_path: String) -> Result<Vec<BranchDto>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.branches(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_ahead_behind(repo_path: String) -> Result<Option<AheadBehindDto>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.ahead_behind(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_remotes(repo_path: String) -> Result<Vec<String>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.remotes(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn set_upstream(repo_path: String, upstream: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.set_upstream(&PathBuf::from(repo_path), &upstream)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn checkout_branch(repo_path: String, name: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.checkout_branch(&PathBuf::from(repo_path), &name)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn create_branch(repo_path: String, name: String, checkout: bool) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.create_branch(&PathBuf::from(repo_path), &name, checkout)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn delete_branch(repo_path: String, name: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.delete_branch(&PathBuf::from(repo_path), &name)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn fetch(repo_path: String, remote: Option<String>) -> Result<FetchResultDto, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.fetch(&PathBuf::from(repo_path), remote.as_deref())
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn pull(
    repo_path: String,
    remote: Option<String>,
    rebase: bool,
) -> Result<PullResultDto, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.pull(&PathBuf::from(repo_path), remote.as_deref(), rebase)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn push(repo_path: String, remote: Option<String>) -> Result<PushResultDto, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.push(&PathBuf::from(repo_path), remote.as_deref())
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_repo_state(repo_path: String) -> Result<String, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.repo_state(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn resolve_ours(repo_path: String, file: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.resolve_ours(&PathBuf::from(repo_path), &file)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn resolve_theirs(repo_path: String, file: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.resolve_theirs(&PathBuf::from(repo_path), &file)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn continue_op(repo_path: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.continue_op(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn abort_op(repo_path: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.abort_op(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn blame(repo_path: String, file: String) -> Result<Vec<BlameLineDto>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.blame(&PathBuf::from(repo_path), &file)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn cherry_pick(repo_path: String, commit_id: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.cherry_pick(&PathBuf::from(repo_path), &commit_id)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn revert(repo_path: String, commit_id: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.revert(&PathBuf::from(repo_path), &commit_id)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn create_tag(
    repo_path: String,
    name: String,
    commit_id: String,
    message: Option<String>,
) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.create_tag(&PathBuf::from(repo_path), &name, &commit_id, message.as_deref())
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn delete_tag(repo_path: String, name: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.delete_tag(&PathBuf::from(repo_path), &name)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn reset(repo_path: String, commit_id: String, mode: String) -> Result<(), IpcError> {
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
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.reset(&PathBuf::from(repo_path), &commit_id, mode)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

/// 写入冲突解决后的内容并标记已解决(写文件 + git add)。防目录穿越。
#[tauri::command]
async fn write_resolved(repo_path: String, file: String, content: String) -> Result<(), IpcError> {
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
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service
            .stage(&repo, std::path::Path::new(&file))
            .map_err(to_ipc)
    })
    .await
    .map_err(join_panic)?
}

/// 读冲突文件三方内容(base/ours/theirs)供三栏合并编辑器渲染。
#[tauri::command]
async fn conflict_sides(repo_path: String, file: String) -> Result<ConflictSidesDto, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.conflict_sides(&PathBuf::from(repo_path), &file)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

/// 读工作区某文件原文(用于冲突文件只读展示)。不经 git,直接 fs;防目录穿越。
#[tauri::command]
async fn read_working_file(repo_path: String, file: String) -> Result<String, IpcError> {
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

#[tauri::command]
async fn stash_list(repo_path: String) -> Result<Vec<StashDto>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.stash_list(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn stash_save(repo_path: String, message: Option<String>) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.stash_save(&PathBuf::from(repo_path), message.as_deref())
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn stash_apply(repo_path: String, index: usize) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.stash_apply(&PathBuf::from(repo_path), index)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn stash_pop(repo_path: String, index: usize) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.stash_pop(&PathBuf::from(repo_path), index)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn stash_drop(repo_path: String, index: usize) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(CompositeBackend::default()));
        service.stash_drop(&PathBuf::from(repo_path), index)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

/// 开始监听某仓库的文件变化。变化经 debounce + 分类后,
/// 通过 `repo-changed` 事件通知前端(payload: "worktree" | "index" | "ref")。
/// 这个命令很快(只注册 OS 监听),无需 spawn_blocking。
#[tauri::command]
fn watch_repo(
    app: tauri::AppHandle,
    state: tauri::State<'_, WatcherState>,
    repo_path: String,
) -> Result<(), IpcError> {
    let watcher = RepoWatcher::new(
        PathBuf::from(&repo_path),
        Duration::from_millis(200),
        move |kind| {
            let label = match kind {
                ChangeKind::WorkingTree => "worktree",
                ChangeKind::Index => "index",
                ChangeKind::GitRef => "ref",
            };
            let _ = app.emit("repo-changed", label);
        },
    )
    .map_err(|e| IpcError {
        code: "WATCH_FAILED".into(),
        message: format!("启动文件监听失败: {e}"),
        recoverable: true,
    })?;
    // 替换旧 watcher(若有):旧的在这里被 drop,停止其监听与后台线程。
    *state.0.lock().unwrap() = Some(watcher);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init()) // 选目录对话框用
        .manage(WatcherState::default())
        .manage(SearchGen::default())
        .invoke_handler(tauri::generate_handler![
            get_head_commit,
            get_status,
            stage_file,
            unstage_file,
            stage_hunk,
            unstage_hunk,
            stage_lines,
            commit,
            get_log,
            get_commit_files,
            get_commit_file_diff,
            get_working_diff,
            get_commit_graph,
            search_commits,
            get_current_branch,
            list_branches,
            get_ahead_behind,
            get_remotes,
            set_upstream,
            checkout_branch,
            create_branch,
            delete_branch,
            fetch,
            pull,
            push,
            get_repo_state,
            resolve_ours,
            resolve_theirs,
            continue_op,
            abort_op,
            cherry_pick,
            revert,
            create_tag,
            delete_tag,
            reset,
            blame,
            conflict_sides,
            read_working_file,
            write_resolved,
            stash_list,
            stash_save,
            stash_apply,
            stash_pop,
            stash_drop,
            watch_repo
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
