use crate::credentials::read_credential;
use crate::review_commands::{
    ReviewRunRegistry, agent_error, map_review_credential_error, review_error,
    review_model_credential,
};
use app_service::{RepoContext, RepoRegistry};
use ipc_types::{
    AgentIpcErrorDto, FileDiffDto, HistoryInvestigationFindingDto, HistoryInvestigationInputDto,
    HistoryInvestigationResultDto, IpcError, ReviewUsageDto,
};
use review_agent::{
    CancelSignal, HistoryConfidence, HistoryEvidence, HistoryEvidenceCommit, HistoryEvidenceFile,
    HistoryInvestigationResult, MAX_HISTORY_COMMITS, MAX_HISTORY_PATCH_BYTES, investigate_history,
    is_sensitive_change_path, validate_repository_path,
};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

const HISTORY_RESOURCE_PREFIX: &str = "history:";
const MAX_FILES_PER_COMMIT: usize = 16;
const MAX_PATCHES_PER_COMMIT: usize = 2;

fn history_resource_key(repo_path: &str) -> String {
    format!(
        "{HISTORY_RESOURCE_PREFIX}{}",
        repo_path.trim().replace('\\', "/").to_ascii_lowercase()
    )
}

fn invalid_input(message: &str) -> IpcError {
    IpcError {
        code: "INVALID_HISTORY_INVESTIGATION_INPUT".into(),
        message: message.into(),
        recoverable: false,
    }
}

fn render_patch(diff: FileDiffDto) -> (String, usize, usize) {
    let mut patch = String::new();
    let mut additions = 0usize;
    let mut deletions = 0usize;
    for hunk in diff.hunks {
        patch.push_str(&hunk.header);
        patch.push('\n');
        for line in hunk.lines {
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
    (patch, additions, deletions)
}

fn scoped_file_evidence(
    context: &RepoContext,
    commit_id: &str,
    file: &str,
    patch_budget: &mut usize,
) -> HistoryEvidenceFile {
    match context.commit_file_diff(commit_id, file) {
        Ok(diff) => {
            let binary = diff.is_binary;
            let inherently_too_large = diff.too_large;
            let (patch, additions, deletions) = render_patch(diff);
            let reviewable = !binary && !inherently_too_large && !is_sensitive_change_path(file);
            let included = reviewable && patch.len() <= *patch_budget;
            if included {
                *patch_budget -= patch.len();
            }
            HistoryEvidenceFile {
                path: file.into(),
                status: "changed".into(),
                additions,
                deletions,
                binary,
                too_large: inherently_too_large || (reviewable && !included),
                patch: included.then_some(patch),
            }
        }
        Err(_) => HistoryEvidenceFile {
            path: file.into(),
            status: "changed".into(),
            additions: 0,
            deletions: 0,
            binary: false,
            too_large: true,
            patch: None,
        },
    }
}

fn collect_history_evidence(
    context: &RepoContext,
    question: String,
    scope_file: Option<String>,
    cancel: &dyn CancelSignal,
) -> Result<HistoryEvidence, IpcError> {
    if cancel.is_cancelled() {
        return Err(review_error(review_agent::ReviewError::Cancelled));
    }
    let commits = match &scope_file {
        Some(file) => context
            .file_history(file, MAX_HISTORY_COMMITS)
            .map_err(crate::to_ipc)?,
        None => context.log(MAX_HISTORY_COMMITS, 0).map_err(crate::to_ipc)?,
    };
    let mut patch_budget = MAX_HISTORY_PATCH_BYTES;
    let mut evidence_commits = Vec::with_capacity(commits.len());

    for commit in commits {
        if cancel.is_cancelled() {
            return Err(review_error(review_agent::ReviewError::Cancelled));
        }
        let files = if let Some(file) = &scope_file {
            vec![scoped_file_evidence(
                context,
                &commit.id,
                file,
                &mut patch_budget,
            )]
        } else {
            let changed = context.commit_files(&commit.id).map_err(crate::to_ipc)?;
            let mut files = Vec::new();
            for (index, change) in changed.into_iter().take(MAX_FILES_PER_COMMIT).enumerate() {
                if cancel.is_cancelled() {
                    return Err(review_error(review_agent::ReviewError::Cancelled));
                }
                let mut binary = false;
                let mut too_large = false;
                let mut patch = None;
                if index < MAX_PATCHES_PER_COMMIT && !is_sensitive_change_path(&change.path) {
                    if let Ok(diff) = context.commit_file_diff(&commit.id, &change.path) {
                        binary = diff.is_binary;
                        too_large = diff.too_large;
                        let (rendered, _, _) = render_patch(diff);
                        if !binary && !too_large && rendered.len() <= patch_budget {
                            patch_budget -= rendered.len();
                            patch = Some(rendered);
                        } else if !binary && !too_large {
                            too_large = true;
                        }
                    }
                }
                files.push(HistoryEvidenceFile {
                    path: change.path,
                    status: change.status,
                    additions: change.additions,
                    deletions: change.deletions,
                    binary,
                    too_large,
                    patch,
                });
            }
            files
        };
        evidence_commits.push(HistoryEvidenceCommit {
            id: commit.id,
            short_id: commit.short_id,
            summary: commit.summary,
            body: commit.body,
            author_name: commit.author_name,
            timestamp: commit.timestamp,
            files,
        });
    }

    let mut evidence = HistoryEvidence {
        snapshot_id: String::new(),
        question,
        scope_file,
        commits: evidence_commits,
    };
    let encoded = serde_json::to_string(&evidence)
        .map_err(|_| invalid_input("Repository history evidence could not be encoded"))?;
    let mut hasher = DefaultHasher::new();
    encoded.hash(&mut hasher);
    evidence.snapshot_id = format!("{:016x}", hasher.finish());
    Ok(evidence)
}

fn result_dto(result: HistoryInvestigationResult) -> HistoryInvestigationResultDto {
    HistoryInvestigationResultDto {
        snapshot_id: result.snapshot_id,
        summary: result.summary,
        confidence: match result.confidence {
            HistoryConfidence::High => "high",
            HistoryConfidence::Medium => "medium",
            HistoryConfidence::Low => "low",
        }
        .into(),
        findings: result
            .findings
            .into_iter()
            .map(|finding| HistoryInvestigationFindingDto {
                title: finding.title,
                explanation: finding.explanation,
                commit_ids: finding.commit_ids,
                paths: finding.paths,
            })
            .collect(),
        caveats: result.caveats,
        usage: ReviewUsageDto {
            input_tokens: result.usage.input_tokens,
            output_tokens: result.usage.output_tokens,
            tool_calls: result.usage.tool_calls,
        },
        model_id: result.model_id,
        provider_attempts: result.provider_attempts,
    }
}

#[tauri::command]
pub(crate) async fn investigate_repository_history(
    registry: tauri::State<'_, RepoRegistry>,
    runs: tauri::State<'_, ReviewRunRegistry>,
    input: HistoryInvestigationInputDto,
) -> Result<HistoryInvestigationResultDto, AgentIpcErrorDto> {
    let question = input.question.trim().to_owned();
    let file = input.file.and_then(|file| {
        let trimmed = file.trim().to_owned();
        (!trimmed.is_empty()).then_some(trimmed)
    });
    if input.run_id.trim().is_empty()
        || input.repo_path.trim().is_empty()
        || input.model_id.trim().is_empty()
        || question.len() < 5
        || question.len() > 1_000
        || question.contains('\0')
    {
        return Err(agent_error(
            invalid_input(
                "Run id, repository, model, and a 5-1000 character question are required",
            ),
            &input.run_id,
        ));
    }
    if let Some(path) = &file {
        validate_repository_path(path)
            .map_err(review_error)
            .map_err(|error| agent_error(error, &input.run_id))?;
    }
    let diagnostic_id = input.run_id.clone();
    let cancellation = runs
        .register_resource(&input.run_id, &history_resource_key(&input.repo_path))
        .map_err(|error| agent_error(error, &diagnostic_id))?;
    let context = registry.context(Path::new(&input.repo_path));
    let evidence_cancellation = cancellation.clone();
    let evidence_result = tokio::task::spawn_blocking(move || {
        collect_history_evidence(&context, question, file, &evidence_cancellation)
    })
    .await
    .map_err(crate::join_panic);

    let result = async {
        let evidence = evidence_result
            .map_err(|error| agent_error(error, &diagnostic_id))?
            .map_err(|error| agent_error(error, &diagnostic_id))?;
        let credential_kind = review_model_credential(&input.model_id)
            .map_err(|error| agent_error(error, &diagnostic_id))?;
        let credential = tokio::task::spawn_blocking(move || {
            read_credential(credential_kind)
                .map_err(|error| map_review_credential_error(credential_kind, error))
        })
        .await
        .map_err(crate::join_panic)
        .and_then(|value| value)
        .map_err(|error| agent_error(error, &diagnostic_id))?;
        let model = review_agent::create_model_provider(credential, &input.model_id)
            .map_err(review_error)
            .map_err(|error| agent_error(error, &diagnostic_id))?;
        investigate_history(model.as_ref(), &cancellation, &input.run_id, &evidence)
            .await
            .map(result_dto)
            .map_err(review_error)
            .map_err(|error| agent_error(error, &diagnostic_id))
    }
    .await;
    runs.finish(&input.run_id);
    result
}

#[tauri::command]
pub(crate) fn cancel_history_investigation(
    runs: tauri::State<'_, ReviewRunRegistry>,
    run_id: String,
) {
    runs.cancel(&run_id);
}
