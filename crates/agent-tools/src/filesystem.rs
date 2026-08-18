use crate::{content_digest, PathScope};
use agent_runtime::{
    ToolDefinition, ToolExecutionContext, ToolHandler, ToolHandlerError, ToolHandlerOutput,
    ToolIntentPrecondition, ToolReceipt, ToolRisk,
};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};
use std::io::Write;

fn failure<T>() -> Result<T, ToolHandlerError> {
    Err(ToolHandlerError)
}

fn string_argument<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, ToolHandlerError> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .ok_or(ToolHandlerError)
}

pub struct FilesystemReadTool {
    scope: PathScope,
    max_bytes: usize,
}

impl FilesystemReadTool {
    pub fn new(scope: PathScope, max_bytes: usize) -> Self {
        Self { scope, max_bytes }
    }

    pub fn definition(max_bytes: usize) -> ToolDefinition {
        ToolDefinition {
            name: "filesystem.read".into(),
            description: "Read one bounded UTF-8 file under the current workspace root".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "minLength": 1, "maxLength": 1024},
                    "max_bytes": {"type": "integer", "minimum": 1, "maximum": max_bytes}
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            risk: ToolRisk::ReadOnly,
            timeout_ms: 10_000,
            max_result_bytes: max_bytes.min(1024 * 1024),
        }
    }
}

#[async_trait]
impl ToolHandler for FilesystemReadTool {
    async fn execute(
        &self,
        _: ToolExecutionContext,
        arguments: Value,
    ) -> Result<ToolHandlerOutput, ToolHandlerError> {
        let resource = string_argument(&arguments, "path")?.replace('\\', "/");
        let path = self
            .scope
            .existing_file(string_argument(&arguments, "path")?)
            .map_err(|_| ToolHandlerError)?;
        let requested = arguments
            .get("max_bytes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(self.max_bytes)
            .min(self.max_bytes);
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|_| ToolHandlerError)?;
        if metadata.len() > requested as u64 {
            return failure();
        }
        let bytes = tokio::fs::read(path).await.map_err(|_| ToolHandlerError)?;
        if bytes.len() > requested {
            return failure();
        }
        let digest = content_digest(&bytes);
        let content = String::from_utf8(bytes).map_err(|_| ToolHandlerError)?;
        Ok(ToolHandlerOutput::new(
            content,
            ToolReceipt::Observation {
                resource,
                version_digest: digest,
            },
        ))
    }

    fn summarize_arguments(&self, arguments: &Value) -> Option<String> {
        Some(format!(
            "Read {}",
            arguments.get("path").and_then(Value::as_str)?
        ))
    }
}

pub struct FilesystemListTool {
    scope: PathScope,
    max_entries: usize,
}

#[derive(Serialize)]
struct DirectoryEntryResult {
    path: String,
    kind: &'static str,
    bytes: Option<u64>,
}

impl FilesystemListTool {
    pub fn new(scope: PathScope, max_entries: usize) -> Self {
        Self { scope, max_entries }
    }

    pub fn definition() -> ToolDefinition {
        ToolDefinition {
            name: "filesystem.list".into(),
            description: "List one bounded directory under the current workspace root".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "maxLength": 1024}
                },
                "additionalProperties": false
            }),
            risk: ToolRisk::ReadOnly,
            timeout_ms: 10_000,
            max_result_bytes: 256 * 1024,
        }
    }
}

#[async_trait]
impl ToolHandler for FilesystemListTool {
    async fn execute(
        &self,
        _: ToolExecutionContext,
        arguments: Value,
    ) -> Result<ToolHandlerOutput, ToolHandlerError> {
        let relative = arguments.get("path").and_then(Value::as_str).unwrap_or("");
        let directory = self
            .scope
            .existing_directory(relative)
            .map_err(|_| ToolHandlerError)?;
        let mut reader = tokio::fs::read_dir(&directory)
            .await
            .map_err(|_| ToolHandlerError)?;
        let mut entries = Vec::new();
        while let Some(entry) = reader.next_entry().await.map_err(|_| ToolHandlerError)? {
            if entry
                .file_name()
                .eq_ignore_ascii_case(std::ffi::OsStr::new(".git"))
            {
                continue;
            }
            if entries.len() >= self.max_entries {
                return failure();
            }
            let file_type = entry.file_type().await.map_err(|_| ToolHandlerError)?;
            let (kind, bytes) = if file_type.is_symlink() {
                ("symlink", None)
            } else if file_type.is_dir() {
                ("directory", None)
            } else if file_type.is_file() {
                let metadata = entry.metadata().await.map_err(|_| ToolHandlerError)?;
                ("file", Some(metadata.len()))
            } else {
                ("other", None)
            };
            entries.push(DirectoryEntryResult {
                path: self
                    .scope
                    .relative_display(&entry.path())
                    .map_err(|_| ToolHandlerError)?,
                kind,
                bytes,
            });
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        let content = serde_json::to_string(&entries).map_err(|_| ToolHandlerError)?;
        Ok(ToolHandlerOutput::new(
            content.clone(),
            ToolReceipt::Observation {
                resource: relative.replace('\\', "/"),
                version_digest: content_digest(content.as_bytes()),
            },
        ))
    }

    fn summarize_arguments(&self, arguments: &Value) -> Option<String> {
        Some(format!(
            "List {}",
            arguments
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("workspace root")
        ))
    }
}

pub struct FilesystemWriteTool {
    scope: PathScope,
    max_bytes: usize,
}

impl FilesystemWriteTool {
    pub fn new(scope: PathScope, max_bytes: usize) -> Self {
        Self { scope, max_bytes }
    }

    pub fn definition(max_bytes: usize) -> ToolDefinition {
        ToolDefinition {
            name: "filesystem.write".into(),
            description: "Atomically create or replace one UTF-8 file under the workspace root"
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "minLength": 1, "maxLength": 1024},
                    "content": {"type": "string", "maxLength": max_bytes},
                    "create_only": {"type": "boolean"},
                    "expected_version": {"type": "string", "minLength": 6, "maxLength": 80}
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
            risk: ToolRisk::Write,
            timeout_ms: 10_000,
            max_result_bytes: 8 * 1024,
        }
    }
}

#[async_trait]
impl ToolHandler for FilesystemWriteTool {
    fn prepare_intent(
        &self,
        _: &ToolExecutionContext,
        arguments: &Value,
    ) -> Result<ToolIntentPrecondition, ToolHandlerError> {
        let relative = string_argument(arguments, "path")?;
        let content = string_argument(arguments, "content")?.as_bytes();
        let target = self
            .scope
            .write_target(relative)
            .map_err(|_| ToolHandlerError)?;
        let before = match std::fs::read(target) {
            Ok(bytes) => content_digest(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "absent".into(),
            Err(_) => return Err(ToolHandlerError),
        };
        if arguments
            .get("expected_version")
            .and_then(Value::as_str)
            .is_some_and(|expected| expected != before)
        {
            return Err(ToolHandlerError);
        }
        Ok(ToolIntentPrecondition {
            resource: Some(relative.replace('\\', "/")),
            before_digest: Some(before),
            expected_after_digest: Some(content_digest(content)),
            replay_policy: None,
        })
    }

    async fn execute(
        &self,
        context: ToolExecutionContext,
        arguments: Value,
    ) -> Result<ToolHandlerOutput, ToolHandlerError> {
        let path = string_argument(&arguments, "path")?.to_owned();
        let content = string_argument(&arguments, "content")?.as_bytes().to_vec();
        let create_only = arguments
            .get("create_only")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if content.len() > self.max_bytes {
            return failure();
        }
        let scope = self.scope.clone();
        let bytes = content.len();
        let write_path = path.clone();
        let after_digest = content_digest(&content);
        let before_digest = tokio::task::spawn_blocking(move || {
            let target = scope
                .write_target(&write_path)
                .map_err(|_| ToolHandlerError)?;
            let before = match std::fs::read(&target) {
                Ok(bytes) => content_digest(&bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => "absent".into(),
                Err(_) => return Err(ToolHandlerError),
            };
            atomic_write(&scope, &write_path, &content, create_only)?;
            Ok::<_, ToolHandlerError>(before)
        })
        .await
        .map_err(|_| ToolHandlerError)??;
        let resource = path.replace('\\', "/");
        Ok(ToolHandlerOutput::new(
            json!({"path": resource, "bytes": bytes, "version_digest": after_digest}).to_string(),
            ToolReceipt::Mutation {
                execution_id: context.execution_id,
                resource,
                before_digest,
                after_digest,
            },
        ))
    }

    fn summarize_arguments(&self, arguments: &Value) -> Option<String> {
        Some(format!(
            "Write {}",
            arguments.get("path").and_then(Value::as_str)?
        ))
    }
}

pub struct PatchApplyTool {
    scope: PathScope,
    max_bytes: usize,
}

impl PatchApplyTool {
    pub fn new(scope: PathScope, max_bytes: usize) -> Self {
        Self { scope, max_bytes }
    }

    pub fn definition(max_bytes: usize) -> ToolDefinition {
        ToolDefinition {
            name: "patch.apply".into(),
            description: "Replace one exact expected text occurrence in a workspace UTF-8 file"
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "minLength": 1, "maxLength": 1024},
                    "expected": {"type": "string", "minLength": 1, "maxLength": max_bytes},
                    "replacement": {"type": "string", "maxLength": max_bytes}
                },
                "required": ["path", "expected", "replacement"],
                "additionalProperties": false
            }),
            risk: ToolRisk::Write,
            timeout_ms: 10_000,
            max_result_bytes: 8 * 1024,
        }
    }
}

#[async_trait]
impl ToolHandler for PatchApplyTool {
    fn prepare_intent(
        &self,
        _: &ToolExecutionContext,
        arguments: &Value,
    ) -> Result<ToolIntentPrecondition, ToolHandlerError> {
        let relative = string_argument(arguments, "path")?;
        let expected = string_argument(arguments, "expected")?;
        let replacement = string_argument(arguments, "replacement")?;
        let target = self
            .scope
            .existing_file(relative)
            .map_err(|_| ToolHandlerError)?;
        let bytes = std::fs::read(target).map_err(|_| ToolHandlerError)?;
        if bytes.len() > self.max_bytes {
            return Err(ToolHandlerError);
        }
        let before = content_digest(&bytes);
        let original = String::from_utf8(bytes).map_err(|_| ToolHandlerError)?;
        let mut matches = original.match_indices(expected);
        let Some((offset, _)) = matches.next() else {
            return Err(ToolHandlerError);
        };
        if matches.next().is_some() {
            return Err(ToolHandlerError);
        }
        let mut updated = original;
        updated.replace_range(offset..offset + expected.len(), replacement);
        Ok(ToolIntentPrecondition {
            resource: Some(relative.replace('\\', "/")),
            before_digest: Some(before),
            expected_after_digest: Some(content_digest(updated.as_bytes())),
            replay_policy: None,
        })
    }

    async fn execute(
        &self,
        context: ToolExecutionContext,
        arguments: Value,
    ) -> Result<ToolHandlerOutput, ToolHandlerError> {
        let path = string_argument(&arguments, "path")?.to_owned();
        let expected = string_argument(&arguments, "expected")?.to_owned();
        let replacement = string_argument(&arguments, "replacement")?.to_owned();
        let scope = self.scope.clone();
        let max_bytes = self.max_bytes;
        let result_path = path.clone();
        let (bytes, before_digest, after_digest) = tokio::task::spawn_blocking(move || {
            let target = scope
                .existing_file(&result_path)
                .map_err(|_| ToolHandlerError)?;
            let original_bytes = std::fs::read(&target).map_err(|_| ToolHandlerError)?;
            if original_bytes.len() > max_bytes {
                return failure();
            }
            let before_digest = content_digest(&original_bytes);
            let original = String::from_utf8(original_bytes).map_err(|_| ToolHandlerError)?;
            let mut matches = original.match_indices(&expected);
            let Some((offset, _)) = matches.next() else {
                return failure();
            };
            if matches.next().is_some() {
                return failure();
            }
            let mut updated = String::with_capacity(
                original
                    .len()
                    .saturating_sub(expected.len())
                    .saturating_add(replacement.len()),
            );
            updated.push_str(&original[..offset]);
            updated.push_str(&replacement);
            updated.push_str(&original[offset + expected.len()..]);
            if updated.len() > max_bytes {
                return failure();
            }
            let after_digest = content_digest(updated.as_bytes());
            atomic_write(&scope, &result_path, updated.as_bytes(), false)?;
            Ok((updated.len(), before_digest, after_digest))
        })
        .await
        .map_err(|_| ToolHandlerError)??;
        let resource = path.replace('\\', "/");
        Ok(ToolHandlerOutput::new(
            json!({"path": resource, "bytes": bytes, "replacements": 1, "version_digest": after_digest}).to_string(),
            ToolReceipt::Mutation {
                execution_id: context.execution_id,
                resource,
                before_digest,
                after_digest,
            },
        ))
    }

    fn summarize_arguments(&self, arguments: &Value) -> Option<String> {
        Some(format!(
            "Patch {}",
            arguments.get("path").and_then(Value::as_str)?
        ))
    }
}

fn atomic_write(
    scope: &PathScope,
    relative: &str,
    content: &[u8],
    create_only: bool,
) -> Result<(), ToolHandlerError> {
    let target = scope.write_target(relative).map_err(|_| ToolHandlerError)?;
    if create_only && target.exists() {
        return failure();
    }
    let parent = target.parent().ok_or(ToolHandlerError)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| ToolHandlerError)?;
    temporary.write_all(content).map_err(|_| ToolHandlerError)?;
    temporary.flush().map_err(|_| ToolHandlerError)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| ToolHandlerError)?;
    temporary.persist(&target).map_err(|_| ToolHandlerError)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_runtime::{NeverCancel, ToolExecutionContext};
    use std::sync::Arc;

    fn context() -> ToolExecutionContext {
        ToolExecutionContext {
            run_id: "run".into(),
            call_id: "call".into(),
            execution_id: "exec-call".into(),
            cancellation: Arc::new(NeverCancel),
        }
    }

    #[tokio::test]
    async fn read_list_write_and_patch_stay_inside_scope() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.txt"), "hello world").unwrap();
        let scope = PathScope::new(root.path(), true).unwrap();
        let read = FilesystemReadTool::new(scope.clone(), 1024);
        assert_eq!(
            read.execute(context(), json!({"path":"a.txt"}))
                .await
                .unwrap(),
            "hello world"
        );

        let write = FilesystemWriteTool::new(scope.clone(), 1024);
        write
            .execute(
                context(),
                json!({"path":"nested/b.txt", "content":"created", "create_only":true}),
            )
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("nested/b.txt")).unwrap(),
            "created"
        );
        assert!(write
            .execute(
                context(),
                json!({"path":"nested/b.txt", "content":"again", "create_only":true})
            )
            .await
            .is_err());

        let patch = PatchApplyTool::new(scope.clone(), 1024);
        patch
            .execute(
                context(),
                json!({"path":"a.txt", "expected":"world", "replacement":"tool"}),
            )
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("a.txt")).unwrap(),
            "hello tool"
        );
        assert!(patch
            .execute(
                context(),
                json!({"path":"a.txt", "expected":"missing", "replacement":"x"})
            )
            .await
            .is_err());

        let list = FilesystemListTool::new(scope, 10);
        let output = list.execute(context(), json!({})).await.unwrap();
        assert!(output.contains("a.txt"));
        assert!(output.contains("nested"));
        std::fs::create_dir(root.path().join(".git")).unwrap();
        let output = list.execute(context(), json!({})).await.unwrap();
        assert!(!output.contains(".git"));
    }

    #[tokio::test]
    async fn read_rejects_oversized_binary_and_traversal() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("large.txt"), "12345").unwrap();
        std::fs::write(root.path().join("binary.bin"), [0xff, 0xfe]).unwrap();
        let read = FilesystemReadTool::new(PathScope::new(root.path(), true).unwrap(), 4);
        assert!(read
            .execute(context(), json!({"path":"large.txt"}))
            .await
            .is_err());
        assert!(read
            .execute(context(), json!({"path":"binary.bin"}))
            .await
            .is_err());
        assert!(read
            .execute(context(), json!({"path":"../outside"}))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn patch_rejects_ambiguous_preimage_without_writing() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.txt"), "same same").unwrap();
        let patch = PatchApplyTool::new(PathScope::new(root.path(), true).unwrap(), 1024);
        assert!(patch
            .execute(
                context(),
                json!({"path":"a.txt", "expected":"same", "replacement":"x"})
            )
            .await
            .is_err());
        assert_eq!(
            std::fs::read_to_string(root.path().join("a.txt")).unwrap(),
            "same same"
        );
    }
}
