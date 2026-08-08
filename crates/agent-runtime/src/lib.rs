use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_calls: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuredOutputSupport {
    None,
    JsonObject,
    JsonSchema,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallingSupport {
    None,
    Serial,
    Parallel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageSupport {
    None,
    InputOutputTokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub structured_output: StructuredOutputSupport,
    pub tool_calling: ToolCallingSupport,
    pub can_disable_tools: bool,
    pub requires_reasoning_replay: bool,
    pub context_window_tokens: u64,
    pub max_output_tokens: u64,
    pub usage: UsageSupport,
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self {
            structured_output: StructuredOutputSupport::None,
            tool_calling: ToolCallingSupport::None,
            can_disable_tools: false,
            requires_reasoning_replay: false,
            context_window_tokens: 0,
            max_output_tokens: 0,
            usage: UsageSupport::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    pub provider_id: String,
    pub model_id: String,
    pub capabilities: ProviderCapabilities,
}

impl ProviderDescriptor {
    pub fn unknown() -> Self {
        Self {
            provider_id: "unknown".into(),
            model_id: "unknown".into(),
            capabilities: ProviderCapabilities::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPricing {
    pub currency: String,
    pub input_cache_hit_per_million_micros: u64,
    pub input_cache_miss_per_million_micros: u64,
    pub output_per_million_micros: u64,
    pub source_url: String,
    pub source_version: String,
    pub checked_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCatalogEntry {
    pub id: String,
    pub label: String,
    pub provider_id: String,
    pub provider_label: String,
    pub capabilities: ProviderCapabilities,
    pub pricing: Option<ModelPricing>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum TranscriptItem {
    System(String),
    User(String),
    AssistantToolCalls(Vec<ToolCall>),
    ToolResult {
        name: String,
        call_id: String,
        content: String,
        counts_toward_budget: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
}

impl ToolCall {
    pub fn with_call_id(
        name: impl Into<String>,
        call_id: impl Into<String>,
        mut arguments: Value,
    ) -> Self {
        if let Some(object) = arguments.as_object_mut() {
            object.insert("_call_id".into(), Value::String(call_id.into()));
        }
        Self {
            name: name.into(),
            arguments,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseFormat {
    Text,
    JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub transcript: Vec<TranscriptItem>,
    pub tools: Vec<ToolDefinition>,
    pub response_format: ResponseFormat,
    /// Optional provider-neutral JSON Schema for structured final output.
    /// Providers that only support a generic JSON-object mode may ignore it.
    pub response_schema: Option<Value>,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelOutput {
    ToolCalls { calls: Vec<ToolCall> },
    FinalText { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelResponse {
    pub output: ModelOutput,
    pub usage: ModelUsage,
}

impl ModelResponse {
    pub fn tool_calls(calls: Vec<ToolCall>, usage: ModelUsage) -> Self {
        Self {
            output: ModelOutput::ToolCalls { calls },
            usage,
        }
    }

    pub fn final_text(text: impl Into<String>, usage: ModelUsage) -> Self {
        Self {
            output: ModelOutput::FinalText { text: text.into() },
            usage,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProviderError {
    #[error("provider credential is missing")]
    CredentialMissing,
    #[error("provider authentication failed")]
    AuthFailed,
    #[error("provider rate limit exceeded")]
    RateLimited,
    #[error("provider network request failed: {0}")]
    Network(String),
    #[error("provider output was truncated")]
    OutputTruncated,
    #[error("provider returned an invalid response: {0}")]
    InvalidResponse(String),
}

impl ProviderError {
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::RateLimited | Self::Network(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub jitter_percent: u8,
}

impl RetryPolicy {
    pub fn delay_after(&self, failed_attempt: u8, jitter_key: &str) -> Duration {
        let exponent = u32::from(failed_attempt.saturating_sub(1)).min(16);
        let delay_ms = self
            .base_delay_ms
            .saturating_mul(1_u64 << exponent)
            .min(self.max_delay_ms);
        let jitter = u64::from(self.jitter_percent.min(100));
        if jitter == 0 || delay_ms == 0 {
            return Duration::from_millis(delay_ms);
        }
        let spread = delay_ms.saturating_mul(jitter) / 100;
        let hash = stable_hash(&format!("{jitter_key}:{failed_attempt}"));
        let offset = hash % spread.saturating_mul(2).saturating_add(1);
        Duration::from_millis(delay_ms.saturating_sub(spread).saturating_add(offset))
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 200,
            max_delay_ms: 2_000,
            jitter_percent: 20,
        }
    }
}

pub fn diagnostic_id(run_id: &str) -> String {
    format!("diag-{:016x}", stable_hash(run_id))
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        hash.wrapping_mul(0x100000001b3) ^ u64::from(byte)
    })
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;

    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, ProviderError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_capabilities_fail_closed() {
        let capabilities = ProviderCapabilities::default();
        assert_eq!(capabilities.tool_calling, ToolCallingSupport::None);
        assert_eq!(
            capabilities.structured_output,
            StructuredOutputSupport::None
        );
        assert_eq!(capabilities.usage, UsageSupport::None);
        assert_eq!(capabilities.context_window_tokens, 0);
    }

    #[test]
    fn catalog_metadata_round_trips_without_losing_pricing_provenance() {
        let entry = ModelCatalogEntry {
            id: "fixture-model".into(),
            label: "Fixture".into(),
            provider_id: "fixture".into(),
            provider_label: "Fixture Provider".into(),
            capabilities: ProviderCapabilities::default(),
            pricing: Some(ModelPricing {
                currency: "USD".into(),
                input_cache_hit_per_million_micros: 1,
                input_cache_miss_per_million_micros: 2,
                output_per_million_micros: 3,
                source_url: "https://example.test/pricing".into(),
                source_version: "fixture-v1".into(),
                checked_at: "2026-08-07".into(),
            }),
        };
        let encoded = serde_json::to_string(&entry).unwrap();
        assert_eq!(
            serde_json::from_str::<ModelCatalogEntry>(&encoded).unwrap(),
            entry
        );
    }

    #[test]
    fn retry_policy_retries_only_transient_provider_failures() {
        assert!(ProviderError::RateLimited.is_transient());
        assert!(ProviderError::Network("offline".into()).is_transient());
        assert!(!ProviderError::AuthFailed.is_transient());
        assert!(!ProviderError::InvalidResponse("bad json".into()).is_transient());
    }

    #[test]
    fn retry_delay_is_bounded_and_diagnostic_id_hides_run_id() {
        let policy = RetryPolicy::default();
        let delay = policy.delay_after(3, "run-secret");
        assert!(delay >= Duration::from_millis(640));
        assert!(delay <= Duration::from_millis(960));
        let diagnostic = diagnostic_id("run-secret");
        assert!(diagnostic.starts_with("diag-"));
        assert!(!diagnostic.contains("run-secret"));
        assert_eq!(diagnostic, diagnostic_id("run-secret"));
    }
}
