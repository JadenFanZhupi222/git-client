use crate::{
    AgentEventEmitter, AgentEventKind, ModelCatalogEntry, ModelPricing, ModelProvider,
    ModelRequest, ModelResponse, ModelUsage, ProviderCapabilities, ProviderDescriptor,
    ProviderError, ResponseFormat, ReviewError, StructuredOutputSupport, ToolCall,
    ToolCallingSupport, TranscriptItem, UsageSupport,
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
const PRICING_SOURCE_URL: &str = "https://api-docs.deepseek.com/quick_start/pricing";
const PRICING_SOURCE_VERSION: &str = "deepseek-v4-models-and-pricing";
const PRICING_CHECKED_AT: &str = "2026-08-07";

fn deepseek_capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        structured_output: StructuredOutputSupport::JsonObject,
        tool_calling: ToolCallingSupport::Serial,
        can_disable_tools: true,
        requires_reasoning_replay: false,
        context_window_tokens: 1_000_000,
        max_output_tokens: 384_000,
        usage: UsageSupport::InputOutputTokens,
    }
}

fn pricing(cache_hit: u64, cache_miss: u64, output: u64) -> ModelPricing {
    ModelPricing {
        currency: "USD".into(),
        input_cache_hit_per_million_micros: cache_hit,
        input_cache_miss_per_million_micros: cache_miss,
        output_per_million_micros: output,
        source_url: PRICING_SOURCE_URL.into(),
        source_version: PRICING_SOURCE_VERSION.into(),
        checked_at: PRICING_CHECKED_AT.into(),
    }
}

pub fn deepseek_model_catalog() -> Vec<ModelCatalogEntry> {
    vec![
        ModelCatalogEntry {
            id: DEEPSEEK_V4_FLASH_MODEL.into(),
            label: "DeepSeek V4 Flash".into(),
            provider_id: "deepseek".into(),
            provider_label: "DeepSeek".into(),
            capabilities: deepseek_capabilities(),
            pricing: Some(pricing(2_800, 140_000, 280_000)),
        },
        ModelCatalogEntry {
            id: DEEPSEEK_V4_PRO_MODEL.into(),
            label: "DeepSeek V4 Pro".into(),
            provider_id: "deepseek".into(),
            provider_label: "DeepSeek".into(),
            capabilities: deepseek_capabilities(),
            pricing: Some(pricing(3_625, 435_000, 870_000)),
        },
    ]
}

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
    pub(crate) fn new_with_base_for_test(api_key: impl Into<String>, base_url: String) -> Self {
        Self {
            client: build_client(Duration::from_millis(50), Duration::from_millis(100))
                .expect("test HTTP client should build"),
            api_key: api_key.into(),
            base_url,
            model: DEEPSEEK_V4_FLASH_MODEL.into(),
        }
    }

    fn request_body(&self, request: &ModelRequest) -> Result<Value, ProviderError> {
        let mut messages = Vec::new();
        for item in &request.transcript {
            match item {
                TranscriptItem::System(text) => {
                    messages.push(json!({"role": "system", "content": text}));
                }
                TranscriptItem::User(text) => {
                    messages.push(json!({"role": "user", "content": text}));
                }
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
                                ProviderError::InvalidResponse("function call id missing".into())
                            })?;
                        tool_calls.push(json!({
                            "id": call_id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": arguments.to_string()
                            }
                        }));
                    }
                    messages.push(json!({
                        "role": "assistant",
                        "content": null,
                        "tool_calls": tool_calls
                    }));
                }
                TranscriptItem::ToolResult {
                    call_id, content, ..
                } => {
                    if call_id.is_empty() {
                        return Err(ProviderError::InvalidResponse(
                            "function call id missing".into(),
                        ));
                    }
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": content
                    }));
                }
            }
        }

        let mut body = json!({
            "model": self.model,
            "stream": false,
            "thinking": {"type": "disabled"},
            "max_tokens": request.max_output_tokens,
            "messages": messages
        });
        let object = body
            .as_object_mut()
            .expect("chat completion request body is an object");
        if request.response_format == ResponseFormat::JsonObject {
            object.insert("response_format".into(), json!({"type": "json_object"}));
        }
        if !request.tools.is_empty() {
            object.insert(
                "tools".into(),
                Value::Array(
                    request
                        .tools
                        .iter()
                        .map(|tool| {
                            json!({
                                "type": "function",
                                "function": {
                                    "name": tool.name,
                                    "description": tool.description,
                                    "parameters": tool.input_schema
                                }
                            })
                        })
                        .collect(),
                ),
            );
        }
        Ok(body)
    }

    fn streaming_request_body(&self, request: &ModelRequest) -> Result<Value, ProviderError> {
        let mut body = self.request_body(request)?;
        let object = body
            .as_object_mut()
            .expect("chat completion request body is an object");
        object.insert("stream".into(), Value::Bool(true));
        object.insert("stream_options".into(), json!({"include_usage": true}));
        Ok(body)
    }
}

fn build_client(
    connect_timeout: Duration,
    request_timeout: Duration,
) -> Result<Client, ReviewError> {
    Client::builder()
        .connect_timeout(connect_timeout)
        .read_timeout(request_timeout)
        .build()
        .map_err(|_| ReviewError::NetworkError("could not initialize HTTP client".into()))
}

#[async_trait]
impl ModelProvider for DeepSeekProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: "deepseek".into(),
            model_id: self.model.clone(),
            capabilities: deepseek_capabilities(),
        }
    }

    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, ProviderError> {
        let response = self
            .client
            .post(format!(
                "{}/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&self.api_key)
            .json(&self.request_body(request)?)
            .send()
            .await
            .map_err(|_| ProviderError::Network("request failed".into()))?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(ProviderError::AuthFailed);
            }
            StatusCode::TOO_MANY_REQUESTS => return Err(ProviderError::RateLimited),
            status if !status.is_success() => {
                return Err(ProviderError::Network("service request failed".into()));
            }
            _ => {}
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| ProviderError::Network("response body could not be read".into()))?;
        let body = serde_json::from_slice::<Value>(&bytes)
            .map_err(|_| ProviderError::Network("service returned an invalid response".into()))?;
        parse_response(body)
    }

    async fn respond_stream(
        &self,
        request: &ModelRequest,
        events: &AgentEventEmitter<'_>,
    ) -> Result<ModelResponse, ProviderError> {
        let response = self
            .client
            .post(format!(
                "{}/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&self.api_key)
            .json(&self.streaming_request_body(request)?)
            .send()
            .await
            .map_err(|_| ProviderError::Network("request failed".into()))?;
        map_status(response.status())?;

        let mut started = false;
        let mut completed = false;
        let mut content = String::new();
        let mut finish_reason = None;
        let mut usage = Value::Null;
        let mut last_usage = None;
        let mut tool_calls = Vec::<StreamingToolCall>::new();
        crate::sse::consume_sse(response.bytes_stream(), |event| {
            if event.data == "[DONE]" {
                completed = true;
                return Ok(false);
            }
            let body = serde_json::from_str::<Value>(&event.data)
                .map_err(|_| crate::sse::SseError::Protocol("invalid JSON event".into()))?;
            if !started {
                started = true;
                events.emit(AgentEventKind::ModelResponseStarted {
                    response_id: body.get("id").and_then(Value::as_str).map(str::to_owned),
                });
            }
            if body.get("usage").is_some_and(|value| !value.is_null()) {
                usage = body.get("usage").cloned().unwrap_or(Value::Null);
                let update = deepseek_usage(&body);
                events.emit(AgentEventKind::UsageUpdated {
                    usage: update.clone(),
                });
                last_usage = Some(update);
            }
            let Some(choice) = body.pointer("/choices/0") else {
                return Ok(true);
            };
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                finish_reason = Some(reason.to_owned());
            }
            let Some(delta) = choice.get("delta") else {
                return Ok(true);
            };
            if let Some(part) = delta.get("content").and_then(Value::as_str) {
                if !part.is_empty() {
                    content.push_str(part);
                    events.emit(AgentEventKind::OutputTextDelta {
                        delta: part.to_owned(),
                    });
                }
            }
            if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for call in calls {
                    let index = call
                        .get("index")
                        .and_then(Value::as_u64)
                        .and_then(|index| usize::try_from(index).ok())
                        .ok_or_else(|| {
                            crate::sse::SseError::Protocol("missing tool call index".into())
                        })?;
                    if index >= 1024 {
                        return Err(crate::sse::SseError::Protocol(
                            "tool call index exceeded limit".into(),
                        ));
                    }
                    if index > tool_calls.len() {
                        return Err(crate::sse::SseError::Protocol(
                            "tool calls arrived out of order".into(),
                        ));
                    }
                    if index == tool_calls.len() {
                        tool_calls.push(StreamingToolCall::default());
                    }
                    let target = &mut tool_calls[index];
                    if let Some(id) = call.get("id").and_then(Value::as_str) {
                        target.id.push_str(id);
                    }
                    if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                        target.name.push_str(name);
                    }
                    if !target.started && !target.id.is_empty() && !target.name.is_empty() {
                        target.started = true;
                        events.emit(AgentEventKind::ToolCallStarted {
                            call_id: target.id.clone(),
                            name: target.name.clone(),
                        });
                    }
                    if let Some(arguments) =
                        call.pointer("/function/arguments").and_then(Value::as_str)
                    {
                        target.arguments.push_str(arguments);
                        if !target.id.is_empty() && !arguments.is_empty() {
                            events.emit(AgentEventKind::ToolArgumentsDelta {
                                call_id: target.id.clone(),
                                delta: arguments.to_owned(),
                            });
                        }
                    }
                }
            }
            Ok(true)
        })
        .await
        .map_err(map_sse_error)?;

        if !completed {
            return Err(ProviderError::InvalidResponse(
                "stream ended before completion".into(),
            ));
        }
        let streamed_calls = tool_calls
            .into_iter()
            .filter(|call| {
                !call.id.is_empty() || !call.name.is_empty() || !call.arguments.is_empty()
            })
            .map(|call| {
                json!({
                    "id": call.id,
                    "type": "function",
                    "function": {"name": call.name, "arguments": call.arguments}
                })
            })
            .collect::<Vec<_>>();
        let message = if streamed_calls.is_empty() {
            json!({"content": content})
        } else {
            json!({"content": null, "tool_calls": streamed_calls})
        };
        let response = parse_response(json!({
            "choices": [{"finish_reason": finish_reason, "message": message}],
            "usage": usage
        }))?;
        if last_usage.as_ref() != Some(&response.usage) {
            events.emit(AgentEventKind::UsageUpdated {
                usage: response.usage.clone(),
            });
        }
        events.emit(AgentEventKind::ModelResponseCompleted);
        Ok(response)
    }
}

#[derive(Default)]
struct StreamingToolCall {
    id: String,
    name: String,
    arguments: String,
    started: bool,
}

fn map_status(status: StatusCode) -> Result<(), ProviderError> {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ProviderError::AuthFailed),
        StatusCode::TOO_MANY_REQUESTS => Err(ProviderError::RateLimited),
        status if !status.is_success() => {
            Err(ProviderError::Network("service request failed".into()))
        }
        _ => Ok(()),
    }
}

fn map_sse_error(error: crate::sse::SseError) -> ProviderError {
    match error {
        crate::sse::SseError::Read(_) => {
            ProviderError::Network("response body could not be read".into())
        }
        _ => ProviderError::InvalidResponse("invalid streaming response".into()),
    }
}

fn parse_response(body: Value) -> Result<ModelResponse, ProviderError> {
    let usage = deepseek_usage(&body);
    let choice = body
        .pointer("/choices/0")
        .and_then(Value::as_object)
        .ok_or_else(|| ProviderError::InvalidResponse("missing response output".into()))?;
    match choice.get("finish_reason").and_then(Value::as_str) {
        Some("length") => return Err(ProviderError::OutputTruncated),
        Some("content_filter" | "insufficient_system_resource") => {
            return Err(ProviderError::Network("model response failed".into()));
        }
        _ => {}
    }
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| ProviderError::InvalidResponse("missing response output".into()))?;
    let mut calls = Vec::new();
    let mut call_ids = HashSet::new();
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for item in tool_calls {
            let function = item
                .get("function")
                .and_then(Value::as_object)
                .ok_or_else(|| ProviderError::InvalidResponse("function name missing".into()))?;
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ProviderError::InvalidResponse("function name missing".into()))?;
            let call_id = item
                .get("id")
                .and_then(Value::as_str)
                .filter(|call_id| !call_id.is_empty())
                .ok_or_else(|| ProviderError::InvalidResponse("function call id missing".into()))?;
            if !call_ids.insert(call_id) {
                return Err(ProviderError::InvalidResponse(
                    "duplicate function call id".into(),
                ));
            }
            let encoded = function
                .get("arguments")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ProviderError::InvalidResponse("function arguments missing".into())
                })?;
            let arguments = serde_json::from_str::<Value>(encoded)
                .map_err(|_| ProviderError::InvalidResponse("invalid function arguments".into()))?;
            if !arguments.is_object() {
                return Err(ProviderError::InvalidResponse(
                    "function arguments must be an object".into(),
                ));
            }
            calls.push(ToolCall::with_call_id(name, call_id, arguments));
        }
    }
    if !calls.is_empty() {
        Ok(ModelResponse::tool_calls(calls, usage))
    } else if let Some(text) = message
        .get("content")
        .and_then(Value::as_str)
        .filter(|content| !content.trim().is_empty())
    {
        Ok(ModelResponse::final_text(text, usage))
    } else {
        Err(ProviderError::InvalidResponse(
            "no tool calls or final output".into(),
        ))
    }
}

fn deepseek_usage(body: &Value) -> ModelUsage {
    ModelUsage {
        input_tokens: body
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: body
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        tool_calls: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentEvent, AgentEventClock, AgentEventSink, ModelOutput, ToolDefinition};
    use std::sync::Mutex;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn request(transcript: Vec<TranscriptItem>, with_tools: bool) -> ModelRequest {
        ModelRequest {
            transcript,
            tools: if with_tools {
                vec![ToolDefinition {
                    name: "read_file".into(),
                    description: "Read a file".into(),
                    input_schema: json!({"type": "object"}),
                }]
            } else {
                Vec::new()
            },
            response_format: ResponseFormat::JsonObject,
            response_schema: None,
            max_output_tokens: 8192,
        }
    }

    #[derive(Default)]
    struct RecordingSink(Mutex<Vec<AgentEvent>>);

    impl AgentEventSink for RecordingSink {
        fn emit(&self, event: AgentEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn catalog_and_descriptor_expose_stable_capabilities_and_pricing() {
        let provider = DeepSeekProvider::new_with_base_for_test("k", "http://localhost".into());
        let descriptor = provider.descriptor();
        assert_eq!(descriptor.provider_id, "deepseek");
        assert_eq!(descriptor.model_id, DEEPSEEK_V4_FLASH_MODEL);
        assert_eq!(descriptor.capabilities.context_window_tokens, 1_000_000);
        assert_eq!(
            descriptor.capabilities.tool_calling,
            ToolCallingSupport::Serial
        );

        let catalog = deepseek_model_catalog();
        assert_eq!(catalog.len(), 2);
        assert_eq!(
            catalog[0]
                .pricing
                .as_ref()
                .unwrap()
                .input_cache_miss_per_million_micros,
            140_000
        );
        assert_eq!(
            catalog[0].pricing.as_ref().unwrap().checked_at,
            "2026-08-07"
        );
    }

    #[test]
    fn selects_only_supported_models() {
        let provider = DeepSeekProvider::new_with_model("k", DEEPSEEK_V4_PRO_MODEL).unwrap();
        assert_eq!(provider.descriptor().model_id, DEEPSEEK_V4_PRO_MODEL);
        assert!(DeepSeekProvider::new_with_model("k", "unknown").is_err());
    }

    #[test]
    fn maps_generic_request_and_omits_tools_when_disabled() {
        let provider = DeepSeekProvider::new_with_base_for_test("k", "http://localhost".into());
        let history = vec![
            TranscriptItem::AssistantToolCalls(vec![crate::read_file_call(
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
        ];
        let body = provider
            .request_body(&request(history.clone(), true))
            .unwrap();
        assert_eq!(
            body.pointer("/messages/0/tool_calls/0/id"),
            Some(&json!("call-1"))
        );
        assert_eq!(body.pointer("/messages/1/role"), Some(&json!("tool")));
        assert!(body.get("tools").is_some());

        let body = provider.request_body(&request(history, false)).unwrap();
        assert!(body.get("tools").is_none());
    }

    #[tokio::test]
    async fn parses_tool_calls_and_usage_through_the_shared_contract() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .and(body_partial_json(json!({
                "model": DEEPSEEK_V4_FLASH_MODEL,
                "max_tokens": 8192,
                "response_format": {"type": "json_object"}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"finish_reason": "tool_calls", "message": {
                    "tool_calls": [{"id": "c1", "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"src/lib.rs\",\"start_line\":1,\"end_line\":2}"
                    }}]
                }}],
                "usage": {"prompt_tokens": 5, "completion_tokens": 3}
            })))
            .mount(&server)
            .await;

        let response = DeepSeekProvider::new_with_base_for_test("test-key", server.uri())
            .respond(&request(vec![TranscriptItem::System("safe".into())], true))
            .await
            .unwrap();
        assert_eq!(response.usage.input_tokens, 5);
        assert!(
            matches!(response.output, ModelOutput::ToolCalls { calls } if calls[0].name == "read_file")
        );
    }

    #[test]
    fn preserves_final_text_for_workflow_specific_decoding() {
        let response = parse_response(json!({
            "choices": [{"finish_reason": "stop", "message": {
                "content": "{\"summary\":\"Complete\",\"findings\":[]}"
            }}]
        }))
        .unwrap();
        assert!(matches!(
            response.output,
            ModelOutput::FinalText { ref text } if text.contains("Complete")
        ));
    }

    #[test]
    fn maps_terminal_reasons_and_rejects_duplicate_call_ids() {
        assert_eq!(
            parse_response(json!({"choices": [{"finish_reason": "length", "message": {}}]}))
                .unwrap_err(),
            ProviderError::OutputTruncated
        );
        let error = parse_response(json!({
            "choices": [{"finish_reason": "tool_calls", "message": {"tool_calls": [
                {"id": "same", "function": {"name": "a", "arguments": "{}"}},
                {"id": "same", "function": {"name": "b", "arguments": "{}"}}
            ]}}]
        }))
        .unwrap_err();
        assert!(matches!(error, ProviderError::InvalidResponse(_)));
    }

    #[tokio::test]
    async fn maps_http_statuses_invalid_json_and_timeout() {
        for (status, expected) in [
            (401, ProviderError::AuthFailed),
            (403, ProviderError::AuthFailed),
            (429, ProviderError::RateLimited),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            let error = DeepSeekProvider::new_with_base_for_test("k", server.uri())
                .respond(&request(Vec::new(), false))
                .await
                .unwrap_err();
            assert_eq!(error, expected);
        }

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not-json"))
            .mount(&server)
            .await;
        assert!(matches!(
            DeepSeekProvider::new_with_base_for_test("k", server.uri())
                .respond(&request(Vec::new(), false))
                .await
                .unwrap_err(),
            ProviderError::Network(_)
        ));

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(250)))
            .mount(&server)
            .await;
        assert!(matches!(
            DeepSeekProvider::new_with_base_for_test("k", server.uri())
                .respond(&request(Vec::new(), false))
                .await
                .unwrap_err(),
            ProviderError::Network(_)
        ));
    }

    #[tokio::test]
    async fn streams_indexed_tool_deltas_and_final_usage() {
        let server = MockServer::start().await;
        let stream = concat!(
            "data: {\"id\":\"chat_1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\"}}]},\"finish_reason\":null}],\"usage\":null}\n\n",
            "data: {\"id\":\"chat_1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"src/lib.rs\\\"}\"}}]},\"finish_reason\":null}],\"usage\":null}\n\n",
            "data: {\"id\":\"chat_1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":null}\n\n",
            "data: {\"id\":\"chat_1\",\"choices\":[],\"usage\":{\"prompt_tokens\":6,\"completion_tokens\":3}}\n\n",
            "data: [DONE]\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_partial_json(json!({
                "stream": true,
                "stream_options": {"include_usage": true}
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(stream),
            )
            .mount(&server)
            .await;

        let sink = RecordingSink::default();
        let clock = AgentEventClock::default();
        let emitter = AgentEventEmitter::new("run-deepseek", 1, &clock, &sink);
        let response = DeepSeekProvider::new_with_base_for_test("test-key", server.uri())
            .respond_stream(&request(Vec::new(), true), &emitter)
            .await
            .unwrap();

        assert_eq!(response.usage.input_tokens, 6);
        assert_eq!(response.usage.output_tokens, 3);
        assert!(matches!(
            response.output,
            ModelOutput::ToolCalls { calls } if calls[0].arguments["path"] == "src/lib.rs"
        ));
        let events = sink.0.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.kind, AgentEventKind::ToolArgumentsDelta { .. }))
                .count(),
            2
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.kind, AgentEventKind::UsageUpdated { .. }))
                .count(),
            1
        );
        assert!(matches!(
            events.last().map(|event| &event.kind),
            Some(AgentEventKind::ModelResponseCompleted)
        ));
    }
}
