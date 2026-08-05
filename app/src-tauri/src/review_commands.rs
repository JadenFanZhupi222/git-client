use crate::credentials::read_credential;
use async_trait::async_trait;
use ipc_types::{
    CredentialKindDto, IpcError, PublishedReviewDto, ReviewPreflightDto, ReviewProgressEventDto,
    ReviewRunInputDto, ReviewRunResultDto, ReviewTargetDto, SubmitReviewDto,
};
use review_agent::{
    CancelSignal, DeepSeekResponsesProvider, GithubReviewSource, ProgressSink, ProgressUpdate,
    ReviewOrchestrator, ReviewSource, SanitizedTraceStore,
};
use std::collections::{HashMap, VecDeque};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

#[async_trait]
trait CredentialReader: Send + Sync {
    async fn read(&self, kind: CredentialKindDto) -> Result<String, IpcError>;
}

trait ReviewBackendFactory: Send + Sync {
    fn source(&self, token: String) -> Result<Box<dyn ReviewSource>, review_agent::ReviewError>;
    fn model(
        &self,
        key: String,
    ) -> Result<Box<dyn review_agent::ModelProvider>, review_agent::ReviewError>;
}

trait ProgressEmitter: Send + Sync {
    fn emit(&self, event: ReviewProgressEventDto);
}

struct ReviewCommandService<'a> {
    credentials: &'a dyn CredentialReader,
    factory: &'a dyn ReviewBackendFactory,
    progress: &'a dyn ProgressEmitter,
    trace: &'a dyn review_agent::TraceSink,
    registry: &'a ReviewRunRegistry,
}

impl<'a> ReviewCommandService<'a> {
    fn new(
        credentials: &'a dyn CredentialReader,
        factory: &'a dyn ReviewBackendFactory,
        progress: &'a dyn ProgressEmitter,
        trace: &'a dyn review_agent::TraceSink,
        registry: &'a ReviewRunRegistry,
    ) -> Self {
        Self {
            credentials,
            factory,
            progress,
            trace,
            registry,
        }
    }

    fn progress(&self, run_id: &str, stage: &str) {
        self.progress.emit(ReviewProgressEventDto {
            run_id: run_id.into(),
            stage: stage.into(),
            tool_name: None,
            tool_calls: None,
        });
    }

    async fn source(&self) -> Result<Box<dyn ReviewSource>, IpcError> {
        let token = self
            .credentials
            .read(CredentialKindDto::Github)
            .await
            .map_err(|e| map_review_credential_error(CredentialKindDto::Github, e))?;
        self.factory.source(token).map_err(review_error)
    }

    async fn preflight(&self, target: ReviewTargetDto) -> Result<ReviewPreflightDto, IpcError> {
        let source = self.source().await?;
        let target = review_agent::ReviewTarget::from(target);
        let head_sha = source.head_sha(&target).await.map_err(review_error)?;
        let files = source
            .pull_files_at_head(&target, &head_sha)
            .await
            .map_err(review_error)?;
        let reviewable_count = files.iter().filter(|file| file.reviewable).count();
        let total_patch_bytes = files
            .iter()
            .filter(|file| file.reviewable)
            .map(|file| file.patch_bytes)
            .sum();
        Ok(review_agent::ReviewPreflight {
            head_sha,
            files,
            total_patch_bytes,
            requires_selection: reviewable_count > review_agent::MAX_AUTO_FILES
                || total_patch_bytes > review_agent::MAX_PATCH_BYTES,
        }
        .into())
    }

    async fn start(&self, input: ReviewRunInputDto) -> Result<ReviewRunResultDto, IpcError> {
        let run_id = input.run_id.clone();
        let cancel = self.registry.register(&run_id)?;
        if cancel.is_cancelled() {
            self.registry.finish(&run_id);
            self.progress(&run_id, "cancelled");
            return Err(review_error(review_agent::ReviewError::Cancelled));
        }
        self.progress(&run_id, "loading_pr");
        let result: Result<ReviewRunResultDto, IpcError> = async {
            let source = self.source().await?;
            let key = self
                .credentials
                .read(CredentialKindDto::Deepseek)
                .await
                .map_err(|e| map_review_credential_error(CredentialKindDto::Deepseek, e))?;
            let model = self.factory.model(key).map_err(review_error)?;
            self.progress(&run_id, "analyzing");
            let tool_progress = ServiceToolProgress {
                emitter: self.progress,
                run_id: &run_id,
            };
            let result = ReviewOrchestrator::new_with_progress(
                model.as_ref(),
                source.as_ref(),
                self.trace,
                &cancel,
                &tool_progress,
            )
            .run(input.into())
            .await
            .map_err(review_error)?;
            self.progress(&run_id, "generating_drafts");
            Ok(result.into())
        }
        .await;
        self.registry.finish(&run_id);
        match &result {
            Ok(_) => self.progress(&run_id, "completed"),
            Err(e) if e.code == "CANCELLED" => self.progress(&run_id, "cancelled"),
            Err(_) => self.progress(&run_id, "failed"),
        }
        result
    }

    fn cancel(&self, run_id: &str) {
        self.registry.cancel(run_id);
    }

    async fn submit(&self, input: SubmitReviewDto) -> Result<PublishedReviewDto, IpcError> {
        let source = self.source().await?;
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
}

struct ServiceToolProgress<'a> {
    emitter: &'a dyn ProgressEmitter,
    run_id: &'a str,
}
impl ProgressSink for ServiceToolProgress<'_> {
    fn report(&self, update: ProgressUpdate) {
        let ProgressUpdate::ToolCall { name, count } = update;
        self.emitter.emit(ReviewProgressEventDto {
            run_id: self.run_id.into(),
            stage: "tool_call".into(),
            tool_name: Some(name),
            tool_calls: Some(count),
        });
    }
}

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

const MAX_PENDING_CANCELLATIONS: usize = 128;
const PENDING_CANCELLATION_TTL: Duration = Duration::from_secs(60);

#[derive(Default)]
struct ReviewRunRegistryInner {
    active: HashMap<String, ReviewCancellation>,
    pending: VecDeque<(String, Instant)>,
}

#[derive(Default)]
pub(crate) struct ReviewRunRegistry(Mutex<ReviewRunRegistryInner>);

impl ReviewRunRegistry {
    fn prune_pending(inner: &mut ReviewRunRegistryInner, now: Instant) {
        while inner
            .pending
            .front()
            .is_some_and(|(_, created)| now.duration_since(*created) >= PENDING_CANCELLATION_TTL)
        {
            inner.pending.pop_front();
        }
        while inner.pending.len() > MAX_PENDING_CANCELLATIONS {
            inner.pending.pop_front();
        }
    }

    pub(crate) fn register(&self, run_id: &str) -> Result<ReviewCancellation, IpcError> {
        self.register_at(run_id, Instant::now())
    }

    fn register_at(&self, run_id: &str, now: Instant) -> Result<ReviewCancellation, IpcError> {
        let mut inner = self.0.lock().expect("review run registry lock poisoned");
        Self::prune_pending(&mut inner, now);
        if inner.active.contains_key(run_id) {
            return Err(IpcError {
                code: "REVIEW_ALREADY_RUNNING".into(),
                message: "A review with this run id is already active".into(),
                recoverable: false,
            });
        }
        let token = ReviewCancellation::new();
        if let Some(index) = inner
            .pending
            .iter()
            .position(|(pending_id, _)| pending_id == run_id)
        {
            inner.pending.remove(index);
            token.cancel();
        }
        inner.active.insert(run_id.to_owned(), token.clone());
        Ok(token)
    }
    pub(crate) fn cancel(&self, run_id: &str) {
        self.cancel_at(run_id, Instant::now());
    }

    fn cancel_at(&self, run_id: &str, now: Instant) {
        let mut inner = self.0.lock().expect("review run registry lock poisoned");
        Self::prune_pending(&mut inner, now);
        if let Some(token) = inner.active.get(run_id) {
            token.cancel();
            return;
        }
        if !inner
            .pending
            .iter()
            .any(|(pending_id, _)| pending_id == run_id)
        {
            inner.pending.push_back((run_id.to_owned(), now));
            Self::prune_pending(&mut inner, now);
        }
    }
    pub(crate) fn finish(&self, run_id: &str) {
        self.0
            .lock()
            .expect("review run registry lock poisoned")
            .active
            .remove(run_id);
    }

    #[cfg(test)]
    fn pending_count(&self) -> usize {
        self.0
            .lock()
            .expect("review run registry lock poisoned")
            .pending
            .len()
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

fn map_review_credential_error(kind: CredentialKindDto, mut error: IpcError) -> IpcError {
    if error.code == "CREDENTIAL_MISSING" {
        error.code = match kind {
            CredentialKindDto::Github => "GITHUB_TOKEN_MISSING",
            CredentialKindDto::Deepseek => "AI_KEY_MISSING",
            CredentialKindDto::Gitlab => "CREDENTIAL_MISSING",
        }
        .into();
    }
    error
}

struct KeyringCredentialReader;
#[async_trait]
impl CredentialReader for KeyringCredentialReader {
    async fn read(&self, kind: CredentialKindDto) -> Result<String, IpcError> {
        tokio::task::spawn_blocking(move || read_credential(kind))
            .await
            .map_err(crate::join_panic)?
    }
}

struct ProductionBackendFactory;
impl ReviewBackendFactory for ProductionBackendFactory {
    fn source(&self, token: String) -> Result<Box<dyn ReviewSource>, review_agent::ReviewError> {
        Ok(Box::new(GithubReviewSource::new(token)?))
    }
    fn model(
        &self,
        key: String,
    ) -> Result<Box<dyn review_agent::ModelProvider>, review_agent::ReviewError> {
        Ok(Box::new(DeepSeekResponsesProvider::new(key)?))
    }
}

struct AppProgressEmitter(tauri::AppHandle);
impl ProgressEmitter for AppProgressEmitter {
    fn emit(&self, event: ReviewProgressEventDto) {
        let _ = self.0.emit("review-progress", event);
    }
}

struct NoopProgressEmitter;
impl ProgressEmitter for NoopProgressEmitter {
    fn emit(&self, _: ReviewProgressEventDto) {}
}
struct NoopTraceSink;
#[async_trait]
impl review_agent::TraceSink for NoopTraceSink {
    async fn record(&self, _: review_agent::TraceEntry) -> Result<(), review_agent::ReviewError> {
        Ok(())
    }
}

#[tauri::command]
pub(crate) async fn get_review_preflight(
    target: ReviewTargetDto,
) -> Result<ReviewPreflightDto, IpcError> {
    ReviewCommandService::new(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &NoopProgressEmitter,
        &NoopTraceSink,
        &ReviewRunRegistry::default(),
    )
    .preflight(target)
    .await
}

#[tauri::command]
pub(crate) async fn start_pr_review(
    app: tauri::AppHandle,
    state: tauri::State<'_, ReviewRunRegistry>,
    input: ReviewRunInputDto,
) -> Result<ReviewRunResultDto, IpcError> {
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
    let emitter = AppProgressEmitter(app);
    ReviewCommandService::new(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &emitter,
        &trace,
        &state,
    )
    .start(input)
    .await
}

#[tauri::command]
pub(crate) fn cancel_pr_review(state: tauri::State<'_, ReviewRunRegistry>, run_id: String) {
    ReviewCommandService::new(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &NoopProgressEmitter,
        &NoopTraceSink,
        &state,
    )
    .cancel(&run_id);
}

#[tauri::command]
pub(crate) async fn submit_pr_review(
    input: SubmitReviewDto,
) -> Result<PublishedReviewDto, IpcError> {
    ReviewCommandService::new(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &NoopProgressEmitter,
        &NoopTraceSink,
        &ReviewRunRegistry::default(),
    )
    .submit(input)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    struct FakeCredentials;
    #[async_trait]
    impl CredentialReader for FakeCredentials {
        async fn read(&self, _: CredentialKindDto) -> Result<String, IpcError> {
            Ok("injected-only".into())
        }
    }

    struct MissingCredentials {
        missing: CredentialKindDto,
    }

    struct CountingCredentials(Arc<AtomicUsize>);
    #[async_trait]
    impl CredentialReader for CountingCredentials {
        async fn read(&self, _: CredentialKindDto) -> Result<String, IpcError> {
            self.0.fetch_add(1, AtomicOrdering::SeqCst);
            Ok("injected-only".into())
        }
    }
    #[async_trait]
    impl CredentialReader for MissingCredentials {
        async fn read(&self, kind: CredentialKindDto) -> Result<String, IpcError> {
            if kind == self.missing {
                Err(IpcError {
                    code: "CREDENTIAL_MISSING".into(),
                    message: "Credential is not configured".into(),
                    recoverable: true,
                })
            } else {
                Ok("injected-only".into())
            }
        }
    }

    #[derive(Clone)]
    struct FakeSource {
        head: Arc<Mutex<String>>,
        published: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl ReviewSource for FakeSource {
        async fn head_sha(
            &self,
            _: &review_agent::ReviewTarget,
        ) -> Result<String, review_agent::ReviewError> {
            Ok(self.head.lock().unwrap().clone())
        }
        async fn pull_files_at_head(
            &self,
            _: &review_agent::ReviewTarget,
            expected: &str,
        ) -> Result<Vec<review_agent::ReviewFile>, review_agent::ReviewError> {
            if self.head.lock().unwrap().as_str() != expected {
                return Err(review_agent::ReviewError::PrUpdated);
            }
            Ok(vec![review_agent::ReviewFile::from_patch(
                "src/lib.rs",
                "@@ -1 +1 @@\n-a\n+b",
            )?])
        }
        async fn list_repository_tree(
            &self,
            _: &review_agent::ReviewTarget,
            _: &str,
            _: Option<&str>,
        ) -> Result<Vec<String>, review_agent::ReviewError> {
            Ok(vec!["src/lib.rs".into()])
        }
        async fn read_file(
            &self,
            _: &review_agent::ReviewTarget,
            _: &str,
            _: &str,
            _: u32,
            _: u32,
        ) -> Result<String, review_agent::ReviewError> {
            Ok("safe".into())
        }
        async fn publish(
            &self,
            _: &review_agent::SubmitReview,
        ) -> Result<review_agent::PublishedReview, review_agent::ReviewError> {
            self.published.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(review_agent::PublishedReview {
                review_id: 9,
                html_url: Some("https://example.invalid/review/9".into()),
            })
        }
    }

    struct FakeModel(
        Arc<Mutex<VecDeque<Result<review_agent::ModelResponse, review_agent::ReviewError>>>>,
    );
    #[async_trait]
    impl review_agent::ModelProvider for FakeModel {
        async fn respond(
            &self,
            _: &[review_agent::TranscriptItem],
        ) -> Result<review_agent::ModelResponse, review_agent::ReviewError> {
            self.0.lock().unwrap().pop_front().unwrap_or_else(|| {
                Err(review_agent::ReviewError::InvalidModelOutput(
                    "empty fake".into(),
                ))
            })
        }
    }

    struct SelfCancellingModel {
        registry: Arc<ReviewRunRegistry>,
        run_id: String,
    }
    #[async_trait]
    impl review_agent::ModelProvider for SelfCancellingModel {
        async fn respond(
            &self,
            _: &[review_agent::TranscriptItem],
        ) -> Result<review_agent::ModelResponse, review_agent::ReviewError> {
            self.registry.cancel(&self.run_id);
            std::future::pending().await
        }
    }

    struct CancellingFactory {
        source: FakeSource,
        registry: Arc<ReviewRunRegistry>,
    }
    impl ReviewBackendFactory for CancellingFactory {
        fn source(&self, _: String) -> Result<Box<dyn ReviewSource>, review_agent::ReviewError> {
            Ok(Box::new(self.source.clone()))
        }
        fn model(
            &self,
            _: String,
        ) -> Result<Box<dyn review_agent::ModelProvider>, review_agent::ReviewError> {
            Ok(Box::new(SelfCancellingModel {
                registry: self.registry.clone(),
                run_id: "live-cancel".into(),
            }))
        }
    }

    struct FakeFactory {
        source: FakeSource,
        responses:
            Arc<Mutex<VecDeque<Result<review_agent::ModelResponse, review_agent::ReviewError>>>>,
    }

    struct CountingFactory {
        source_calls: Arc<AtomicUsize>,
        model_calls: Arc<AtomicUsize>,
    }
    impl ReviewBackendFactory for CountingFactory {
        fn source(&self, _: String) -> Result<Box<dyn ReviewSource>, review_agent::ReviewError> {
            self.source_calls.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(Box::new(fake_factory(vec![]).source))
        }
        fn model(
            &self,
            _: String,
        ) -> Result<Box<dyn review_agent::ModelProvider>, review_agent::ReviewError> {
            self.model_calls.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(Box::new(FakeModel(Arc::new(Mutex::new(VecDeque::new())))))
        }
    }
    impl ReviewBackendFactory for FakeFactory {
        fn source(&self, _: String) -> Result<Box<dyn ReviewSource>, review_agent::ReviewError> {
            Ok(Box::new(self.source.clone()))
        }
        fn model(
            &self,
            _: String,
        ) -> Result<Box<dyn review_agent::ModelProvider>, review_agent::ReviewError> {
            Ok(Box::new(FakeModel(self.responses.clone())))
        }
    }

    #[derive(Default)]
    struct RecordingEmitter(Mutex<Vec<ReviewProgressEventDto>>);
    impl ProgressEmitter for RecordingEmitter {
        fn emit(&self, event: ReviewProgressEventDto) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn fake_factory(
        responses: Vec<Result<review_agent::ModelResponse, review_agent::ReviewError>>,
    ) -> FakeFactory {
        FakeFactory {
            source: FakeSource {
                head: Arc::new(Mutex::new("abc".into())),
                published: Arc::new(AtomicUsize::new(0)),
            },
            responses: Arc::new(Mutex::new(responses.into())),
        }
    }
    fn target_dto() -> ReviewTargetDto {
        ReviewTargetDto {
            owner: "o".into(),
            repo: "r".into(),
            pull_number: 1,
        }
    }
    fn run_input(run_id: &str) -> ReviewRunInputDto {
        ReviewRunInputDto {
            run_id: run_id.into(),
            target: target_dto(),
            expected_head_sha: "abc".into(),
            selected_files: vec!["src/lib.rs".into()],
        }
    }
    fn submit_input() -> SubmitReviewDto {
        SubmitReviewDto {
            target: target_dto(),
            head_sha: "abc".into(),
            findings: vec![],
        }
    }

    #[tokio::test]
    async fn service_preflight_and_start_success_emit_ordered_real_progress_and_cleanup() {
        let factory = fake_factory(vec![
            Ok(review_agent::ModelResponse::tool_calls(
                vec![review_agent::ToolCall::list_tree("tree", "src")],
                review_agent::ReviewUsage::default(),
            )),
            Ok(review_agent::ModelResponse::final_review(
                "No correctness issues found.",
                vec![],
                review_agent::ReviewUsage::default(),
            )),
        ]);
        let emitter = RecordingEmitter::default();
        let registry = ReviewRunRegistry::default();
        let service = ReviewCommandService::new(
            &FakeCredentials,
            &factory,
            &emitter,
            &NoopTraceSink,
            &registry,
        );
        let preflight = service.preflight(target_dto()).await.unwrap();
        assert_eq!(preflight.head_sha, "abc");
        assert_eq!(preflight.files.len(), 1);
        let result = service.start(run_input("success")).await.unwrap();
        assert_eq!(result.summary, "No correctness issues found.");
        assert_eq!(result.reviewed_files, ["src/lib.rs"]);
        let events = emitter.0.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .map(|event| event.stage.as_str())
                .collect::<Vec<_>>(),
            [
                "loading_pr",
                "analyzing",
                "tool_call",
                "generating_drafts",
                "completed"
            ]
        );
        assert_eq!(events[2].tool_name.as_deref(), Some("list_repository_tree"));
        drop(events);
        assert!(registry.register("success").is_ok());
    }

    #[tokio::test]
    async fn service_exposes_exact_provider_missing_key_codes() {
        let factory = fake_factory(vec![]);
        let registry = ReviewRunRegistry::default();
        let github = MissingCredentials {
            missing: CredentialKindDto::Github,
        };
        let service = ReviewCommandService::new(
            &github,
            &factory,
            &NoopProgressEmitter,
            &NoopTraceSink,
            &registry,
        );
        assert_eq!(
            service.preflight(target_dto()).await.unwrap_err().code,
            "GITHUB_TOKEN_MISSING"
        );

        let deepseek = MissingCredentials {
            missing: CredentialKindDto::Deepseek,
        };
        let service = ReviewCommandService::new(
            &deepseek,
            &factory,
            &NoopProgressEmitter,
            &NoopTraceSink,
            &registry,
        );
        assert_eq!(
            service
                .start(run_input("missing-ai"))
                .await
                .unwrap_err()
                .code,
            "AI_KEY_MISSING"
        );
        assert!(registry.register("missing-ai").is_ok());
    }

    #[tokio::test]
    async fn service_rejects_duplicate_and_cleans_up_cancelled_and_failed_runs() {
        let registry = ReviewRunRegistry::default();
        let _ = registry.register("duplicate").unwrap();
        let factory = fake_factory(vec![]);
        let emitter = RecordingEmitter::default();
        let service = ReviewCommandService::new(
            &FakeCredentials,
            &factory,
            &emitter,
            &NoopTraceSink,
            &registry,
        );
        assert_eq!(
            service
                .start(run_input("duplicate"))
                .await
                .unwrap_err()
                .code,
            "REVIEW_ALREADY_RUNNING"
        );
        registry.finish("duplicate");

        let cancelled_factory = fake_factory(vec![Err(review_agent::ReviewError::Cancelled)]);
        let cancelled = ReviewCommandService::new(
            &FakeCredentials,
            &cancelled_factory,
            &emitter,
            &NoopTraceSink,
            &registry,
        );
        assert_eq!(
            cancelled
                .start(run_input("cancelled"))
                .await
                .unwrap_err()
                .code,
            "CANCELLED"
        );
        assert!(registry.register("cancelled").is_ok());
        registry.finish("cancelled");

        let failed_factory = fake_factory(vec![]);
        let failed = ReviewCommandService::new(
            &FakeCredentials,
            &failed_factory,
            &emitter,
            &NoopTraceSink,
            &registry,
        );
        assert_eq!(
            failed.start(run_input("failed")).await.unwrap_err().code,
            "INVALID_MODEL_OUTPUT"
        );
        assert!(registry.register("failed").is_ok());
    }

    #[tokio::test]
    async fn service_cancel_interrupts_an_active_run_and_cleans_registry() {
        let registry = Arc::new(ReviewRunRegistry::default());
        let factory = CancellingFactory {
            source: fake_factory(vec![]).source,
            registry: registry.clone(),
        };
        let emitter = RecordingEmitter::default();
        let service = ReviewCommandService::new(
            &FakeCredentials,
            &factory,
            &emitter,
            &NoopTraceSink,
            &registry,
        );
        let error = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            service.start(run_input("live-cancel")),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(error.code, "CANCELLED");
        assert_eq!(emitter.0.lock().unwrap().last().unwrap().stage, "cancelled");
        assert!(registry.register("live-cancel").is_ok());
    }

    #[tokio::test]
    async fn service_submit_publishes_only_when_head_is_unchanged() {
        let factory = fake_factory(vec![]);
        let registry = ReviewRunRegistry::default();
        let service = ReviewCommandService::new(
            &FakeCredentials,
            &factory,
            &NoopProgressEmitter,
            &NoopTraceSink,
            &registry,
        );
        let published = service.submit(submit_input()).await.unwrap();
        assert_eq!(published.review_id, 9);
        assert_eq!(factory.source.published.load(AtomicOrdering::SeqCst), 1);
        *factory.source.head.lock().unwrap() = "changed".into();
        assert_eq!(
            service.submit(submit_input()).await.unwrap_err().code,
            "PR_UPDATED"
        );
        assert_eq!(factory.source.published.load(AtomicOrdering::SeqCst), 1);
    }

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
    fn cancel_before_register_is_consumed_as_cancelled_and_tombstones_are_bounded() {
        let registry = ReviewRunRegistry::default();
        registry.cancel("before");
        registry.cancel("before");
        let token = registry.register("before").unwrap();
        assert!(token.is_cancelled());
        assert_eq!(registry.pending_count(), 0);
        registry.finish("before");

        for index in 0..(MAX_PENDING_CANCELLATIONS + 20) {
            registry.cancel(&format!("unknown-{index}"));
        }
        assert_eq!(registry.pending_count(), MAX_PENDING_CANCELLATIONS);
    }

    #[test]
    fn expired_pending_cancellation_does_not_cancel_a_later_run() {
        let registry = ReviewRunRegistry::default();
        let now = Instant::now();
        registry.cancel_at("expired", now);
        let token = registry
            .register_at("expired", now + PENDING_CANCELLATION_TTL)
            .unwrap();
        assert!(!token.is_cancelled());
        assert_eq!(registry.pending_count(), 0);
    }

    #[tokio::test]
    async fn cancel_before_start_prevents_all_credential_source_and_model_work() {
        let registry = ReviewRunRegistry::default();
        registry.cancel("cancel-first");
        let credential_reads = Arc::new(AtomicUsize::new(0));
        let source_calls = Arc::new(AtomicUsize::new(0));
        let model_calls = Arc::new(AtomicUsize::new(0));
        let credentials = CountingCredentials(credential_reads.clone());
        let factory = CountingFactory {
            source_calls: source_calls.clone(),
            model_calls: model_calls.clone(),
        };
        let service = ReviewCommandService::new(
            &credentials,
            &factory,
            &NoopProgressEmitter,
            &NoopTraceSink,
            &registry,
        );
        assert_eq!(
            service
                .start(run_input("cancel-first"))
                .await
                .unwrap_err()
                .code,
            "CANCELLED"
        );
        assert_eq!(credential_reads.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(source_calls.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(model_calls.load(AtomicOrdering::SeqCst), 0);
        assert!(registry.register("cancel-first").is_ok());
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

    #[test]
    fn review_credentials_use_provider_specific_missing_codes() {
        let missing = IpcError {
            code: "CREDENTIAL_MISSING".into(),
            message: "Credential is not configured".into(),
            recoverable: true,
        };
        assert_eq!(
            map_review_credential_error(CredentialKindDto::Github, missing.clone()).code,
            "GITHUB_TOKEN_MISSING"
        );
        assert_eq!(
            map_review_credential_error(CredentialKindDto::Deepseek, missing).code,
            "AI_KEY_MISSING"
        );
    }

    #[test]
    fn command_service_is_constructed_from_injected_dependencies() {
        fn assert_service_type(_: &ReviewCommandService<'_>) {}
        let _ = assert_service_type;
    }
}
