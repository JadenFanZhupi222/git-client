use std::path::PathBuf;
use std::sync::Arc;
use app_service::RepoService;
use git_engine::Git2Backend; // 真实后端
use ipc_types::{CommitDto, IpcError, StatusDto};

// 把领域错误翻译成给前端的结构化错误(带 code,前端可据此做分支)
fn to_ipc(e: git_core::GitError) -> IpcError {
    use git_core::GitError::*;
    let (code, recoverable) = match &e {
        RepoNotFound(_) => ("REPO_NOT_FOUND", false),
        NoHead => ("NO_HEAD", false),
        Cancelled => ("CANCELLED", true),
        NothingToCommit    => ("NOTHING_TO_COMMIT", false),
        EmptyCommitMessage => ("EMPTY_COMMIT_MESSAGE", false),
        EmptySignature     => ("EMPTY_SIGNATURE", false),
        Backend(_) => ("BACKEND", true),
    };
    IpcError { code: code.into(), message: e.to_string(), recoverable }
}

/// 命令层:极薄。只做"接参数 → 丢阻塞线程池调 service → 返回"。
///
/// 关键铁律:git2 是同步阻塞的,绝不能在 async 命令里直接调,
/// 否则卡死整个 tokio 运行时、UI 冻结。一律 spawn_blocking。
#[tauri::command]
async fn get_head_commit(repo_path: String) -> Result<CommitDto, IpcError> {
    let result = tokio::task::spawn_blocking(move || {
        // 在阻塞线程里:注入真实后端,执行用例
        let service = RepoService::new(Arc::new(Git2Backend::default()));
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
        let service = RepoService::new(Arc::new(Git2Backend::default()));
        service.status(&PathBuf::from(repo_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn stage_file(repo_path: String, file_path: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend::default()));
        service.stage(&PathBuf::from(repo_path), &PathBuf::from(file_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn unstage_file(repo_path: String, file_path: String) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend::default()));
        service.unstage(&PathBuf::from(repo_path), &PathBuf::from(file_path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
async fn commit(repo_path: String, message: String) -> Result<String, IpcError> {
    tokio::task::spawn_blocking(move || {
        let service = RepoService::new(Arc::new(Git2Backend::default()));
        service.commit(&PathBuf::from(repo_path), &message)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init()) // 选目录对话框用
        .invoke_handler(tauri::generate_handler![
            get_head_commit,
            get_status,
            stage_file,
            unstage_file,
            commit
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
