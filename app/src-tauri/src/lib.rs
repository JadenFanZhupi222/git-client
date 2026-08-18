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
use tauri::Emitter;

mod agent_events;
mod agent_session_commands;
mod change_commands;
mod credentials;
mod history_commands;
mod review_commands;
use agent_events::{ToolApprovalRegistry, resolve_tool_approval};
use agent_session_commands::{
    AgentSessionState, cancel_agent_turn, get_agent_session, reset_agent_session, start_agent_turn,
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

#[tauri::command]
async fn get_head_commit(
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
async fn init_repo(registry: tauri::State<'_, RepoRegistry>, path: String) -> Result<(), IpcError> {
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
async fn clone_repo(
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
async fn get_status(
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
async fn refresh_status(
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
async fn stage_file(
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
async fn unstage_file(
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
async fn stage_hunk(
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
async fn stage_lines(
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
async fn unstage_hunk(
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
async fn commit(
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
async fn amend_commit(
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

#[tauri::command]
async fn get_log(
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
async fn file_history(
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
async fn pickaxe(
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
async fn line_history(
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
async fn get_commit_files(
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
async fn get_commit_file_diff(
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
async fn get_commit_signature(
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
async fn list_submodules(
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
async fn update_submodule(
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
async fn list_worktrees(
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
async fn sparse_checkout_patterns(
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
async fn compare_files(
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
async fn compare_file_diff(
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
async fn get_working_diff(
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
async fn get_commit_graph(
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
async fn search_commits(
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
async fn get_reflog(
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

#[tauri::command]
async fn get_current_branch(
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
async fn list_branches(
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
async fn get_ahead_behind(
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
async fn get_remotes(
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
async fn list_refs(
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
async fn set_upstream(
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
async fn remote_list(
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
async fn add_remote(
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
async fn remove_remote(
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
async fn rename_remote(
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
async fn checkout_branch(
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
async fn create_branch(
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
async fn delete_branch(
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
async fn merge_branch(
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
async fn branch_delete_impact(
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
async fn fetch(
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
async fn pull(
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
async fn push(
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
async fn get_repo_state(
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
async fn resolve_ours(
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
async fn resolve_theirs(
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
async fn continue_op(
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
async fn abort_op(
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
async fn blame(
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
async fn cherry_pick(
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
async fn revert(
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
async fn create_tag(
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
async fn delete_tag(
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
struct RebaseStepInput {
    sha: String,
    action: String,
    message: Option<String>,
}

#[tauri::command]
async fn interactive_rebase(
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
async fn reset(
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
async fn undo_state(
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
async fn undo(
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
async fn redo(
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
async fn op_log(
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
async fn op_goto(
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
async fn write_resolved(
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
async fn conflict_sides(
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

/// 读一侧图片的原始字节(M6.2:不再走 base64-in-JSON)。字节以 ArrayBuffer 直传前端,
/// 前端转 Blob URL 渲染。`oid` 非空 → 对象库 blob;为空 → 工作区文件(safe_join 防越权)。
#[tauri::command]
async fn read_image(
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
async fn stash_list(
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
async fn stash_save(
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
async fn stash_apply(
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
async fn stash_pop(
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
async fn stash_drop(
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
        init_repo,
        clone_repo,
        get_head_commit,
        get_status,
        refresh_status,
        stage_file,
        unstage_file,
        stage_hunk,
        unstage_hunk,
        stage_lines,
        commit,
        amend_commit,
        get_log,
        file_history,
        line_history,
        pickaxe,
        get_commit_files,
        get_commit_file_diff,
        get_commit_signature,
        list_submodules,
        update_submodule,
        list_worktrees,
        sparse_checkout_patterns,
        get_working_diff,
        get_commit_graph,
        search_commits,
        get_reflog,
        compare_files,
        compare_file_diff,
        get_current_branch,
        list_branches,
        get_ahead_behind,
        get_remotes,
        list_refs,
        set_upstream,
        remote_list,
        add_remote,
        remove_remote,
        rename_remote,
        checkout_branch,
        create_branch,
        delete_branch,
        merge_branch,
        branch_delete_impact,
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
        undo_state,
        undo,
        redo,
        op_log,
        op_goto,
        interactive_rebase,
        blame,
        conflict_sides,
        read_working_file,
        read_image,
        write_resolved,
        stash_list,
        stash_save,
        stash_apply,
        stash_pop,
        stash_drop,
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
        init_repo,
        clone_repo,
        get_head_commit,
        get_status,
        refresh_status,
        stage_file,
        unstage_file,
        stage_hunk,
        unstage_hunk,
        stage_lines,
        commit,
        amend_commit,
        get_log,
        file_history,
        line_history,
        pickaxe,
        get_commit_files,
        get_commit_file_diff,
        get_commit_signature,
        list_submodules,
        update_submodule,
        list_worktrees,
        sparse_checkout_patterns,
        get_working_diff,
        get_commit_graph,
        search_commits,
        get_reflog,
        compare_files,
        compare_file_diff,
        get_current_branch,
        list_branches,
        get_ahead_behind,
        get_remotes,
        list_refs,
        set_upstream,
        remote_list,
        add_remote,
        remove_remote,
        rename_remote,
        checkout_branch,
        create_branch,
        delete_branch,
        merge_branch,
        branch_delete_impact,
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
        undo_state,
        undo,
        redo,
        op_log,
        op_goto,
        interactive_rebase,
        blame,
        conflict_sides,
        read_working_file,
        read_image,
        write_resolved,
        stash_list,
        stash_save,
        stash_apply,
        stash_pop,
        stash_drop,
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
