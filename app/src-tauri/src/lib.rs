use app_service::RepoService;
use app_service::watcher::{ChangeKind, RepoWatcher};
use git_engine::Git2Backend; // 真实后端
use ipc_types::{
    BranchDto, CommitDto, FileChangeDto, FileDiffDto, GraphRowDto, IpcError, StatusDto,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;

/// 持有当前仓库的文件监听器。切仓库时替换 → 旧 watcher 被 drop → 自动停止监听。
#[derive(Default)]
struct WatcherState(Mutex<Option<RepoWatcher>>);

// 把领域错误翻译成给前端的结构化错误(带 code,前端可据此做分支)
fn to_ipc(e: git_core::GitError) -> IpcError {
    use git_core::GitError::*;
    let (code, recoverable) = match &e {
        RepoNotFound(_) => ("REPO_NOT_FOUND", false),
        NoHead => ("NO_HEAD", false),
        Cancelled => ("CANCELLED", true),
        NothingToCommit => ("NOTHING_TO_COMMIT", false),
        EmptyCommitMessage => ("EMPTY_COMMIT_MESSAGE", false),
        EmptySignature => ("EMPTY_SIGNATURE", false),
        BranchNotFound(_) => ("BRANCH_NOT_FOUND", false),
        CheckoutConflict => ("CHECKOUT_CONFLICT", true),
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
        let service = RepoService::new(Arc::new(Git2Backend));
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
        let service = RepoService::new(Arc::new(Git2Backend));
        service.status(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn stage_file(repo_path: String, file_path: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend));
        service.stage(&PathBuf::from(repo_path), &PathBuf::from(file_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn unstage_file(repo_path: String, file_path: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend));
        service.unstage(&PathBuf::from(repo_path), &PathBuf::from(file_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn commit(repo_path: String, message: String) -> Result<String, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend));
        service.commit(&PathBuf::from(repo_path), &message)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_log(repo_path: String, limit: usize, skip: usize) -> Result<Vec<CommitDto>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend));
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
        let service = RepoService::new(Arc::new(Git2Backend));
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
        let service = RepoService::new(Arc::new(Git2Backend));
        service.commit_file_diff(&PathBuf::from(repo_path), &commit_id, &file)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_commit_graph(repo_path: String, limit: usize) -> Result<Vec<GraphRowDto>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend));
        service.commit_graph(&PathBuf::from(repo_path), limit)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn get_current_branch(repo_path: String) -> Result<Option<String>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend));
        service.current_branch(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn list_branches(repo_path: String) -> Result<Vec<BranchDto>, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend));
        service.branches(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn checkout_branch(repo_path: String, name: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend));
        service.checkout_branch(&PathBuf::from(repo_path), &name)
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
        .invoke_handler(tauri::generate_handler![
            get_head_commit,
            get_status,
            stage_file,
            unstage_file,
            commit,
            get_log,
            get_commit_files,
            get_commit_file_diff,
            get_commit_graph,
            get_current_branch,
            list_branches,
            checkout_branch,
            watch_repo
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
