use crate::{
    ModelCatalogEntry, ModelPricing, ModelProvider, ModelRequest, ModelResponse, ModelUsage,
    ProviderCapabilities, ProviderDescriptor, ProviderError, ResponseFormat, ReviewError,
    StructuredOutputSupport, ToolCall, ToolCallingSupport, TranscriptItem, UsageSupport,
};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::collections::HashSet;
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
    let mut arguments = call.arguments.clone();
    let call_id = arguments
        .as_object_mut()
        .and_then(|object| object.remove("_call_id"))
        .and_then(|value| value.as_str().map(str::to_owned))
        .filter(|call_id| !call_id.is_empty())
        .ok_or_else(|| ProviderError::InvalidResponse("function call id missing".into()))?;
    Ok((call_id, arguments))
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
    let input_tokens = [
        "/usage/input_tokens",
        "/usage/cache_creation_input_tokens",
        "/usage/cache_read_input_tokens",
    ]
    .into_iter()
    .filter_map(|pointer| body.pointer(pointer).and_then(Value::as_u64))
    .sum();
    let usage = ModelUsage {
        input_tokens,
        output_tokens: body
            .pointer("/usage/output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        tool_calls: 0,
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModelOutput, ToolDefinition};
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
}
