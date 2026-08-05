use crate::{ReviewError, TraceEntry, TraceSink};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::sync::Mutex;

pub struct SanitizedTraceStore {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl SanitizedTraceStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Mutex::new(()),
        }
    }
}

#[async_trait]
impl TraceSink for SanitizedTraceStore {
    async fn record(&self, entry: TraceEntry) -> Result<(), ReviewError> {
        let _guard = self.write_lock.lock().await;
        let mut entries: Vec<TraceEntry> = match tokio::fs::read(&self.path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
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
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|_| {
                ReviewError::NetworkError("trace directory could not be created".into())
            })?;
        }
        let temporary = self.path.with_extension("tmp");
        tokio::fs::write(&temporary, bytes)
            .await
            .map_err(|_| ReviewError::NetworkError("trace file could not be written".into()))?;
        if tokio::fs::rename(&temporary, &self.path).await.is_err() {
            match tokio::fs::remove_file(&self.path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => {
                    return Err(ReviewError::NetworkError(
                        "trace file could not be replaced".into(),
                    ))
                }
            }
            tokio::fs::rename(&temporary, &self.path)
                .await
                .map_err(|_| {
                    ReviewError::NetworkError("trace file could not be replaced".into())
                })?;
        }
        Ok(())
    }
}

fn sanitize(mut entry: TraceEntry) -> TraceEntry {
    if entry.model != "deepseek-v4-flash" {
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
    entry
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
                })
                .await
                .unwrap();
        }
        let serialized = std::fs::read_to_string(path).unwrap();
        let entries: Vec<TraceEntry> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(entries.len(), 100);
        assert_eq!(entries[0].duration_ms, 5);
        assert!(!serialized.contains("SECRET_KEY"));
        assert!(!serialized.contains("CODE_MARKER"));
        assert!(!serialized.contains("prompt"));
    }
}
