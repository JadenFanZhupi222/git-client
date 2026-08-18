use crate::{content_digest, PathScope};
use agent_runtime::{
    ToolDefinition, ToolExecutionContext, ToolHandler, ToolHandlerError, ToolHandlerOutput,
    ToolIntentPrecondition, ToolReceipt, ToolRisk,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::io::Write;

pub struct ArtifactWriteTool {
    scope: PathScope,
    max_bytes: usize,
}

impl ArtifactWriteTool {
    pub fn new(scope: PathScope, max_bytes: usize) -> Self {
        Self { scope, max_bytes }
    }

    pub fn definition(max_bytes: usize) -> ToolDefinition {
        ToolDefinition {
            name: "artifact.write".into(),
            description: "Store one bounded UTF-8 artifact and return an opaque artifact ID".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "minLength": 1, "maxLength": 255},
                    "media_type": {"type": "string", "minLength": 3, "maxLength": 127, "pattern": "^[A-Za-z0-9.+-]+/[A-Za-z0-9.+-]+$"},
                    "content": {"type": "string", "maxLength": max_bytes}
                },
                "required": ["name", "media_type", "content"],
                "additionalProperties": false
            }),
            risk: ToolRisk::Write,
            timeout_ms: 10_000,
            max_result_bytes: 8 * 1024,
        }
    }
}

#[async_trait]
impl ToolHandler for ArtifactWriteTool {
    fn prepare_intent(
        &self,
        context: &ToolExecutionContext,
        arguments: &Value,
    ) -> Result<ToolIntentPrecondition, ToolHandlerError> {
        let name = arguments
            .get("name")
            .and_then(Value::as_str)
            .ok_or(ToolHandlerError)?;
        let content = arguments
            .get("content")
            .and_then(Value::as_str)
            .ok_or(ToolHandlerError)?;
        let artifact_id = format!(
            "artifact-{:016x}",
            stable_hash(&format!("{}:{}:{name}", context.run_id, context.call_id))
        );
        Ok(ToolIntentPrecondition {
            resource: Some(artifact_id),
            before_digest: Some("absent".into()),
            expected_after_digest: Some(content_digest(content.as_bytes())),
            replay_policy: None,
        })
    }

    async fn execute(
        &self,
        context: ToolExecutionContext,
        arguments: Value,
    ) -> Result<ToolHandlerOutput, ToolHandlerError> {
        let name = arguments
            .get("name")
            .and_then(Value::as_str)
            .ok_or(ToolHandlerError)?
            .to_owned();
        let media_type = arguments
            .get("media_type")
            .and_then(Value::as_str)
            .ok_or(ToolHandlerError)?
            .to_owned();
        let content = arguments
            .get("content")
            .and_then(Value::as_str)
            .ok_or(ToolHandlerError)?
            .as_bytes()
            .to_vec();
        if content.len() > self.max_bytes {
            return Err(ToolHandlerError);
        }
        let artifact_id = format!(
            "artifact-{:016x}",
            stable_hash(&format!("{}:{}:{name}", context.run_id, context.call_id))
        );
        let relative = format!("{artifact_id}.data");
        let scope = self.scope.clone();
        let bytes = content.len();
        let digest = content_digest(&content);
        tokio::task::spawn_blocking(move || {
            let target = scope
                .write_target(&relative)
                .map_err(|_| ToolHandlerError)?;
            let parent = target.parent().ok_or(ToolHandlerError)?;
            let mut temporary =
                tempfile::NamedTempFile::new_in(parent).map_err(|_| ToolHandlerError)?;
            temporary
                .write_all(&content)
                .map_err(|_| ToolHandlerError)?;
            temporary.flush().map_err(|_| ToolHandlerError)?;
            temporary
                .as_file()
                .sync_all()
                .map_err(|_| ToolHandlerError)?;
            temporary.persist(target).map_err(|_| ToolHandlerError)?;
            Ok::<(), ToolHandlerError>(())
        })
        .await
        .map_err(|_| ToolHandlerError)??;
        Ok(ToolHandlerOutput::new(
            json!({
                "artifact_id": artifact_id,
                "name": name,
                "media_type": media_type,
                "bytes": bytes,
                "content_digest": digest
            })
            .to_string(),
            ToolReceipt::Artifact {
                execution_id: context.execution_id,
                artifact_id,
                content_digest: digest,
            },
        ))
    }

    fn summarize_arguments(&self, arguments: &Value) -> Option<String> {
        Some(format!(
            "Create artifact {}",
            arguments.get("name").and_then(Value::as_str)?
        ))
    }
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        hash.wrapping_mul(0x100000001b3) ^ u64::from(byte)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_runtime::{NeverCancel, ToolExecutionContext};
    use std::sync::Arc;

    #[tokio::test]
    async fn stores_under_artifact_root_and_returns_no_host_path() {
        let root = tempfile::tempdir().unwrap();
        let tool = ArtifactWriteTool::new(PathScope::new(root.path(), false).unwrap(), 1024);
        let output = tool
            .execute(
                ToolExecutionContext {
                    run_id: "run".into(),
                    call_id: "call".into(),
                    execution_id: "exec-call".into(),
                    cancellation: Arc::new(NeverCancel),
                },
                json!({"name":"report.md", "media_type":"text/markdown", "content":"hello"}),
            )
            .await
            .unwrap();
        assert!(output.contains("artifact-"));
        assert!(!output.contains(&root.path().to_string_lossy().to_string()));
        let stored = std::fs::read_dir(root.path())
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(std::fs::read_to_string(stored[0].path()).unwrap(), "hello");
        assert!(tool
            .execute(
                ToolExecutionContext {
                    run_id: "run".into(),
                    call_id: "large".into(),
                    execution_id: "exec-large".into(),
                    cancellation: Arc::new(NeverCancel),
                },
                json!({"name":"large.txt", "media_type":"text/plain", "content":"x".repeat(1025)}),
            )
            .await
            .is_err());
    }
}
