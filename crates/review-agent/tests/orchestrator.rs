use async_trait::async_trait;
use review_agent::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

fn final_review(
    summary: impl Into<String>,
    findings: Vec<ReviewFinding>,
    usage: ReviewUsage,
) -> ModelResponse {
    ModelResponse::final_text(
        serde_json::json!({"summary": summary.into(), "findings": findings}).to_string(),
        usage,
    )
}

fn final_findings(findings: Vec<ReviewFinding>, usage: ReviewUsage) -> ModelResponse {
    let summary = if findings.is_empty() {
        "No actionable issues found.".to_owned()
    } else {
        format!("Review found {} actionable issue(s).", findings.len())
    };
    final_review(summary, findings, usage)
}

fn fixture_descriptor() -> ProviderDescriptor {
    ProviderDescriptor {
        provider_id: "fixture".into(),
        model_id: "fixture-review-v1".into(),
        capabilities: ProviderCapabilities {
            structured_output: StructuredOutputSupport::JsonObject,
            tool_calling: ToolCallingSupport::Serial,
            can_disable_tools: true,
            requires_reasoning_replay: false,
            context_window_tokens: 100_000,
            max_output_tokens: 8_192,
            usage: UsageSupport::InputOutputTokens,
        },
    }
}

struct FakeModel(Mutex<VecDeque<ModelResponse>>);

#[async_trait]
impl ModelProvider for FakeModel {
    fn descriptor(&self) -> ProviderDescriptor {
        fixture_descriptor()
    }

    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, ProviderError> {
        assert!(request
            .transcript
            .first()
            .is_some_and(|m| matches!(m, TranscriptItem::System(s) if s.contains("untrusted") && s.contains("in English"))));
        if request.tools.is_empty() {
            assert!(request.transcript.last().is_some_and(
                |item| matches!(item, TranscriptItem::System(text) if text.contains("tool budget is exhausted"))
            ));
        } else {
            assert_eq!(request.tools.len(), 2);
        }
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::InvalidResponse("empty fake".into()))
    }
}

#[derive(Clone)]
struct FakeSource {
    head: Arc<Mutex<String>>,
    files: Vec<ReviewFile>,
    tool_io: Arc<AtomicUsize>,
    race_on_pull: bool,
    read_output_bytes: usize,
}

#[async_trait]
impl ReviewSource for FakeSource {
    async fn head_sha(&self, _: &ReviewTarget) -> Result<String, ReviewError> {
        Ok(self.head.lock().unwrap().clone())
    }
    async fn pull_files_at_head(
        &self,
        _: &ReviewTarget,
        expected_head_sha: &str,
    ) -> Result<Vec<ReviewFile>, ReviewError> {
        if self.race_on_pull {
            *self.head.lock().unwrap() = "def".into();
            return Err(ReviewError::PrUpdated);
        }
        if self.head.lock().unwrap().as_str() != expected_head_sha {
            return Err(ReviewError::PrUpdated);
        }
        Ok(self.files.clone())
    }
    async fn list_repository_tree(
        &self,
        _: &ReviewTarget,
        sha: &str,
        _: Option<&str>,
    ) -> Result<Vec<String>, ReviewError> {
        self.tool_io.fetch_add(1, Ordering::SeqCst);
        assert_eq!(sha, "abc");
        Ok(vec!["src/lib.rs".into()])
    }
    async fn read_file(
        &self,
        _: &ReviewTarget,
        sha: &str,
        path: &str,
        start: u32,
        end: u32,
    ) -> Result<String, ReviewError> {
        self.tool_io.fetch_add(1, Ordering::SeqCst);
        assert_eq!(sha, "abc");
        assert_eq!(path, "src/lib.rs");
        assert!(start > 0 && end >= start);
        if self.read_output_bytes == 0 {
            Ok("one\ntwo".into())
        } else {
            Ok("x".repeat(self.read_output_bytes))
        }
    }
    async fn publish(&self, _: &SubmitReview) -> Result<PublishedReview, ReviewError> {
        unreachable!()
    }
}

struct NeverCancel;
impl CancelSignal for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}
struct AlwaysCancel;
impl CancelSignal for AlwaysCancel {
    fn is_cancelled(&self) -> bool {
        true
    }
}

struct ControllableCancel {
    cancelled: std::sync::atomic::AtomicBool,
    notify: Notify,
}

struct PollingOnlyCancel(std::sync::atomic::AtomicBool);

impl PollingOnlyCancel {
    fn new() -> Self {
        Self(std::sync::atomic::AtomicBool::new(false))
    }

    fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl CancelSignal for PollingOnlyCancel {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl ControllableCancel {
    fn new() -> Self {
        Self {
            cancelled: std::sync::atomic::AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
}

#[async_trait]
impl CancelSignal for ControllableCancel {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    async fn cancelled(&self) {
        if !self.is_cancelled() {
            self.notify.notified().await;
        }
    }
}
struct NoTrace;
#[async_trait]
impl TraceSink for NoTrace {
    async fn record(&self, _: TraceEntry) -> Result<(), ReviewError> {
        Ok(())
    }
}

#[derive(Default)]
struct RecordingTrace(Mutex<Vec<TraceEntry>>);

#[async_trait]
impl TraceSink for RecordingTrace {
    async fn record(&self, entry: TraceEntry) -> Result<(), ReviewError> {
        self.0.lock().unwrap().push(entry);
        Ok(())
    }
}

#[derive(Default)]
struct RecordingProgress(Mutex<Vec<ProgressUpdate>>);

impl ProgressSink for RecordingProgress {
    fn report(&self, update: ProgressUpdate) {
        self.0.lock().unwrap().push(update);
    }
}

#[tokio::test]
async fn reports_real_tool_calls_with_running_counter() {
    let model = FakeModel(Mutex::new(VecDeque::from([
        ModelResponse::tool_calls(vec![list_tree_call("tree", "src")], ReviewUsage::default()),
        final_findings(vec![], ReviewUsage::default()),
    ])));
    let progress = RecordingProgress::default();
    ReviewOrchestrator::new_with_progress(&model, &source(), &NoTrace, &NeverCancel, &progress)
        .run(input())
        .await
        .unwrap();
    assert_eq!(
        progress.0.lock().unwrap().as_slice(),
        &[ProgressUpdate::ToolCall {
            name: "list_repository_tree".into(),
            count: 1
        }]
    );
}

fn target() -> ReviewTarget {
    ReviewTarget {
        owner: "o".into(),
        repo: "r".into(),
        pull_number: 1,
    }
}
fn input() -> ReviewRunInput {
    ReviewRunInput {
        run_id: "run".into(),
        target: target(),
        expected_head_sha: "abc".into(),
        selected_files: vec!["src/lib.rs".into()],
        output_language: ReviewLanguage::English,
    }
}
fn source() -> FakeSource {
    FakeSource {
        head: Arc::new(Mutex::new("abc".into())),
        files: vec![
            ReviewFile::from_patch("src/lib.rs", "@@ -1,2 +1,2 @@\n-old\n+new\n same\n").unwrap(),
        ],
        tool_io: Arc::new(AtomicUsize::new(0)),
        race_on_pull: false,
        read_output_bytes: 0,
    }
}
fn finding(id: &str, line: u32) -> ReviewFinding {
    ReviewFinding {
        id: id.into(),
        severity: Severity::High,
        path: "src/lib.rs".into(),
        side: ReviewSide::RIGHT,
        line,
        title: "bug".into(),
        failure_scenario: "when called".into(),
        explanation: "fails".into(),
        draft_comment: "fix it".into(),
    }
}

#[tokio::test]
async fn performs_stateless_multi_turn_tool_loop() {
    let model = FakeModel(Mutex::new(VecDeque::from([
        ModelResponse::tool_calls(vec![list_tree_call("c1", "src")], ReviewUsage::default()),
        ModelResponse::tool_calls(
            vec![read_file_call("c2", "src/lib.rs", 1, 2)],
            ReviewUsage::default(),
        ),
        final_review(
            "One correctness issue.",
            vec![finding("f1", 1)],
            ReviewUsage {
                input_tokens: 3,
                cached_input_tokens: 0,
                output_tokens: 2,
                tool_calls: 0,
            },
        ),
    ])));
    let result = ReviewOrchestrator::new(&model, &source(), &NoTrace, &NeverCancel)
        .run(input())
        .await
        .unwrap();
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.summary, "One correctness issue.");
    assert_eq!(result.reviewed_files, vec!["src/lib.rs"]);
    assert_eq!(result.usage.tool_calls, 2);
}

#[tokio::test]
async fn rejects_cancelled_run_before_model_call() {
    let model = FakeModel(Mutex::new(VecDeque::new()));
    let err = ReviewOrchestrator::new(&model, &source(), &NoTrace, &AlwaysCancel)
        .run(input())
        .await
        .unwrap_err();
    assert_eq!(err, ReviewError::Cancelled);
}

struct PendingModel {
    started: Arc<Notify>,
}

#[async_trait]
impl ModelProvider for PendingModel {
    fn descriptor(&self) -> ProviderDescriptor {
        fixture_descriptor()
    }

    async fn respond(&self, _: &ModelRequest) -> Result<ModelResponse, ProviderError> {
        self.started.notify_one();
        std::future::pending().await
    }
}

#[tokio::test]
async fn cancellation_interrupts_in_flight_model_response() {
    let started = Arc::new(Notify::new());
    let model = PendingModel {
        started: started.clone(),
    };
    let cancel = ControllableCancel::new();
    let source = source();
    let orchestrator = ReviewOrchestrator::new(&model, &source, &NoTrace, &cancel);
    let cancel_when_started = async {
        started.notified().await;
        cancel.cancel();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_millis(250), async {
        tokio::join!(orchestrator.run(input()), cancel_when_started)
    })
    .await
    .expect("cancellation should interrupt pending model IO");
    assert_eq!(result.unwrap_err(), ReviewError::Cancelled);
}

#[tokio::test]
async fn default_cancel_signal_interrupts_after_request_starts() {
    let started = Arc::new(Notify::new());
    let model = PendingModel {
        started: started.clone(),
    };
    let cancel = PollingOnlyCancel::new();
    let source = source();
    let orchestrator = ReviewOrchestrator::new(&model, &source, &NoTrace, &cancel);
    let cancel_when_started = async {
        started.notified().await;
        cancel.cancel();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_millis(250), async {
        tokio::join!(orchestrator.run(input()), cancel_when_started)
    })
    .await
    .expect("default cancellation should interrupt pending model IO");
    assert_eq!(result.unwrap_err(), ReviewError::Cancelled);
}

struct PendingHeadSource {
    started: Arc<Notify>,
}

#[async_trait]
impl ReviewSource for PendingHeadSource {
    async fn head_sha(&self, _: &ReviewTarget) -> Result<String, ReviewError> {
        self.started.notify_one();
        std::future::pending().await
    }
    async fn pull_files_at_head(
        &self,
        _: &ReviewTarget,
        _: &str,
    ) -> Result<Vec<ReviewFile>, ReviewError> {
        unreachable!()
    }
    async fn list_repository_tree(
        &self,
        _: &ReviewTarget,
        _: &str,
        _: Option<&str>,
    ) -> Result<Vec<String>, ReviewError> {
        unreachable!()
    }
    async fn read_file(
        &self,
        _: &ReviewTarget,
        _: &str,
        _: &str,
        _: u32,
        _: u32,
    ) -> Result<String, ReviewError> {
        unreachable!()
    }
    async fn publish(&self, _: &SubmitReview) -> Result<PublishedReview, ReviewError> {
        unreachable!()
    }
}

#[tokio::test]
async fn cancellation_interrupts_in_flight_source_request() {
    let started = Arc::new(Notify::new());
    let source = PendingHeadSource {
        started: started.clone(),
    };
    let model = FakeModel(Mutex::new(VecDeque::new()));
    let cancel = ControllableCancel::new();
    let orchestrator = ReviewOrchestrator::new(&model, &source, &NoTrace, &cancel);
    let cancel_when_started = async {
        started.notified().await;
        cancel.cancel();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_millis(250), async {
        tokio::join!(orchestrator.run(input()), cancel_when_started)
    })
    .await
    .expect("cancellation should interrupt pending source IO");
    assert_eq!(result.unwrap_err(), ReviewError::Cancelled);
}

#[tokio::test]
async fn returns_tool_error_to_model_for_unknown_tool_then_finishes_review() {
    let model = FakeModel(Mutex::new(VecDeque::from([
        ModelResponse::tool_calls(
            vec![ToolCall {
                call_id: "unknown".into(),
                name: "shell".into(),
                arguments: serde_json::json!({}),
            }],
            ReviewUsage::default(),
        ),
        final_findings(vec![], ReviewUsage::default()),
    ])));
    let source = source();
    let result = ReviewOrchestrator::new(&model, &source, &NoTrace, &NeverCancel)
        .run(input())
        .await
        .unwrap();
    assert!(result.findings.is_empty());
    assert_eq!(source.tool_io.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn rejects_oversized_read() {
    let model = FakeModel(Mutex::new(VecDeque::from([ModelResponse::tool_calls(
        vec![read_file_call("oversized", "src/lib.rs", 1, 402)],
        ReviewUsage::default(),
    )])));
    let err = ReviewOrchestrator::new(&model, &source(), &NoTrace, &NeverCancel)
        .run(input())
        .await
        .unwrap_err();
    assert_eq!(err, ReviewError::ReviewBudgetExceeded);
}

#[tokio::test]
async fn rejects_malformed_or_extra_tool_arguments_without_source_io() {
    let calls = [
        ToolCall {
            call_id: "bad1".into(),
            name: "list_repository_tree".into(),
            arguments: serde_json::json!({"prefix": 42}),
        },
        ToolCall {
            call_id: "bad2".into(),
            name: "list_repository_tree".into(),
            arguments: serde_json::json!({"prefix": "src", "recursive": true}),
        },
        ToolCall {
            call_id: "bad3".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "src/lib.rs", "start_line": 1, "end_line": 2, "bytes": true}),
        },
        ToolCall {
            call_id: "bad4".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": 7, "start_line": "1", "end_line": 2}),
        },
    ];
    for call in calls {
        let source = source();
        let model = FakeModel(Mutex::new(VecDeque::from([ModelResponse::tool_calls(
            vec![call],
            ReviewUsage::default(),
        )])));
        let error = ReviewOrchestrator::new(&model, &source, &NoTrace, &NeverCancel)
            .run(input())
            .await
            .unwrap_err();
        assert!(matches!(error, ReviewError::InvalidModelOutput(_)));
        assert_eq!(source.tool_io.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn rejects_missing_empty_or_duplicate_call_ids_before_source_io() {
    let responses = [
        vec![ToolCall {
            call_id: String::new(),
            name: "list_repository_tree".into(),
            arguments: serde_json::json!({}),
        }],
        vec![ToolCall {
            call_id: String::new(),
            name: "list_repository_tree".into(),
            arguments: serde_json::json!({}),
        }],
        vec![
            ToolCall {
                call_id: "same".into(),
                name: "list_repository_tree".into(),
                arguments: serde_json::json!({}),
            },
            ToolCall {
                call_id: "same".into(),
                name: "list_repository_tree".into(),
                arguments: serde_json::json!({}),
            },
        ],
    ];
    for calls in responses {
        let source = source();
        let model = FakeModel(Mutex::new(VecDeque::from([ModelResponse::tool_calls(
            calls,
            ReviewUsage::default(),
        )])));
        let error = ReviewOrchestrator::new(&model, &source, &NoTrace, &NeverCancel)
            .run(input())
            .await
            .unwrap_err();
        assert!(matches!(error, ReviewError::InvalidModelOutput(_)));
        assert_eq!(source.tool_io.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn rejects_call_id_reused_in_later_round_before_duplicate_source_io() {
    let model = FakeModel(Mutex::new(VecDeque::from([
        ModelResponse::tool_calls(vec![list_tree_call("c1", "src")], ReviewUsage::default()),
        ModelResponse::tool_calls(vec![list_tree_call("c1", "src")], ReviewUsage::default()),
    ])));
    let source = source();
    let error = ReviewOrchestrator::new(&model, &source, &NoTrace, &NeverCancel)
        .run(input())
        .await
        .unwrap_err();
    assert!(matches!(error, ReviewError::InvalidModelOutput(_)));
    assert_eq!(source.tool_io.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn deduplicates_and_drops_unmapped_findings_then_sorts() {
    let mut medium = finding("same", 1);
    medium.severity = Severity::Medium;
    let mut high = finding("same", 1);
    high.severity = Severity::High;
    let mut low = finding("low", 2);
    low.severity = Severity::Low;
    let invalid = finding("bad", 99);
    let model = FakeModel(Mutex::new(VecDeque::from([final_findings(
        vec![low, medium, invalid, high],
        ReviewUsage::default(),
    )])));
    let result = ReviewOrchestrator::new(&model, &source(), &NoTrace, &NeverCancel)
        .run(input())
        .await
        .unwrap();
    assert_eq!(result.findings.len(), 2);
    assert_eq!(result.findings[0].severity, Severity::High);
}

#[tokio::test]
async fn deduplicates_semantically_identical_findings_with_different_model_ids() {
    let first = finding("model-a", 1);
    let second = finding("model-b", 1);
    let model = FakeModel(Mutex::new(VecDeque::from([final_findings(
        vec![first, second],
        ReviewUsage::default(),
    )])));
    let result = ReviewOrchestrator::new(&model, &source(), &NoTrace, &NeverCancel)
        .run(input())
        .await
        .unwrap();
    assert_eq!(result.findings.len(), 1);
}

#[tokio::test]
async fn distinct_findings_with_same_model_id_survive_with_stable_ids() {
    let first = finding("duplicate-id", 1);
    let mut second = finding("duplicate-id", 2);
    second.title = "different bug".into();
    second.draft_comment = "different fix".into();

    async fn run(findings: Vec<ReviewFinding>) -> Vec<String> {
        let model = FakeModel(Mutex::new(VecDeque::from([final_findings(
            findings,
            ReviewUsage::default(),
        )])));
        ReviewOrchestrator::new(&model, &source(), &NoTrace, &NeverCancel)
            .run(input())
            .await
            .unwrap()
            .findings
            .into_iter()
            .map(|finding| finding.id)
            .collect()
    }

    let ids = run(vec![first.clone(), second.clone()]).await;
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    assert_eq!(ids, run(vec![first, second]).await);
}

#[tokio::test]
async fn rejects_changed_head_and_selection_budget() {
    let model = FakeModel(Mutex::new(VecDeque::new()));
    let changed = FakeSource {
        head: Arc::new(Mutex::new("def".into())),
        ..source()
    };
    assert_eq!(
        ReviewOrchestrator::new(&model, &changed, &NoTrace, &NeverCancel)
            .run(input())
            .await
            .unwrap_err(),
        ReviewError::PrUpdated
    );
    let mut too_many = input();
    too_many.selected_files = (0..31).map(|n| format!("src/{n}.rs")).collect();
    assert_eq!(
        ReviewOrchestrator::new(&model, &source(), &NoTrace, &NeverCancel)
            .run(too_many)
            .await
            .unwrap_err(),
        ReviewError::ReviewBudgetExceeded
    );
}

#[tokio::test]
async fn repeated_identical_calls_use_one_source_read_until_round_ceiling() {
    let responses = (0..MAX_TOOL_CALLS + 2)
        .map(|round| {
            ModelResponse::tool_calls(
                vec![list_tree_call(format!("round-{round}"), "src")],
                ReviewUsage::default(),
            )
        })
        .collect();
    let model = FakeModel(Mutex::new(responses));
    let source = source();
    let error = ReviewOrchestrator::new(&model, &source, &NoTrace, &NeverCancel)
        .run(input())
        .await
        .unwrap_err();
    assert_eq!(error, ReviewError::ReviewBudgetExceeded);
    assert_eq!(source.tool_io.load(Ordering::SeqCst), 1);
    assert_eq!(model.0.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn stops_before_executing_a_call_beyond_the_tool_ceiling() {
    let calls_at_ceiling = (0..MAX_TOOL_CALLS)
        .map(|call| list_tree_call(format!("call-{call}"), format!("src/{call}")))
        .collect();
    let model = FakeModel(Mutex::new(VecDeque::from([
        ModelResponse::tool_calls(calls_at_ceiling, ReviewUsage::default()),
        ModelResponse::tool_calls(
            vec![list_tree_call("call-over-ceiling", "src/over-ceiling")],
            ReviewUsage::default(),
        ),
        final_findings(vec![], ReviewUsage::default()),
    ])));
    let source = source();
    let result = ReviewOrchestrator::new(&model, &source, &NoTrace, &NeverCancel)
        .run(input())
        .await
        .unwrap();
    assert!(result.findings.is_empty());
    assert_eq!(source.tool_io.load(Ordering::SeqCst), MAX_TOOL_CALLS);
    assert_eq!(model.0.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn stops_when_cumulative_tool_output_exceeds_three_hundred_kilobytes() {
    let model = FakeModel(Mutex::new(VecDeque::from([
        ModelResponse::tool_calls(
            vec![
                read_file_call("read-1", "src/lib.rs", 1, 2),
                read_file_call("read-2", "src/lib.rs", 2, 3),
            ],
            ReviewUsage::default(),
        ),
        ModelResponse::tool_calls(
            vec![read_file_call("read-3", "src/lib.rs", 3, 4)],
            ReviewUsage::default(),
        ),
        final_findings(vec![], ReviewUsage::default()),
    ])));
    let source = FakeSource {
        read_output_bytes: 150_001,
        ..source()
    };
    let error = ReviewOrchestrator::new(&model, &source, &NoTrace, &NeverCancel)
        .run(input())
        .await
        .unwrap_err();
    assert_eq!(error, ReviewError::ReviewBudgetExceeded);
    assert_eq!(source.tool_io.load(Ordering::SeqCst), 2);
    assert_eq!(model.0.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn patch_fetch_head_race_returns_pr_updated_before_model_analysis() {
    let model = FakeModel(Mutex::new(VecDeque::from([final_findings(
        vec![],
        ReviewUsage::default(),
    )])));
    let racing_source = FakeSource {
        race_on_pull: true,
        ..source()
    };
    let error = ReviewOrchestrator::new(&model, &racing_source, &NoTrace, &NeverCancel)
        .run(input())
        .await
        .unwrap_err();
    assert_eq!(error, ReviewError::PrUpdated);
    assert_eq!(model.0.lock().unwrap().len(), 1);
}

struct DelayedFinalModel;

#[async_trait]
impl ModelProvider for DelayedFinalModel {
    fn descriptor(&self) -> ProviderDescriptor {
        fixture_descriptor()
    }

    async fn respond(&self, _: &ModelRequest) -> Result<ModelResponse, ProviderError> {
        tokio::time::sleep(Duration::from_millis(15)).await;
        Ok(final_findings(
            vec![],
            ReviewUsage {
                input_tokens: 4,
                cached_input_tokens: 0,
                output_tokens: 2,
                tool_calls: 0,
            },
        ))
    }
}

#[tokio::test]
async fn trace_records_measured_success_once() {
    let trace = RecordingTrace::default();
    let result = ReviewOrchestrator::new(&DelayedFinalModel, &source(), &trace, &NeverCancel)
        .run(input())
        .await
        .unwrap();
    assert!(result.duration_ms >= 10);
    assert!(result.diagnostic_id.starts_with("diag-"));
    assert_eq!(result.provider_attempts, 1);
    let entries = trace.0.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].status, "completed");
    assert!(entries[0].duration_ms >= 10);
    assert_eq!(entries[0].diagnostic_id, result.diagnostic_id);
    assert_eq!(entries[0].provider_attempts, 1);
    assert_eq!(entries[0].input_tokens, 4);
    assert_eq!(entries[0].error_code, None);
}

#[tokio::test]
async fn trace_records_cancelled_and_error_exits_once_with_stable_codes() {
    let cancelled_trace = RecordingTrace::default();
    let empty_model = FakeModel(Mutex::new(VecDeque::new()));
    let cancelled =
        ReviewOrchestrator::new(&empty_model, &source(), &cancelled_trace, &AlwaysCancel)
            .run(input())
            .await
            .unwrap_err();
    assert_eq!(cancelled, ReviewError::Cancelled);
    {
        let cancelled_entries = cancelled_trace.0.lock().unwrap();
        assert_eq!(cancelled_entries.len(), 1);
        assert_eq!(cancelled_entries[0].status, "cancelled");
        assert_eq!(
            cancelled_entries[0].error_code.as_deref(),
            Some("CANCELLED")
        );
    }

    let error_trace = RecordingTrace::default();
    let bad_model = FakeModel(Mutex::new(VecDeque::from([
        ModelResponse::tool_calls(
            vec![ToolCall {
                call_id: "trace-error".into(),
                name: "shell".into(),
                arguments: serde_json::json!({}),
            }],
            ReviewUsage::default(),
        ),
        final_findings(vec![], ReviewUsage::default()),
    ])));
    let result = ReviewOrchestrator::new(&bad_model, &source(), &error_trace, &NeverCancel)
        .run(input())
        .await
        .unwrap();
    assert!(result.findings.is_empty());
    let error_entries = error_trace.0.lock().unwrap();
    assert_eq!(error_entries.len(), 1);
    assert_eq!(error_entries[0].status, "completed");
    assert_eq!(error_entries[0].model, "fixture-review-v1");
    assert_eq!(error_entries[0].error_code, None);
}
