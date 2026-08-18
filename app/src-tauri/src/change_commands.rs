use crate::agent_events::AppAgentEventEmitter;
use crate::credentials::read_credential;
use crate::review_commands::{
    ReviewRunRegistry, agent_error, map_review_credential_error, review_error,
    review_model_credential,
};
use app_service::{RepoContext, RepoRegistry};
use ipc_types::{
    AgentIpcErrorDto, ChangeCommitGroupDto, ChangeGroupCommitResultDto, ChangePlanFileDto,
    ChangePlanInputDto, ChangePlanResultDto, ChangeWarningDto, ChangeWarningSeverityDto,
    CommitChangeGroupInputDto, IpcError, ReviewUsageDto,
};
use review_agent::{
    AgentEventPublisher, ChangeEvidence, ChangeEvidenceFile, ChangePlanResult,
    ChangeWarningSeverity, MAX_CHANGE_FILES, MAX_CHANGE_PATCH_BYTES, build_local_change_plan,
    enhance_change_plan_with_events, is_sensitive_change_path,
};
use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const CHANGE_PLAN_RESOURCE_PREFIX: &str = "changes:";

pub(crate) fn change_plan_result_dto(plan: ChangePlanResult) -> ChangePlanResultDto {
    ChangePlanResultDto {
        snapshot_id: plan.snapshot_id,
        summary: plan.summary,
        warnings: plan
            .warnings
            .into_iter()
            .map(|warning| ChangeWarningDto {
                code: warning.code,
                severity: match warning.severity {
                    ChangeWarningSeverity::Info => ChangeWarningSeverityDto::Info,
                    ChangeWarningSeverity::Warning => ChangeWarningSeverityDto::Warning,
                    ChangeWarningSeverity::Blocker => ChangeWarningSeverityDto::Blocker,
                },
                message: warning.message,
                paths: warning.paths,
            })
            .collect(),
        groups: plan
            .groups
            .into_iter()
            .map(|group| ChangeCommitGroupDto {
                id: group.id,
                title: group.title,
                rationale: group.rationale,
                commit_message: group.commit_message,
                files: group
                    .files
                    .into_iter()
                    .map(|file| ChangePlanFileDto {
                        path: file.path,
                        state: file.state,
                        staged: file.staged,
                        additions: file.additions,
                        deletions: file.deletions,
                    })
                    .collect(),
                executable: group.executable,
                blocked_reason: group.blocked_reason,
            })
            .collect(),
        enhanced: plan.enhanced,
        usage: ReviewUsageDto {
            input_tokens: plan.usage.input_tokens,
            output_tokens: plan.usage.output_tokens,
            tool_calls: plan.usage.tool_calls,
        },
        model_id: plan.model_id,
        provider_attempts: plan.provider_attempts,
    }
}

fn change_resource_key(repo_path: &str) -> String {
    format!(
        "{CHANGE_PLAN_RESOURCE_PREFIX}{}",
        repo_path.trim().replace('\\', "/").to_ascii_lowercase()
    )
}

fn invalid_input(code: &str, message: &str) -> IpcError {
    IpcError {
        code: code.into(),
        message: message.into(),
        recoverable: false,
    }
}

fn collect_change_evidence(context: &RepoContext) -> Result<ChangeEvidence, IpcError> {
    let mut status = context.refresh_status().map_err(crate::to_ipc)?.entries;
    if status.len() > MAX_CHANGE_FILES {
        return Err(review_error(
            review_agent::ReviewError::ChangePlanBudgetExceeded,
        ));
    }
    status.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| right.staged.cmp(&left.staged))
    });
    let history = match context.log(20, 0) {
        Ok(history) => history,
        Err(git_core::GitError::NoHead) => Vec::new(),
        Err(error) => return Err(crate::to_ipc(error)),
    };
    let head_sha = history.first().map(|commit| commit.id.clone());
    let recent_commit_messages = history
        .iter()
        .map(|commit| commit.summary.clone())
        .collect();
    let mut hasher = DefaultHasher::new();
    head_sha.hash(&mut hasher);
    let mut patch_budget = MAX_CHANGE_PATCH_BYTES;
    let mut files = Vec::with_capacity(status.len());

    for entry in status {
        entry.path.hash(&mut hasher);
        entry.state.hash(&mut hasher);
        entry.staged.hash(&mut hasher);
        let diff = context
            .working_diff(&entry.path, entry.staged)
            .map_err(crate::to_ipc)?;
        diff.is_binary.hash(&mut hasher);
        diff.too_large.hash(&mut hasher);
        let mut additions = 0u32;
        let mut deletions = 0u32;
        let mut patch = String::new();
        for hunk in diff.hunks {
            hunk.header.hash(&mut hasher);
            patch.push_str(&hunk.header);
            patch.push('\n');
            for line in hunk.lines {
                line.kind.hash(&mut hasher);
                line.content.hash(&mut hasher);
                let prefix = match line.kind.as_str() {
                    "add" => {
                        additions = additions.saturating_add(1);
                        '+'
                    }
                    "del" => {
                        deletions = deletions.saturating_add(1);
                        '-'
                    }
                    _ => ' ',
                };
                patch.push(prefix);
                patch.push_str(&line.content);
                patch.push('\n');
            }
        }
        additions.hash(&mut hasher);
        deletions.hash(&mut hasher);
        let sensitive = is_sensitive_change_path(&entry.path);
        let reviewable = !diff.is_binary && !diff.too_large && !sensitive;
        let patch_len = patch.len();
        let included_patch = if reviewable && patch_len <= patch_budget {
            patch_budget -= patch_len;
            Some(patch)
        } else {
            None
        };
        files.push(ChangeEvidenceFile {
            path: entry.path,
            state: entry.state,
            staged: entry.staged,
            additions,
            deletions,
            binary: diff.is_binary,
            too_large: diff.too_large || (reviewable && included_patch.is_none()),
            patch: included_patch,
        });
    }

    Ok(ChangeEvidence {
        snapshot_id: format!("{:016x}", hasher.finish()),
        head_sha,
        recent_commit_messages,
        files,
    })
}

#[tauri::command]
pub(crate) async fn analyze_change_plan(
    app: tauri::AppHandle,
    registry: tauri::State<'_, RepoRegistry>,
    runs: tauri::State<'_, ReviewRunRegistry>,
    input: ChangePlanInputDto,
) -> Result<ChangePlanResultDto, AgentIpcErrorDto> {
    if input.run_id.trim().is_empty() || input.repo_path.trim().is_empty() {
        return Err(agent_error(
            invalid_input(
                "INVALID_CHANGE_PLAN_INPUT",
                "Run id and repository path are required",
            ),
            &input.run_id,
        ));
    }
    let diagnostic_id = input.run_id.clone();
    let cancellation = runs
        .register_resource(&input.run_id, &change_resource_key(&input.repo_path))
        .map_err(|error| agent_error(error, &diagnostic_id))?;
    let context = registry.context(Path::new(&input.repo_path));
    let evidence_result = tokio::task::spawn_blocking(move || collect_change_evidence(&context))
        .await
        .map_err(crate::join_panic);
    let result = async {
        match evidence_result {
            Ok(Ok(evidence)) => {
                let local = build_local_change_plan(&evidence).map_err(review_error);
                match (local, input.model_id.filter(|id| !id.trim().is_empty())) {
                    (Ok(plan), None) => Ok(change_plan_result_dto(plan)),
                    (Ok(plan), Some(model_id)) => {
                        let credential_kind = review_model_credential(&model_id)
                            .map_err(|error| agent_error(error, &diagnostic_id))?;
                        let credential = tokio::task::spawn_blocking(move || {
                            read_credential(credential_kind).map_err(|error| {
                                map_review_credential_error(credential_kind, error)
                            })
                        })
                        .await
                        .map_err(crate::join_panic)
                        .and_then(|value| value)
                        .map_err(|error| agent_error(error, &diagnostic_id))?;
                        let model = review_agent::create_model_provider(credential, &model_id)
                            .map_err(review_error)
                            .map_err(|error| agent_error(error, &diagnostic_id))?;
                        let sink = AppAgentEventEmitter(app.clone());
                        let events = AgentEventPublisher::new(&input.run_id, &sink);
                        enhance_change_plan_with_events(
                            model.as_ref(),
                            &cancellation,
                            &input.run_id,
                            &evidence,
                            plan,
                            &events,
                        )
                        .await
                        .map(change_plan_result_dto)
                        .map_err(review_error)
                        .map_err(|error| agent_error(error, &diagnostic_id))
                    }
                    (Err(error), _) => Err(agent_error(error, &diagnostic_id)),
                }
            }
            Ok(Err(error)) | Err(error) => Err(agent_error(error, &diagnostic_id)),
        }
    }
    .await;
    runs.finish(&input.run_id);
    result
}

#[tauri::command]
pub(crate) fn cancel_change_plan(runs: tauri::State<'_, ReviewRunRegistry>, run_id: String) {
    runs.cancel(&run_id);
}

fn rollback_staged(context: &RepoContext, staged_paths: &[PathBuf]) {
    for path in staged_paths.iter().rev() {
        let _ = context.unstage(path);
    }
}

fn commit_group(
    context: Arc<RepoContext>,
    input: CommitChangeGroupInputDto,
) -> Result<ChangeGroupCommitResultDto, IpcError> {
    if !input.confirmed {
        return Err(invalid_input(
            "CHANGE_COMMIT_CONFIRMATION_REQUIRED",
            "Explicit confirmation is required before staging or committing",
        ));
    }
    let message = input.commit_message.trim();
    if message.is_empty() || message.len() > 500 || message.contains('\0') {
        return Err(invalid_input(
            "INVALID_COMMIT_MESSAGE",
            "Commit message must contain 1 to 500 characters",
        ));
    }
    let evidence = collect_change_evidence(&context)?;
    if evidence.snapshot_id != input.snapshot_id {
        return Err(review_error(review_agent::ReviewError::WorktreeUpdated));
    }
    let plan = build_local_change_plan(&evidence).map_err(review_error)?;
    let group = plan
        .groups
        .into_iter()
        .find(|group| group.id == input.group_id)
        .ok_or_else(|| {
            invalid_input(
                "CHANGE_GROUP_NOT_FOUND",
                "The selected commit group is no longer available",
            )
        })?;
    if !group.executable {
        return Err(review_error(review_agent::ReviewError::IndexNotClean));
    }
    let currently_staged: BTreeSet<_> = evidence
        .files
        .iter()
        .filter(|file| file.staged)
        .map(|file| file.path.as_str())
        .collect();
    let group_staged: BTreeSet<_> = group
        .files
        .iter()
        .filter(|file| file.staged)
        .map(|file| file.path.as_str())
        .collect();
    if group.files.iter().any(|file| file.staged) {
        if group.files.iter().any(|file| !file.staged) || currently_staged != group_staged {
            return Err(review_error(review_agent::ReviewError::IndexNotClean));
        }
        return context
            .commit(message)
            .map(|sha| ChangeGroupCommitResultDto { sha })
            .map_err(|error| {
                review_error(review_agent::ReviewError::ChangeCommitFailed(
                    error.to_string(),
                ))
            });
    }
    if !currently_staged.is_empty() {
        return Err(review_error(review_agent::ReviewError::IndexNotClean));
    }
    let mut staged_paths = Vec::new();
    for file in group.files {
        let path = PathBuf::from(file.path);
        if let Err(error) = context.stage(&path) {
            rollback_staged(&context, &staged_paths);
            return Err(review_error(review_agent::ReviewError::ChangeCommitFailed(
                error.to_string(),
            )));
        }
        staged_paths.push(path);
    }
    match context.commit(message) {
        Ok(sha) => Ok(ChangeGroupCommitResultDto { sha }),
        Err(error) => {
            rollback_staged(&context, &staged_paths);
            Err(review_error(review_agent::ReviewError::ChangeCommitFailed(
                error.to_string(),
            )))
        }
    }
}

#[tauri::command]
pub(crate) async fn commit_change_group(
    registry: tauri::State<'_, RepoRegistry>,
    runs: tauri::State<'_, ReviewRunRegistry>,
    input: CommitChangeGroupInputDto,
) -> Result<ChangeGroupCommitResultDto, IpcError> {
    if input.run_id.trim().is_empty() || input.repo_path.trim().is_empty() {
        return Err(invalid_input(
            "INVALID_CHANGE_PLAN_INPUT",
            "Run id and repository path are required",
        ));
    }
    let run_id = input.run_id.clone();
    runs.register_resource(&run_id, &change_resource_key(&input.repo_path))?;
    let context = registry.context(Path::new(&input.repo_path));
    let result = tokio::task::spawn_blocking(move || commit_group(context, input))
        .await
        .map_err(crate::join_panic)
        .and_then(|result| result);
    runs.finish(&run_id);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::model::{FileEntry, FileState};
    use git_engine::FakeBackend;

    fn registry_with_change(path: &str) -> (RepoRegistry, Arc<FakeBackend>) {
        let backend = Arc::new(FakeBackend::with_status(vec![FileEntry {
            path: path.into(),
            state: FileState::Modified,
            staged: false,
        }]));
        (RepoRegistry::new(backend.clone()), backend)
    }

    #[test]
    fn confirmation_is_required_before_any_git_write() {
        let (registry, backend) = registry_with_change("app/src/App.tsx");
        let context = registry.context(Path::new("/repo"));
        let evidence = collect_change_evidence(&context).unwrap();
        let result = commit_group(
            context,
            CommitChangeGroupInputDto {
                run_id: "commit-1".into(),
                repo_path: "/repo".into(),
                snapshot_id: evidence.snapshot_id,
                group_id: "area-app-frontend".into(),
                commit_message: "feat(changes): add planner".into(),
                confirmed: false,
            },
        );
        assert_eq!(
            result.unwrap_err().code,
            "CHANGE_COMMIT_CONFIRMATION_REQUIRED"
        );
        assert!(backend.staged_files().is_empty());
        assert!(backend.commit_messages().is_empty());
    }

    #[test]
    fn stale_snapshot_is_rejected_before_staging() {
        let (registry, backend) = registry_with_change("app/src/App.tsx");
        let context = registry.context(Path::new("/repo"));
        let result = commit_group(
            context,
            CommitChangeGroupInputDto {
                run_id: "commit-2".into(),
                repo_path: "/repo".into(),
                snapshot_id: "stale".into(),
                group_id: "area-app-frontend".into(),
                commit_message: "feat(changes): add planner".into(),
                confirmed: true,
            },
        );
        assert_eq!(result.unwrap_err().code, "WORKTREE_UPDATED");
        assert!(backend.staged_files().is_empty());
    }

    #[test]
    fn confirmed_unstaged_group_is_staged_and_committed() {
        let (registry, backend) = registry_with_change("app/src/App.tsx");
        let context = registry.context(Path::new("/repo"));
        let evidence = collect_change_evidence(&context).unwrap();
        let result = commit_group(
            context,
            CommitChangeGroupInputDto {
                run_id: "commit-3".into(),
                repo_path: "/repo".into(),
                snapshot_id: evidence.snapshot_id,
                group_id: "area-app-frontend".into(),
                commit_message: "feat(changes): add planner".into(),
                confirmed: true,
            },
        )
        .unwrap();
        assert!(result.sha.starts_with("fake"));
        assert_eq!(
            backend.staged_files(),
            vec![PathBuf::from("app/src/App.tsx")]
        );
        assert_eq!(
            backend.commit_messages(),
            vec!["feat(changes): add planner"]
        );
    }

    #[test]
    fn failed_commit_unstages_every_path_added_by_the_operation() {
        let backend = Arc::new(
            FakeBackend::with_status(vec![
                FileEntry {
                    path: "app/src/App.tsx".into(),
                    state: FileState::Modified,
                    staged: false,
                },
                FileEntry {
                    path: "app/src/ipc.ts".into(),
                    state: FileState::Modified,
                    staged: false,
                },
            ])
            .fail_commit_with("fixture failure"),
        );
        let registry = RepoRegistry::new(backend.clone());
        let context = registry.context(Path::new("/repo"));
        let evidence = collect_change_evidence(&context).unwrap();
        let result = commit_group(
            context,
            CommitChangeGroupInputDto {
                run_id: "commit-4".into(),
                repo_path: "/repo".into(),
                snapshot_id: evidence.snapshot_id,
                group_id: "area-app-frontend".into(),
                commit_message: "feat(changes): add planner".into(),
                confirmed: true,
            },
        );
        assert_eq!(result.unwrap_err().code, "CHANGE_COMMIT_FAILED");
        assert_eq!(
            backend.unstaged_files(),
            vec![
                PathBuf::from("app/src/ipc.ts"),
                PathBuf::from("app/src/App.tsx"),
            ]
        );
        assert!(backend.commit_messages().is_empty());
    }
}
