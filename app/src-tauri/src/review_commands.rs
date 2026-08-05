use crate::credentials::read_credential;
use ipc_types::{
    CredentialKindDto, IpcError, PublishedReviewDto, ReviewPreflightDto, ReviewProgressEventDto,
    ReviewRunInputDto, ReviewRunResultDto, ReviewTargetDto, SubmitReviewDto,
};
use review_agent::{
    CancelSignal, DeepSeekResponsesProvider, GithubReviewSource, ProgressSink, ProgressUpdate,
    ReviewOrchestrator, ReviewSource, SanitizedTraceStore,
};
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Emitter, Manager};

#[derive(Clone, Debug)]
pub(crate) struct ReviewCancellation(Arc<AtomicBool>);

impl ReviewCancellation {
    fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}

impl CancelSignal for ReviewCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Default)]
pub(crate) struct ReviewRunRegistry(Mutex<HashMap<String, ReviewCancellation>>);

impl ReviewRunRegistry {
    pub(crate) fn register(&self, run_id: &str) -> Result<ReviewCancellation, IpcError> {
        let mut runs = self.0.lock().expect("review run registry lock poisoned");
        if runs.contains_key(run_id) {
            return Err(IpcError {
                code: "REVIEW_ALREADY_RUNNING".into(),
                message: "A review with this run id is already active".into(),
                recoverable: false,
            });
        }
        let token = ReviewCancellation::new();
        runs.insert(run_id.to_owned(), token.clone());
        Ok(token)
    }
    pub(crate) fn cancel(&self, run_id: &str) {
        if let Some(token) = self
            .0
            .lock()
            .expect("review run registry lock poisoned")
            .get(run_id)
        {
            token.cancel();
        }
    }
    pub(crate) fn finish(&self, run_id: &str) {
        self.0
            .lock()
            .expect("review run registry lock poisoned")
            .remove(run_id);
    }
}

pub(crate) fn review_error(error: review_agent::ReviewError) -> IpcError {
    let recoverable = matches!(
        error,
        review_agent::ReviewError::RateLimited
            | review_agent::ReviewError::NetworkError(_)
            | review_agent::ReviewError::PrUpdated
            | review_agent::ReviewError::Cancelled
            | review_agent::ReviewError::ReviewPublishFailed(_)
    );
    IpcError {
        code: error.code().into(),
        message: match error {
            review_agent::ReviewError::NetworkError(_) => "Network request failed".into(),
            review_agent::ReviewError::InvalidModelOutput(_) => {
                "The model returned invalid review data".into()
            }
            review_agent::ReviewError::ReviewPublishFailed(_) => "Review publication failed".into(),
            other => other.to_string(),
        },
        recoverable,
    }
}

fn credential_join(error: tokio::task::JoinError) -> IpcError {
    crate::join_panic(error)
}
async fn github_source() -> Result<GithubReviewSource, IpcError> {
    let token = tokio::task::spawn_blocking(|| read_credential(CredentialKindDto::Github))
        .await
        .map_err(credential_join)??;
    GithubReviewSource::new(token).map_err(review_error)
}

#[tauri::command]
pub(crate) async fn get_review_preflight(
    target: ReviewTargetDto,
) -> Result<ReviewPreflightDto, IpcError> {
    Ok(github_source()
        .await?
        .preflight(&target.into())
        .await
        .map_err(review_error)?
        .into())
}

fn emit_progress(app: &tauri::AppHandle, run_id: &str, stage: &str) {
    let _ = app.emit(
        "review-progress",
        ReviewProgressEventDto {
            run_id: run_id.into(),
            stage: stage.into(),
            tool_name: None,
            tool_calls: None,
        },
    );
}

struct AppProgress {
    app: tauri::AppHandle,
    run_id: String,
}

impl ProgressSink for AppProgress {
    fn report(&self, update: ProgressUpdate) {
        let ProgressUpdate::ToolCall { name, count } = update;
        let _ = self.app.emit(
            "review-progress",
            ReviewProgressEventDto {
                run_id: self.run_id.clone(),
                stage: "tool_call".into(),
                tool_name: Some(name),
                tool_calls: Some(count),
            },
        );
    }
}

#[tauri::command]
pub(crate) async fn start_pr_review(
    app: tauri::AppHandle,
    state: tauri::State<'_, ReviewRunRegistry>,
    input: ReviewRunInputDto,
) -> Result<ReviewRunResultDto, IpcError> {
    let run_id = input.run_id.clone();
    let cancel = state.register(&run_id)?;
    emit_progress(&app, &run_id, "loading_pr");
    let result: Result<ReviewRunResultDto, IpcError> = async {
        let source = github_source().await?;
        let key = tokio::task::spawn_blocking(|| read_credential(CredentialKindDto::Deepseek))
            .await
            .map_err(credential_join)??;
        let model = DeepSeekResponsesProvider::new(key).map_err(review_error)?;
        let trace_path = app
            .path()
            .app_data_dir()
            .map_err(|_| IpcError {
                code: "APP_DATA_DIR".into(),
                message: "Application data directory is unavailable".into(),
                recoverable: false,
            })?
            .join("review-agent-trace.json");
        let trace = SanitizedTraceStore::new(trace_path);
        let progress = AppProgress {
            app: app.clone(),
            run_id: run_id.clone(),
        };
        emit_progress(&app, &run_id, "analyzing");
        let result =
            ReviewOrchestrator::new_with_progress(&model, &source, &trace, &cancel, &progress)
                .run(input.into())
                .await
                .map_err(review_error)?;
        emit_progress(&app, &run_id, "generating_drafts");
        Ok(result.into())
    }
    .await;
    state.finish(&run_id);
    match &result {
        Ok(_) => emit_progress(&app, &run_id, "completed"),
        Err(error) if error.code == "CANCELLED" => emit_progress(&app, &run_id, "cancelled"),
        Err(_) => emit_progress(&app, &run_id, "failed"),
    }
    result
}

#[tauri::command]
pub(crate) fn cancel_pr_review(state: tauri::State<'_, ReviewRunRegistry>, run_id: String) {
    state.cancel(&run_id);
}

#[tauri::command]
pub(crate) async fn submit_pr_review(
    input: SubmitReviewDto,
) -> Result<PublishedReviewDto, IpcError> {
    let source = github_source().await?;
    let review: review_agent::SubmitReview = input.try_into().map_err(|message| IpcError {
        code: "INVALID_REVIEW".into(),
        message,
        recoverable: false,
    })?;
    if source
        .head_sha(&review.target)
        .await
        .map_err(review_error)?
        != review.head_sha
    {
        return Err(review_error(review_agent::ReviewError::PrUpdated));
    }
    Ok(source.publish(&review).await.map_err(review_error)?.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_rejects_duplicates_and_cleanup_makes_id_reusable() {
        let registry = ReviewRunRegistry::default();
        let token = registry.register("same").unwrap();
        assert_eq!(
            registry.register("same").unwrap_err().code,
            "REVIEW_ALREADY_RUNNING"
        );
        registry.finish("same");
        assert!(registry.register("same").is_ok());
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancellation_is_idempotent_for_unknown_and_completed_runs() {
        let registry = ReviewRunRegistry::default();
        registry.cancel("unknown");
        let _ = registry.register("done").unwrap();
        registry.finish("done");
        registry.cancel("done");
    }

    #[test]
    fn review_errors_have_stable_codes_and_recoverability() {
        let auth = review_error(review_agent::ReviewError::AuthFailed);
        assert_eq!(auth.code, "AUTH_FAILED");
        assert!(!auth.recoverable);
        let network = review_error(review_agent::ReviewError::NetworkError("secret".into()));
        assert_eq!(network.code, "NETWORK_ERROR");
        assert!(network.recoverable);
        assert!(!network.message.contains("secret"));
    }
}
