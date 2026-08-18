use crate::{
    AgentEventEmitter, AgentEventKind, ModelCatalogEntry, ModelPricing, ModelProvider,
    ModelRequest, ModelResponse, ModelUsage, ProviderCapabilities, ProviderDescriptor,
    ProviderError, ResponseFormat, ReviewError, StructuredOutputSupport, ToolCall,
    ToolCallingSupport, TranscriptItem, UsageSupport,
};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";
pub const CLAUDE_SONNET_5_MODEL: &str = "claude-sonnet-5";
pub const CLAUDE_OPUS_5_MODEL: &str = "claude-opus-5";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const PRICING_SOURCE_URL: &str = "https://platform.claude.com/docs/en/about-claude/models/overview";
const PRICING_SOURCE_VERSION: &str = "claude-5-model-catalog-sonnet-introductory";
const PRICING_CHECKED_AT: &str = "2026-08-08";

fn anthropic_capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        structured_output: StructuredOutputSupport::JsonSchema,
        tool_calling: ToolCallingSupport::Parallel,
        can_disable_tools: true,
        // Thinking is disabled in requests so tool rounds only replay normalized transcript items.
        requires_reasoning_replay: false,
        context_window_tokens: 1_000_000,
        max_output_tokens: 128_000,
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

pub fn anthropic_model_catalog() -> Vec<ModelCatalogEntry> {
    vec![
        ModelCatalogEntry {
            id: CLAUDE_SONNET_5_MODEL.into(),
            label: "Claude Sonnet 5".into(),
            provider_id: "anthropic".into(),
            provider_label: "Anthropic".into(),
            capabilities: anthropic_capabilities(),
            // Introductory pricing is effective through 2026-08-31.
            pricing: Some(pricing(200_000, 2_000_000, 10_000_000)),
        },
        ModelCatalogEntry {
            id: CLAUDE_OPUS_5_MODEL.into(),
            label: "Claude Opus 5".into(),
            provider_id: "anthropic".into(),
            provider_label: "Anthropic".into(),
            capabilities: anthropic_capabilities(),
            pricing: Some(pricing(500_000, 5_000_000, 25_000_000)),
        },
    ]
}

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new_with_model(
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self, ReviewError> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(ReviewError::AiKeyMissing);
        }
        let model = model.into();
        if !matches!(model.as_str(), CLAUDE_SONNET_5_MODEL | CLAUDE_OPUS_5_MODEL) {
            return Err(ReviewError::InvalidModelOutput(
                "unsupported Anthropic model".into(),
            ));
        }
        Ok(Self {
            client: build_client(CONNECT_TIMEOUT, REQUEST_TIMEOUT)?,
            api_key,
            base_url: ANTHROPIC_BASE_URL.into(),
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
            model: CLAUDE_SONNET_5_MODEL.into(),
        }
    }

    fn request_body(&self, request: &ModelRequest) -> Result<Value, ProviderError> {
        let mut system = Vec::new();
        let mut messages = Vec::new();
        for item in &request.transcript {
            match item {
                TranscriptItem::System(text) => system.push(text.clone()),
                TranscriptItem::User(text) => push_blocks(
                    &mut messages,
                    "user",
                    vec![json!({"type":"text","text":text})],
                ),
                TranscriptItem::AssistantText(text) => push_blocks(
                    &mut messages,
                    "assistant",
                    vec![json!({"type":"text","text":text})],
                ),
                TranscriptItem::AssistantToolCalls(calls) => {
                    let mut blocks = Vec::new();
                    for call in calls {
                        let (call_id, arguments) = split_call_id(call)?;
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": call_id,
                            "name": call.name,
                            "input": arguments
                        }));
                    }
                    push_blocks(&mut messages, "assistant", blocks);
                }
                TranscriptItem::ToolResult {
                    call_id, content, ..
                } => {
                    if call_id.is_empty() {
                        return Err(ProviderError::InvalidResponse(
                            "function call id missing".into(),
                        ));
                    }
                    push_blocks(
                        &mut messages,
                        "user",
                        vec![json!({
                            "type": "tool_result",
                            "tool_use_id": call_id,
                            "content": content
                        })],
                    );
                }
            }
        }

        let mut body = json!({
            "model": self.model,
            "max_tokens": request.max_output_tokens,
            "messages": messages,
            "thinking": {"type": "disabled"}
        });
        let object = body
            .as_object_mut()
            .expect("Messages request body is an object");
        if !system.is_empty() {
            object.insert("system".into(), Value::String(system.join("\n\n")));
        }
        if request.response_format == ResponseFormat::JsonObject {
            if let Some(schema) = &request.response_schema {
                object.insert(
                    "output_config".into(),
                    json!({"format":{"type":"json_schema","schema":schema}}),
                );
            }
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
                                "name": tool.name,
                                "description": tool.description,
                                "input_schema": tool.input_schema
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
        body.as_object_mut()
            .expect("Messages request body is an object")
            .insert("stream".into(), Value::Bool(true));
        Ok(body)
    }
}

fn push_blocks(messages: &mut Vec<Value>, role: &str, mut blocks: Vec<Value>) {
    if messages
        .last()
        .and_then(|message| message.get("role"))
        .and_then(Value::as_str)
        == Some(role)
    {
        if let Some(content) = messages
            .last_mut()
            .and_then(|message| message.get_mut("content"))
            .and_then(Value::as_array_mut)
        {
            content.append(&mut blocks);
            return;
        }
    }
    messages.push(json!({"role":role,"content":blocks}));
}

fn split_call_id(call: &ToolCall) -> Result<(String, Value), ProviderError> {
    let call_id = (!call.call_id.is_empty())
        .then(|| call.call_id.clone())
        .ok_or_else(|| ProviderError::InvalidResponse("function call id missing".into()))?;
    Ok((call_id, call.arguments.clone()))
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
impl ModelProvider for AnthropicProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: "anthropic".into(),
            model_id: self.model.clone(),
            capabilities: anthropic_capabilities(),
        }
    }

    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, ProviderError> {
        let response = self
            .client
            .post(format!("{}/messages", self.base_url.trim_end_matches('/')))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&self.request_body(request)?)
            .send()
            .await
            .map_err(|_| ProviderError::Network("request failed".into()))?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(ProviderError::AuthFailed);
            }
            StatusCode::TOO_MANY_REQUESTS => return Err(ProviderError::RateLimited),
            status if status.is_server_error() => {
                return Err(ProviderError::Network("service request failed".into()));
            }
            status if !status.is_success() => {
                return Err(ProviderError::InvalidResponse(
                    "service rejected the request".into(),
                ));
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
            .post(format!("{}/messages", self.base_url.trim_end_matches('/')))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&self.streaming_request_body(request)?)
            .send()
            .await
            .map_err(|_| ProviderError::Network("request failed".into()))?;
        map_status(response.status())?;

        let mut message = None;
        let mut content = Vec::<Value>::new();
        let mut tool_arguments = HashMap::<usize, String>::new();
        let mut terminal_error = None;
        let mut completed = false;
        let mut last_usage = None;
        crate::sse::consume_sse(response.bytes_stream(), |event| {
            let body = serde_json::from_str::<Value>(&event.data)
                .map_err(|_| crate::sse::SseError::Protocol("invalid JSON event".into()))?;
            match body.get("type").and_then(Value::as_str) {
                Some("message_start") => {
                    message = body.get("message").cloned();
                    let response_id = body
                        .pointer("/message/id")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    events.emit(AgentEventKind::ModelResponseStarted { response_id });
                    if let Some(started_message) = message.as_ref() {
                        let usage = anthropic_usage(started_message);
                        events.emit(AgentEventKind::UsageUpdated {
                            usage: usage.clone(),
                        });
                        last_usage = Some(usage);
                    }
                }
                Some("content_block_start") => {
                    let index = event_index(&body)?;
                    if index != content.len() {
                        return Err(crate::sse::SseError::Protocol(
                            "content blocks arrived out of order".into(),
                        ));
                    }
                    let block = body.get("content_block").cloned().ok_or_else(|| {
                        crate::sse::SseError::Protocol("missing content block".into())
                    })?;
                    if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                        let call_id = block.get("id").and_then(Value::as_str).unwrap_or_default();
                        let name = block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if !call_id.is_empty() && !name.is_empty() {
                            events.emit(AgentEventKind::ToolCallStarted {
                                call_id: call_id.to_owned(),
                                name: name.to_owned(),
                            });
                        }
                    }
                    content.push(block);
                }
                Some("content_block_delta") => {
                    let index = event_index(&body)?;
                    let delta = body.get("delta").ok_or_else(|| {
                        crate::sse::SseError::Protocol("missing content delta".into())
                    })?;
                    match delta.get("type").and_then(Value::as_str) {
                        Some("text_delta") => {
                            let part = delta
                                .get("text")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            if !part.is_empty() {
                                let Some(block) =
                                    content.get_mut(index).and_then(Value::as_object_mut)
                                else {
                                    return Err(crate::sse::SseError::Protocol(
                                        "text delta has no content block".into(),
                                    ));
                                };
                                let text = block
                                    .get("text")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned()
                                    + part;
                                block.insert("text".into(), Value::String(text));
                                events.emit(AgentEventKind::OutputTextDelta {
                                    delta: part.to_owned(),
                                });
                            }
                        }
                        Some("input_json_delta") => {
                            let part = delta
                                .get("partial_json")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            tool_arguments.entry(index).or_default().push_str(part);
                            let call_id = content
                                .get(index)
                                .and_then(|block| block.get("id"))
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            if !call_id.is_empty() && !part.is_empty() {
                                events.emit(AgentEventKind::ToolArgumentsDelta {
                                    call_id: call_id.to_owned(),
                                    delta: part.to_owned(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                Some("content_block_stop") => {
                    let index = event_index(&body)?;
                    if let Some(arguments) = tool_arguments.remove(&index) {
                        if !arguments.trim().is_empty() {
                            let input =
                                serde_json::from_str::<Value>(&arguments).map_err(|_| {
                                    crate::sse::SseError::Protocol("invalid tool arguments".into())
                                })?;
                            content
                                .get_mut(index)
                                .and_then(Value::as_object_mut)
                                .ok_or_else(|| {
                                    crate::sse::SseError::Protocol(
                                        "tool delta has no content block".into(),
                                    )
                                })?
                                .insert("input".into(), input);
                        }
                    }
                }
                Some("message_delta") => {
                    let target =
                        message
                            .as_mut()
                            .and_then(Value::as_object_mut)
                            .ok_or_else(|| {
                                crate::sse::SseError::Protocol("message delta before start".into())
                            })?;
                    if let Some(stop_reason) = body.pointer("/delta/stop_reason").cloned() {
                        target.insert("stop_reason".into(), stop_reason);
                    }
                    if let Some(update) = body.get("usage").and_then(Value::as_object) {
                        let usage = target
                            .entry("usage")
                            .or_insert_with(|| json!({}))
                            .as_object_mut()
                            .ok_or_else(|| {
                                crate::sse::SseError::Protocol("invalid usage object".into())
                            })?;
                        usage.extend(update.clone());
                    }
                    let usage = anthropic_usage(
                        message
                            .as_ref()
                            .expect("message was validated before applying its delta"),
                    );
                    if last_usage.as_ref() != Some(&usage) {
                        events.emit(AgentEventKind::UsageUpdated {
                            usage: usage.clone(),
                        });
                        last_usage = Some(usage);
                    }
                }
                Some("message_stop") => {
                    completed = true;
                    return Ok(false);
                }
                Some("error") => {
                    terminal_error = Some(
                        if body.pointer("/error/type").and_then(Value::as_str)
                            == Some("overloaded_error")
                        {
                            ProviderError::Network("service request failed".into())
                        } else {
                            ProviderError::InvalidResponse("model response failed".into())
                        },
                    );
                    return Ok(false);
                }
                _ => {}
            }
            Ok(true)
        })
        .await
        .map_err(map_sse_error)?;

        if let Some(error) = terminal_error {
            return Err(error);
        }
        if !completed {
            return Err(ProviderError::InvalidResponse(
                "stream ended before completion".into(),
            ));
        }
        let mut message = message.ok_or_else(|| {
            ProviderError::InvalidResponse("stream ended before message start".into())
        })?;
        message
            .as_object_mut()
            .ok_or_else(|| ProviderError::InvalidResponse("invalid message start".into()))?
            .insert("content".into(), Value::Array(content));
        let response = parse_response(message)?;
        if last_usage.as_ref() != Some(&response.usage) {
            events.emit(AgentEventKind::UsageUpdated {
                usage: response.usage.clone(),
            });
        }
        events.emit(AgentEventKind::ModelResponseCompleted);
        Ok(response)
    }
}

fn event_index(body: &Value) -> Result<usize, crate::sse::SseError> {
    let index = body
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .ok_or_else(|| crate::sse::SseError::Protocol("missing content index".into()))?;
    if index >= 1024 {
        Err(crate::sse::SseError::Protocol(
            "content index exceeded limit".into(),
        ))
    } else {
        Ok(index)
    }
}

fn map_status(status: StatusCode) -> Result<(), ProviderError> {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ProviderError::AuthFailed),
        StatusCode::TOO_MANY_REQUESTS => Err(ProviderError::RateLimited),
        status if status.is_server_error() => {
            Err(ProviderError::Network("service request failed".into()))
        }
        status if !status.is_success() => Err(ProviderError::InvalidResponse(
            "service rejected the request".into(),
        )),
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
    match body.get("stop_reason").and_then(Value::as_str) {
        Some("max_tokens" | "model_context_window_exceeded") => {
            return Err(ProviderError::OutputTruncated);
        }
        Some("refusal") => {
            return Err(ProviderError::InvalidResponse(
                "model refused request".into(),
            ));
        }
        _ => {}
    }
    let usage = anthropic_usage(&body);
    let content = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::InvalidResponse("missing response output".into()))?;
    let mut calls = Vec::new();
    let mut call_ids = HashSet::new();
    let mut text = String::new();
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("tool_use") => {
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| {
                        ProviderError::InvalidResponse("function name missing".into())
                    })?;
                let call_id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|call_id| !call_id.is_empty())
                    .ok_or_else(|| {
                        ProviderError::InvalidResponse("function call id missing".into())
                    })?;
                if !call_ids.insert(call_id) {
                    return Err(ProviderError::InvalidResponse(
                        "duplicate function call id".into(),
                    ));
                }
                let arguments = block
                    .get("input")
                    .filter(|input| input.is_object())
                    .cloned()
                    .ok_or_else(|| {
                        ProviderError::InvalidResponse("invalid function arguments".into())
                    })?;
                calls.push(ToolCall::with_call_id(name, call_id, arguments));
            }
            Some("text") => {
                if let Some(part) = block.get("text").and_then(Value::as_str) {
                    text.push_str(part);
                }
            }
            _ => {}
        }
    }
    if !calls.is_empty() {
        Ok(ModelResponse::tool_calls(calls, usage))
    } else if !text.trim().is_empty() {
        Ok(ModelResponse::final_text(text, usage))
    } else {
        Err(ProviderError::InvalidResponse(
            "no tool calls or final output".into(),
        ))
    }
}

fn anthropic_usage(body: &Value) -> ModelUsage {
    let input_tokens = [
        "/usage/input_tokens",
        "/usage/cache_creation_input_tokens",
        "/usage/cache_read_input_tokens",
    ]
    .into_iter()
    .filter_map(|pointer| body.pointer(pointer).and_then(Value::as_u64))
    .sum();
    ModelUsage {
        input_tokens,
        output_tokens: body
            .pointer("/usage/output_tokens")
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

    fn request(with_tools: bool) -> ModelRequest {
        ModelRequest {
            transcript: vec![
                TranscriptItem::System("safe".into()),
                TranscriptItem::User("review".into()),
            ],
            tools: if with_tools {
                vec![ToolDefinition {
                    name: "read_file".into(),
                    description: "Read a file".into(),
                    input_schema: json!({"type":"object"}),
                    risk: crate::ToolRisk::ReadOnly,
                    timeout_ms: crate::default_tool_timeout_ms(),
                    max_result_bytes: crate::default_tool_result_bytes(),
                }]
            } else {
                Vec::new()
            },
            response_format: ResponseFormat::JsonObject,
            response_schema: Some(
                json!({"type":"object","properties":{"summary":{"type":"string"}},"required":["summary"],"additionalProperties":false}),
            ),
            max_output_tokens: 4096,
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
    fn catalog_and_descriptor_are_provider_qualified() {
        let provider = AnthropicProvider::new_with_base_for_test("k", "http://localhost".into());
        assert_eq!(provider.descriptor().provider_id, "anthropic");
        assert_eq!(provider.descriptor().model_id, CLAUDE_SONNET_5_MODEL);
        let catalog = anthropic_model_catalog();
        assert_eq!(catalog.len(), 2);
        assert!(catalog.iter().all(|entry| entry.provider_id == "anthropic"));
    }

    #[test]
    fn maps_tool_rounds_and_structured_output_without_thinking_replay() {
        let provider = AnthropicProvider::new_with_base_for_test("k", "http://localhost".into());
        let mut request = request(true);
        request.transcript.extend([
            TranscriptItem::AssistantText("Earlier answer".into()),
            TranscriptItem::AssistantToolCalls(vec![ToolCall::with_call_id(
                "read_file",
                "call-1",
                json!({"path":"src/lib.rs"}),
            )]),
            TranscriptItem::ToolResult {
                name: "read_file".into(),
                call_id: "call-1".into(),
                content: "fn main() {}".into(),
                counts_toward_budget: true,
            },
        ]);
        let body = provider.request_body(&request).unwrap();
        assert_eq!(body.pointer("/thinking/type"), Some(&json!("disabled")));
        assert_eq!(
            body.pointer("/messages/1/content/0/type"),
            Some(&json!("text"))
        );
        assert_eq!(
            body.pointer("/messages/1/content/0/text"),
            Some(&json!("Earlier answer"))
        );
        assert_eq!(
            body.pointer("/messages/1/content/1/type"),
            Some(&json!("tool_use"))
        );
        assert_eq!(
            body.pointer("/messages/2/content/0/type"),
            Some(&json!("tool_result"))
        );
        assert_eq!(
            body.pointer("/output_config/format/type"),
            Some(&json!("json_schema"))
        );
        assert!(body.get("tools").is_some());
    }

    #[tokio::test]
    async fn maps_tool_calls_usage_headers_and_auth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/messages"))
            .and(header("x-api-key", "test-key"))
            .and(header("anthropic-version", ANTHROPIC_VERSION))
            .and(body_partial_json(json!({"model":CLAUDE_SONNET_5_MODEL})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "stop_reason":"tool_use",
                "content":[{"type":"tool_use","id":"c1","name":"read_file","input":{"path":"src/lib.rs"}}],
                "usage":{"input_tokens":7,"cache_read_input_tokens":2,"output_tokens":3}
            })))
            .mount(&server)
            .await;
        let response = AnthropicProvider::new_with_base_for_test("test-key", server.uri())
            .respond(&request(true))
            .await
            .unwrap();
        assert_eq!(response.usage.input_tokens, 9);
        assert!(
            matches!(response.output, ModelOutput::ToolCalls { calls } if calls[0].name == "read_file")
        );

        let final_response = parse_response(json!({
            "stop_reason":"end_turn",
            "content":[{"type":"text","text":"{\"summary\":\"done\"}"}],
            "usage":{"input_tokens":2,"output_tokens":1}
        }))
        .unwrap();
        assert!(
            matches!(final_response.output, ModelOutput::FinalText { text } if text.contains("done"))
        );

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        assert_eq!(
            AnthropicProvider::new_with_base_for_test("bad", server.uri())
                .respond(&request(false))
                .await
                .unwrap_err(),
            ProviderError::AuthFailed
        );
    }

    #[test]
    fn rejects_truncation_refusal_and_duplicate_call_ids() {
        assert_eq!(
            parse_response(json!({"stop_reason":"max_tokens","content":[]})).unwrap_err(),
            ProviderError::OutputTruncated
        );
        assert!(matches!(
            parse_response(
                json!({"stop_reason":"refusal","content":[{"type":"text","text":"no"}]})
            )
            .unwrap_err(),
            ProviderError::InvalidResponse(_)
        ));
        assert!(matches!(
            parse_response(json!({
                "stop_reason":"tool_use",
                "content":[
                    {"type":"tool_use","id":"same","name":"a","input":{}},
                    {"type":"tool_use","id":"same","name":"b","input":{}}
                ]
            }))
            .unwrap_err(),
            ProviderError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn streams_tool_json_and_reconstructs_a_message() {
        let server = MockServer::start().await;
        let stream = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"content\":[],\"stop_reason\":null,\"usage\":{\"input_tokens\":7,\"output_tokens\":1}}}\n\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tool_1\",\"name\":\"read_file\",\"input\":{}}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\":\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"src/lib.rs\\\"}\"}}\n\n",
            "event: content_block_stop\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":4}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/messages"))
            .and(body_partial_json(json!({"stream": true})))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(stream),
            )
            .mount(&server)
            .await;

        let sink = RecordingSink::default();
        let clock = AgentEventClock::default();
        let emitter = AgentEventEmitter::new("run-anthropic", 1, &clock, &sink);
        let response = AnthropicProvider::new_with_base_for_test("test-key", server.uri())
            .respond_stream(&request(true), &emitter)
            .await
            .unwrap();

        assert_eq!(response.usage.input_tokens, 7);
        assert_eq!(response.usage.output_tokens, 4);
        assert!(matches!(
            response.output,
            ModelOutput::ToolCalls { calls } if calls[0].arguments["path"] == "src/lib.rs"
        ));
        let events = sink.0.lock().unwrap();
        assert!(events
            .iter()
            .any(|event| matches!(event.kind, AgentEventKind::ToolCallStarted { .. })));
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
            2
        );
        assert!(matches!(
            events.last().map(|event| &event.kind),
            Some(AgentEventKind::ModelResponseCompleted)
        ));
    }
}
