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

const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const GPT_5_6_SOL_MODEL: &str = "gpt-5.6-sol";
pub const GPT_5_6_TERRA_MODEL: &str = "gpt-5.6-terra";
pub const GPT_5_6_LUNA_MODEL: &str = "gpt-5.6-luna";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const PRICING_SOURCE_URL: &str = "https://developers.openai.com/api/docs/models";
const PRICING_SOURCE_VERSION: &str = "gpt-5.6-model-catalog";
const PRICING_CHECKED_AT: &str = "2026-08-08";

fn openai_capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        structured_output: StructuredOutputSupport::JsonSchema,
        tool_calling: ToolCallingSupport::Serial,
        can_disable_tools: true,
        // The adapter explicitly disables reasoning so the provider-neutral transcript does not
        // need to retain encrypted or provider-specific reasoning items between tool rounds.
        requires_reasoning_replay: false,
        context_window_tokens: 1_050_000,
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

pub fn openai_model_catalog() -> Vec<ModelCatalogEntry> {
    vec![
        ModelCatalogEntry {
            id: GPT_5_6_TERRA_MODEL.into(),
            label: "GPT-5.6 Terra".into(),
            provider_id: "openai".into(),
            provider_label: "OpenAI".into(),
            capabilities: openai_capabilities(),
            pricing: Some(pricing(250_000, 2_500_000, 15_000_000)),
        },
        ModelCatalogEntry {
            id: GPT_5_6_LUNA_MODEL.into(),
            label: "GPT-5.6 Luna".into(),
            provider_id: "openai".into(),
            provider_label: "OpenAI".into(),
            capabilities: openai_capabilities(),
            pricing: Some(pricing(100_000, 1_000_000, 6_000_000)),
        },
        ModelCatalogEntry {
            id: GPT_5_6_SOL_MODEL.into(),
            label: "GPT-5.6 Sol".into(),
            provider_id: "openai".into(),
            provider_label: "OpenAI".into(),
            capabilities: openai_capabilities(),
            pricing: Some(pricing(500_000, 5_000_000, 30_000_000)),
        },
    ]
}

pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenAiProvider {
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
            GPT_5_6_SOL_MODEL | GPT_5_6_TERRA_MODEL | GPT_5_6_LUNA_MODEL
        ) {
            return Err(ReviewError::InvalidModelOutput(
                "unsupported OpenAI model".into(),
            ));
        }
        Ok(Self {
            client: build_client(CONNECT_TIMEOUT, REQUEST_TIMEOUT)?,
            api_key,
            base_url: OPENAI_BASE_URL.into(),
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
            model: GPT_5_6_TERRA_MODEL.into(),
        }
    }

    fn request_body(&self, request: &ModelRequest) -> Result<Value, ProviderError> {
        let mut instructions = Vec::new();
        let mut input = Vec::new();
        for item in &request.transcript {
            match item {
                TranscriptItem::System(text) => instructions.push(text.clone()),
                TranscriptItem::User(text) => input.push(json!({
                    "role": "user",
                    "content": text
                })),
                TranscriptItem::AssistantToolCalls(calls) => {
                    for call in calls {
                        let (call_id, arguments) = split_call_id(call)?;
                        input.push(json!({
                            "type": "function_call",
                            "call_id": call_id,
                            "name": call.name,
                            "arguments": arguments.to_string(),
                            "status": "completed"
                        }));
                    }
                }
                TranscriptItem::ToolResult {
                    call_id, content, ..
                } => {
                    if call_id.is_empty() {
                        return Err(ProviderError::InvalidResponse(
                            "function call id missing".into(),
                        ));
                    }
                    input.push(json!({
                        "type": "function_call_output",
                        "call_id": call_id,
                        "output": content
                    }));
                }
            }
        }

        let mut body = json!({
            "model": self.model,
            "input": input,
            "max_output_tokens": request.max_output_tokens,
            "reasoning": {"effort": "none"},
            "parallel_tool_calls": false,
            "store": false
        });
        let object = body
            .as_object_mut()
            .expect("Responses request body is an object");
        if !instructions.is_empty() {
            object.insert(
                "instructions".into(),
                Value::String(instructions.join("\n\n")),
            );
        }
        if request.response_format == ResponseFormat::JsonObject {
            let format = match &request.response_schema {
                Some(schema) => json!({
                    "type": "json_schema",
                    "name": "agent_result",
                    "strict": true,
                    "schema": schema
                }),
                None => json!({"type": "json_object"}),
            };
            object.insert("text".into(), json!({"format": format}));
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
                                "name": tool.name,
                                "description": tool.description,
                                "parameters": tool.input_schema,
                                "strict": false
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
            .expect("Responses request body is an object")
            .insert("stream".into(), Value::Bool(true));
        Ok(body)
    }
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
        .read_timeout(request_timeout)
        .build()
        .map_err(|_| ReviewError::NetworkError("could not initialize HTTP client".into()))
}

#[async_trait]
impl ModelProvider for OpenAiProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: "openai".into(),
            model_id: self.model.clone(),
            capabilities: openai_capabilities(),
        }
    }

    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, ProviderError> {
        let response = self
            .client
            .post(format!("{}/responses", self.base_url.trim_end_matches('/')))
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
            .post(format!("{}/responses", self.base_url.trim_end_matches('/')))
            .bearer_auth(&self.api_key)
            .json(&self.streaming_request_body(request)?)
            .send()
            .await
            .map_err(|_| ProviderError::Network("request failed".into()))?;
        map_status(response.status())?;

        let mut terminal_response = None;
        let mut terminal_error = None;
        let mut item_call_ids = HashMap::<String, String>::new();
        crate::sse::consume_sse(response.bytes_stream(), |event| {
            let body = serde_json::from_str::<Value>(&event.data)
                .map_err(|_| crate::sse::SseError::Protocol("invalid JSON event".into()))?;
            match body.get("type").and_then(Value::as_str) {
                Some("response.created") => {
                    events.emit(AgentEventKind::ModelResponseStarted {
                        response_id: body
                            .pointer("/response/id")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    });
                }
                Some("response.output_text.delta") => {
                    if let Some(delta) = body.get("delta").and_then(Value::as_str) {
                        if !delta.is_empty() {
                            events.emit(AgentEventKind::OutputTextDelta {
                                delta: delta.to_owned(),
                            });
                        }
                    }
                }
                Some("response.output_item.added")
                    if body.pointer("/item/type").and_then(Value::as_str)
                        == Some("function_call") =>
                {
                    let item_id = body
                        .pointer("/item/id")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let call_id = body
                        .pointer("/item/call_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let name = body
                        .pointer("/item/name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if !item_id.is_empty() && !call_id.is_empty() {
                        if !item_call_ids.contains_key(item_id) && item_call_ids.len() >= 1024 {
                            return Err(crate::sse::SseError::Protocol(
                                "tool call count exceeded limit".into(),
                            ));
                        }
                        item_call_ids.insert(item_id.to_owned(), call_id.to_owned());
                    }
                    if !call_id.is_empty() && !name.is_empty() {
                        events.emit(AgentEventKind::ToolCallStarted {
                            call_id: call_id.to_owned(),
                            name: name.to_owned(),
                        });
                    }
                }
                Some("response.function_call_arguments.delta") => {
                    let call_id = body
                        .get("call_id")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            body.get("item_id")
                                .and_then(Value::as_str)
                                .and_then(|item_id| item_call_ids.get(item_id).map(String::as_str))
                        })
                        .unwrap_or_default();
                    let delta = body
                        .get("delta")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if !call_id.is_empty() && !delta.is_empty() {
                        events.emit(AgentEventKind::ToolArgumentsDelta {
                            call_id: call_id.to_owned(),
                            delta: delta.to_owned(),
                        });
                    }
                }
                Some("response.completed") => {
                    terminal_response = body.get("response").cloned();
                    return Ok(false);
                }
                Some("response.incomplete") => {
                    terminal_error = Some(ProviderError::OutputTruncated);
                    return Ok(false);
                }
                Some("response.failed" | "error") => {
                    terminal_error = Some(ProviderError::InvalidResponse(
                        "model response failed".into(),
                    ));
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
        let response = parse_response(terminal_response.ok_or_else(|| {
            ProviderError::InvalidResponse("stream ended before completion".into())
        })?)?;
        events.emit(AgentEventKind::UsageUpdated {
            usage: response.usage.clone(),
        });
        events.emit(AgentEventKind::ModelResponseCompleted);
        Ok(response)
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
    if body.get("status").and_then(Value::as_str) == Some("incomplete")
        || body
            .pointer("/incomplete_details/reason")
            .and_then(Value::as_str)
            == Some("max_output_tokens")
    {
        return Err(ProviderError::OutputTruncated);
    }
    if body.get("error").is_some_and(|error| !error.is_null()) {
        return Err(ProviderError::InvalidResponse(
            "model response failed".into(),
        ));
    }
    let usage = ModelUsage {
        input_tokens: body
            .pointer("/usage/input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: body
            .pointer("/usage/output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        tool_calls: 0,
    };
    let output = body
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::InvalidResponse("missing response output".into()))?;
    let mut calls = Vec::new();
    let mut call_ids = HashSet::new();
    let mut text = String::new();
    for item in output {
        match item.get("type").and_then(Value::as_str) {
            Some("function_call") => {
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| {
                        ProviderError::InvalidResponse("function name missing".into())
                    })?;
                let call_id = item
                    .get("call_id")
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
                let encoded = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        ProviderError::InvalidResponse("function arguments missing".into())
                    })?;
                let arguments = serde_json::from_str::<Value>(encoded).map_err(|_| {
                    ProviderError::InvalidResponse("invalid function arguments".into())
                })?;
                if !arguments.is_object() {
                    return Err(ProviderError::InvalidResponse(
                        "function arguments must be an object".into(),
                    ));
                }
                calls.push(ToolCall::with_call_id(name, call_id, arguments));
            }
            Some("message") => {
                if let Some(content) = item.get("content").and_then(Value::as_array) {
                    for block in content {
                        if block.get("type").and_then(Value::as_str) == Some("output_text") {
                            if let Some(part) = block.get("text").and_then(Value::as_str) {
                                text.push_str(part);
                            }
                        }
                    }
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
    use crate::{AgentEvent, AgentEventSink, ModelOutput, ToolDefinition};
    use std::sync::atomic::AtomicU64;
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
        let provider = OpenAiProvider::new_with_base_for_test("k", "http://localhost".into());
        assert_eq!(provider.descriptor().provider_id, "openai");
        assert_eq!(provider.descriptor().model_id, GPT_5_6_TERRA_MODEL);
        let catalog = openai_model_catalog();
        assert_eq!(catalog.len(), 3);
        assert!(catalog.iter().all(|entry| entry.provider_id == "openai"));
        assert_eq!(
            catalog[0].pricing.as_ref().unwrap().checked_at,
            "2026-08-08"
        );
        assert_eq!(
            catalog[0]
                .pricing
                .as_ref()
                .unwrap()
                .input_cache_miss_per_million_micros,
            2_500_000
        );
        assert_eq!(
            catalog[0]
                .pricing
                .as_ref()
                .unwrap()
                .output_per_million_micros,
            15_000_000
        );
        assert_eq!(
            catalog[1]
                .pricing
                .as_ref()
                .unwrap()
                .input_cache_miss_per_million_micros,
            1_000_000
        );
        assert_eq!(
            catalog[1]
                .pricing
                .as_ref()
                .unwrap()
                .output_per_million_micros,
            6_000_000
        );
    }

    #[test]
    fn maps_stateless_transcript_and_structured_output_without_reasoning_replay() {
        let provider = OpenAiProvider::new_with_base_for_test("k", "http://localhost".into());
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
        assert_eq!(body.pointer("/reasoning/effort"), Some(&json!("none")));
        assert_eq!(body.pointer("/input/1/type"), Some(&json!("function_call")));
        assert_eq!(
            body.pointer("/input/2/type"),
            Some(&json!("function_call_output"))
        );
        assert_eq!(
            body.pointer("/text/format/type"),
            Some(&json!("json_schema"))
        );
        assert!(body.get("tools").is_some());
    }

    #[tokio::test]
    async fn maps_function_calls_final_text_usage_and_auth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/responses"))
            .and(header("authorization", "Bearer test-key"))
            .and(body_partial_json(json!({"model":GPT_5_6_TERRA_MODEL,"store":false})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status":"completed",
                "output":[{"type":"function_call","call_id":"c1","name":"read_file","arguments":"{\"path\":\"src/lib.rs\"}"}],
                "usage":{"input_tokens":7,"output_tokens":3}
            })))
            .mount(&server)
            .await;
        let response = OpenAiProvider::new_with_base_for_test("test-key", server.uri())
            .respond(&request(true))
            .await
            .unwrap();
        assert_eq!(response.usage.input_tokens, 7);
        assert!(
            matches!(response.output, ModelOutput::ToolCalls { calls } if calls[0].name == "read_file")
        );

        let final_response = parse_response(json!({
            "status":"completed",
            "output":[{"type":"message","content":[{"type":"output_text","text":"{\"summary\":\"done\"}"}]}],
            "usage":{"input_tokens":2,"output_tokens":1}
        })).unwrap();
        assert!(
            matches!(final_response.output, ModelOutput::FinalText { text } if text.contains("done"))
        );

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        assert_eq!(
            OpenAiProvider::new_with_base_for_test("bad", server.uri())
                .respond(&request(false))
                .await
                .unwrap_err(),
            ProviderError::AuthFailed
        );
    }

    #[test]
    fn rejects_truncation_and_duplicate_call_ids() {
        assert_eq!(
            parse_response(json!({"status":"incomplete","output":[]})).unwrap_err(),
            ProviderError::OutputTruncated
        );
        assert!(matches!(
            parse_response(json!({
                "status":"completed",
                "output":[
                    {"type":"function_call","call_id":"same","name":"a","arguments":"{}"},
                    {"type":"function_call","call_id":"same","name":"b","arguments":"{}"}
                ]
            }))
            .unwrap_err(),
            ProviderError::InvalidResponse(_)
        ));
    }

    #[tokio::test]
    async fn streams_text_and_reconstructs_the_canonical_response() {
        let server = MockServer::start().await;
        let stream = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"{\\\"summary\\\":\"}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"done\\\"}\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"{\\\"summary\\\":\\\"done\\\"}\"}]}],\"usage\":{\"input_tokens\":5,\"output_tokens\":2}}}\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/responses"))
            .and(body_partial_json(json!({"stream": true})))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(stream),
            )
            .mount(&server)
            .await;

        let sink = RecordingSink::default();
        let sequence = AtomicU64::new(1);
        let emitter = AgentEventEmitter::new("run-1", 1, &sequence, &sink);
        let response = OpenAiProvider::new_with_base_for_test("test-key", server.uri())
            .respond_stream(&request(false), &emitter)
            .await
            .unwrap();

        assert_eq!(response.usage.input_tokens, 5);
        assert!(matches!(
            response.output,
            ModelOutput::FinalText { ref text } if text == "{\"summary\":\"done\"}"
        ));
        let events = sink.0.lock().unwrap();
        assert!(matches!(
            events.first().map(|event| &event.kind),
            Some(AgentEventKind::ModelResponseStarted { .. })
        ));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.kind, AgentEventKind::OutputTextDelta { .. }))
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
    async fn streams_tool_arguments_without_executing_partial_json() {
        let server = MockServer::start().await;
        let stream = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_2\"}}\n\n",
            "data: {\"type\":\"response.output_item.added\",\"item\":{\"type\":\"function_call\",\"id\":\"item_1\",\"call_id\":\"call_1\",\"name\":\"read_file\"}}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"item_1\",\"delta\":\"{\\\"path\\\":\"}\n\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"item_1\",\"delta\":\"\\\"src/lib.rs\\\"}\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\\\"src/lib.rs\\\"}\"}],\"usage\":{\"input_tokens\":4,\"output_tokens\":3}}}\n\n"
        );
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(stream))
            .mount(&server)
            .await;
        let sink = RecordingSink::default();
        let sequence = AtomicU64::new(1);
        let emitter = AgentEventEmitter::new("run-tools", 1, &sequence, &sink);
        let response = OpenAiProvider::new_with_base_for_test("test-key", server.uri())
            .respond_stream(&request(true), &emitter)
            .await
            .unwrap();
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
    }
}
