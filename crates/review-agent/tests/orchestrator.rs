use async_trait::async_trait;
use review_agent::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

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
}

#[async_trait]
impl ReviewSource for FakeSource {
    async fn head_sha(&self, _: &ReviewTarget) -> Result<String, ReviewError> {
        Ok(self.head.lock().unwrap().clone())
    }
    async fn pull_files(&self, _: &ReviewTarget) -> Result<Vec<ReviewFile>, ReviewError> {
        Ok(self.files.clone())
    }
    async fn list_repository_tree(
        &self,
        _: &ReviewTarget,
        sha: &str,
        _: Option<&str>,
    ) -> Result<Vec<String>, ReviewError> {
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
struct NoTrace;
#[async_trait]
impl TraceSink for NoTrace {
    async fn record(&self, _: TraceEntry) -> Result<(), ReviewError> {
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
    assert_eq!(
        result.findings.iter().map(|f| &f.id).collect::<Vec<_>>(),
        vec![&"same".to_string(), &"low".to_string()]
    );
    assert_eq!(result.findings[0].severity, Severity::High);
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
