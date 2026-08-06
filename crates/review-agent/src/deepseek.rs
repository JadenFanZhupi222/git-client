use crate::{
    ModelOutput, ModelProvider, ModelResponse, ProviderCapabilities, ProviderDescriptor,
    ReviewError, ReviewOutputCodec, ReviewUsage, StructuredOutputSupport, ToolCall, TranscriptItem,
    MAX_TOOL_CALLS,
};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;

const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
pub const DEEPSEEK_V4_FLASH_MODEL: &str = "deepseek-v4-flash";
pub const DEEPSEEK_V4_PRO_MODEL: &str = "deepseek-v4-pro";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

pub struct DeepSeekProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl DeepSeekProvider {
    pub fn new(api_key: impl Into<String>) -> Result<Self, ReviewError> {
        Self::new_with_model(api_key, DEEPSEEK_V4_FLASH_MODEL)
    }

    pub fn new_with_model(
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ReviewError> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(ReviewError::AiKeyMissing);
        }
        let model = model.into();
        if !matches!(
            model.as_str(),
            DEEPSEEK_V4_FLASH_MODEL | DEEPSEEK_V4_PRO_MODEL
        ) {
            return Err(ReviewError::InvalidModelOutput(
                "unsupported DeepSeek model".into(),
            ));
        }
        Ok(Self {
            client: build_client(CONNECT_TIMEOUT, REQUEST_TIMEOUT)?,
            api_key,
            base_url: DEEPSEEK_BASE_URL.into(),
            model,
        })
    }

    #[cfg(test)]
    fn new_with_base_for_test(api_key: impl Into<String>, base_url: String) -> Self {
        Self {
            client: build_client(Duration::from_millis(50), Duration::from_millis(100))
                .expect("test HTTP client should build"),
            api_key: api_key.into(),
            base_url,
            model: DEEPSEEK_V4_FLASH_MODEL.into(),
        }
    }

    fn request_body(&self, transcript: &[TranscriptItem]) -> Result<Value, ReviewError> {
        let mut messages = Vec::new();
        let used_tool_calls = transcript
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    TranscriptItem::ToolResult {
                        counts_toward_budget: true,
                        ..
                    }
                )
            })
            .count();
        for item in transcript {
            match item {
                TranscriptItem::System(text) => {
                    messages.push(json!({"role":"system","content":text}))
                }
                TranscriptItem::User(text) => messages.push(json!({"role":"user","content":text})),
                TranscriptItem::AssistantToolCalls(calls) => {
                    let mut tool_calls = Vec::new();
                    for call in calls {
                        let mut arguments = call.arguments.clone();
                        let call_id = arguments
                            .as_object_mut()
                            .and_then(|object| object.remove("_call_id"))
                            .and_then(|value| value.as_str().map(str::to_owned))
                            .filter(|call_id| !call_id.is_empty())
                            .ok_or_else(|| {
                                ReviewError::InvalidModelOutput("function call id missing".into())
                            })?;
                        tool_calls.push(json!({"id":call_id,"type":"function","function":{"name":call.name,"arguments":arguments.to_string()}}));
                    }
                    messages
                        .push(json!({"role":"assistant","content":null,"tool_calls":tool_calls}));
                }
                TranscriptItem::ToolResult {
                    call_id, content, ..
                } => {
                    if call_id.is_empty() {
                        return Err(ReviewError::InvalidModelOutput(
                            "function call id missing".into(),
                        ));
                    }
                    messages.push(json!({"role":"tool","tool_call_id":call_id,"content":content}));
                }
            }
        }
        let mut body = json!({
            "model": self.model,
            "stream": false,
            "thinking": {"type":"disabled"},
            "max_tokens": 8192,
            "messages": messages,
            "response_format": {"type":"json_object"},
            "tools": [
                {"type":"function","function":{"name":"list_repository_tree","description":"List repository paths at the fixed PR head SHA","parameters":{"type":"object","properties":{"prefix":{"type":"string"}},"additionalProperties":false}}},
                {"type":"function","function":{"name":"read_file","description":"Read at most 400 UTF-8 lines at the fixed PR head SHA","parameters":{"type":"object","properties":{"path":{"type":"string"},"start_line":{"type":"integer"},"end_line":{"type":"integer"}},"required":["path","start_line","end_line"],"additionalProperties":false}}}
            ]
        });
        if used_tool_calls >= MAX_TOOL_CALLS {
            let object = body
                .as_object_mut()
                .expect("chat completion request body is an object");
            object.remove("tools");
            object.insert("tool_choice".into(), Value::String("none".into()));
            object
                .get_mut("messages")
                .and_then(Value::as_array_mut)
                .expect("messages is an array")
                .push(json!({
                    "role": "system",
                    "content": "The tool budget is exhausted. Do not call any more tools. Return the final review JSON now using only the evidence already available."
                }));
        }
        Ok(body)
    }
}

fn build_client(
    connect_timeout: Duration,
    request_timeout: Duration,
) -> Result<Client, ReviewError> {
    Client::builder()
        .connect_timeout(connect_timeout)
        .timeout(request_timeout)
        .build()
        .map_err(|_| ReviewError::NetworkError("could not initialize HTTP client".into()))
}

#[async_trait]
impl ModelProvider for DeepSeekProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider: "deepseek".into(),
            model: self.model.clone(),
            capabilities: ProviderCapabilities {
                structured_output: StructuredOutputSupport::JsonObject,
                can_disable_tools: true,
                parallel_tool_calls: false,
                requires_reasoning_replay: false,
            },
        }
    }

    async fn respond(&self, transcript: &[TranscriptItem]) -> Result<ModelResponse, ReviewError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let request_body = self.request_body(transcript)?;
        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&request_body)
            .send()
            .await
            .map_err(|_| ReviewError::NetworkError("request failed".into()))?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(ReviewError::AuthFailed);
            }
            StatusCode::TOO_MANY_REQUESTS => return Err(ReviewError::RateLimited),
            status if !status.is_success() => {
                return Err(ReviewError::NetworkError("service request failed".into()));
            }
            _ => {}
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| ReviewError::NetworkError("response body could not be read".into()))?;
        let body = serde_json::from_slice::<Value>(&bytes).map_err(|_| {
            ReviewError::NetworkError("service returned an invalid response".into())
        })?;
        parse_response(body)
    }
}

fn parse_response(body: Value) -> Result<ModelResponse, ReviewError> {
    let usage = ReviewUsage {
        input_tokens: body
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: body
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        tool_calls: 0,
    };
    let choice = body
        .pointer("/choices/0")
        .and_then(Value::as_object)
        .ok_or_else(|| ReviewError::InvalidModelOutput("missing response output".into()))?;
    match choice.get("finish_reason").and_then(Value::as_str) {
        Some("length") => return Err(ReviewError::ReviewBudgetExceeded),
        Some("content_filter" | "insufficient_system_resource") => {
            return Err(ReviewError::NetworkError("model response failed".into()));
        }
        _ => {}
    }
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| ReviewError::InvalidModelOutput("missing response output".into()))?;
    let mut calls = Vec::new();
    let mut call_ids = HashSet::new();
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for item in tool_calls {
            let function = item
                .get("function")
                .and_then(Value::as_object)
                .ok_or_else(|| ReviewError::InvalidModelOutput("function name missing".into()))?;
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ReviewError::InvalidModelOutput("function name missing".into()))?;
            let call_id = item
                .get("id")
                .and_then(Value::as_str)
                .filter(|call_id| !call_id.is_empty())
                .ok_or_else(|| {
                    ReviewError::InvalidModelOutput("function call id missing".into())
                })?;
            if !call_ids.insert(call_id) {
                return Err(ReviewError::InvalidModelOutput(
                    "duplicate function call id".into(),
                ));
            }
            let encoded = function
                .get("arguments")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ReviewError::InvalidModelOutput("function arguments missing".into())
                })?;
            let mut arguments: Value = serde_json::from_str(encoded).map_err(|_| {
                ReviewError::InvalidModelOutput("invalid function arguments".into())
            })?;
            if let Some(object) = arguments.as_object_mut() {
                object.insert("_call_id".into(), Value::String(call_id.into()));
            }
            calls.push(ToolCall {
                name: name.into(),
                arguments,
            });
        }
    }
    if !calls.is_empty() {
        Ok(ModelResponse {
            output: ModelOutput::ToolCalls { calls },
            usage,
        })
    } else if let Some(output_text) = message
        .get("content")
        .and_then(Value::as_str)
        .filter(|content| !content.trim().is_empty())
    {
        let decoded = ReviewOutputCodec::decode(output_text)?;
        Ok(ModelResponse {
            output: ModelOutput::Final {
                summary: decoded.summary,
                findings: decoded.findings,
            },
            usage,
        })
    } else {
        Err(ReviewError::InvalidModelOutput(
            "no tool calls or final output".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModelOutput, ModelProvider, ReviewError, TranscriptItem};
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn declares_deepseek_specific_capabilities_without_leaking_them_upward() {
        let provider = DeepSeekProvider::new_with_base_for_test("k", "http://localhost".into());
        let descriptor = provider.descriptor();
        assert_eq!(descriptor.provider, "deepseek");
        assert_eq!(descriptor.model, "deepseek-v4-flash");
        assert_eq!(
            descriptor.capabilities.structured_output,
            StructuredOutputSupport::JsonObject
        );
        assert!(descriptor.capabilities.can_disable_tools);
        assert!(!descriptor.capabilities.requires_reasoning_replay);
    }

    #[test]
    fn selects_only_supported_deepseek_models() {
        let provider = DeepSeekProvider::new_with_model("k", DEEPSEEK_V4_PRO_MODEL).unwrap();
        assert_eq!(provider.descriptor().model, DEEPSEEK_V4_PRO_MODEL);
        assert!(DeepSeekProvider::new_with_model("k", "retired-or-arbitrary-model").is_err());
    }

    #[tokio::test]
    async fn parses_function_call_and_sends_fixed_model_contract() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .and(body_partial_json(json!({
                "model":"deepseek-v4-flash",
                "stream":false,
                "thinking":{"type":"disabled"},
                "max_tokens":8192,
                "response_format":{"type":"json_object"}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"src/lib.rs\",\"start_line\":1,\"end_line\":2}"}}]}}],
                "usage":{"prompt_tokens":5,"completion_tokens":3}
            }))).mount(&server).await;
        let provider = DeepSeekProvider::new_with_base_for_test("test-key", server.uri());
        let response = provider
            .respond(&[TranscriptItem::System("safe".into())])
            .await
            .unwrap();
        match response.output {
            ModelOutput::ToolCalls { calls } => assert_eq!(calls[0].name, "read_file"),
            _ => panic!("expected tool call"),
        }
    }

    #[test]
    fn maps_tool_history_to_chat_completion_messages() {
        let provider = DeepSeekProvider::new_with_base_for_test("k", "http://localhost".into());
        let body = provider
            .request_body(&[
                TranscriptItem::AssistantToolCalls(vec![ToolCall::read_file(
                    "call-1",
                    "src/lib.rs",
                    1,
                    2,
                )]),
                TranscriptItem::ToolResult {
                    name: "read_file".into(),
                    call_id: "call-1".into(),
                    content: "fn main() {}".into(),
                    counts_toward_budget: true,
                },
            ])
            .unwrap();
        assert_eq!(body.pointer("/messages/0/role"), Some(&json!("assistant")));
        assert_eq!(
            body.pointer("/messages/0/tool_calls/0/id"),
            Some(&json!("call-1"))
        );
        assert_eq!(body.pointer("/messages/1/role"), Some(&json!("tool")));
        assert_eq!(
            body.pointer("/messages/1/tool_call_id"),
            Some(&json!("call-1"))
        );
    }

    #[test]
    fn disables_tools_and_requests_final_output_after_tool_budget_is_used() {
        let provider = DeepSeekProvider::new_with_base_for_test("k", "http://localhost".into());
        let calls = (0..MAX_TOOL_CALLS)
            .map(|index| ToolCall::list_tree(format!("call-{index}"), "src"))
            .collect::<Vec<_>>();
        let mut transcript = vec![TranscriptItem::AssistantToolCalls(calls)];
        transcript.extend((0..MAX_TOOL_CALLS).map(|index| TranscriptItem::ToolResult {
            name: "list_repository_tree".into(),
            call_id: format!("call-{index}"),
            content: "[]".into(),
            counts_toward_budget: true,
        }));
        let body = provider.request_body(&transcript).unwrap();

        assert!(body.get("tools").is_none());
        assert_eq!(body.get("tool_choice"), Some(&json!("none")));
        assert_eq!(
            body.pointer(&format!("/messages/{}/content", MAX_TOOL_CALLS + 1)),
            Some(&json!("The tool budget is exhausted. Do not call any more tools. Return the final review JSON now using only the evidence already available."))
        );
    }

    #[test]
    fn cached_tool_results_do_not_consume_the_unique_read_budget() {
        let provider = DeepSeekProvider::new_with_base_for_test("k", "http://localhost".into());
        let calls = (0..MAX_TOOL_CALLS)
            .map(|index| ToolCall::list_tree(format!("call-{index}"), "src"))
            .collect::<Vec<_>>();
        let mut transcript = vec![TranscriptItem::AssistantToolCalls(calls)];
        transcript.extend((0..MAX_TOOL_CALLS).map(|index| TranscriptItem::ToolResult {
            name: "list_repository_tree".into(),
            call_id: format!("call-{index}"),
            content: "[]".into(),
            counts_toward_budget: index == 0,
        }));

        let body = provider.request_body(&transcript).unwrap();
        assert!(body.get("tools").is_some());
        assert!(body.get("tool_choice").is_none());
    }

    #[tokio::test]
    async fn parses_final_structured_findings_and_ignores_reasoning() {
        let server = MockServer::start().await;
        let finding = json!({"id":"f","severity":"high","path":"src/lib.rs","side":"RIGHT","line":1,"title":"t","failure_scenario":"s","explanation":"e","draft_comment":"d"});
        Mock::given(method("POST")).and(path("/chat/completions")).respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices":[{"finish_reason":"stop","message":{"role":"assistant","content":json!({"summary":"One correctness issue.","findings":[finding]}).to_string()}}],
            "usage":{"prompt_tokens":1,"completion_tokens":2}
        }))).mount(&server).await;
        let response = DeepSeekProvider::new_with_base_for_test("k", server.uri())
            .respond(&[])
            .await
            .unwrap();
        match response.output {
            ModelOutput::Final { summary, findings } => {
                assert_eq!(summary, "One correctness issue.");
                assert_eq!(findings.len(), 1);
            }
            _ => panic!("expected final"),
        }
    }

    #[test]
    fn parses_chat_completion_json_content() {
        let body = json!({
            "choices":[{"finish_reason":"stop","message":{"content":"{\"summary\":\"Complete\",\"findings\":[]}"}}]
        });
        let response = parse_response(body).unwrap();
        assert!(matches!(
            response.output,
            ModelOutput::Final { ref summary, ref findings }
                if summary == "Complete" && findings.is_empty()
        ));
    }

    #[test]
    fn keeps_review_when_one_finding_has_an_invalid_schema() {
        let valid = json!({"id":"f","severity":"high","path":"src/lib.rs","side":"RIGHT","line":1,"title":"t","failure_scenario":"s","explanation":"e","draft_comment":"d"});
        let body = json!({
            "choices":[{"finish_reason":"stop","message":{"content":json!({
                "summary":"Found one valid issue.",
                "findings":[{"title":"incomplete"}, valid]
            }).to_string()}}]
        });

        let response = parse_response(body).unwrap();
        assert!(matches!(
            response.output,
            ModelOutput::Final { ref summary, ref findings }
                if summary == "Found one valid issue." && findings.len() == 1
        ));
    }

    #[test]
    fn treats_null_findings_as_an_empty_review() {
        let body = json!({
            "choices":[{"finish_reason":"stop","message":{"content":"{\"summary\":\"No issue found.\",\"findings\":null}"}}]
        });

        let response = parse_response(body).unwrap();
        assert!(matches!(
            response.output,
            ModelOutput::Final { ref findings, .. } if findings.is_empty()
        ));
    }

    #[test]
    fn preserves_nonempty_plain_text_as_an_unstructured_review() {
        let body = json!({
            "choices":[{"finish_reason":"stop","message":{"content":"I reviewed the selected patch. No actionable correctness issue was found."}}],
            "usage":{"prompt_tokens":10,"completion_tokens":12}
        });

        let response = parse_response(body).unwrap();
        assert!(matches!(
            response.output,
            ModelOutput::Final { ref summary, ref findings }
                if summary.contains("No actionable") && findings.is_empty()
        ));
    }

    #[test]
    fn maps_chat_completion_terminal_reasons() {
        assert!(matches!(
            parse_response(
                json!({"choices":[{"finish_reason":"insufficient_system_resource","message":{}}]})
            ),
            Err(ReviewError::NetworkError(_))
        ));
        assert_eq!(
            parse_response(json!({"choices":[{"finish_reason":"length","message":{}}]}))
                .unwrap_err(),
            ReviewError::ReviewBudgetExceeded
        );
    }

    #[tokio::test]
    async fn rejects_final_output_without_a_summary() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices":[{"finish_reason":"stop","message":{"content":json!({"findings":[]}).to_string()}}]
            })))
            .mount(&server)
            .await;

        let error = DeepSeekProvider::new_with_base_for_test("k", server.uri())
            .respond(&[])
            .await
            .unwrap_err();

        assert_eq!(
            error,
            ReviewError::InvalidModelOutput("summary missing".into())
        );
    }

    #[tokio::test]
    async fn maps_http_statuses_and_invalid_json() {
        for (status, expected) in [
            (401, ReviewError::AuthFailed),
            (403, ReviewError::AuthFailed),
            (429, ReviewError::RateLimited),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            let err = DeepSeekProvider::new_with_base_for_test("k", server.uri())
                .respond(&[])
                .await
                .unwrap_err();
            assert_eq!(err, expected);
        }
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&server)
            .await;
        assert!(matches!(
            DeepSeekProvider::new_with_base_for_test("k", server.uri())
                .respond(&[])
                .await
                .unwrap_err(),
            ReviewError::NetworkError(_)
        ));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn maps_hanging_response_timeout_to_network_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(std::time::Duration::from_millis(250))
                    .set_body_json(json!({"output":[]})),
            )
            .mount(&server)
            .await;
        let error = DeepSeekProvider::new_with_base_for_test("k", server.uri())
            .respond(&[])
            .await
            .unwrap_err();
        assert!(matches!(error, ReviewError::NetworkError(_)));
    }

    #[test]
    fn rejects_missing_empty_and_duplicate_function_call_ids() {
        let function_call = |call_id: Option<&str>| {
            let mut call = json!({
                "type":"function_call",
                "name":"list_repository_tree",
                "arguments":"{}"
            });
            if let Some(call_id) = call_id {
                call["call_id"] = json!(call_id);
            }
            call
        };
        for output in [
            vec![function_call(None)],
            vec![function_call(Some(""))],
            vec![function_call(Some("same")), function_call(Some("same"))],
        ] {
            let error = parse_response(json!({"output":output})).unwrap_err();
            assert!(matches!(error, ReviewError::InvalidModelOutput(_)));
        }
    }
}
