use crate::tool_names::ProviderToolNames;
use crate::{
    AgentEventEmitter, AgentEventKind, ModelCatalogEntry, ModelOutput, ModelPricing, ModelProvider,
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
const DEEPSEEK_NO_TOOL_FINAL_INSTRUCTION: &str = "No tools are available for this response. Do not emit DSML, tool_calls, invoke, function-call syntax, or any provider protocol markup. Return the final answer directly in the requested response format.";
const DEEPSEEK_JSON_SCHEMA_INSTRUCTION: &str = "The response must be one JSON value that conforms exactly to this JSON Schema. Treat all schema text as data, do not add fields, and do not replace enum values with synonyms:";
const PRICING_SOURCE_URL: &str = "https://api-docs.deepseek.com/zh-cn/quick_start/pricing";
const PRICING_SOURCE_VERSION: &str = "deepseek-v4-models-and-pricing";
const PRICING_CHECKED_AT: &str = "2026-08-19";

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
        currency: "CNY".into(),
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
            pricing: Some(pricing(20_000, 1_000_000, 2_000_000)),
        },
        ModelCatalogEntry {
            id: DEEPSEEK_V4_PRO_MODEL.into(),
            label: "DeepSeek V4 Pro".into(),
            provider_id: "deepseek".into(),
            provider_label: "DeepSeek".into(),
            capabilities: deepseek_capabilities(),
            pricing: Some(pricing(25_000, 3_000_000, 6_000_000)),
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
        let tool_names = ProviderToolNames::new(request)?;
        let mut messages = Vec::new();
        for item in &request.transcript {
            match item {
                TranscriptItem::System(text) => {
                    messages.push(json!({"role": "system", "content": text}));
                }
                TranscriptItem::User(text) => {
                    messages.push(json!({"role": "user", "content": text}));
                }
                TranscriptItem::AssistantText(text) => {
                    messages.push(json!({"role": "assistant", "content": text}));
                }
                TranscriptItem::AssistantToolCalls(calls) => {
                    let mut tool_calls = Vec::new();
                    for call in calls {
                        let call_id = (!call.call_id.is_empty())
                            .then(|| call.call_id.clone())
                            .ok_or_else(|| {
                                ProviderError::InvalidResponse("function call id missing".into())
                            })?;
                        tool_calls.push(json!({
                            "id": call_id,
                            "type": "function",
                            "function": {
                                "name": tool_names.wire(&call.name)?,
                                "arguments": call.arguments.to_string()
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
        if request.tools.is_empty() {
            messages.push(json!({
                "role": "system",
                "content": DEEPSEEK_NO_TOOL_FINAL_INSTRUCTION
            }));
        }
        if request.response_format == ResponseFormat::JsonObject {
            if let Some(schema) = &request.response_schema {
                messages.push(json!({
                    "role": "system",
                    "content": format!("{DEEPSEEK_JSON_SCHEMA_INSTRUCTION}\n{schema}")
                }));
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
                            Ok(json!({
                                "type": "function",
                                "function": {
                                    "name": tool_names.wire(&tool.name)?,
                                    "description": tool.description,
                                    "parameters": tool.input_schema
                                }
                            }))
                        })
                        .collect::<Result<Vec<_>, ProviderError>>()?,
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
        let tool_names = ProviderToolNames::new(request)?;
        tracing::info!(
            provider = "deepseek",
            model = %self.model,
            stream = false,
            transcript_items = request.transcript.len(),
            tool_definitions = request.tools.len(),
            "model request started"
        );
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
        tracing::info!(
            provider = "deepseek",
            model = %self.model,
            stream = false,
            http_status = response.status().as_u16(),
            "model request returned"
        );
        map_status(response.status())?;
        let bytes = response
            .bytes()
            .await
            .map_err(|_| ProviderError::Network("response body could not be read".into()))?;
        let body = serde_json::from_slice::<Value>(&bytes)
            .map_err(|_| ProviderError::Network("service returned an invalid response".into()))?;
        tool_names.restore_response(parse_response(body)?)
    }

    async fn respond_stream(
        &self,
        request: &ModelRequest,
        events: &AgentEventEmitter<'_>,
    ) -> Result<ModelResponse, ProviderError> {
        let tool_names = ProviderToolNames::new(request)?;
        tracing::info!(
            provider = "deepseek",
            model = %self.model,
            stream = true,
            transcript_items = request.transcript.len(),
            tool_definitions = request.tools.len(),
            "model request started"
        );
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
        tracing::info!(
            provider = "deepseek",
            model = %self.model,
            stream = true,
            http_status = response.status().as_u16(),
            "model request returned"
        );
        map_status(response.status())?;

        let mut started = false;
        let mut completed = false;
        let mut content = String::new();
        let mut content_filter = StreamedContentFilter::default();
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
                    if let Some(delta) = content_filter.push(part) {
                        events.emit(AgentEventKind::OutputTextDelta { delta });
                    }
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
                    if !target.started
                        && !target.id.is_empty()
                        && tool_names.canonical(&target.name).is_some()
                    {
                        target.started = true;
                        events.emit(AgentEventKind::ToolCallStarted {
                            call_id: target.id.clone(),
                            name: tool_names
                                .canonical(&target.name)
                                .expect("tool name checked above")
                                .to_owned(),
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
        let response = tool_names.restore_response(parse_response(json!({
            "choices": [{"finish_reason": finish_reason, "message": message}],
            "usage": usage
        }))?)?;
        if matches!(response.output, ModelOutput::FinalText { .. }) {
            if let Some(delta) = content_filter.finish() {
                events.emit(AgentEventKind::OutputTextDelta { delta });
            }
        }
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StreamedContentMode {
    #[default]
    LineStart,
    Text,
    Protocol,
}

#[derive(Debug, Default)]
struct StreamedContentFilter {
    mode: StreamedContentMode,
    pending: String,
}

impl StreamedContentFilter {
    fn push(&mut self, part: &str) -> Option<String> {
        if self.mode == StreamedContentMode::Protocol {
            return None;
        }
        self.pending.push_str(part);
        let mut output = String::new();

        loop {
            match self.mode {
                StreamedContentMode::Protocol => {
                    self.pending.clear();
                    break;
                }
                StreamedContentMode::Text => {
                    if let Some(end) = newline_end(&self.pending) {
                        output.push_str(&self.pending[..end]);
                        self.pending.drain(..end);
                        self.mode = StreamedContentMode::LineStart;
                    } else {
                        output.push_str(&self.pending);
                        self.pending.clear();
                        break;
                    }
                }
                StreamedContentMode::LineStart => {
                    let candidate = trim_protocol_leading(&self.pending);
                    if candidate.is_empty() {
                        break;
                    }
                    if candidate.starts_with('<') {
                        if let Some(end) = candidate.find('>') {
                            self.mode = if looks_like_dsml_protocol(&candidate[..=end]) {
                                StreamedContentMode::Protocol
                            } else {
                                StreamedContentMode::Text
                            };
                        } else if candidate.contains('\n') {
                            self.mode = if looks_like_dsml_protocol(candidate) {
                                StreamedContentMode::Protocol
                            } else {
                                StreamedContentMode::Text
                            };
                        } else {
                            break;
                        }
                    } else {
                        self.mode = StreamedContentMode::Text;
                    }
                }
            }
        }

        (!output.is_empty()).then_some(output)
    }

    fn finish(&mut self) -> Option<String> {
        if self.mode == StreamedContentMode::Protocol || self.pending.is_empty() {
            self.pending.clear();
            return None;
        }
        Some(std::mem::take(&mut self.pending))
    }
}

fn newline_end(text: &str) -> Option<usize> {
    text.find('\n').map(|index| index + 1)
}

fn map_status(status: StatusCode) -> Result<(), ProviderError> {
    let result = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ProviderError::AuthFailed),
        StatusCode::PAYMENT_REQUIRED => Err(ProviderError::QuotaExceeded),
        StatusCode::TOO_MANY_REQUESTS => Err(ProviderError::RateLimited),
        status if status.is_client_error() => Err(ProviderError::InvalidRequest),
        status if !status.is_success() => {
            Err(ProviderError::Network("service request failed".into()))
        }
        _ => Ok(()),
    };
    if let Err(error) = &result {
        let error_code = match error {
            ProviderError::CredentialMissing => "credential_missing",
            ProviderError::AuthFailed => "authentication_failed",
            ProviderError::QuotaExceeded => "quota_exceeded",
            ProviderError::InvalidRequest => "invalid_request",
            ProviderError::RateLimited => "rate_limited",
            ProviderError::Network(_) => "network",
            ProviderError::OutputTruncated => "output_truncated",
            ProviderError::InvalidResponse(_) => "invalid_response",
        };
        tracing::warn!(
            provider = "deepseek",
            http_status = status.as_u16(),
            error_code,
            "model request failed"
        );
    }
    result
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
        match parse_dsml_tool_calls(text)? {
            Some(calls) => {
                tracing::warn!(
                    provider = "deepseek",
                    tool_calls = calls.len(),
                    had_text_prefix = dsml_has_text_prefix(text),
                    "recovered complete DSML tool calls from text output"
                );
                Ok(ModelResponse::tool_calls(calls, usage))
            }
            None => Ok(ModelResponse::final_text(text, usage)),
        }
    } else {
        Err(ProviderError::InvalidResponse(
            "no tool calls or final output".into(),
        ))
    }
}

const MAX_DSML_TOOL_CALLS: usize = 64;
const MAX_DSML_PARAMETERS: usize = 128;

/// DeepSeek V4 natively encodes tool calls as DSML. The hosted compatibility
/// layer normally converts them into `message.tool_calls`, but can occasionally
/// return the complete markup as text. Only a fully consumed, strictly formed
/// block is recovered here; partial or orphaned protocol is never executable.
fn parse_dsml_tool_calls(text: &str) -> Result<Option<Vec<ToolCall>>, ProviderError> {
    let normalized = normalize_dsml_tags(text);
    let Some(candidate) = dsml_protocol_suffix(&normalized) else {
        return if looks_like_dsml_protocol(text) {
            Err(ProviderError::InvalidResponse(
                "incomplete provider tool protocol".into(),
            ))
        } else {
            Ok(None)
        };
    };
    let candidate = candidate.trim_end();
    if candidate.starts_with("<|DSML|tool_calls>") {
        return parse_dsml_block(candidate).map(Some);
    }
    if candidate.starts_with("<|DSML") {
        return Err(ProviderError::InvalidResponse(
            "incomplete provider tool protocol".into(),
        ));
    }
    Ok(None)
}

fn dsml_protocol_suffix(text: &str) -> Option<&str> {
    let mut line = text;
    loop {
        let candidate = trim_protocol_leading(line);
        if candidate.starts_with("<|DSML") {
            return Some(candidate);
        }
        let (_, remaining) = line.split_once('\n')?;
        line = remaining;
    }
}

fn dsml_has_text_prefix(text: &str) -> bool {
    let normalized = normalize_dsml_tags(text);
    dsml_protocol_suffix(&normalized).is_some_and(|suffix| {
        let prefix_bytes = normalized.len().saturating_sub(suffix.len());
        !normalized[..prefix_bytes].trim().is_empty()
    })
}

fn trim_protocol_leading(text: &str) -> &str {
    text.trim_start_matches(|character: char| {
        character.is_whitespace() || is_ignored_protocol_format(character)
    })
}

fn looks_like_dsml_protocol(text: &str) -> bool {
    let compact = text
        .chars()
        .filter_map(|character| {
            if character.is_whitespace() || is_ignored_protocol_format(character) {
                None
            } else if is_vertical_line(character) {
                Some('|')
            } else {
                Some(character.to_ascii_lowercase())
            }
        })
        .collect::<String>();
    compact.contains('<')
        && compact.contains("dsml")
        && (compact.contains("tool_calls")
            || compact.contains("invoke")
            || compact.contains("parameter"))
}

fn normalize_dsml_tags(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some(open) = remaining.find('<') {
        normalized.push_str(&remaining[..open]);
        let tag = &remaining[open..];
        let Some(close) = tag.find('>') else {
            normalized.push_str(tag);
            return normalized;
        };
        let (tag, tail) = tag.split_at(close + 1);
        if looks_like_dsml_protocol(tag) {
            normalized.push_str(&normalize_dsml_tag(tag));
        } else {
            normalized.push_str(tag);
        }
        remaining = tail;
    }
    normalized.push_str(remaining);
    normalized
}

fn normalize_dsml_tag(tag: &str) -> String {
    let mut normalized = String::with_capacity(tag.len());
    let mut quoted = false;
    for character in tag.chars() {
        if is_ignored_protocol_format(character) {
            continue;
        }
        let character = if is_vertical_line(character) {
            '|'
        } else {
            character
        };
        if character == '"' {
            quoted = !quoted;
            normalized.push(character);
        } else if !quoted && character.is_whitespace() {
            continue;
        } else {
            normalized.push(character);
        }
    }
    normalized
}

fn is_vertical_line(character: char) -> bool {
    matches!(
        character,
        '|' | '｜' | '∣' | '丨' | '︱' | '￨' | '¦' | 'ǀ' | '│' | '❘' | '⏐' | '‖' | '∥'
    )
}

fn is_ignored_protocol_format(character: char) -> bool {
    matches!(
        character,
        '\u{feff}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}'
    )
}

fn parse_dsml_block(text: &str) -> Result<Vec<ToolCall>, ProviderError> {
    const OPEN: &str = "<|DSML|tool_calls>";
    const CLOSE: &str = "</|DSML|tool_calls>";
    const INVOKE_OPEN: &str = "<|DSML|invokename=\"";
    const INVOKE_CLOSE: &str = "</|DSML|invoke>";
    const PARAMETER_OPEN: &str = "<|DSML|parametername=\"";
    const PARAMETER_CLOSE: &str = "</|DSML|parameter>";
    let mut remaining = text.strip_prefix(OPEN).ok_or_else(invalid_dsml)?;
    let mut calls = Vec::new();

    loop {
        remaining = remaining.trim_start();
        if let Some(tail) = remaining.strip_prefix(CLOSE) {
            if !tail.trim().is_empty() || calls.is_empty() {
                return Err(invalid_dsml());
            }
            return Ok(calls);
        }
        if calls.len() >= MAX_DSML_TOOL_CALLS {
            return Err(invalid_dsml());
        }
        remaining = remaining
            .strip_prefix(INVOKE_OPEN)
            .ok_or_else(invalid_dsml)?;
        let (name, tail) = remaining.split_once("\">").ok_or_else(invalid_dsml)?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(invalid_dsml());
        }
        remaining = tail;
        let mut arguments = serde_json::Map::new();

        loop {
            remaining = remaining.trim_start();
            if let Some(tail) = remaining.strip_prefix(INVOKE_CLOSE) {
                remaining = tail;
                break;
            }
            if arguments.len() >= MAX_DSML_PARAMETERS {
                return Err(invalid_dsml());
            }
            remaining = remaining
                .strip_prefix(PARAMETER_OPEN)
                .ok_or_else(invalid_dsml)?;
            let (parameter_name, tail) = remaining
                .split_once("\"string=\"")
                .ok_or_else(invalid_dsml)?;
            if parameter_name.is_empty() || arguments.contains_key(parameter_name) {
                return Err(invalid_dsml());
            }
            let (is_string, tail) = if let Some(tail) = tail.strip_prefix("true\">") {
                (true, tail)
            } else if let Some(tail) = tail.strip_prefix("false\">") {
                (false, tail)
            } else {
                return Err(invalid_dsml());
            };
            let (encoded, tail) = tail.split_once(PARAMETER_CLOSE).ok_or_else(invalid_dsml)?;
            let value = if is_string {
                Value::String(encoded.to_owned())
            } else {
                serde_json::from_str(encoded.trim()).map_err(|_| invalid_dsml())?
            };
            arguments.insert(parameter_name.to_owned(), value);
            remaining = tail;
        }

        let call_id = format!(
            "call_dsml_{}_{:016x}",
            calls.len(),
            dsml_call_hash(name, &arguments)
        );
        calls.push(ToolCall::with_call_id(
            name,
            call_id,
            Value::Object(arguments),
        ));
    }
}

fn invalid_dsml() -> ProviderError {
    ProviderError::InvalidResponse("invalid provider tool protocol".into())
}

fn dsml_call_hash(name: &str, arguments: &serde_json::Map<String, Value>) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in name
        .bytes()
        .chain(serde_json::to_vec(arguments).unwrap_or_default())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn deepseek_usage(body: &Value) -> ModelUsage {
    ModelUsage {
        input_tokens: body
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cached_input_tokens: body
            .pointer("/usage/prompt_tokens_details/cached_tokens")
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
                    risk: crate::ToolRisk::ReadOnly,
                    timeout_ms: crate::default_tool_timeout_ms(),
                    max_result_bytes: crate::default_tool_result_bytes(),
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
            1_000_000
        );
        assert_eq!(
            catalog[0].pricing.as_ref().unwrap().checked_at,
            "2026-08-19"
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
            TranscriptItem::AssistantText("Earlier answer".into()),
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
            body.pointer("/messages/0/content"),
            Some(&json!("Earlier answer"))
        );
        assert_eq!(
            body.pointer("/messages/1/tool_calls/0/id"),
            Some(&json!("call-1"))
        );
        assert_eq!(body.pointer("/messages/2/role"), Some(&json!("tool")));
        assert!(body.get("tools").is_some());

        let body = provider.request_body(&request(history, false)).unwrap();
        assert!(body.get("tools").is_none());
        let messages = body.get("messages").and_then(Value::as_array).unwrap();
        assert_eq!(messages.last().unwrap()["role"], "system");
        assert_eq!(
            messages.last().unwrap()["content"],
            DEEPSEEK_NO_TOOL_FINAL_INSTRUCTION
        );
    }

    #[test]
    fn conveys_provider_neutral_json_schema_to_deepseek() {
        let provider = DeepSeekProvider::new_with_base_for_test("k", "http://localhost".into());
        let mut model_request = request(vec![TranscriptItem::User("verify".into())], false);
        model_request.response_schema = Some(json!({
            "type": "object",
            "properties": {
                "decision": {"type": "string", "enum": ["accepted", "continue", "blocked"]}
            },
            "required": ["decision"],
            "additionalProperties": false
        }));

        let body = provider.request_body(&model_request).unwrap();
        let messages = body.get("messages").and_then(Value::as_array).unwrap();
        let schema_instruction = messages.last().unwrap()["content"].as_str().unwrap();
        assert!(schema_instruction.starts_with(DEEPSEEK_JSON_SCHEMA_INSTRUCTION));
        assert!(schema_instruction.contains("accepted"));
        assert!(schema_instruction.contains("additionalProperties"));
        assert_eq!(
            body.pointer("/response_format/type"),
            Some(&json!("json_object"))
        );
    }

    #[test]
    fn encodes_provider_neutral_tool_names_on_the_wire() {
        let provider = DeepSeekProvider::new_with_base_for_test("k", "http://localhost".into());
        let mut request = request(
            vec![TranscriptItem::AssistantToolCalls(vec![
                ToolCall::with_call_id("filesystem.read", "call-1", json!({"path": "src/lib.rs"})),
            ])],
            true,
        );
        request.tools[0].name = "filesystem.read".into();

        let body = provider.request_body(&request).unwrap();
        let definition_name = body
            .pointer("/tools/0/function/name")
            .and_then(Value::as_str)
            .unwrap();
        let replay_name = body
            .pointer("/messages/0/tool_calls/0/function/name")
            .and_then(Value::as_str)
            .unwrap();
        assert_eq!(definition_name, replay_name);
        assert!(!definition_name.contains('.'));
        assert!(definition_name.len() <= 64);
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

    #[tokio::test]
    async fn restores_wire_tool_names_before_returning_a_response() {
        let server = MockServer::start().await;
        let mut model_request = request(Vec::new(), true);
        model_request.tools[0].name = "filesystem.read".into();
        let wire_name = ProviderToolNames::new(&model_request)
            .unwrap()
            .wire("filesystem.read")
            .unwrap()
            .to_owned();
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_partial_json(json!({
                "tools": [{"function": {"name": wire_name}}]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"finish_reason": "tool_calls", "message": {
                    "tool_calls": [{"id": "c1", "function": {
                        "name": wire_name,
                        "arguments": "{\"path\":\"src/lib.rs\"}"
                    }}]
                }}]
            })))
            .mount(&server)
            .await;

        let response = DeepSeekProvider::new_with_base_for_test("test-key", server.uri())
            .respond(&model_request)
            .await
            .unwrap();
        assert!(matches!(
            response.output,
            ModelOutput::ToolCalls { calls } if calls[0].name == "filesystem.read"
        ));
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

    #[test]
    fn recovers_only_complete_dsml_tool_calls() {
        let text = concat!(
            "<｜DSML｜tool_calls>\n",
            "<｜DSML｜invoke name=\"filesystem_list_wire\">\n",
            "<｜DSML｜parameter name=\"path\" string=\"true\">crates/agent-session/src</｜DSML｜parameter>\n",
            "<｜DSML｜parameter name=\"depth\" string=\"false\">2</｜DSML｜parameter>\n",
            "</｜DSML｜invoke>\n",
            "</｜DSML｜tool_calls>"
        );
        let calls = parse_dsml_tool_calls(text).unwrap().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "filesystem_list_wire");
        assert_eq!(calls[0].arguments["path"], "crates/agent-session/src");
        assert_eq!(calls[0].arguments["depth"], 2);
        assert!(calls[0].call_id.starts_with("call_dsml_0_"));

        let prefixed = format!("I need one more repository read.\n\n{text}");
        let calls = parse_dsml_tool_calls(&prefixed).unwrap().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "filesystem_list_wire");

        for malformed in [
            "<｜DSML｜invoke name=\"filesystem_list_wire\"></｜DSML｜invoke>",
            "<｜DSML｜tool_calls><｜DSML｜invoke name=\"filesystem_list_wire\"><｜DSML｜parameter name=\"path\" string=\"true\">src</｜DSML｜invoke></｜DSML｜tool_calls>",
            "<｜DSML｜tool_calls><｜DSML｜invoke name=\"filesystem_list_wire\"></｜DSML｜invoke></｜DSML｜tool_calls> trailing",
            "I need one more repository read.\n<｜DSML｜invoke name=\"filesystem_list_wire\"></｜DSML｜invoke>",
        ] {
            assert!(matches!(
                parse_dsml_tool_calls(malformed),
                Err(ProviderError::InvalidResponse(_))
            ));
        }
        assert!(matches!(
            parse_dsml_tool_calls("The repository mentions <｜DSML｜tool_calls> in its docs."),
            Err(ProviderError::InvalidResponse(_))
        ));
        assert_eq!(
            parse_dsml_tool_calls("The repository discusses the DSML format.").unwrap(),
            None
        );

        let variant = concat!(
            "\u{feff}< ∣ DSML ∣ tool_calls >\n",
            "< ∣ DSML ∣ invoke   name = \"filesystem_list_wire\" >\n",
            "< ∣ DSML ∣ parameter name = \"path\" string = \"true\" >src</ ∣ DSML ∣ parameter >\n",
            "</ ∣ DSML ∣ invoke >\n",
            "</ ∣ DSML ∣ tool_calls >"
        );
        let calls = parse_dsml_tool_calls(variant).unwrap().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "filesystem_list_wire");
        assert_eq!(calls[0].arguments["path"], "src");
    }

    #[test]
    fn streamed_filter_hides_spaced_unicode_dsml_variants() {
        let mut filter = StreamedContentFilter::default();
        assert_eq!(
            filter.push("Visible explanation.\n\u{200b}< ∣ DS"),
            Some("Visible explanation.\n".into())
        );
        assert_eq!(filter.push("ML ∣ tool_calls >secret protocol"), None);
        assert_eq!(filter.finish(), None);
    }

    #[tokio::test]
    async fn restores_complete_dsml_leaks_to_the_provider_neutral_contract() {
        let server = MockServer::start().await;
        let mut model_request = request(Vec::new(), true);
        model_request.tools[0].name = "filesystem.list".into();
        let wire_name = ProviderToolNames::new(&model_request)
            .unwrap()
            .wire("filesystem.list")
            .unwrap()
            .to_owned();
        let leaked = format!(
            "<｜DSML｜tool_calls><｜DSML｜invoke name=\"{wire_name}\"><｜DSML｜parameter name=\"path\" string=\"true\">crates</｜DSML｜parameter></｜DSML｜invoke></｜DSML｜tool_calls>"
        );
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"finish_reason": "stop", "message": {"content": leaked}}]
            })))
            .mount(&server)
            .await;

        let response = DeepSeekProvider::new_with_base_for_test("test-key", server.uri())
            .respond(&model_request)
            .await
            .unwrap();
        assert!(matches!(
            response.output,
            ModelOutput::ToolCalls { calls }
                if calls[0].name == "filesystem.list"
                    && calls[0].arguments["path"] == "crates"
        ));
    }

    #[tokio::test]
    async fn maps_http_statuses_invalid_json_and_timeout() {
        for (status, expected) in [
            (401, ProviderError::AuthFailed),
            (403, ProviderError::AuthFailed),
            (400, ProviderError::InvalidRequest),
            (402, ProviderError::QuotaExceeded),
            (422, ProviderError::InvalidRequest),
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

    #[tokio::test]
    async fn buffers_streamed_dsml_until_the_complete_call_is_validated() {
        let server = MockServer::start().await;
        let mut model_request = request(Vec::new(), true);
        model_request.tools[0].name = "filesystem.list".into();
        let wire_name = ProviderToolNames::new(&model_request)
            .unwrap()
            .wire("filesystem.list")
            .unwrap()
            .to_owned();
        let first = "<｜DSM";
        let second = format!(
            "L｜tool_calls><｜DSML｜invoke name=\"{wire_name}\"><｜DSML｜parameter name=\"path\" string=\"true\">crates</｜DSML｜parameter></｜DSML｜invoke></｜DSML｜tool_calls>"
        );
        let stream = [
            json!({"id":"chat_dsml","choices":[{"delta":{"content":first},"finish_reason":null}],"usage":null}),
            json!({"id":"chat_dsml","choices":[{"delta":{"content":second},"finish_reason":null}],"usage":null}),
            json!({"id":"chat_dsml","choices":[{"delta":{},"finish_reason":"stop"}],"usage":null}),
            json!({"id":"chat_dsml","choices":[],"usage":{"prompt_tokens":4,"completion_tokens":2}}),
        ]
        .into_iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect::<String>()
            + "data: [DONE]\n\n";
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(stream),
            )
            .mount(&server)
            .await;

        let sink = RecordingSink::default();
        let clock = AgentEventClock::default();
        let emitter = AgentEventEmitter::new("run-dsml", 1, &clock, &sink);
        let response = DeepSeekProvider::new_with_base_for_test("test-key", server.uri())
            .respond_stream(&model_request, &emitter)
            .await
            .unwrap();

        assert!(matches!(
            response.output,
            ModelOutput::ToolCalls { calls }
                if calls[0].name == "filesystem.list"
                    && calls[0].arguments["path"] == "crates"
        ));
        assert!(!sink
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event.kind, AgentEventKind::OutputTextDelta { .. })));
    }

    #[tokio::test]
    async fn streams_prose_but_never_dsml_when_a_tool_block_follows_it() {
        let server = MockServer::start().await;
        let mut model_request = request(Vec::new(), true);
        model_request.tools[0].name = "filesystem.list".into();
        let wire_name = ProviderToolNames::new(&model_request)
            .unwrap()
            .wire("filesystem.list")
            .unwrap()
            .to_owned();
        let first = "I will inspect the host composition.\n\n<｜DSM";
        let second = format!(
            "L｜tool_calls><｜DSML｜invoke name=\"{wire_name}\"><｜DSML｜parameter name=\"path\" string=\"true\">app/src-tauri</｜DSML｜parameter></｜DSML｜invoke></｜DSML｜tool_calls>"
        );
        let stream = [
            json!({"id":"chat_mixed","choices":[{"delta":{"content":first},"finish_reason":null}],"usage":null}),
            json!({"id":"chat_mixed","choices":[{"delta":{"content":second},"finish_reason":null}],"usage":null}),
            json!({"id":"chat_mixed","choices":[{"delta":{},"finish_reason":"stop"}],"usage":null}),
            json!({"id":"chat_mixed","choices":[],"usage":{"prompt_tokens":5,"completion_tokens":3}}),
        ]
        .into_iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect::<String>()
            + "data: [DONE]\n\n";
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(stream),
            )
            .mount(&server)
            .await;

        let sink = RecordingSink::default();
        let clock = AgentEventClock::default();
        let emitter = AgentEventEmitter::new("run-mixed-dsml", 1, &clock, &sink);
        let response = DeepSeekProvider::new_with_base_for_test("test-key", server.uri())
            .respond_stream(&model_request, &emitter)
            .await
            .unwrap();

        assert!(matches!(
            response.output,
            ModelOutput::ToolCalls { calls }
                if calls[0].name == "filesystem.list"
                    && calls[0].arguments["path"] == "app/src-tauri"
        ));
        let streamed_text = sink
            .0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match &event.kind {
                AgentEventKind::OutputTextDelta { delta } => Some(delta.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(streamed_text, "I will inspect the host composition.\n");
        assert!(!streamed_text.contains("DSML"));
    }
}
