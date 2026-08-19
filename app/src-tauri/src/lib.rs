use app_service::RepoRegistry;
#[cfg(feature = "e2e")]
use app_service::RepoService;
use app_service::watcher::{ChangeKind, RepoWatcher};
use git_engine::CompositeBackend; // 生产后端:git2(本地)+ cli(网络)组合
use ipc_types::{
    AheadBehindDto, BlameLineDto, BranchDeleteImpactDto, BranchDto, CommitDto, ConflictSidesDto,
    FetchResultDto, FileChangeDto, FileDiffDto, GraphRowDto, IpcError, LineHistoryEntryDto,
    MergeResultDto, OpLogDto, PullResultDto, PushResultDto, RefDto, ReflogEntryDto, RemoteInfoDto,
    SignatureInfoDto, StashDto, StatusDto, SubmoduleInfoDto, UndoStateDto, UndoStepDto,
    WorktreeInfoDto,
};
use std::path::PathBuf;
#[cfg(feature = "e2e")]
use std::path::{Component, Path};
#[cfg(feature = "e2e")]
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager};

mod agent_events;
mod agent_logging;
mod agent_run_manager;
mod agent_session_commands;
mod agent_store;
mod agent_support;
mod change_commands;
mod credentials;
mod history_commands;
mod review_commands;
use agent_events::{ToolApprovalRegistry, resolve_tool_approval};
use agent_session_commands::{
    AgentSessionState, cancel_agent_goal, cancel_agent_turn, create_agent_goal,
    extend_agent_budget, get_agent_goal, get_agent_session, pause_agent_goal, reset_agent_session,
    resume_agent_goal, start_agent_turn, steer_agent_goal,
};
use change_commands::{analyze_change_plan, cancel_change_plan, commit_change_group};
use credentials::{clear_credential, credential_status, save_credential, test_credential};
use history_commands::{cancel_history_investigation, investigate_repository_history};
use review_commands::{
    ReviewRunRegistry, cancel_issue_triage, cancel_pr_review, get_github_issue_context,
    get_gitlab_review_preflight, get_review_preflight, list_github_issues, list_review_models,
    publish_issue_triage, start_gitlab_mr_review, start_issue_triage, start_pr_review,
    submit_gitlab_mr_review, submit_pr_review,
};

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
        RemoteAlreadyExists(_) => ("REMOTE_EXISTS", false),
        RemoteNotFound(_) => ("REMOTE_NOT_FOUND", false),
        InvalidRemoteName => ("INVALID_REMOTE_NAME", false),
        DestinationNotEmpty(_) => ("DESTINATION_NOT_EMPTY", false),
        InvalidUrl => ("INVALID_URL", false),
        NoUpstream => ("NO_UPSTREAM", false),
        PushRejected => ("PUSH_REJECTED", true),
        MergeConflict { .. } => ("MERGE_CONFLICT", true),
        NothingToUndo => ("NOTHING_TO_UNDO", false),
        NothingToRedo => ("NOTHING_TO_REDO", false),
        UncommittedChanges { .. } => ("UNCOMMITTED_CHANGES", true),
        FileTooLarge { .. } => ("FILE_TOO_LARGE", true),
        BinaryFile => ("BINARY_FILE", true),
        Unsupported => ("UNSUPPORTED", false),
        Backend(_) => ("BACKEND", true),
    };
    IpcError {
        code: code.into(),
        message: e.to_string(),
        recoverable,
    }
}

/// spawn_blocking 自身失败(线程 panic)→ 统一转可识别错误,绝不让进程崩。
fn join_panic(e: tokio::task::JoinError) -> IpcError {
    IpcError {
        code: "TASK_PANIC".into(),
        message: format!("后台任务异常: {e}"),
        recoverable: true,
    }
}

#[cfg(feature = "e2e")]
fn e2e_error(code: &str, message: impl Into<String>) -> IpcError {
    IpcError {
        code: code.into(),
        message: message.into(),
        recoverable: false,
    }
}

#[cfg(feature = "e2e")]
fn configured_e2e_root() -> Result<PathBuf, IpcError> {
    std::env::var_os("GIT_CLIENT_E2E_ROOT")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            e2e_error(
                "E2E_ROOT_NOT_CONFIGURED",
                "GIT_CLIENT_E2E_ROOT must be configured by the desktop test harness",
            )
        })
}

#[cfg(feature = "e2e")]
fn safe_e2e_join(root: &Path, relative: &str) -> Result<PathBuf, IpcError> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(e2e_error(
            "E2E_PATH_OUTSIDE_ROOT",
            "E2E fixture path must be a non-empty relative path without traversal",
        ));
    }
    Ok(root.join(relative))
}

#[cfg(feature = "e2e")]
fn run_e2e_git(repo: &Path, args: &[&str]) -> Result<(), IpcError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| {
            e2e_error(
                "E2E_GIT_FAILED",
                format!("failed to start git for E2E fixture: {error}"),
            )
        })?;
    if !output.status.success() {
        return Err(e2e_error(
            "E2E_GIT_FAILED",
            format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    Ok(())
}

#[cfg(feature = "e2e")]
fn prepare_e2e_repo_at(root: &Path, run_id: &str) -> Result<PathBuf, IpcError> {
    std::fs::create_dir_all(root).map_err(|error| {
        e2e_error(
            "E2E_FIXTURE_FAILED",
            format!("failed to create E2E root: {error}"),
        )
    })?;
    let root = root.canonicalize().map_err(|error| {
        e2e_error(
            "E2E_FIXTURE_FAILED",
            format!("failed to canonicalize E2E root: {error}"),
        )
    })?;
    let repo = safe_e2e_join(&root, run_id)?;
    std::fs::create_dir(&repo).map_err(|error| {
        e2e_error(
            "E2E_FIXTURE_FAILED",
            format!("failed to create isolated E2E repository: {error}"),
        )
    })?;
    let repo = repo.canonicalize().map_err(|error| {
        e2e_error(
            "E2E_FIXTURE_FAILED",
            format!("failed to canonicalize E2E repository: {error}"),
        )
    })?;
    if !repo.starts_with(&root) {
        return Err(e2e_error(
            "E2E_PATH_OUTSIDE_ROOT",
            "E2E repository escaped its fixture root",
        ));
    }

    RepoService::new(Arc::new(CompositeBackend::default()))
        .init_repo(&repo)
        .map_err(to_ipc)?;
    run_e2e_git(&repo, &["config", "user.name", "VersionArc E2E"])?;
    run_e2e_git(&repo, &["config", "user.email", "e2e@versionarc.invalid"])?;
    Ok(repo)
}

#[cfg(feature = "e2e")]
fn write_e2e_file_at(
    root: &Path,
    run_id: &str,
    relative: &str,
    contents: &str,
) -> Result<(), IpcError> {
    let root = root.canonicalize().map_err(|error| {
        e2e_error(
            "E2E_FIXTURE_FAILED",
            format!("failed to canonicalize E2E root: {error}"),
        )
    })?;
    let repo = safe_e2e_join(&root, run_id)?;
    let repo = repo.canonicalize().map_err(|error| {
        e2e_error(
            "E2E_FIXTURE_FAILED",
            format!("failed to canonicalize E2E repository: {error}"),
        )
    })?;
    if !repo.starts_with(&root) {
        return Err(e2e_error(
            "E2E_PATH_OUTSIDE_ROOT",
            "E2E repository escaped its configured fixture root",
        ));
    }
    if !repo.join(".git").is_dir() {
        return Err(e2e_error(
            "E2E_NOT_A_REPOSITORY",
            "E2E fixture target is not a Git repository",
        ));
    }

    let target = safe_e2e_join(&repo, relative)?;
    let parent = target.parent().ok_or_else(|| {
        e2e_error(
            "E2E_PATH_OUTSIDE_ROOT",
            "E2E fixture file has no parent directory",
        )
    })?;
    std::fs::create_dir_all(parent).map_err(|error| {
        e2e_error(
            "E2E_FIXTURE_FAILED",
            format!("failed to create E2E fixture directory: {error}"),
        )
    })?;
    let parent = parent.canonicalize().map_err(|error| {
        e2e_error(
            "E2E_FIXTURE_FAILED",
            format!("failed to canonicalize E2E fixture directory: {error}"),
        )
    })?;
    if !parent.starts_with(&repo) {
        return Err(e2e_error(
            "E2E_PATH_OUTSIDE_ROOT",
            "E2E fixture file escaped its repository",
        ));
    }
    if target.exists() {
        let canonical_target = target.canonicalize().map_err(|error| {
            e2e_error(
                "E2E_FIXTURE_FAILED",
                format!("failed to canonicalize existing E2E fixture file: {error}"),
            )
        })?;
        if !canonical_target.starts_with(&repo) {
            return Err(e2e_error(
                "E2E_PATH_OUTSIDE_ROOT",
                "E2E fixture file escaped its repository",
            ));
        }
    }
    std::fs::write(target, contents).map_err(|error| {
        e2e_error(
            "E2E_FIXTURE_FAILED",
            format!("failed to write E2E fixture file: {error}"),
        )
    })
}

#[cfg(feature = "e2e")]
#[tauri::command]
async fn e2e_prepare_repo(run_id: String) -> Result<String, IpcError> {
    tokio::task::spawn_blocking(move || {
        let root = configured_e2e_root()?;
        prepare_e2e_repo_at(&root, &run_id).map(|path| path.to_string_lossy().into_owned())
    })
    .await
    .map_err(join_panic)?
}

#[cfg(feature = "e2e")]
#[tauri::command]
async fn e2e_write_file(
    run_id: String,
    relative_path: String,
    contents: String,
) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || {
        let root = configured_e2e_root()?;
        write_e2e_file_at(&root, &run_id, &relative_path, &contents)
    })
    .await
    .map_err(join_panic)?
}

#[cfg(all(test, feature = "e2e"))]
mod e2e_fixture_tests {
    use super::*;

    #[test]
    fn e2e_fixture_rejects_path_traversal() {
        let root = tempfile::tempdir().unwrap();
        let error = safe_e2e_join(root.path(), "../outside.txt").unwrap_err();

        assert_eq!(error.code, "E2E_PATH_OUTSIDE_ROOT");
    }

    #[test]
    fn e2e_fixture_initializes_repo_identity_and_file() {
        let root = tempfile::tempdir().unwrap();
        let repo = prepare_e2e_repo_at(root.path(), "fixture-run").unwrap();

        assert!(repo.join(".git").is_dir());
        let config = std::fs::read_to_string(repo.join(".git/config")).unwrap();
        assert!(config.contains("VersionArc E2E"));
        assert!(config.contains("e2e@versionarc.invalid"));

        write_e2e_file_at(root.path(), "fixture-run", "hello.txt", "hello from e2e\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.join("hello.txt")).unwrap(),
            "hello from e2e\n"
        );
    }

    #[test]
    fn e2e_fixture_cannot_write_to_a_repo_outside_the_configured_root() {
        let trusted_root = tempfile::tempdir().unwrap();
        let outside_root = tempfile::tempdir().unwrap();
        prepare_e2e_repo_at(outside_root.path(), "outside-repo").unwrap();

        let error = write_e2e_file_at(trusted_root.path(), "../outside-repo", "owned.txt", "nope")
            .unwrap_err();

        assert_eq!(error.code, "E2E_PATH_OUTSIDE_ROOT");
        assert!(!outside_root.path().join("outside-repo/owned.txt").exists());
    }
}

/// 命令层:极薄。只做"取长驻上下文 → 丢阻塞线程池调它 → 返回"。
///
/// 关键铁律:
/// - git2 是同步阻塞的,绝不能在 async 命令里直接调,一律 spawn_blocking。
/// - 仓库上下文从 `RepoRegistry`(Tauri State)取,不再每次 `RepoService::new` +
///   重建后端。`registry.context()` 只在查表那一瞬持锁,返回的 `Arc<RepoContext>`
///   move 进阻塞线程后才跑 git,绝不持锁做阻塞操作。
const TOKEN_SERVICE: &str = "com.versionarc.desktop";
const LEGACY_TOKEN_SERVICE: &str = "com.gitclient.desktop";
const GITHUB_TOKEN_USER: &str = "github-token";
const GITLAB_TOKEN_USER: &str = "gitlab-token";

fn normalize_github_token(token: String) -> Result<String, IpcError> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(IpcError {
            code: "GITHUB_TOKEN_EMPTY".into(),
            message: "GitHub token 不能为空".into(),
            recoverable: false,
        });
    }
    Ok(token)
}

fn normalize_gitlab_token(token: String) -> Result<String, IpcError> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(IpcError {
            code: "GITLAB_TOKEN_EMPTY".into(),
            message: "GitLab token 不能为空".into(),
            recoverable: false,
        });
    }
    Ok(token)
}

fn github_token_missing_error() -> IpcError {
    IpcError {
        code: "GITHUB_TOKEN_MISSING".into(),
        message: "尚未设置 GitHub token".into(),
        recoverable: true,
    }
}

fn gitlab_token_missing_error() -> IpcError {
    IpcError {
        code: "GITLAB_TOKEN_MISSING".into(),
        message: "尚未设置 GitLab token".into(),
        recoverable: true,
    }
}

fn keyring_error(error: keyring::Error) -> IpcError {
    IpcError {
        code: "KEYRING".into(),
        message: error.to_string(),
        recoverable: true,
    }
}

fn github_token_entry() -> Result<keyring::Entry, IpcError> {
    keyring::Entry::new(TOKEN_SERVICE, GITHUB_TOKEN_USER).map_err(keyring_error)
}

fn gitlab_token_entry() -> Result<keyring::Entry, IpcError> {
    keyring::Entry::new(TOKEN_SERVICE, GITLAB_TOKEN_USER).map_err(keyring_error)
}

fn migrated_token(user: &str) -> Result<Option<String>, IpcError> {
    let current = keyring::Entry::new(TOKEN_SERVICE, user).map_err(keyring_error)?;
    match current.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => {
            let legacy = keyring::Entry::new(LEGACY_TOKEN_SERVICE, user).map_err(keyring_error)?;
            match legacy.get_password() {
                Ok(token) => {
                    current.set_password(&token).map_err(keyring_error)?;
                    Ok(Some(token))
                }
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(error) => Err(keyring_error(error)),
            }
        }
        Err(error) => Err(keyring_error(error)),
    }
}

fn clear_migrated_token(user: &str) -> Result<(), IpcError> {
    for service in [TOKEN_SERVICE, LEGACY_TOKEN_SERVICE] {
        match keyring::Entry::new(service, user)
            .map_err(keyring_error)?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(error) => return Err(keyring_error(error)),
        }
    }
    Ok(())
}

#[tauri::command]
async fn set_github_token(token: String) -> Result<(), IpcError> {
    let token = normalize_github_token(token)?;
    tokio::task::spawn_blocking(move || {
        github_token_entry()?
            .set_password(&token)
            .map_err(keyring_error)
    })
    .await
    .map_err(join_panic)?
}

#[tauri::command]
async fn has_github_token() -> Result<bool, IpcError> {
    tokio::task::spawn_blocking(move || {
        migrated_token(GITHUB_TOKEN_USER).map(|token| token.is_some())
    })
    .await
    .map_err(join_panic)?
}

#[tauri::command]
async fn get_github_token() -> Result<String, IpcError> {
    tokio::task::spawn_blocking(move || {
        migrated_token(GITHUB_TOKEN_USER)?.ok_or_else(github_token_missing_error)
    })
    .await
    .map_err(join_panic)?
}

#[tauri::command]
async fn clear_github_token() -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || clear_migrated_token(GITHUB_TOKEN_USER))
        .await
        .map_err(join_panic)?
}

#[tauri::command]
async fn set_gitlab_token(token: String) -> Result<(), IpcError> {
    let token = normalize_gitlab_token(token)?;
    tokio::task::spawn_blocking(move || {
        gitlab_token_entry()?
            .set_password(&token)
            .map_err(keyring_error)
    })
    .await
    .map_err(join_panic)?
}

#[tauri::command]
async fn has_gitlab_token() -> Result<bool, IpcError> {
    tokio::task::spawn_blocking(move || {
        migrated_token(GITLAB_TOKEN_USER).map(|token| token.is_some())
    })
    .await
    .map_err(join_panic)?
}

#[tauri::command]
async fn get_gitlab_token() -> Result<String, IpcError> {
    tokio::task::spawn_blocking(move || {
        migrated_token(GITLAB_TOKEN_USER)?.ok_or_else(gitlab_token_missing_error)
    })
    .await
    .map_err(join_panic)?
}

#[tauri::command]
async fn clear_gitlab_token() -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || clear_migrated_token(GITLAB_TOKEN_USER))
        .await
        .map_err(join_panic)?
}

#[cfg(test)]
mod github_token_tests {
    use super::*;

    #[test]
    fn normalize_github_token_trims_non_empty_tokens() {
        assert_eq!(
            normalize_github_token("  ghp_secret  ".to_string()).unwrap(),
            "ghp_secret"
        );
    }

    #[test]
    fn normalize_github_token_rejects_empty_tokens() {
        let err = normalize_github_token("   ".to_string()).unwrap_err();
        assert_eq!(err.code, "GITHUB_TOKEN_EMPTY");
        assert!(!err.recoverable);
    }

    #[test]
    fn normalize_gitlab_token_rejects_empty_tokens() {
        let err = normalize_gitlab_token("   ".to_string()).unwrap_err();
        assert_eq!(err.code, "GITLAB_TOKEN_EMPTY");
        assert!(!err.recoverable);
    }

    #[test]
    fn github_token_missing_error_is_recoverable() {
        let err = github_token_missing_error();
        assert_eq!(err.code, "GITHUB_TOKEN_MISSING");
        assert!(err.recoverable);
    }

    #[test]
    fn gitlab_token_missing_error_is_recoverable() {
        let err = gitlab_token_missing_error();
        assert_eq!(err.code, "GITLAB_TOKEN_MISSING");
        assert!(err.recoverable);
    }
}

mod git_commands;

/// 开始监听某仓库的文件变化。变化经 debounce + 分类后,
/// 通过 `repo-changed` 事件通知前端(payload: "worktree" | "index" | "ref")。
/// 这个命令很快(只注册 OS 监听),无需 spawn_blocking。
#[tauri::command]
fn watch_repo(
    app: tauri::AppHandle,
    registry: tauri::State<'_, RepoRegistry>,
    state: tauri::State<'_, WatcherState>,
    repo_path: String,
) -> Result<(), IpcError> {
    // 失效源 #2(外部改动:IDE 改文件 / 命令行 git):变化经分类后,先失效该仓库
    // 的后端读缓存(M1.2),再 emit 给前端触发重取。把长驻上下文的 Arc move 进回调
    // (回调在 watcher 后台线程跑,缓存 Mutex 保护,安全)。
    let ctx = registry.context(&PathBuf::from(&repo_path));
    let watcher = RepoWatcher::new(
        PathBuf::from(&repo_path),
        Duration::from_millis(200),
        move |kind| {
            ctx.invalidate(kind);
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
    // 整个应用一个共享后端,启动时建一次;按仓库路由的长驻上下文由 RepoRegistry 管理。
    let registry = RepoRegistry::new(Arc::new(CompositeBackend::default()));
    let builder = tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            if let Some(log_path) = agent_logging::init(&app_data_dir) {
                tracing::info!(
                    log_path = %log_path.display(),
                    contains_prompts = false,
                    contains_credentials = false,
                    "application diagnostics initialized"
                );
            }
            Ok(())
        })
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init()) // 选目录对话框用
        .manage(registry)
        .manage(WatcherState::default())
        .manage(SearchGen::default())
        .manage(ReviewRunRegistry::default())
        .manage(AgentSessionState::default())
        .manage(ToolApprovalRegistry::default());

    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    #[cfg(feature = "e2e")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        git_commands::repository::discover_repo,
        git_commands::repository::init_repo,
        git_commands::repository::clone_repo,
        git_commands::repository::get_head_commit,
        git_commands::repository::get_status,
        git_commands::repository::refresh_status,
        git_commands::repository::stage_file,
        git_commands::repository::unstage_file,
        git_commands::repository::stage_hunk,
        git_commands::repository::unstage_hunk,
        git_commands::repository::stage_lines,
        git_commands::repository::commit,
        git_commands::repository::amend_commit,
        git_commands::history::get_log,
        git_commands::history::file_history,
        git_commands::history::line_history,
        git_commands::history::pickaxe,
        git_commands::history::get_commit_files,
        git_commands::history::get_commit_file_diff,
        git_commands::history::get_commit_signature,
        git_commands::history::list_submodules,
        git_commands::history::update_submodule,
        git_commands::history::list_worktrees,
        git_commands::history::sparse_checkout_patterns,
        git_commands::history::get_working_diff,
        git_commands::history::get_commit_graph,
        git_commands::history::search_commits,
        git_commands::history::get_reflog,
        git_commands::history::compare_files,
        git_commands::history::compare_file_diff,
        git_commands::worktree::get_current_branch,
        git_commands::worktree::list_branches,
        git_commands::worktree::get_ahead_behind,
        git_commands::worktree::get_remotes,
        git_commands::worktree::list_refs,
        git_commands::worktree::set_upstream,
        git_commands::worktree::remote_list,
        git_commands::worktree::add_remote,
        git_commands::worktree::remove_remote,
        git_commands::worktree::rename_remote,
        git_commands::worktree::checkout_branch,
        git_commands::worktree::create_branch,
        git_commands::worktree::delete_branch,
        git_commands::worktree::merge_branch,
        git_commands::worktree::branch_delete_impact,
        git_commands::worktree::fetch,
        git_commands::worktree::pull,
        git_commands::worktree::push,
        git_commands::worktree::get_repo_state,
        git_commands::worktree::resolve_ours,
        git_commands::worktree::resolve_theirs,
        git_commands::worktree::continue_op,
        git_commands::worktree::abort_op,
        git_commands::worktree::cherry_pick,
        git_commands::worktree::revert,
        git_commands::worktree::create_tag,
        git_commands::worktree::delete_tag,
        git_commands::worktree::reset,
        git_commands::worktree::undo_state,
        git_commands::worktree::undo,
        git_commands::worktree::redo,
        git_commands::worktree::op_log,
        git_commands::worktree::op_goto,
        git_commands::worktree::interactive_rebase,
        git_commands::worktree::blame,
        git_commands::worktree::conflict_sides,
        git_commands::worktree::read_working_file,
        git_commands::worktree::read_image,
        git_commands::worktree::write_resolved,
        git_commands::worktree::stash_list,
        git_commands::worktree::stash_save,
        git_commands::worktree::stash_apply,
        git_commands::worktree::stash_pop,
        git_commands::worktree::stash_drop,
        set_github_token,
        has_github_token,
        get_github_token,
        clear_github_token,
        set_gitlab_token,
        has_gitlab_token,
        get_gitlab_token,
        clear_gitlab_token,
        credential_status,
        save_credential,
        clear_credential,
        test_credential,
        list_review_models,
        get_review_preflight,
        get_gitlab_review_preflight,
        start_pr_review,
        start_gitlab_mr_review,
        cancel_pr_review,
        resolve_tool_approval,
        get_agent_session,
        get_agent_goal,
        create_agent_goal,
        steer_agent_goal,
        pause_agent_goal,
        resume_agent_goal,
        cancel_agent_goal,
        extend_agent_budget,
        reset_agent_session,
        start_agent_turn,
        cancel_agent_turn,
        submit_pr_review,
        submit_gitlab_mr_review,
        list_github_issues,
        get_github_issue_context,
        start_issue_triage,
        cancel_issue_triage,
        publish_issue_triage,
        analyze_change_plan,
        cancel_change_plan,
        commit_change_group,
        investigate_repository_history,
        cancel_history_investigation,
        watch_repo,
        e2e_prepare_repo,
        e2e_write_file
    ]);

    #[cfg(not(feature = "e2e"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        git_commands::repository::discover_repo,
        git_commands::repository::init_repo,
        git_commands::repository::clone_repo,
        git_commands::repository::get_head_commit,
        git_commands::repository::get_status,
        git_commands::repository::refresh_status,
        git_commands::repository::stage_file,
        git_commands::repository::unstage_file,
        git_commands::repository::stage_hunk,
        git_commands::repository::unstage_hunk,
        git_commands::repository::stage_lines,
        git_commands::repository::commit,
        git_commands::repository::amend_commit,
        git_commands::history::get_log,
        git_commands::history::file_history,
        git_commands::history::line_history,
        git_commands::history::pickaxe,
        git_commands::history::get_commit_files,
        git_commands::history::get_commit_file_diff,
        git_commands::history::get_commit_signature,
        git_commands::history::list_submodules,
        git_commands::history::update_submodule,
        git_commands::history::list_worktrees,
        git_commands::history::sparse_checkout_patterns,
        git_commands::history::get_working_diff,
        git_commands::history::get_commit_graph,
        git_commands::history::search_commits,
        git_commands::history::get_reflog,
        git_commands::history::compare_files,
        git_commands::history::compare_file_diff,
        git_commands::worktree::get_current_branch,
        git_commands::worktree::list_branches,
        git_commands::worktree::get_ahead_behind,
        git_commands::worktree::get_remotes,
        git_commands::worktree::list_refs,
        git_commands::worktree::set_upstream,
        git_commands::worktree::remote_list,
        git_commands::worktree::add_remote,
        git_commands::worktree::remove_remote,
        git_commands::worktree::rename_remote,
        git_commands::worktree::checkout_branch,
        git_commands::worktree::create_branch,
        git_commands::worktree::delete_branch,
        git_commands::worktree::merge_branch,
        git_commands::worktree::branch_delete_impact,
        git_commands::worktree::fetch,
        git_commands::worktree::pull,
        git_commands::worktree::push,
        git_commands::worktree::get_repo_state,
        git_commands::worktree::resolve_ours,
        git_commands::worktree::resolve_theirs,
        git_commands::worktree::continue_op,
        git_commands::worktree::abort_op,
        git_commands::worktree::cherry_pick,
        git_commands::worktree::revert,
        git_commands::worktree::create_tag,
        git_commands::worktree::delete_tag,
        git_commands::worktree::reset,
        git_commands::worktree::undo_state,
        git_commands::worktree::undo,
        git_commands::worktree::redo,
        git_commands::worktree::op_log,
        git_commands::worktree::op_goto,
        git_commands::worktree::interactive_rebase,
        git_commands::worktree::blame,
        git_commands::worktree::conflict_sides,
        git_commands::worktree::read_working_file,
        git_commands::worktree::read_image,
        git_commands::worktree::write_resolved,
        git_commands::worktree::stash_list,
        git_commands::worktree::stash_save,
        git_commands::worktree::stash_apply,
        git_commands::worktree::stash_pop,
        git_commands::worktree::stash_drop,
        set_github_token,
        has_github_token,
        get_github_token,
        clear_github_token,
        set_gitlab_token,
        has_gitlab_token,
        get_gitlab_token,
        clear_gitlab_token,
        credential_status,
        save_credential,
        clear_credential,
        test_credential,
        list_review_models,
        get_review_preflight,
        get_gitlab_review_preflight,
        start_pr_review,
        start_gitlab_mr_review,
        cancel_pr_review,
        resolve_tool_approval,
        get_agent_session,
        get_agent_goal,
        create_agent_goal,
        steer_agent_goal,
        pause_agent_goal,
        resume_agent_goal,
        cancel_agent_goal,
        extend_agent_budget,
        reset_agent_session,
        start_agent_turn,
        cancel_agent_turn,
        submit_pr_review,
        submit_gitlab_mr_review,
        list_github_issues,
        get_github_issue_context,
        start_issue_triage,
        cancel_issue_triage,
        publish_issue_triage,
        analyze_change_plan,
        cancel_change_plan,
        commit_change_group,
        investigate_repository_history,
        cancel_history_investigation,
        watch_repo
    ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
