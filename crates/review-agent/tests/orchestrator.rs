use async_trait::async_trait;
use review_agent::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

struct FakeModel(Mutex<VecDeque<ModelResponse>>);

#[async_trait]
impl ModelProvider for FakeModel {
    async fn respond(&self, transcript: &[TranscriptItem]) -> Result<ModelResponse, ReviewError> {
        assert!(transcript
            .first()
            .is_some_and(|m| matches!(m, TranscriptItem::System(s) if s.contains("untrusted"))));
        self.0
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ReviewError::InvalidModelOutput("empty fake".into()))
    }
}

#[derive(Clone)]
struct FakeSource {
    head: Arc<Mutex<String>>,
    files: Vec<ReviewFile>,
    tool_io: Arc<AtomicUsize>,
    race_on_pull: bool,
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
        assert_eq!((start, end), (1, 2));
        Ok("one\ntwo".into())
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
        ModelResponse::tool_calls(vec![ToolCall::list_tree("src")], ReviewUsage::default()),
        ModelResponse::tool_calls(
            vec![ToolCall::read_file("src/lib.rs", 1, 2)],
            ReviewUsage::default(),
        ),
        ModelResponse::final_findings(
            vec![finding("f1", 1)],
            ReviewUsage {
                input_tokens: 3,
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
    async fn respond(&self, _: &[TranscriptItem]) -> Result<ModelResponse, ReviewError> {
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
async fn rejects_unknown_tool_and_oversized_read() {
    for call in [
        ToolCall {
            name: "shell".into(),
            arguments: serde_json::json!({}),
        },
        ToolCall::read_file("src/lib.rs", 1, 402),
    ] {
        let model = FakeModel(Mutex::new(VecDeque::from([ModelResponse::tool_calls(
            vec![call],
            ReviewUsage::default(),
        )])));
        let err = ReviewOrchestrator::new(&model, &source(), &NoTrace, &NeverCancel)
            .run(input())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            ReviewError::InvalidModelOutput(_) | ReviewError::ReviewBudgetExceeded
        ));
    }
}

#[tokio::test]
async fn rejects_malformed_or_extra_tool_arguments_without_source_io() {
    let calls = [
        ToolCall {
            name: "list_repository_tree".into(),
            arguments: serde_json::json!({"prefix": 42}),
        },
        ToolCall {
            name: "list_repository_tree".into(),
            arguments: serde_json::json!({"prefix": "src", "recursive": true}),
        },
        ToolCall {
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "src/lib.rs", "start_line": 1, "end_line": 2, "bytes": true}),
        },
        ToolCall {
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
async fn deduplicates_and_drops_unmapped_findings_then_sorts() {
    let mut medium = finding("same", 1);
    medium.severity = Severity::Medium;
    let mut high = finding("same", 1);
    high.severity = Severity::High;
    let mut low = finding("low", 2);
    low.severity = Severity::Low;
    let invalid = finding("bad", 99);
    let model = FakeModel(Mutex::new(VecDeque::from([ModelResponse::final_findings(
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
    let model = FakeModel(Mutex::new(VecDeque::from([ModelResponse::final_findings(
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
        let model = FakeModel(Mutex::new(VecDeque::from([ModelResponse::final_findings(
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
async fn patch_fetch_head_race_returns_pr_updated_before_model_analysis() {
    let model = FakeModel(Mutex::new(VecDeque::from([ModelResponse::final_findings(
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
    async fn respond(&self, _: &[TranscriptItem]) -> Result<ModelResponse, ReviewError> {
        tokio::time::sleep(Duration::from_millis(15)).await;
        Ok(ModelResponse::final_findings(
            vec![],
            ReviewUsage {
                input_tokens: 4,
                output_tokens: 2,
                tool_calls: 0,
            },
        ))
    }
}

#[tokio::test]
async fn trace_records_measured_success_once() {
    let trace = RecordingTrace::default();
    ReviewOrchestrator::new(&DelayedFinalModel, &source(), &trace, &NeverCancel)
        .run(input())
        .await
        .unwrap();
    let entries = trace.0.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].status, "completed");
    assert!(entries[0].duration_ms >= 10);
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
    let bad_model = FakeModel(Mutex::new(VecDeque::from([ModelResponse::tool_calls(
        vec![ToolCall {
            name: "shell".into(),
            arguments: serde_json::json!({}),
        }],
        ReviewUsage::default(),
    )])));
    let error = ReviewOrchestrator::new(&bad_model, &source(), &error_trace, &NeverCancel)
        .run(input())
        .await
        .unwrap_err();
    assert!(matches!(error, ReviewError::InvalidModelOutput(_)));
    let error_entries = error_trace.0.lock().unwrap();
    assert_eq!(error_entries.len(), 1);
    assert_eq!(error_entries[0].status, "error");
    assert_eq!(
        error_entries[0].error_code.as_deref(),
        Some("INVALID_MODEL_OUTPUT")
    );
}
