use async_trait::async_trait;
use review_agent::*;
use std::collections::VecDeque;
use std::sync::Mutex;

struct ContractProvider {
    requests: Mutex<Vec<ModelRequest>>,
    responses: Mutex<VecDeque<ModelResponse>>,
}

impl ContractProvider {
    fn new(responses: Vec<ModelResponse>) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            responses: Mutex::new(responses.into()),
        }
    }
}

#[async_trait]
impl ModelProvider for ContractProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: "contract-fixture".into(),
            model_id: "contract-model".into(),
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

    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, ProviderError> {
        self.requests.lock().unwrap().push(request.clone());
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::InvalidResponse("missing fixture response".into()))
    }
}

struct ContractSource;

#[async_trait]
impl ReviewSource for ContractSource {
    async fn head_sha(&self, _: &ReviewTarget) -> Result<String, ReviewError> {
        Ok("abc".into())
    }

    async fn pull_files_at_head(
        &self,
        _: &ReviewTarget,
        _: &str,
    ) -> Result<Vec<ReviewFile>, ReviewError> {
        Ok(vec![ReviewFile::from_patch(
            "src/lib.rs",
            "@@ -1 +1 @@\n-old\n+new",
        )?])
    }

    async fn list_repository_tree(
        &self,
        _: &ReviewTarget,
        _: &str,
        _: Option<&str>,
    ) -> Result<Vec<String>, ReviewError> {
        Ok(Vec::new())
    }

    async fn read_file(
        &self,
        _: &ReviewTarget,
        _: &str,
        _: &str,
        _: u32,
        _: u32,
    ) -> Result<String, ReviewError> {
        Ok(String::new())
    }

    async fn publish(&self, _: &SubmitReview) -> Result<PublishedReview, ReviewError> {
        unreachable!("the contract test is read-only")
    }
}

#[async_trait]
impl IssueSource for ContractSource {
    async fn list_open_issues(
        &self,
        _: &IssueRepositoryTarget,
    ) -> Result<Vec<IssueSummary>, ReviewError> {
        Ok(Vec::new())
    }

    async fn issue_context(&self, _: &IssueTarget) -> Result<IssueContext, ReviewError> {
        Ok(IssueContext {
            issue: IssueSummary {
                number: 7,
                title: "Crash".into(),
                url: String::new(),
                author: None,
                updated_at: "snapshot".into(),
                comments: 0,
                labels: Vec::new(),
            },
            body: "Steps to reproduce".into(),
            comments: Vec::new(),
            comments_truncated: false,
            available_labels: vec![IssueLabel {
                name: "bug".into(),
                color: "d73a4a".into(),
            }],
            similar_issues: Vec::new(),
            snapshot: IssueSnapshot {
                updated_at: "snapshot".into(),
                comments: 0,
            },
        })
    }
}

struct NeverCancel;

impl CancelSignal for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

struct NoTrace;

#[async_trait]
impl TraceSink for NoTrace {
    async fn record(&self, _: TraceEntry) -> Result<(), ReviewError> {
        Ok(())
    }
}

#[tokio::test]
async fn one_provider_contract_drives_both_workflows_without_provider_branches() {
    let provider = ContractProvider::new(vec![
        ModelResponse::final_text(
            r#"{"summary":"Reviewed","findings":[]}"#,
            ModelUsage {
                input_tokens: 10,
                output_tokens: 2,
                tool_calls: 0,
            },
        ),
        ModelResponse::final_text(
            r#"{"summary":"Crash report","category":"bug","priority":"high","confidence":0.9,"suggested_labels":["bug"],"suspected_duplicate_numbers":[],"suggested_reply":"Thanks","rationale":["Reproduction steps supplied"]}"#,
            ModelUsage {
                input_tokens: 12,
                output_tokens: 4,
                tool_calls: 0,
            },
        ),
    ]);
    let source = ContractSource;

    let review = ReviewOrchestrator::new(&provider, &source, &NoTrace, &NeverCancel)
        .run(ReviewRunInput {
            run_id: "review".into(),
            target: ReviewTarget {
                owner: "acme".into(),
                repo: "rocket".into(),
                pull_number: 1,
            },
            expected_head_sha: "abc".into(),
            selected_files: vec!["src/lib.rs".into()],
            output_language: ReviewLanguage::English,
        })
        .await
        .unwrap();
    assert_eq!(review.summary, "Reviewed");
    assert_eq!(review.usage.input_tokens, 10);

    let triage = IssueTriageOrchestrator::new(&provider, &source, &NeverCancel)
        .run(IssueTriageInput {
            run_id: "triage".into(),
            target: IssueTarget {
                owner: "acme".into(),
                repo: "rocket".into(),
                issue_number: 7,
            },
            expected_updated_at: "snapshot".into(),
            expected_comments: 0,
            output_language: ReviewLanguage::English,
        })
        .await
        .unwrap();
    assert_eq!(triage.proposal.category, "bug");
    assert_eq!(triage.usage.input_tokens, 12);

    let requests = provider.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].response_format, ResponseFormat::JsonObject);
    assert_eq!(requests[0].tools.len(), 2);
    assert!(requests[1].tools.is_empty());
    assert!(matches!(
        requests[1].transcript.first(),
        Some(TranscriptItem::System(system)) if system.contains("suggestions only")
    ));
}

#[test]
fn installed_provider_model_matrix_satisfies_both_workflow_contracts() {
    let catalog = deepseek_model_catalog();
    assert!(!catalog.is_empty());
    for model in catalog {
        assert_eq!(model.provider_id, "deepseek", "{} provider", model.id);
        assert_ne!(
            model.capabilities.structured_output,
            StructuredOutputSupport::None,
            "{} issue triage structured output",
            model.id
        );
        assert_ne!(
            model.capabilities.tool_calling,
            ToolCallingSupport::None,
            "{} PR review tool calling",
            model.id
        );
        assert!(
            model.capabilities.can_disable_tools,
            "{} issue triage must be able to disable PR tools",
            model.id
        );
        assert_eq!(
            model.capabilities.usage,
            UsageSupport::InputOutputTokens,
            "{} cost reporting",
            model.id
        );
    }
}
