use crate::agent_events::AppAgentEventEmitter;
use crate::credentials::read_credential;
use crate::review_commands::{
    ReviewRunRegistry, agent_error, map_review_credential_error, review_error,
    review_model_credential,
};
use app_service::{RepoContext, RepoRegistry};
use ipc_types::{
    AgentIpcErrorDto, BlameLineDto, CommitDto, FileDiffDto, HistoryEvidenceLinkDto,
    HistoryInvestigationFindingDto, HistoryInvestigationInputDto, HistoryInvestigationResultDto,
    IpcError, ReviewUsageDto,
};
use review_agent::{
    AgentEventPublisher, CancelSignal, HistoryBlameLine, HistoryConfidence, HistoryEvidence,
    HistoryEvidenceCommit, HistoryEvidenceFile, HistoryInvestigationResult, MAX_HISTORY_COMMITS,
    MAX_HISTORY_PATCH_BYTES, investigate_history_with_events, is_sensitive_change_path,
    validate_repository_path,
};
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

const HISTORY_RESOURCE_PREFIX: &str = "history:";
const MAX_FILES_PER_COMMIT: usize = 16;
const MAX_PATCHES_PER_COMMIT: usize = 2;
const BASE_HISTORY_COMMITS: usize = 16;
const PICKAXE_COMMITS_PER_TERM: usize = 8;
const MAX_BLAME_SAMPLES: usize = 24;

const SEARCH_STOP_WORDS: &[&str] = &[
    "and",
    "are",
    "behavior",
    "call",
    "change",
    "changed",
    "code",
    "commit",
    "current",
    "file",
    "function",
    "history",
    "here",
    "introduced",
    "that",
    "the",
    "this",
    "was",
    "what",
    "when",
    "where",
    "which",
    "why",
    "with",
];

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

fn push_search_term(terms: &mut Vec<String>, candidate: &str) {
    let candidate = candidate.trim().trim_matches(['.', ',', ':', ';']);
    let lower = candidate.to_ascii_lowercase();
    if candidate.len() < 3
        || candidate.len() > 80
        || candidate
            .chars()
            .all(|character| character.is_ascii_digit())
        || SEARCH_STOP_WORDS.contains(&lower.as_str())
        || terms
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    terms.push(candidate.to_owned());
}

fn extract_search_terms(question: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for (open, close) in [
        ('`', '`'),
        ('"', '"'),
        ('\'', '\''),
        ('“', '”'),
        ('「', '」'),
    ] {
        let mut remaining = question;
        while let Some(start) = remaining.find(open) {
            let content = &remaining[start + open.len_utf8()..];
            let Some(end) = content.find(close) else {
                break;
            };
            push_search_term(&mut terms, &content[..end]);
            remaining = &content[end + close.len_utf8()..];
            if terms.len() == 3 {
                return terms;
            }
        }
    }
    for token in question.split(|character: char| {
        !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':'))
    }) {
        push_search_term(&mut terms, token);
        if terms.len() == 3 {
            break;
        }
    }
    terms
}

fn add_unique_commit(commits: &mut Vec<CommitDto>, seen: &mut HashSet<String>, commit: CommitDto) {
    if commits.len() < MAX_HISTORY_COMMITS && seen.insert(commit.id.clone()) {
        commits.push(commit);
    }
}

fn push_blame_sample(
    selected: &mut Vec<HistoryBlameLine>,
    seen_lines: &mut HashSet<u32>,
    seen_commits: &mut HashSet<String>,
    allowed_commits: &HashSet<String>,
    line: &BlameLineDto,
) {
    if selected.len() >= MAX_BLAME_SAMPLES
        || !allowed_commits.contains(&line.short_id)
        || !seen_lines.insert(line.line_no)
    {
        return;
    }
    seen_commits.insert(line.short_id.clone());
    selected.push(HistoryBlameLine {
        line_no: line.line_no,
        commit_id: line.short_id.clone(),
        author_name: line.author_name.clone(),
        content: line.content.chars().take(300).collect(),
    });
}

fn select_blame_samples(
    lines: Vec<BlameLineDto>,
    terms: &[String],
    allowed_commits: &HashSet<String>,
) -> Vec<HistoryBlameLine> {
    let lower_terms: Vec<_> = terms.iter().map(|term| term.to_lowercase()).collect();
    let mut selected = Vec::new();
    let mut seen_lines = HashSet::new();
    let mut seen_commits = HashSet::new();
    for line in lines.iter().filter(|line| {
        let content = line.content.to_lowercase();
        lower_terms.iter().any(|term| content.contains(term))
    }) {
        push_blame_sample(
            &mut selected,
            &mut seen_lines,
            &mut seen_commits,
            allowed_commits,
            line,
        );
    }
    for line in &lines {
        if selected.len() >= MAX_BLAME_SAMPLES {
            break;
        }
        if !seen_commits.contains(&line.short_id) {
            push_blame_sample(
                &mut selected,
                &mut seen_lines,
                &mut seen_commits,
                allowed_commits,
                line,
            );
        }
    }
    selected
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
    let base_commits = match &scope_file {
        Some(file) => context
            .file_history(file, MAX_HISTORY_COMMITS)
            .map_err(crate::to_ipc)?,
        None => context.log(MAX_HISTORY_COMMITS, 0).map_err(crate::to_ipc)?,
    };
    let search_terms = extract_search_terms(&question);
    let mut evidence_sources = vec![if scope_file.is_some() {
        "file_history".into()
    } else {
        "recent_history".into()
    }];
    let mut commits = Vec::with_capacity(MAX_HISTORY_COMMITS);
    let mut seen = HashSet::new();
    for commit in base_commits.iter().take(BASE_HISTORY_COMMITS).cloned() {
        add_unique_commit(&mut commits, &mut seen, commit);
    }
    let mut used_pickaxe = false;
    for term in &search_terms {
        if cancel.is_cancelled() {
            return Err(review_error(review_agent::ReviewError::Cancelled));
        }
        let Ok(hits) = context.pickaxe(term, false, PICKAXE_COMMITS_PER_TERM) else {
            continue;
        };
        for hit in hits {
            let in_scope = match &scope_file {
                Some(file) => context
                    .commit_files(&hit.id)
                    .is_ok_and(|files| files.iter().any(|change| change.path == *file)),
                None => true,
            };
            if in_scope {
                used_pickaxe = true;
                add_unique_commit(&mut commits, &mut seen, hit);
            }
        }
    }
    for commit in base_commits.into_iter().skip(BASE_HISTORY_COMMITS) {
        add_unique_commit(&mut commits, &mut seen, commit);
    }
    if used_pickaxe {
        evidence_sources.push("pickaxe".into());
    }
    let allowed_commits: HashSet<_> = commits
        .iter()
        .map(|commit| commit.short_id.clone())
        .collect();
    let blame = match &scope_file {
        Some(file) if !is_sensitive_change_path(file) => context
            .blame(file)
            .map(|lines| select_blame_samples(lines, &search_terms, &allowed_commits))
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    if !blame.is_empty() {
        evidence_sources.push("blame".into());
    }
    evidence_sources.push("commit_diffs".into());
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
                if index < MAX_PATCHES_PER_COMMIT
                    && !is_sensitive_change_path(&change.path)
                    && let Ok(diff) = context.commit_file_diff(&commit.id, &change.path)
                {
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
        search_terms,
        evidence_sources,
        blame,
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
                evidence_links: finding
                    .evidence_links
                    .into_iter()
                    .map(|link| HistoryEvidenceLinkDto {
                        commit_id: link.commit_id,
                        path: link.path,
                    })
                    .collect(),
            })
            .collect(),
        caveats: result.caveats,
        search_terms: result.search_terms,
        evidence_sources: result.evidence_sources,
        evidence_commit_count: result.evidence_commit_count,
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
    app: tauri::AppHandle,
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
        let sink = AppAgentEventEmitter(app.clone());
        let events = AgentEventPublisher::new(&input.run_id, &sink);
        investigate_history_with_events(
            model.as_ref(),
            &cancellation,
            &input.run_id,
            &evidence,
            &events,
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_terms_prioritize_quoted_and_code_like_identifiers() {
        assert_eq!(
            extract_search_terms("Why was `useGraph` changed to call graph_cache here?"),
            vec!["useGraph", "graph_cache"]
        );
        assert_eq!(
            extract_search_terms("为什么要保留“空仓库”这个分支？"),
            vec!["空仓库"]
        );
    }

    #[test]
    fn search_terms_are_bounded_and_deduplicated() {
        assert_eq!(
            extract_search_terms("`needle` needle another_identifier fourth_identifier"),
            vec!["needle", "another_identifier", "fourth_identifier"]
        );
    }
}
