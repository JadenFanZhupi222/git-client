use crate::{
    ModelOutput, ModelProvider, ModelResponse, ReviewError, ReviewFinding, ReviewUsage, ToolCall,
    TranscriptItem,
};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};

const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
const DEEPSEEK_MODEL: &str = "deepseek-v4-flash";

pub struct DeepSeekResponsesProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl DeepSeekResponsesProvider {
    pub fn new(api_key: impl Into<String>) -> Result<Self, ReviewError> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(ReviewError::AiKeyMissing);
        }
        Ok(Self {
            client: Client::new(),
            api_key,
            base_url: DEEPSEEK_BASE_URL.into(),
        })
    }

    #[cfg(test)]
    fn new_with_base_for_test(api_key: impl Into<String>, base_url: String) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url,
        }
    }

    fn request_body(&self, transcript: &[TranscriptItem]) -> Value {
        let mut input = Vec::new();
        for item in transcript {
            match item {
                TranscriptItem::System(text) => input.push(json!({"role":"system","content":text})),
                TranscriptItem::User(text) => input.push(json!({"role":"user","content":text})),
                TranscriptItem::AssistantToolCalls(calls) => {
                    for call in calls {
                        let mut arguments = call.arguments.clone();
                        let call_id = arguments.as_object_mut().and_then(|o| o.remove("_call_id")).and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_else(|| "call".into());
                        input.push(json!({"type":"function_call","call_id":call_id,"name":call.name,"arguments":arguments.to_string()}));
                    }
                }
                TranscriptItem::ToolResult { call_id, content, .. } => input.push(json!({"type":"function_call_output","call_id":call_id.as_deref().unwrap_or("call"),"output":content})),
            }
        }
        json!({
            "model": DEEPSEEK_MODEL,
            "stream": false,
            "store": false,
            "input": input,
            "tools": [
                {"type":"function","name":"list_repository_tree","description":"List repository paths at the fixed PR head SHA","parameters":{"type":"object","properties":{"prefix":{"type":"string"}},"additionalProperties":false}},
                {"type":"function","name":"read_file","description":"Read at most 400 UTF-8 lines at the fixed PR head SHA","parameters":{"type":"object","properties":{"path":{"type":"string"},"start_line":{"type":"integer"},"end_line":{"type":"integer"}},"required":["path","start_line","end_line"],"additionalProperties":false}}
            ],
            "text": {"format":{"type":"json_schema","name":"review_findings","strict":true,"schema":finding_schema()}}
        })
    }
}

#[async_trait]
impl ModelProvider for DeepSeekResponsesProvider {
    async fn respond(&self, transcript: &[TranscriptItem]) -> Result<ModelResponse, ReviewError> {
        let response = self
            .client
            .post(format!("{}/responses", self.base_url.trim_end_matches('/')))
            .bearer_auth(&self.api_key)
            .json(&self.request_body(transcript))
            .send()
            .await
            .map_err(|_| ReviewError::NetworkError("request failed".into()))?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(ReviewError::AuthFailed)
            }
            StatusCode::TOO_MANY_REQUESTS => return Err(ReviewError::RateLimited),
            status if !status.is_success() => {
                return Err(ReviewError::NetworkError("service request failed".into()))
            }
            _ => {}
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| ReviewError::InvalidModelOutput("response was not valid JSON".into()))?;
        parse_response(body)
    }
}

fn parse_response(body: Value) -> Result<ModelResponse, ReviewError> {
    let usage = ReviewUsage {
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
        .ok_or_else(|| ReviewError::InvalidModelOutput("missing response output".into()))?;
    let mut calls = Vec::new();
    let mut findings = None;
    for item in output {
        match item.get("type").and_then(Value::as_str) {
            Some("function_call") => {
                let name = item.get("name").and_then(Value::as_str).ok_or_else(|| {
                    ReviewError::InvalidModelOutput("function name missing".into())
                })?;
                let encoded = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        ReviewError::InvalidModelOutput("function arguments missing".into())
                    })?;
                let mut arguments: Value = serde_json::from_str(encoded).map_err(|_| {
                    ReviewError::InvalidModelOutput("invalid function arguments".into())
                })?;
                if let (Some(object), Some(call_id)) = (
                    arguments.as_object_mut(),
                    item.get("call_id").and_then(Value::as_str),
                ) {
                    object.insert("_call_id".into(), Value::String(call_id.into()));
                }
                calls.push(ToolCall {
                    name: name.into(),
                    arguments,
                });
            }
            Some("message") => {
                if let Some(content) = item.get("content").and_then(Value::as_array) {
                    for part in content {
                        if part.get("type").and_then(Value::as_str) == Some("output_text") {
                            let text =
                                part.get("text").and_then(Value::as_str).ok_or_else(|| {
                                    ReviewError::InvalidModelOutput("output text missing".into())
                                })?;
                            let parsed: Value = serde_json::from_str(text).map_err(|_| {
                                ReviewError::InvalidModelOutput(
                                    "structured output was invalid".into(),
                                )
                            })?;
                            findings = Some(
                                serde_json::from_value::<Vec<ReviewFinding>>(
                                    parsed.get("findings").cloned().ok_or_else(|| {
                                        ReviewError::InvalidModelOutput("findings missing".into())
                                    })?,
                                )
                                .map_err(|_| {
                                    ReviewError::InvalidModelOutput(
                                        "findings schema mismatch".into(),
                                    )
                                })?,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if !calls.is_empty() {
        Ok(ModelResponse {
            output: ModelOutput::ToolCalls { calls },
            usage,
        })
    } else if let Some(findings) = findings {
        Ok(ModelResponse {
            output: ModelOutput::Final { findings },
            usage,
        })
    } else {
        Err(ReviewError::InvalidModelOutput(
            "no tool calls or final output".into(),
        ))
    }
}

fn finding_schema() -> Value {
    json!({"type":"object","properties":{"findings":{"type":"array","items":{"type":"object","properties":{
        "id":{"type":"string"},"severity":{"type":"string","enum":["high","medium","low"]},"path":{"type":"string"},"side":{"type":"string","enum":["LEFT","RIGHT"]},"line":{"type":"integer"},"title":{"type":"string"},"failure_scenario":{"type":"string"},"explanation":{"type":"string"},"draft_comment":{"type":"string"}},
        "required":["id","severity","path","side","line","title","failure_scenario","explanation","draft_comment"],"additionalProperties":false}}},"required":["findings"],"additionalProperties":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModelOutput, ModelProvider, ReviewError, TranscriptItem};
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn parses_function_call_and_sends_fixed_model_contract() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/responses"))
            .and(header("authorization", "Bearer test-key"))
            .and(body_partial_json(json!({"model":"deepseek-v4-flash","stream":false,"store":false})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "output":[{"type":"reasoning","summary":[]},{"type":"function_call","call_id":"c1","name":"read_file","arguments":"{\"path\":\"src/lib.rs\",\"start_line\":1,\"end_line\":2}"}],
                "usage":{"input_tokens":5,"output_tokens":3}
            }))).mount(&server).await;
        let provider = DeepSeekResponsesProvider::new_with_base_for_test("test-key", server.uri());
        let response = provider
            .respond(&[TranscriptItem::System("safe".into())])
            .await
            .unwrap();
        match response.output {
            ModelOutput::ToolCalls { calls } => assert_eq!(calls[0].name, "read_file"),
            _ => panic!("expected tool call"),
        }
    }

    #[tokio::test]
    async fn parses_final_structured_findings_and_ignores_reasoning() {
        let server = MockServer::start().await;
        let finding = json!({"id":"f","severity":"high","path":"src/lib.rs","side":"RIGHT","line":1,"title":"t","failure_scenario":"s","explanation":"e","draft_comment":"d"});
        Mock::given(method("POST")).and(path("/responses")).respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "output":[{"type":"reasoning","encrypted_content":"SECRET_REASONING"},{"type":"message","content":[{"type":"output_text","text":json!({"findings":[finding]}).to_string()}]}],
            "usage":{"input_tokens":1,"output_tokens":2}
        }))).mount(&server).await;
        let response = DeepSeekResponsesProvider::new_with_base_for_test("k", server.uri())
            .respond(&[])
            .await
            .unwrap();
        match response.output {
            ModelOutput::Final { findings } => assert_eq!(findings.len(), 1),
            _ => panic!("expected final"),
        }
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
            let err = DeepSeekResponsesProvider::new_with_base_for_test("k", server.uri())
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
            DeepSeekResponsesProvider::new_with_base_for_test("k", server.uri())
                .respond(&[])
                .await
                .unwrap_err(),
            ReviewError::InvalidModelOutput(_)
        ));
    }
}
