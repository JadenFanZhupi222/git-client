use crate::{content_digest, PathScope};
use agent_runtime::{
    ToolDefinition, ToolExecutionContext, ToolHandler, ToolHandlerError, ToolHandlerOutput,
    ToolReceipt, ToolRisk,
};
use async_trait::async_trait;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct SearchTextTool {
    scope: PathScope,
    max_file_bytes: usize,
}

#[derive(Serialize)]
struct SearchMatch {
    path: String,
    line: usize,
    column: usize,
    preview: String,
}

impl SearchTextTool {
    pub fn new(scope: PathScope, max_file_bytes: usize) -> Self {
        Self {
            scope,
            max_file_bytes,
        }
    }

    pub fn definition() -> ToolDefinition {
        ToolDefinition {
            name: "search.text".into(),
            description: "Search bounded UTF-8 workspace files using literal text or a Rust regex"
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "minLength": 1, "maxLength": 512},
                    "path": {"type": "string", "maxLength": 1024},
                    "regex": {"type": "boolean"},
                    "case_sensitive": {"type": "boolean"},
                    "max_results": {"type": "integer", "minimum": 1, "maximum": 200}
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            risk: ToolRisk::ReadOnly,
            timeout_ms: 20_000,
            max_result_bytes: 256 * 1024,
        }
    }
}

#[async_trait]
impl ToolHandler for SearchTextTool {
    async fn execute(
        &self,
        context: ToolExecutionContext,
        arguments: Value,
    ) -> Result<ToolHandlerOutput, ToolHandlerError> {
        let query = arguments
            .get("query")
            .and_then(Value::as_str)
            .ok_or(ToolHandlerError)?;
        let regex_mode = arguments
            .get("regex")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let case_sensitive = arguments
            .get("case_sensitive")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let source = if regex_mode {
            query.to_owned()
        } else {
            regex::escape(query)
        };
        let source = if case_sensitive {
            source
        } else {
            format!("(?i:{source})")
        };
        let matcher = Regex::new(&source).map_err(|_| ToolHandlerError)?;
        let relative = arguments.get("path").and_then(Value::as_str).unwrap_or("");
        let start = self
            .scope
            .existing_directory(relative)
            .map_err(|_| ToolHandlerError)?;
        let max_results = arguments
            .get("max_results")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(50)
            .min(200);
        let scope = self.scope.clone();
        let max_file_bytes = self.max_file_bytes;
        let cancellation = context.cancellation;
        let results = tokio::task::spawn_blocking(move || {
            search_tree(
                &scope,
                start,
                &matcher,
                max_file_bytes,
                max_results,
                cancellation.as_ref(),
            )
        })
        .await
        .map_err(|_| ToolHandlerError)??;
        let content = serde_json::to_string(&results).map_err(|_| ToolHandlerError)?;
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
            "Search workspace for {}",
            arguments.get("query").and_then(Value::as_str)?
        ))
    }
}

fn search_tree(
    scope: &PathScope,
    start: PathBuf,
    matcher: &Regex,
    max_file_bytes: usize,
    max_results: usize,
    cancellation: &dyn agent_runtime::ToolCancellation,
) -> Result<Vec<SearchMatch>, ToolHandlerError> {
    let mut pending = vec![start];
    let mut matches = Vec::new();
    let mut visited_directories = 0usize;
    let mut visited_files = 0usize;
    while let Some(path) = pending.pop() {
        visited_directories = visited_directories.saturating_add(1);
        if visited_directories > 5_000 {
            return Err(ToolHandlerError);
        }
        if cancellation.is_cancelled() {
            return Err(ToolHandlerError);
        }
        let mut entries = std::fs::read_dir(path)
            .map_err(|_| ToolHandlerError)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ToolHandlerError)?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries.into_iter().rev() {
            if cancellation.is_cancelled() {
                return Err(ToolHandlerError);
            }
            let file_type = entry.file_type().map_err(|_| ToolHandlerError)?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                let name = entry.file_name();
                if matches!(name.to_str(), Some(".git" | "node_modules" | "target")) {
                    continue;
                }
                pending.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            visited_files = visited_files.saturating_add(1);
            if visited_files > 20_000 {
                return Err(ToolHandlerError);
            }
            let metadata = entry.metadata().map_err(|_| ToolHandlerError)?;
            if metadata.len() > max_file_bytes as u64 {
                continue;
            }
            let bytes = std::fs::read(entry.path()).map_err(|_| ToolHandlerError)?;
            let Ok(text) = String::from_utf8(bytes) else {
                continue;
            };
            let relative = scope
                .relative_display(&entry.path())
                .map_err(|_| ToolHandlerError)?;
            for (line_index, line) in text.lines().enumerate() {
                if cancellation.is_cancelled() {
                    return Err(ToolHandlerError);
                }
                for found in matcher.find_iter(line) {
                    matches.push(SearchMatch {
                        path: relative.clone(),
                        line: line_index + 1,
                        column: line[..found.start()].chars().count() + 1,
                        preview: line.chars().take(300).collect(),
                    });
                    if matches.len() >= max_results {
                        return Ok(matches);
                    }
                }
            }
        }
    }
    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_runtime::{NeverCancel, ToolExecutionContext};
    use std::sync::Arc;

    struct Cancelled;

    #[async_trait::async_trait]
    impl agent_runtime::ToolCancellation for Cancelled {
        fn is_cancelled(&self) -> bool {
            true
        }
    }

    fn context() -> ToolExecutionContext {
        ToolExecutionContext {
            run_id: "run".into(),
            call_id: "call".into(),
            execution_id: "exec-call".into(),
            cancellation: Arc::new(NeverCancel),
        }
    }

    #[tokio::test]
    async fn searches_literal_regex_case_and_respects_bounds_and_ignores() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::create_dir(root.path().join("node_modules")).unwrap();
        std::fs::write(root.path().join("src/a.rs"), "Alpha beta\nalpha BETA").unwrap();
        std::fs::write(root.path().join("node_modules/secret.js"), "Alpha").unwrap();
        let tool = SearchTextTool::new(PathScope::new(root.path(), true).unwrap(), 1024);
        let literal = tool
            .execute(
                context(),
                json!({"query":"alpha", "case_sensitive":false, "max_results":1}),
            )
            .await
            .unwrap();
        let values: Vec<Value> = serde_json::from_str(&literal).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0]["path"], "src/a.rs");
        let regex = tool
            .execute(context(), json!({"query":"B.TA", "regex":true}))
            .await
            .unwrap();
        assert!(regex.contains("src/a.rs"));
        assert!(!regex.contains("secret.js"));
    }

    #[tokio::test]
    async fn stops_before_scanning_when_run_is_cancelled() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.txt"), "needle").unwrap();
        let tool = SearchTextTool::new(PathScope::new(root.path(), true).unwrap(), 1024);
        let result = tool
            .execute(
                ToolExecutionContext {
                    run_id: "run".into(),
                    call_id: "call".into(),
                    execution_id: "exec-call".into(),
                    cancellation: Arc::new(Cancelled),
                },
                json!({"query":"needle"}),
            )
            .await;
        assert!(result.is_err());
    }
}
