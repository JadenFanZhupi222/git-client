use crate::{ModelOutput, ModelRequest, ModelResponse, ProviderError, TranscriptItem};
use std::collections::HashMap;

const MAX_WIRE_NAME_BYTES: usize = 64;
const HASH_BYTES: usize = 16;

/// Maps provider-neutral tool names to the restricted identifier format used on
/// provider wires. Canonical names remain authoritative everywhere outside an
/// individual provider adapter.
pub(crate) struct ProviderToolNames {
    canonical_to_wire: HashMap<String, String>,
    wire_to_canonical: HashMap<String, String>,
}

impl ProviderToolNames {
    pub(crate) fn new(request: &ModelRequest) -> Result<Self, ProviderError> {
        let mut canonical_to_wire = HashMap::new();
        let mut wire_to_canonical = HashMap::new();
        for tool in &request.tools {
            register_name(&tool.name, &mut canonical_to_wire, &mut wire_to_canonical)?;
        }
        for item in &request.transcript {
            if let TranscriptItem::AssistantToolCalls(calls) = item {
                for call in calls {
                    register_name(&call.name, &mut canonical_to_wire, &mut wire_to_canonical)?;
                }
            }
        }
        Ok(Self {
            canonical_to_wire,
            wire_to_canonical,
        })
    }

    pub(crate) fn wire<'a>(&'a self, canonical: &'a str) -> Result<&'a str, ProviderError> {
        self.canonical_to_wire
            .get(canonical)
            .map(String::as_str)
            .ok_or_else(|| ProviderError::InvalidResponse("unknown transcript tool name".into()))
    }

    pub(crate) fn canonical(&self, wire: &str) -> Option<&str> {
        self.wire_to_canonical.get(wire).map(String::as_str)
    }

    pub(crate) fn restore_response(
        &self,
        mut response: ModelResponse,
    ) -> Result<ModelResponse, ProviderError> {
        if let ModelOutput::ToolCalls { calls } = &mut response.output {
            for call in calls {
                call.name = self
                    .canonical(&call.name)
                    .ok_or_else(|| {
                        ProviderError::InvalidResponse("provider returned an unknown tool".into())
                    })?
                    .to_owned();
            }
        }
        Ok(response)
    }
}

fn register_name(
    canonical: &str,
    canonical_to_wire: &mut HashMap<String, String>,
    wire_to_canonical: &mut HashMap<String, String>,
) -> Result<(), ProviderError> {
    let wire = encode_wire_name(canonical);
    if let Some(existing) = wire_to_canonical.insert(wire.clone(), canonical.to_owned()) {
        if existing != canonical {
            return Err(ProviderError::InvalidResponse(
                "tool names collide after provider encoding".into(),
            ));
        }
    }
    canonical_to_wire.insert(canonical.to_owned(), wire);
    Ok(())
}

fn encode_wire_name(canonical: &str) -> String {
    if canonical.len() <= MAX_WIRE_NAME_BYTES
        && canonical
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return canonical.to_owned();
    }

    let mut prefix = canonical
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') {
                char::from(byte)
            } else {
                '_'
            }
        })
        .collect::<String>();
    let prefix_limit = MAX_WIRE_NAME_BYTES - HASH_BYTES - 1;
    prefix.truncate(prefix_limit);
    format!("{prefix}_{:016x}", fnv1a(canonical.as_bytes()))
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        default_tool_result_bytes, default_tool_timeout_ms, ModelUsage, ResponseFormat, ToolCall,
        ToolDefinition, ToolRisk,
    };
    use serde_json::json;

    fn request(names: &[&str]) -> ModelRequest {
        ModelRequest {
            transcript: Vec::new(),
            tools: names
                .iter()
                .map(|name| ToolDefinition {
                    name: (*name).into(),
                    description: String::new(),
                    input_schema: json!({"type": "object"}),
                    risk: ToolRisk::ReadOnly,
                    timeout_ms: default_tool_timeout_ms(),
                    max_result_bytes: default_tool_result_bytes(),
                })
                .collect(),
            response_format: ResponseFormat::Text,
            response_schema: None,
            max_output_tokens: 10,
        }
    }

    #[test]
    fn preserves_safe_names_and_encodes_canonical_names_deterministically() {
        let names = ProviderToolNames::new(&request(&["read_file", "filesystem.read"])).unwrap();
        assert_eq!(names.wire("read_file").unwrap(), "read_file");
        let encoded = names.wire("filesystem.read").unwrap();
        assert!(encoded.starts_with("filesystem_read_"));
        assert!(encoded.len() <= MAX_WIRE_NAME_BYTES);
        assert!(encoded
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')));
        assert_eq!(names.canonical(encoded), Some("filesystem.read"));
    }

    #[test]
    fn distinct_canonical_names_do_not_collapse_to_the_same_wire_name() {
        let names =
            ProviderToolNames::new(&request(&["filesystem.read", "filesystem/read"])).unwrap();
        assert_ne!(
            names.wire("filesystem.read").unwrap(),
            names.wire("filesystem/read").unwrap()
        );
    }

    #[test]
    fn restores_provider_tool_calls_to_canonical_names() {
        let names = ProviderToolNames::new(&request(&["filesystem.read"])).unwrap();
        let response = ModelResponse::tool_calls(
            vec![ToolCall::with_call_id(
                names.wire("filesystem.read").unwrap(),
                "call-1",
                json!({}),
            )],
            ModelUsage::default(),
        );
        let restored = names.restore_response(response).unwrap();
        assert!(matches!(
            restored.output,
            ModelOutput::ToolCalls { calls } if calls[0].name == "filesystem.read"
        ));
    }
}
