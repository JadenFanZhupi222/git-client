use crate::{ReviewError, TraceEntry, TraceSink};
use async_trait::async_trait;
use fs2::FileExt;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct SanitizedTraceStore {
    path: PathBuf,
}

impl SanitizedTraceStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl TraceSink for SanitizedTraceStore {
    async fn record(&self, entry: TraceEntry) -> Result<(), ReviewError> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || record_locked(&path, entry))
            .await
            .map_err(|_| ReviewError::NetworkError("trace writer task failed".into()))?
    }
}

fn record_locked(path: &Path, entry: TraceEntry) -> Result<(), ReviewError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|_| ReviewError::NetworkError("trace directory could not be created".into()))?;
    let lock_path = lock_path(path);
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|_| ReviewError::NetworkError("trace lock could not be opened".into()))?;
    lock_file
        .lock_exclusive()
        .map_err(|_| ReviewError::NetworkError("trace lock could not be acquired".into()))?;
    let result = rewrite_trace(path, parent, entry);
    let _ = FileExt::unlock(&lock_file);
    result
}

fn rewrite_trace(path: &Path, parent: &Path, entry: TraceEntry) -> Result<(), ReviewError> {
    let mut entries: Vec<TraceEntry> = match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| {
            ReviewError::NetworkError("trace file is corrupted or incompatible".into())
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(_) => {
            return Err(ReviewError::NetworkError(
                "trace file could not be read".into(),
            ))
        }
    };
    entries.push(sanitize(entry));
    if entries.len() > 100 {
        entries.drain(..entries.len() - 100);
    }
    let bytes = serde_json::to_vec(&entries)
        .map_err(|_| ReviewError::NetworkError("trace metadata could not be encoded".into()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| ReviewError::NetworkError("trace temp file could not be created".into()))?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.as_file_mut().sync_all())
        .map_err(|_| ReviewError::NetworkError("trace temp file could not be written".into()))?;
    temporary.persist(path).map_err(|_| {
        ReviewError::NetworkError("trace file could not be atomically replaced".into())
    })?;
    Ok(())
}

fn lock_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(".lock");
    PathBuf::from(value)
}

fn sanitize(mut entry: TraceEntry) -> TraceEntry {
    if !is_safe_model_id(&entry.model) {
        entry.model = "unknown".into();
    }
    entry.tool_names = entry
        .tool_names
        .into_iter()
        .map(|name| match name.as_str() {
            "list_repository_tree" | "read_file" => name,
            _ => "unknown".into(),
        })
        .collect();
    if !matches!(entry.status.as_str(), "completed" | "error" | "cancelled") {
        entry.status = "unknown".into();
    }
    if !entry
        .error_code
        .as_deref()
        .is_some_and(is_stable_error_code)
    {
        entry.error_code = None;
    }
    if !entry
        .error_detail
        .as_deref()
        .is_some_and(is_stable_error_detail)
    {
        entry.error_detail = None;
    }
    entry
}

fn is_safe_model_id(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 80
        && model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'/'))
}

fn is_stable_error_detail(detail: &str) -> bool {
    matches!(
        detail,
        "response_not_json"
            | "response_output_missing"
            | "structured_output_invalid"
            | "summary_missing"
            | "findings_missing"
            | "findings_schema_mismatch"
            | "no_final_output"
            | "function_name_missing"
            | "function_call_id_missing"
            | "duplicate_function_call_id"
            | "function_arguments_missing"
            | "function_arguments_invalid"
            | "output_text_missing"
            | "empty_tool_calls"
            | "unknown_tool"
            | "tool_arguments_not_object"
            | "tool_arguments_malformed"
            | "tree_prefix_invalid"
            | "read_path_missing"
            | "read_start_invalid"
            | "read_end_invalid"
            | "other_validation_failure"
    )
}

fn is_stable_error_code(code: &str) -> bool {
    matches!(
        code,
        "AI_KEY_MISSING"
            | "GITHUB_TOKEN_MISSING"
            | "AUTH_FAILED"
            | "RATE_LIMITED"
            | "NETWORK_ERROR"
            | "PR_UPDATED"
            | "REVIEW_BUDGET_EXCEEDED"
            | "INVALID_MODEL_OUTPUT"
            | "CANCELLED"
            | "REVIEW_PUBLISH_FAILED"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TraceEntry, TraceSink};
    use chrono::Utc;
    use std::collections::HashSet;
    use std::sync::Arc;

    #[tokio::test]
    async fn keeps_only_100_metadata_entries_and_sanitizes_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trace.json");
        let store = SanitizedTraceStore::new(path.clone());
        for n in 0..105 {
            store
                .record(TraceEntry {
                    timestamp: Utc::now(),
                    model: "deepseek-v4-flash SECRET_KEY".into(),
                    duration_ms: n,
                    input_tokens: 1,
                    output_tokens: 2,
                    tool_names: vec!["read_file CODE_MARKER".into()],
                    status: "ok SECRET_KEY".into(),
                    error_code: Some("INVALID_MODEL_OUTPUT CODE_MARKER".into()),
                    error_detail: Some("unsafe detail".into()),
                })
                .await
                .unwrap();
        }
        let serialized = std::fs::read_to_string(path).unwrap();
        let entries: Vec<TraceEntry> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(entries.len(), 100);
        assert_eq!(entries[0].duration_ms, 5);
        assert_eq!(entries[0].error_code, None);
        assert_eq!(entries[0].error_detail, None);
        assert!(!serialized.contains("SECRET_KEY"));
        assert!(!serialized.contains("CODE_MARKER"));
        assert!(!serialized.contains("prompt"));
    }

    #[test]
    fn accepts_provider_neutral_model_ids_without_allowing_free_form_text() {
        assert!(is_safe_model_id("deepseek-v4-flash"));
        assert!(is_safe_model_id("openai/gpt-5.6-terra"));
        assert!(!is_safe_model_id("model SECRET_KEY"));
        assert!(!is_safe_model_id(""));
    }

    #[tokio::test]
    async fn corrupted_trace_returns_error_without_overwriting_original() {
        for original in [b"{corrupted trace".as_slice(), b"{}".as_slice()] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("trace.json");
            std::fs::write(&path, original).unwrap();
            let store = SanitizedTraceStore::new(path.clone());
            let error = store
                .record(entry(1))
                .await
                .expect_err("invalid trace metadata must not be treated as empty");
            assert!(matches!(error, ReviewError::NetworkError(_)));
            assert_eq!(std::fs::read(path).unwrap(), original);
        }
    }

    #[tokio::test]
    async fn separate_store_instances_serialize_concurrent_writers_without_loss() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trace.json");
        let first = Arc::new(SanitizedTraceStore::new(path.clone()));
        let second = Arc::new(SanitizedTraceStore::new(path.clone()));
        let mut tasks = Vec::new();
        for number in 0..100 {
            let store = if number % 2 == 0 {
                first.clone()
            } else {
                second.clone()
            };
            tasks.push(tokio::spawn(
                async move { store.record(entry(number)).await },
            ));
        }
        for task in tasks {
            task.await.unwrap().unwrap();
        }
        let entries: Vec<TraceEntry> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(entries.len(), 100);
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.duration_ms)
                .collect::<HashSet<_>>()
                .len(),
            100
        );

        let mut overflow_tasks = Vec::new();
        for number in 100..120 {
            let store = if number % 2 == 0 {
                first.clone()
            } else {
                second.clone()
            };
            overflow_tasks.push(tokio::spawn(
                async move { store.record(entry(number)).await },
            ));
        }
        for task in overflow_tasks {
            task.await.unwrap().unwrap();
        }
        let entries: Vec<TraceEntry> =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let durations: HashSet<_> = entries.iter().map(|entry| entry.duration_ms).collect();
        assert_eq!(entries.len(), 100);
        assert_eq!(durations.len(), 100);
        assert!((100..120).all(|number| durations.contains(&number)));
    }

    fn entry(duration_ms: u64) -> TraceEntry {
        TraceEntry {
            timestamp: Utc::now(),
            model: "deepseek-v4-flash".into(),
            duration_ms,
            input_tokens: 1,
            output_tokens: 2,
            tool_names: vec!["read_file".into()],
            status: "completed".into(),
            error_code: None,
            error_detail: None,
        }
    }
}
