use crate::*;

#[tauri::command]
pub(crate) async fn get_head_commit(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<CommitDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.head_commit())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// onboarding:在 path 处新建空仓库(不经 context —— 仓库刚诞生)。
#[tauri::command]
pub(crate) async fn init_repo(
    registry: tauri::State<'_, RepoRegistry>,
    path: String,
) -> Result<(), IpcError> {
    let backend = registry.backend_arc();
    tokio::task::spawn_blocking(move || {
        app_service::RepoService::new(backend).init_repo(&PathBuf::from(path))
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

/// onboarding:把 url 克隆进 parent_dir,返回克隆出的仓库根路径(前端拿去打开)。
#[tauri::command]
pub(crate) async fn clone_repo(
    registry: tauri::State<'_, RepoRegistry>,
    url: String,
    parent_dir: String,
) -> Result<String, IpcError> {
    let backend = registry.backend_arc();
    tokio::task::spawn_blocking(move || {
        app_service::RepoService::new(backend)
            .clone_repo(&url, &PathBuf::from(parent_dir))
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn get_status(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<StatusDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.status())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn refresh_status(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<StatusDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.refresh_status())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn stage_file(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file_path: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.stage(&PathBuf::from(file_path)))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn unstage_file(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file_path: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.unstage(&PathBuf::from(file_path)))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn stage_hunk(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file: String,
    hunk_index: usize,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.stage_hunk(&file, hunk_index))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn stage_lines(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file: String,
    hunk_index: usize,
    lines: Vec<usize>,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.stage_lines(&file, hunk_index, &lines))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn unstage_hunk(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file: String,
    hunk_index: usize,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.unstage_hunk(&file, hunk_index))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn commit(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    message: String,
) -> Result<String, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.commit(&message))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn amend_commit(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    message: Option<String>,
) -> Result<String, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.amend_commit(message.as_deref()))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}
