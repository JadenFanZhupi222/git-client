use crate::*;

#[tauri::command]
pub(crate) async fn get_log(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    limit: usize,
    skip: usize,
) -> Result<Vec<CommitDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.log(limit, skip))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn file_history(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file: String,
    limit: usize,
) -> Result<Vec<CommitDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.file_history(&file, limit))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn pickaxe(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    query: String,
    regex: bool,
    limit: usize,
) -> Result<Vec<CommitDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.pickaxe(&query, regex, limit))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn line_history(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file: String,
    start: u32,
    end: u32,
) -> Result<Vec<LineHistoryEntryDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.line_history(&file, start, end))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn get_commit_files(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    commit_id: String,
) -> Result<Vec<FileChangeDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.commit_files(&commit_id))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn get_commit_file_diff(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    commit_id: String,
    file: String,
) -> Result<FileDiffDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.commit_file_diff(&commit_id, &file))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn get_commit_signature(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    commit_id: String,
) -> Result<SignatureInfoDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.commit_signature(&commit_id))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 列出子模块(路径/URL/提交/状态)。只读;走 CLI(`git submodule status` + `.gitmodules`)。
#[tauri::command]
pub(crate) async fn list_submodules(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<Vec<SubmoduleInfoDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.list_submodules())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 初始化并更新某子模块到记录提交(`git submodule update --init`)。可能联网,故 spawn_blocking。
#[tauri::command]
pub(crate) async fn update_submodule(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    path: String,
) -> Result<(), IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.update_submodule(&path))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 列出工作树(主 + 链接)。只读;走 CLI(`git worktree list --porcelain`)。
#[tauri::command]
pub(crate) async fn list_worktrees(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<Vec<WorktreeInfoDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.list_worktrees())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

/// 稀疏检出范围规则(只读);未开启稀疏检出 → 空。
#[tauri::command]
pub(crate) async fn sparse_checkout_patterns(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
) -> Result<Vec<String>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.sparse_checkout_patterns())
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn compare_files(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    from: String,
    to: String,
) -> Result<Vec<FileChangeDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.compare_files(&from, &to))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn compare_file_diff(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    from: String,
    to: String,
    file: String,
) -> Result<FileDiffDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.compare_file_diff(&from, &to, &file))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn get_working_diff(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    file: String,
    staged: bool,
) -> Result<FileDiffDto, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.working_diff(&file, staged))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn get_commit_graph(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    limit: usize,
) -> Result<Vec<GraphRowDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.commit_graph(limit))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn search_commits(
    registry: tauri::State<'_, RepoRegistry>,
    gen_state: tauri::State<'_, SearchGen>,
    repo_path: String,
    query: String,
    limit: usize,
) -> Result<Vec<CommitDto>, IpcError> {
    // 领号:本次搜索的代次。后来的搜索 fetch_add 会推进全局号,使本次的闭包失效。
    let generation = gen_state.0.clone();
    let mine = generation.fetch_add(1, Ordering::SeqCst) + 1;
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || {
        let cancelled = || generation.load(Ordering::SeqCst) != mine;
        ctx.search_commits(&query, limit, &cancelled)
    })
    .await
    .map_err(join_panic)?
    .map_err(to_ipc)
}

#[tauri::command]
pub(crate) async fn get_reflog(
    registry: tauri::State<'_, RepoRegistry>,
    repo_path: String,
    limit: usize,
) -> Result<Vec<ReflogEntryDto>, IpcError> {
    let ctx = registry.context(&PathBuf::from(repo_path));
    tokio::task::spawn_blocking(move || ctx.reflog(limit))
        .await
        .map_err(join_panic)?
        .map_err(to_ipc)
}
