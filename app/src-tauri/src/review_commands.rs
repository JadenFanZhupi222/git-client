use crate::agent_events::AppAgentEventEmitter;
use crate::credentials::read_credential;
use async_trait::async_trait;
use ipc_types::{
    AgentIpcErrorDto, CredentialKindDto, IpcError, IssueContextDto, IssueRepositoryTargetDto,
    IssueSummaryDto, IssueTargetDto, IssueTriageInputDto, IssueTriagePublishInputDto,
    IssueTriagePublishResultDto, IssueTriageResultDto, PublishedReviewDto,
    ReviewModelCapabilitiesDto, ReviewModelOptionDto, ReviewModelPricingDto, ReviewPreflightDto,
    ReviewProgressEventDto, ReviewRunInputDto, ReviewRunResultDto, ReviewTargetDto,
    SubmitReviewDto,
};
use review_agent::{
    CancelSignal, GithubIssueSource, GithubReviewSource, GitlabReviewSource,
    IssuePublicationSource, IssueSource, IssueTriageOrchestrator, IssueTriagePublisher,
    ProgressSink, ProgressUpdate, ReviewOrchestrator, ReviewSource, SanitizedTraceStore,
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
    fn source(
        &self,
        platform: ReviewPlatform,
        token: String,
    ) -> Result<Box<dyn ReviewSource>, review_agent::ReviewError>;
    fn model(
        &self,
        model_id: &str,
        key: String,
    ) -> Result<Box<dyn review_agent::ModelProvider>, review_agent::ReviewError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReviewPlatform {
    Github,
    Gitlab,
}

impl ReviewPlatform {
    fn credential(self) -> CredentialKindDto {
        match self {
            Self::Github => CredentialKindDto::Github,
            Self::Gitlab => CredentialKindDto::Gitlab,
        }
    }

    fn resource_prefix(self) -> &'static str {
        match self {
            Self::Github => "github-pr",
            Self::Gitlab => "gitlab-mr",
        }
    }
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
    platform: ReviewPlatform,
    agent_events: Option<&'a dyn review_agent::AgentEventSink>,
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
            platform: ReviewPlatform::Github,
            agent_events: None,
        }
    }

    fn new_for_platform(
        credentials: &'a dyn CredentialReader,
        factory: &'a dyn ReviewBackendFactory,
        progress: &'a dyn ProgressEmitter,
        trace: &'a dyn review_agent::TraceSink,
        registry: &'a ReviewRunRegistry,
        platform: ReviewPlatform,
    ) -> Self {
        Self {
            credentials,
            factory,
            progress,
            trace,
            registry,
            platform,
            agent_events: None,
        }
    }

    fn with_agent_events(mut self, sink: &'a dyn review_agent::AgentEventSink) -> Self {
        self.agent_events = Some(sink);
        self
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
        let credential = self.platform.credential();
        let token = self
            .credentials
            .read(credential)
            .await
            .map_err(|e| map_review_credential_error(credential, e))?;
        self.factory
            .source(self.platform, token)
            .map_err(review_error)
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

    async fn start(
        &self,
        input: ReviewRunInputDto,
    ) -> Result<ReviewRunResultDto, AgentIpcErrorDto> {
        let run_id = input.run_id.clone();
        let diagnostic_id = review_agent::diagnostic_id(&run_id);
        let resource_key = review_resource_key_for(self.platform, &input.target);
        let cancel = self
            .registry
            .register_resource(&run_id, &resource_key)
            .map_err(|error| agent_error(error, &diagnostic_id))?;
        if cancel.is_cancelled() {
            self.registry.finish(&run_id);
            self.progress(&run_id, "cancelled");
            return Err(agent_error(
                review_error(review_agent::ReviewError::Cancelled),
                &diagnostic_id,
            ));
        }
        self.progress(&run_id, "loading_pr");
        let result: Result<ReviewRunResultDto, IpcError> = async {
            let source = self.source().await?;
            let credential_kind = review_model_credential(&input.model_id)?;
            let key = self
                .credentials
                .read(credential_kind)
                .await
                .map_err(|e| map_review_credential_error(credential_kind, e))?;
            let model = self
                .factory
                .model(&input.model_id, key)
                .map_err(review_error)?;
            self.progress(&run_id, "analyzing");
            let tool_progress = ServiceToolProgress {
                emitter: self.progress,
                run_id: &run_id,
            };
            let orchestrator = ReviewOrchestrator::new_with_progress(
                model.as_ref(),
                source.as_ref(),
                self.trace,
                &cancel,
                &tool_progress,
            );
            let event_publisher = self
                .agent_events
                .map(|sink| review_agent::AgentEventPublisher::new(&run_id, sink));
            let orchestrator = if let Some(events) = event_publisher.as_ref() {
                orchestrator.with_agent_events(events)
            } else {
                orchestrator
            };
            let result = orchestrator.run(input.into()).await.map_err(review_error)?;
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
        result.map_err(|error| agent_error(error, &diagnostic_id))
    }

    fn cancel(&self, run_id: &str) {
        self.registry.cancel(run_id);
    }

    async fn submit(&self, input: SubmitReviewDto) -> Result<PublishedReviewDto, IpcError> {
        let source = self.source().await?;
        let review: review_agent::SubmitReview = input.try_into().map_err(|message| IpcError {
            code: "INVALID_MODEL_OUTPUT".into(),
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

trait IssueBackendFactory: Send + Sync {
    fn issue_source(
        &self,
        token: String,
    ) -> Result<Box<dyn IssueSource>, review_agent::ReviewError>;
    fn issue_model(
        &self,
        model_id: &str,
        key: String,
    ) -> Result<Box<dyn review_agent::ModelProvider>, review_agent::ReviewError>;
    fn issue_publisher(
        &self,
        token: String,
    ) -> Result<Box<dyn IssuePublicationSource>, review_agent::ReviewError>;
}

struct IssueCommandService<'a> {
    credentials: &'a dyn CredentialReader,
    factory: &'a dyn IssueBackendFactory,
    progress: &'a dyn ProgressEmitter,
    trace: Option<&'a dyn review_agent::TraceSink>,
    registry: &'a ReviewRunRegistry,
    agent_events: Option<&'a dyn review_agent::AgentEventSink>,
}

impl<'a> IssueCommandService<'a> {
    fn new(
        credentials: &'a dyn CredentialReader,
        factory: &'a dyn IssueBackendFactory,
        progress: &'a dyn ProgressEmitter,
        registry: &'a ReviewRunRegistry,
    ) -> Self {
        Self {
            credentials,
            factory,
            progress,
            trace: None,
            registry,
            agent_events: None,
        }
    }

    fn new_with_trace(
        credentials: &'a dyn CredentialReader,
        factory: &'a dyn IssueBackendFactory,
        progress: &'a dyn ProgressEmitter,
        trace: &'a dyn review_agent::TraceSink,
        registry: &'a ReviewRunRegistry,
    ) -> Self {
        Self {
            credentials,
            factory,
            progress,
            trace: Some(trace),
            registry,
            agent_events: None,
        }
    }

    fn with_agent_events(mut self, sink: &'a dyn review_agent::AgentEventSink) -> Self {
        self.agent_events = Some(sink);
        self
    }

    fn progress(&self, run_id: &str, stage: &str) {
        self.progress.emit(ReviewProgressEventDto {
            run_id: run_id.into(),
            stage: stage.into(),
            tool_name: None,
            tool_calls: None,
        });
    }

    async fn source(&self) -> Result<Box<dyn IssueSource>, IpcError> {
        let token = self
            .credentials
            .read(CredentialKindDto::Github)
            .await
            .map_err(|error| map_review_credential_error(CredentialKindDto::Github, error))?;
        self.factory.issue_source(token).map_err(review_error)
    }

    async fn list(
        &self,
        target: IssueRepositoryTargetDto,
    ) -> Result<Vec<IssueSummaryDto>, IpcError> {
        Ok(self
            .source()
            .await?
            .list_open_issues(&target.into())
            .await
            .map_err(review_error)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    async fn context(&self, target: IssueTargetDto) -> Result<IssueContextDto, IpcError> {
        Ok(self
            .source()
            .await?
            .issue_context(&target.into())
            .await
            .map_err(review_error)?
            .into())
    }

    async fn start(
        &self,
        input: IssueTriageInputDto,
    ) -> Result<IssueTriageResultDto, AgentIpcErrorDto> {
        let run_id = input.run_id.clone();
        let diagnostic_id = review_agent::diagnostic_id(&run_id);
        let resource_key = issue_resource_key(&input.target);
        let cancel = self
            .registry
            .register_resource(&run_id, &resource_key)
            .map_err(|error| agent_error(error, &diagnostic_id))?;
        if cancel.is_cancelled() {
            self.registry.finish(&run_id);
            self.progress(&run_id, "cancelled");
            return Err(agent_error(
                review_error(review_agent::ReviewError::Cancelled),
                &diagnostic_id,
            ));
        }
        self.progress(&run_id, "loading_issue");
        let result: Result<IssueTriageResultDto, IpcError> = async {
            let source = self.source().await?;
            let credential_kind = review_model_credential(&input.model_id)?;
            let key = self
                .credentials
                .read(credential_kind)
                .await
                .map_err(|error| map_review_credential_error(credential_kind, error))?;
            let model = self
                .factory
                .issue_model(&input.model_id, key)
                .map_err(review_error)?;
            self.progress(&run_id, "analyzing_issue");
            let orchestrator = if let Some(trace) = self.trace {
                IssueTriageOrchestrator::new_with_trace(
                    model.as_ref(),
                    source.as_ref(),
                    &cancel,
                    trace,
                )
            } else {
                IssueTriageOrchestrator::new(model.as_ref(), source.as_ref(), &cancel)
            };
            let event_publisher = self
                .agent_events
                .map(|sink| review_agent::AgentEventPublisher::new(&run_id, sink));
            let orchestrator = if let Some(events) = event_publisher.as_ref() {
                orchestrator.with_agent_events(events)
            } else {
                orchestrator
            };
            let result = orchestrator.run(input.into()).await;
            Ok(result.map_err(review_error)?.into())
        }
        .await;
        self.registry.finish(&run_id);
        match &result {
            Ok(_) => self.progress(&run_id, "completed"),
            Err(error) if error.code == "CANCELLED" => self.progress(&run_id, "cancelled"),
            Err(_) => self.progress(&run_id, "failed"),
        }
        result.map_err(|error| agent_error(error, &diagnostic_id))
    }

    async fn publish(
        &self,
        input: IssueTriagePublishInputDto,
    ) -> Result<IssueTriagePublishResultDto, IpcError> {
        let token = self
            .credentials
            .read(CredentialKindDto::Github)
            .await
            .map_err(|error| map_review_credential_error(CredentialKindDto::Github, error))?;
        let source = self.factory.issue_publisher(token).map_err(review_error)?;
        Ok(IssueTriagePublisher::new(source.as_ref())
            .publish(input.into())
            .await
            .map_err(review_error)?
            .into())
    }

    fn cancel(&self, run_id: &str) {
        self.registry.cancel(run_id);
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

impl review_agent::ToolCancellation for ReviewCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

const MAX_PENDING_CANCELLATIONS: usize = 128;
const PENDING_CANCELLATION_TTL: Duration = Duration::from_secs(60);

#[derive(Default)]
struct ReviewRunRegistryInner {
    active: HashMap<String, ActiveReviewRun>,
    active_resources: HashMap<String, String>,
    pending: VecDeque<(String, Instant)>,
}

struct ActiveReviewRun {
    cancellation: ReviewCancellation,
    resource_key: String,
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

    #[cfg(test)]
    pub(crate) fn register(&self, run_id: &str) -> Result<ReviewCancellation, IpcError> {
        self.register_resource(run_id, &format!("run:{run_id}"))
    }

    pub(crate) fn register_resource(
        &self,
        run_id: &str,
        resource_key: &str,
    ) -> Result<ReviewCancellation, IpcError> {
        self.register_at(run_id, resource_key, Instant::now())
    }

    fn register_at(
        &self,
        run_id: &str,
        resource_key: &str,
        now: Instant,
    ) -> Result<ReviewCancellation, IpcError> {
        let mut inner = self.0.lock().expect("review run registry lock poisoned");
        Self::prune_pending(&mut inner, now);
        if inner.active.contains_key(run_id) {
            return Err(IpcError {
                code: "REVIEW_ALREADY_RUNNING".into(),
                message: "A review with this run id is already active".into(),
                recoverable: false,
            });
        }
        if inner.active_resources.contains_key(resource_key) {
            return Err(IpcError {
                code: "AGENT_RESOURCE_BUSY".into(),
                message: "An agent task is already active for this resource".into(),
                recoverable: true,
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
        inner.active.insert(
            run_id.to_owned(),
            ActiveReviewRun {
                cancellation: token.clone(),
                resource_key: resource_key.to_owned(),
            },
        );
        inner
            .active_resources
            .insert(resource_key.to_owned(), run_id.to_owned());
        Ok(token)
    }
    pub(crate) fn cancel(&self, run_id: &str) {
        self.cancel_at(run_id, Instant::now());
    }

    fn cancel_at(&self, run_id: &str, now: Instant) {
        let mut inner = self.0.lock().expect("review run registry lock poisoned");
        Self::prune_pending(&mut inner, now);
        if let Some(run) = inner.active.get(run_id) {
            run.cancellation.cancel();
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
        let mut inner = self.0.lock().expect("review run registry lock poisoned");
        if let Some(run) = inner.active.remove(run_id) {
            inner.active_resources.remove(&run.resource_key);
        }
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

#[cfg(test)]
fn review_resource_key(target: &ReviewTargetDto) -> String {
    review_resource_key_for(ReviewPlatform::Github, target)
}

fn review_resource_key_for(platform: ReviewPlatform, target: &ReviewTargetDto) -> String {
    format!(
        "{}:{}/{}#{}",
        platform.resource_prefix(),
        target.owner.trim().to_ascii_lowercase(),
        target.repo.trim().to_ascii_lowercase(),
        target.pull_number
    )
}

fn issue_resource_key(target: &IssueTargetDto) -> String {
    format!(
        "issue:{}/{}#{}",
        target.owner.trim().to_ascii_lowercase(),
        target.repo.trim().to_ascii_lowercase(),
        target.issue_number
    )
}

pub(crate) fn review_error(error: review_agent::ReviewError) -> IpcError {
    let recoverable = matches!(
        error,
        review_agent::ReviewError::RateLimited
            | review_agent::ReviewError::NetworkError(_)
            | review_agent::ReviewError::PrUpdated
            | review_agent::ReviewError::IssueUpdated
            | review_agent::ReviewError::WorktreeUpdated
            | review_agent::ReviewError::IndexNotClean
            | review_agent::ReviewError::ChangeCommitFailed(_)
            | review_agent::ReviewError::Cancelled
            | review_agent::ReviewError::ReviewPublishFailed(_)
            | review_agent::ReviewError::IssuePublishFailed(_)
    );
    IpcError {
        code: error.code().into(),
        message: match error {
            review_agent::ReviewError::NetworkError(_) => "Network request failed".into(),
            review_agent::ReviewError::InvalidModelOutput(_) => {
                "The model returned invalid review data".into()
            }
            review_agent::ReviewError::ReviewPublishFailed(_) => "Review publication failed".into(),
            review_agent::ReviewError::IssuePublishFailed(_) => "Issue publication failed".into(),
            other => other.to_string(),
        },
        recoverable,
    }
}

pub(crate) fn agent_error(error: IpcError, diagnostic_id: &str) -> AgentIpcErrorDto {
    AgentIpcErrorDto::from_ipc(error, diagnostic_id)
}

pub(crate) fn map_review_credential_error(
    kind: CredentialKindDto,
    mut error: IpcError,
) -> IpcError {
    if error.code == "CREDENTIAL_MISSING" {
        error.code = match kind {
            CredentialKindDto::Github => "GITHUB_TOKEN_MISSING",
            CredentialKindDto::Deepseek => "AI_KEY_MISSING",
            CredentialKindDto::Openai => "OPENAI_KEY_MISSING",
            CredentialKindDto::Anthropic => "ANTHROPIC_KEY_MISSING",
            CredentialKindDto::Gitlab => "GITLAB_TOKEN_MISSING",
        }
        .into();
    }
    error
}

fn review_model_options() -> Vec<ReviewModelOptionDto> {
    review_agent::model_catalog()
        .into_iter()
        .map(|entry| ReviewModelOptionDto {
            id: entry.id,
            label: entry.label,
            provider: entry.provider_label,
            provider_id: entry.provider_id,
            capabilities: ReviewModelCapabilitiesDto {
                supports_tool_calling: entry.capabilities.tool_calling
                    != review_agent::ToolCallingSupport::None,
                supports_structured_output: entry.capabilities.structured_output
                    != review_agent::StructuredOutputSupport::None,
                context_window_tokens: entry.capabilities.context_window_tokens,
                max_output_tokens: entry.capabilities.max_output_tokens,
                reports_usage: entry.capabilities.usage != review_agent::UsageSupport::None,
            },
            pricing: entry.pricing.map(|pricing| ReviewModelPricingDto {
                currency: pricing.currency,
                input_cache_hit_per_million_micros: pricing.input_cache_hit_per_million_micros,
                input_cache_miss_per_million_micros: pricing.input_cache_miss_per_million_micros,
                output_per_million_micros: pricing.output_per_million_micros,
                source_url: pricing.source_url,
                source_version: pricing.source_version,
                checked_at: pricing.checked_at,
            }),
        })
        .collect()
}

pub(crate) fn review_model_credential(model_id: &str) -> Result<CredentialKindDto, IpcError> {
    match review_agent::model_provider_id(model_id) {
        Some("deepseek") => Ok(CredentialKindDto::Deepseek),
        Some("openai") => Ok(CredentialKindDto::Openai),
        Some("anthropic") => Ok(CredentialKindDto::Anthropic),
        _ => Err(IpcError {
            code: "INVALID_REVIEW_MODEL".into(),
            message: "The selected review model is not supported".into(),
            recoverable: false,
        }),
    }
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
    fn source(
        &self,
        platform: ReviewPlatform,
        token: String,
    ) -> Result<Box<dyn ReviewSource>, review_agent::ReviewError> {
        match platform {
            ReviewPlatform::Github => Ok(Box::new(GithubReviewSource::new(token)?)),
            ReviewPlatform::Gitlab => Ok(Box::new(GitlabReviewSource::new(token)?)),
        }
    }
    fn model(
        &self,
        model_id: &str,
        key: String,
    ) -> Result<Box<dyn review_agent::ModelProvider>, review_agent::ReviewError> {
        review_agent::create_model_provider(key, model_id)
    }
}

impl IssueBackendFactory for ProductionBackendFactory {
    fn issue_source(
        &self,
        token: String,
    ) -> Result<Box<dyn IssueSource>, review_agent::ReviewError> {
        Ok(Box::new(GithubIssueSource::new(token)?))
    }

    fn issue_model(
        &self,
        model_id: &str,
        key: String,
    ) -> Result<Box<dyn review_agent::ModelProvider>, review_agent::ReviewError> {
        review_agent::create_model_provider(key, model_id)
    }

    fn issue_publisher(
        &self,
        token: String,
    ) -> Result<Box<dyn IssuePublicationSource>, review_agent::ReviewError> {
        Ok(Box::new(GithubIssueSource::new(token)?))
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
pub(crate) fn list_review_models() -> Vec<ReviewModelOptionDto> {
    review_model_options()
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
pub(crate) async fn get_gitlab_review_preflight(
    target: ReviewTargetDto,
) -> Result<ReviewPreflightDto, IpcError> {
    ReviewCommandService::new_for_platform(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &NoopProgressEmitter,
        &NoopTraceSink,
        &ReviewRunRegistry::default(),
        ReviewPlatform::Gitlab,
    )
    .preflight(target)
    .await
}

#[tauri::command]
pub(crate) async fn start_pr_review(
    app: tauri::AppHandle,
    state: tauri::State<'_, ReviewRunRegistry>,
    input: ReviewRunInputDto,
) -> Result<ReviewRunResultDto, AgentIpcErrorDto> {
    let diagnostic_id = review_agent::diagnostic_id(&input.run_id);
    let trace_path = app
        .path()
        .app_data_dir()
        .map_err(|_| IpcError {
            code: "APP_DATA_DIR".into(),
            message: "Application data directory is unavailable".into(),
            recoverable: false,
        })
        .map_err(|error| agent_error(error, &diagnostic_id))?
        .join("review-agent-trace.json");
    let trace = SanitizedTraceStore::new(trace_path);
    let emitter = AppProgressEmitter(app.clone());
    let agent_events = AppAgentEventEmitter(app);
    ReviewCommandService::new(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &emitter,
        &trace,
        &state,
    )
    .with_agent_events(&agent_events)
    .start(input)
    .await
}

#[tauri::command]
pub(crate) async fn start_gitlab_mr_review(
    app: tauri::AppHandle,
    state: tauri::State<'_, ReviewRunRegistry>,
    input: ReviewRunInputDto,
) -> Result<ReviewRunResultDto, AgentIpcErrorDto> {
    let diagnostic_id = review_agent::diagnostic_id(&input.run_id);
    let trace_path = app
        .path()
        .app_data_dir()
        .map_err(|_| IpcError {
            code: "APP_DATA_DIR".into(),
            message: "Application data directory is unavailable".into(),
            recoverable: false,
        })
        .map_err(|error| agent_error(error, &diagnostic_id))?
        .join("gitlab-review-agent-trace.json");
    let trace = SanitizedTraceStore::new(trace_path);
    let emitter = AppProgressEmitter(app.clone());
    let agent_events = AppAgentEventEmitter(app);
    ReviewCommandService::new_for_platform(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &emitter,
        &trace,
        &state,
        ReviewPlatform::Gitlab,
    )
    .with_agent_events(&agent_events)
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

#[tauri::command]
pub(crate) async fn submit_gitlab_mr_review(
    input: SubmitReviewDto,
) -> Result<PublishedReviewDto, IpcError> {
    ReviewCommandService::new_for_platform(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &NoopProgressEmitter,
        &NoopTraceSink,
        &ReviewRunRegistry::default(),
        ReviewPlatform::Gitlab,
    )
    .submit(input)
    .await
}

#[tauri::command]
pub(crate) async fn list_github_issues(
    target: IssueRepositoryTargetDto,
) -> Result<Vec<IssueSummaryDto>, IpcError> {
    IssueCommandService::new(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &NoopProgressEmitter,
        &ReviewRunRegistry::default(),
    )
    .list(target)
    .await
}

#[tauri::command]
pub(crate) async fn get_github_issue_context(
    target: IssueTargetDto,
) -> Result<IssueContextDto, IpcError> {
    IssueCommandService::new(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &NoopProgressEmitter,
        &ReviewRunRegistry::default(),
    )
    .context(target)
    .await
}

#[tauri::command]
pub(crate) async fn start_issue_triage(
    app: tauri::AppHandle,
    state: tauri::State<'_, ReviewRunRegistry>,
    input: IssueTriageInputDto,
) -> Result<IssueTriageResultDto, AgentIpcErrorDto> {
    let diagnostic_id = review_agent::diagnostic_id(&input.run_id);
    let trace_path = app
        .path()
        .app_data_dir()
        .map_err(|_| IpcError {
            code: "APP_DATA_DIR".into(),
            message: "Application data directory is unavailable".into(),
            recoverable: false,
        })
        .map_err(|error| agent_error(error, &diagnostic_id))?
        .join("review-agent-trace.json");
    let trace = SanitizedTraceStore::new(trace_path);
    let emitter = AppProgressEmitter(app.clone());
    let agent_events = AppAgentEventEmitter(app);
    IssueCommandService::new_with_trace(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &emitter,
        &trace,
        &state,
    )
    .with_agent_events(&agent_events)
    .start(input)
    .await
}

#[tauri::command]
pub(crate) fn cancel_issue_triage(state: tauri::State<'_, ReviewRunRegistry>, run_id: String) {
    IssueCommandService::new(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &NoopProgressEmitter,
        &state,
    )
    .cancel(&run_id);
}

#[tauri::command]
pub(crate) async fn publish_issue_triage(
    input: IssueTriagePublishInputDto,
) -> Result<IssueTriagePublishResultDto, IpcError> {
    IssueCommandService::new(
        &KeyringCredentialReader,
        &ProductionBackendFactory,
        &NoopProgressEmitter,
        &ReviewRunRegistry::default(),
    )
    .publish(input)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    fn fixture_descriptor(
        tool_calling: review_agent::ToolCallingSupport,
    ) -> review_agent::ProviderDescriptor {
        review_agent::ProviderDescriptor {
            provider_id: "fixture".into(),
            model_id: "fixture-model".into(),
            capabilities: review_agent::ProviderCapabilities {
                structured_output: review_agent::StructuredOutputSupport::JsonObject,
                tool_calling,
                can_disable_tools: true,
                requires_reasoning_replay: false,
                context_window_tokens: 100_000,
                max_output_tokens: 8_192,
                usage: review_agent::UsageSupport::InputOutputTokens,
            },
        }
    }

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
        Arc<Mutex<VecDeque<Result<review_agent::ModelResponse, review_agent::ProviderError>>>>,
    );
    #[async_trait]
    impl review_agent::ModelProvider for FakeModel {
        fn descriptor(&self) -> review_agent::ProviderDescriptor {
            fixture_descriptor(review_agent::ToolCallingSupport::Serial)
        }

        async fn respond(
            &self,
            _: &review_agent::ModelRequest,
        ) -> Result<review_agent::ModelResponse, review_agent::ProviderError> {
            self.0.lock().unwrap().pop_front().unwrap_or_else(|| {
                Err(review_agent::ProviderError::InvalidResponse(
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
        fn descriptor(&self) -> review_agent::ProviderDescriptor {
            fixture_descriptor(review_agent::ToolCallingSupport::Serial)
        }

        async fn respond(
            &self,
            _: &review_agent::ModelRequest,
        ) -> Result<review_agent::ModelResponse, review_agent::ProviderError> {
            self.registry.cancel(&self.run_id);
            std::future::pending().await
        }
    }

    struct CancellingFactory {
        source: FakeSource,
        registry: Arc<ReviewRunRegistry>,
    }

    struct GateModel {
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }

    #[async_trait]
    impl review_agent::ModelProvider for GateModel {
        fn descriptor(&self) -> review_agent::ProviderDescriptor {
            fixture_descriptor(review_agent::ToolCallingSupport::Serial)
        }

        async fn respond(
            &self,
            _: &review_agent::ModelRequest,
        ) -> Result<review_agent::ModelResponse, review_agent::ProviderError> {
            self.entered.notify_one();
            self.release.notified().await;
            Ok(review_agent::ModelResponse::final_text(
                r#"{"summary":"Completed","findings":[]}"#,
                review_agent::ReviewUsage::default(),
            ))
        }
    }

    struct GateFactory {
        source: FakeSource,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }

    impl ReviewBackendFactory for GateFactory {
        fn source(
            &self,
            _: ReviewPlatform,
            _: String,
        ) -> Result<Box<dyn ReviewSource>, review_agent::ReviewError> {
            Ok(Box::new(self.source.clone()))
        }

        fn model(
            &self,
            _: &str,
            _: String,
        ) -> Result<Box<dyn review_agent::ModelProvider>, review_agent::ReviewError> {
            Ok(Box::new(GateModel {
                entered: self.entered.clone(),
                release: self.release.clone(),
            }))
        }
    }
    impl ReviewBackendFactory for CancellingFactory {
        fn source(
            &self,
            _: ReviewPlatform,
            _: String,
        ) -> Result<Box<dyn ReviewSource>, review_agent::ReviewError> {
            Ok(Box::new(self.source.clone()))
        }
        fn model(
            &self,
            _: &str,
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
            Arc<Mutex<VecDeque<Result<review_agent::ModelResponse, review_agent::ProviderError>>>>,
    }

    struct CountingFactory {
        source_calls: Arc<AtomicUsize>,
        model_calls: Arc<AtomicUsize>,
    }
    impl ReviewBackendFactory for CountingFactory {
        fn source(
            &self,
            _: ReviewPlatform,
            _: String,
        ) -> Result<Box<dyn ReviewSource>, review_agent::ReviewError> {
            self.source_calls.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(Box::new(fake_factory(vec![]).source))
        }
        fn model(
            &self,
            _: &str,
            _: String,
        ) -> Result<Box<dyn review_agent::ModelProvider>, review_agent::ReviewError> {
            self.model_calls.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(Box::new(FakeModel(Arc::new(Mutex::new(VecDeque::new())))))
        }
    }
    impl ReviewBackendFactory for FakeFactory {
        fn source(
            &self,
            _: ReviewPlatform,
            _: String,
        ) -> Result<Box<dyn ReviewSource>, review_agent::ReviewError> {
            Ok(Box::new(self.source.clone()))
        }
        fn model(
            &self,
            _: &str,
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

    #[derive(Default)]
    struct RecordingAgentSink(Mutex<Vec<review_agent::AgentEvent>>);
    impl review_agent::AgentEventSink for RecordingAgentSink {
        fn emit(&self, event: review_agent::AgentEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    fn fake_factory(
        responses: Vec<Result<review_agent::ModelResponse, review_agent::ProviderError>>,
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

    #[test]
    fn review_model_catalog_is_allowlisted_and_provider_qualified() {
        let models = list_review_models();
        assert_eq!(models.len(), 7);
        assert_eq!(models[0].id, "deepseek-v4-flash");
        assert_eq!(models[0].provider, "DeepSeek");
        assert_eq!(models[0].provider_id, "deepseek");
        assert!(models[0].capabilities.supports_tool_calling);
        assert!(models[0].capabilities.supports_structured_output);
        assert!(models[0].capabilities.reports_usage);
        assert_eq!(models[0].capabilities.context_window_tokens, 1_000_000);
        let pricing = models[0].pricing.as_ref().unwrap();
        assert_eq!(pricing.currency, "CNY");
        assert_eq!(pricing.checked_at, "2026-08-19");
        assert!(
            pricing
                .source_url
                .starts_with("https://api-docs.deepseek.com/")
        );
        assert_eq!(models[1].id, "deepseek-v4-pro");
        assert_eq!(models[2].id, "gpt-5.6-terra");
        assert_eq!(models[2].provider_id, "openai");
        assert_eq!(models[5].id, "claude-sonnet-5");
        assert_eq!(models[5].provider_id, "anthropic");
        assert_eq!(
            review_model_credential(&models[0].id).unwrap(),
            CredentialKindDto::Deepseek
        );
        assert_eq!(
            review_model_credential(&models[2].id).unwrap(),
            CredentialKindDto::Openai
        );
        assert_eq!(
            review_model_credential(&models[5].id).unwrap(),
            CredentialKindDto::Anthropic
        );
        let error = review_model_credential("user-controlled-model").unwrap_err();
        assert_eq!(error.code, "INVALID_REVIEW_MODEL");
    }
    fn run_input(run_id: &str) -> ReviewRunInputDto {
        ReviewRunInputDto {
            run_id: run_id.into(),
            target: target_dto(),
            expected_head_sha: "abc".into(),
            selected_files: vec!["src/lib.rs".into()],
            model_id: "deepseek-v4-flash".into(),
            output_language: ipc_types::ReviewLanguageDto::English,
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
                vec![review_agent::list_tree_call("tree", "src")],
                review_agent::ReviewUsage::default(),
            )),
            Ok(review_agent::ModelResponse::final_text(
                r#"{"summary":"No correctness issues found.","findings":[]}"#,
                review_agent::ReviewUsage::default(),
            )),
        ]);
        let emitter = RecordingEmitter::default();
        let agent_events = RecordingAgentSink::default();
        let registry = ReviewRunRegistry::default();
        let service = ReviewCommandService::new(
            &FakeCredentials,
            &factory,
            &emitter,
            &NoopTraceSink,
            &registry,
        )
        .with_agent_events(&agent_events);
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
        let events = agent_events.0.lock().unwrap();
        assert_eq!(events.len(), 8);
        assert!(events.iter().all(|event| event.run_id == "success"));
        assert_eq!(
            events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            (1..=8).collect::<Vec<_>>()
        );
        assert_eq!(
            events
                .iter()
                .map(|event| event.attempt_id)
                .collect::<Vec<_>>(),
            [1, 1, 1, 1, 2, 2, 2, 2]
        );
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

        let gitlab = MissingCredentials {
            missing: CredentialKindDto::Gitlab,
        };
        let service = ReviewCommandService::new_for_platform(
            &gitlab,
            &factory,
            &NoopProgressEmitter,
            &NoopTraceSink,
            &registry,
            ReviewPlatform::Gitlab,
        );
        assert_eq!(
            service.preflight(target_dto()).await.unwrap_err().code,
            "GITLAB_TOKEN_MISSING"
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
    async fn service_rejects_duplicate_and_cleans_up_failed_runs() {
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
        assert_eq!(
            failed
                .start(run_input("failed-diagnostic"))
                .await
                .unwrap_err()
                .diagnostic_id,
            review_agent::diagnostic_id("failed-diagnostic")
        );
        assert!(registry.register("failed").is_ok());
    }

    #[tokio::test]
    async fn service_rejects_a_second_run_for_the_same_pr_without_replacing_the_first() {
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let factory = GateFactory {
            source: fake_factory(vec![]).source,
            entered: entered.clone(),
            release: release.clone(),
        };
        let registry = ReviewRunRegistry::default();
        let service = ReviewCommandService::new(
            &FakeCredentials,
            &factory,
            &NoopProgressEmitter,
            &NoopTraceSink,
            &registry,
        );
        let entered_event = entered.notified();
        tokio::pin!(entered_event);
        let first = service.start(run_input("resource-first"));
        tokio::pin!(first);
        tokio::select! {
            result = &mut first => panic!("first run completed before the concurrency check: {result:?}"),
            _ = &mut entered_event => {}
        }

        let second = service
            .start(run_input("resource-second"))
            .await
            .unwrap_err();
        assert_eq!(second.code, "AGENT_RESOURCE_BUSY");
        assert_eq!(
            second.diagnostic_id,
            review_agent::diagnostic_id("resource-second")
        );

        release.notify_one();
        assert_eq!(first.await.unwrap().summary, "Completed");
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
        assert_eq!(
            error.diagnostic_id,
            review_agent::diagnostic_id("live-cancel")
        );
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

    #[tokio::test]
    async fn service_submit_maps_tampered_finding_enums_to_stable_error() {
        let factory = fake_factory(vec![]);
        let registry = ReviewRunRegistry::default();
        let service = ReviewCommandService::new(
            &FakeCredentials,
            &factory,
            &NoopProgressEmitter,
            &NoopTraceSink,
            &registry,
        );

        for (severity, side) in [("critical", "RIGHT"), ("high", "CENTER")] {
            let mut input = submit_input();
            input.findings.push(ipc_types::ReviewFindingDto {
                id: "tampered".into(),
                severity: severity.into(),
                path: "src/lib.rs".into(),
                side: side.into(),
                line: 1,
                title: "Tampered finding".into(),
                failure_scenario: "Invalid client input".into(),
                explanation: "The enum value is outside the public contract.".into(),
                draft_comment: "Do not publish this.".into(),
            });

            assert_eq!(
                service.submit(input).await.unwrap_err().code,
                "INVALID_MODEL_OUTPUT"
            );
        }
        assert_eq!(factory.source.published.load(AtomicOrdering::SeqCst), 0);
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
    fn registry_serializes_each_resource_without_blocking_other_resources() {
        let registry = ReviewRunRegistry::default();
        registry
            .register_resource("pr-first", "pr:acme/rocket#17")
            .unwrap();
        assert_eq!(
            registry
                .register_resource("pr-second", "pr:acme/rocket#17")
                .unwrap_err()
                .code,
            "AGENT_RESOURCE_BUSY"
        );
        assert!(
            registry
                .register_resource("other-pr", "pr:acme/rocket#18")
                .is_ok()
        );
        assert!(
            registry
                .register_resource("same-number-issue", "issue:acme/rocket#17")
                .is_ok()
        );

        registry.finish("pr-first");
        assert!(
            registry
                .register_resource("pr-after-finish", "pr:acme/rocket#17")
                .is_ok()
        );
    }

    #[test]
    fn resource_keys_are_case_insensitive_and_workflow_scoped() {
        assert_eq!(
            review_resource_key(&ReviewTargetDto {
                owner: " Acme ".into(),
                repo: "ROCKET".into(),
                pull_number: 17,
            }),
            "github-pr:acme/rocket#17"
        );
        assert_eq!(
            review_resource_key_for(
                ReviewPlatform::Gitlab,
                &ReviewTargetDto {
                    owner: " Acme ".into(),
                    repo: "ROCKET".into(),
                    pull_number: 17,
                }
            ),
            "gitlab-mr:acme/rocket#17"
        );
        assert_eq!(
            issue_resource_key(&IssueTargetDto {
                owner: "ACME".into(),
                repo: "Rocket".into(),
                issue_number: 17,
            }),
            "issue:acme/rocket#17"
        );
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
            .register_at("expired", "run:expired", now + PENDING_CANCELLATION_TTL)
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
            map_review_credential_error(CredentialKindDto::Deepseek, missing.clone()).code,
            "AI_KEY_MISSING"
        );
        assert_eq!(
            map_review_credential_error(CredentialKindDto::Openai, missing.clone()).code,
            "OPENAI_KEY_MISSING"
        );
        assert_eq!(
            map_review_credential_error(CredentialKindDto::Anthropic, missing.clone()).code,
            "ANTHROPIC_KEY_MISSING"
        );
        assert_eq!(
            map_review_credential_error(CredentialKindDto::Gitlab, missing).code,
            "GITLAB_TOKEN_MISSING"
        );
    }

    #[derive(Clone)]
    struct FakeIssueSource {
        context: review_agent::IssueContext,
    }

    #[async_trait]
    impl IssueSource for FakeIssueSource {
        async fn list_open_issues(
            &self,
            _: &review_agent::IssueRepositoryTarget,
        ) -> Result<Vec<review_agent::IssueSummary>, review_agent::ReviewError> {
            Ok(vec![self.context.issue.clone()])
        }

        async fn issue_context(
            &self,
            _: &review_agent::IssueTarget,
        ) -> Result<review_agent::IssueContext, review_agent::ReviewError> {
            Ok(self.context.clone())
        }
    }

    #[async_trait]
    impl IssuePublicationSource for FakeIssueSource {
        async fn current_snapshot(
            &self,
            _: &review_agent::IssueTarget,
        ) -> Result<review_agent::IssueSnapshot, review_agent::ReviewError> {
            Ok(self.context.snapshot.clone())
        }

        async fn add_label(
            &self,
            _: &review_agent::IssueTarget,
            _: &str,
        ) -> Result<(), review_agent::ReviewError> {
            Ok(())
        }

        async fn ensure_comment(
            &self,
            _: &review_agent::IssueTarget,
            _: &str,
            _: &str,
        ) -> Result<review_agent::IssueMutationOutcome, review_agent::ReviewError> {
            Ok(review_agent::IssueMutationOutcome::Applied)
        }
    }

    struct FakeIssueModel;

    #[async_trait]
    impl review_agent::ModelProvider for FakeIssueModel {
        fn descriptor(&self) -> review_agent::ProviderDescriptor {
            fixture_descriptor(review_agent::ToolCallingSupport::None)
        }

        async fn respond(
            &self,
            request: &review_agent::ModelRequest,
        ) -> Result<review_agent::ModelResponse, review_agent::ProviderError> {
            assert!(request.tools.is_empty());
            Ok(review_agent::ModelResponse::final_text(
                serde_json::to_string(&review_agent::IssueTriageProposal {
                    summary: "Reproducible crash".into(),
                    category: "bug".into(),
                    priority: "high".into(),
                    confidence: 0.9,
                    suggested_labels: vec!["bug".into()],
                    suspected_duplicate_numbers: vec![3],
                    suggested_reply: "Please share the app version.".into(),
                    rationale: vec!["Steps are present.".into()],
                })
                .unwrap(),
                review_agent::ReviewUsage {
                    input_tokens: 20,
                    cached_input_tokens: 0,
                    output_tokens: 10,
                    tool_calls: 0,
                },
            ))
        }
    }

    struct FakeIssueFactory {
        source: FakeIssueSource,
    }

    impl IssueBackendFactory for FakeIssueFactory {
        fn issue_source(
            &self,
            _: String,
        ) -> Result<Box<dyn IssueSource>, review_agent::ReviewError> {
            Ok(Box::new(self.source.clone()))
        }

        fn issue_model(
            &self,
            _: &str,
            _: String,
        ) -> Result<Box<dyn review_agent::ModelProvider>, review_agent::ReviewError> {
            Ok(Box::new(FakeIssueModel))
        }

        fn issue_publisher(
            &self,
            _: String,
        ) -> Result<Box<dyn IssuePublicationSource>, review_agent::ReviewError> {
            Ok(Box::new(self.source.clone()))
        }
    }

    fn fake_issue_factory() -> FakeIssueFactory {
        let issue = review_agent::IssueSummary {
            number: 7,
            title: "App crashes".into(),
            url: "https://example.invalid/issues/7".into(),
            author: Some("lin".into()),
            updated_at: "2026-08-07T08:00:00Z".into(),
            comments: 1,
            labels: vec![review_agent::IssueLabel {
                name: "bug".into(),
                color: "d73a4a".into(),
            }],
        };
        FakeIssueFactory {
            source: FakeIssueSource {
                context: review_agent::IssueContext {
                    issue: issue.clone(),
                    body: "Steps to reproduce".into(),
                    comments: vec![],
                    comments_truncated: false,
                    available_labels: issue.labels.clone(),
                    similar_issues: vec![review_agent::IssueSummary {
                        number: 3,
                        title: "Similar crash".into(),
                        ..issue.clone()
                    }],
                    snapshot: review_agent::IssueSnapshot {
                        updated_at: issue.updated_at.clone(),
                        comments: issue.comments,
                    },
                },
            },
        }
    }

    #[tokio::test]
    async fn issue_service_lists_loads_and_triages_with_ordered_progress() {
        let factory = fake_issue_factory();
        let emitter = RecordingEmitter::default();
        let registry = ReviewRunRegistry::default();
        let service = IssueCommandService::new(&FakeCredentials, &factory, &emitter, &registry);
        let repository = IssueRepositoryTargetDto {
            owner: "acme".into(),
            repo: "rocket".into(),
        };
        assert_eq!(service.list(repository).await.unwrap()[0].number, 7);

        let target = IssueTargetDto {
            owner: "acme".into(),
            repo: "rocket".into(),
            issue_number: 7,
        };
        let context = service.context(target.clone()).await.unwrap();
        assert_eq!(context.body, "Steps to reproduce");
        let result = service
            .start(IssueTriageInputDto {
                run_id: "issue-success".into(),
                target,
                expected_updated_at: context.snapshot.updated_at,
                expected_comments: context.snapshot.comments,
                model_id: "deepseek-v4-flash".into(),
                output_language: ipc_types::ReviewLanguageDto::English,
            })
            .await
            .unwrap();
        assert_eq!(result.proposal.category, "bug");
        assert_eq!(result.proposal.suspected_duplicate_numbers, [3]);
        assert_eq!(
            emitter
                .0
                .lock()
                .unwrap()
                .iter()
                .map(|event| event.stage.as_str())
                .collect::<Vec<_>>(),
            ["loading_issue", "analyzing_issue", "completed"]
        );
        assert!(registry.register("issue-success").is_ok());
    }

    #[tokio::test]
    async fn issue_service_publishes_only_an_explicit_confirmed_batch() {
        let factory = fake_issue_factory();
        let registry = ReviewRunRegistry::default();
        let service =
            IssueCommandService::new(&FakeCredentials, &factory, &NoopProgressEmitter, &registry);
        let result = service
            .publish(IssueTriagePublishInputDto {
                publish_id: "batch-1".into(),
                confirmed: true,
                target: IssueTargetDto {
                    owner: "acme".into(),
                    repo: "rocket".into(),
                    issue_number: 7,
                },
                expected_snapshot: factory.source.context.snapshot.clone().into(),
                labels: vec![],
                reply: Some("Thanks for the report.".into()),
            })
            .await
            .unwrap();

        assert_eq!(result.publish_id, "batch-1");
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.actions[0].kind, "comment");
        assert_eq!(result.actions[0].status, "applied");
    }

    #[test]
    fn command_service_is_constructed_from_injected_dependencies() {
        fn assert_service_type(_: &ReviewCommandService<'_>) {}
        fn assert_issue_service_type(_: &IssueCommandService<'_>) {}
        let _ = assert_service_type;
        let _ = assert_issue_service_type;
    }
}
