use crate::PathScope;
use agent_runtime::{
    ToolDefinition, ToolExecutionContext, ToolHandler, ToolHandlerError, ToolRisk,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

pub struct ShellExecTool {
    scope: PathScope,
    programs: HashMap<String, PathBuf>,
    max_output_bytes: usize,
}

impl ShellExecTool {
    pub fn new(
        scope: PathScope,
        programs: HashMap<String, PathBuf>,
        max_output_bytes: usize,
    ) -> Self {
        Self {
            scope,
            programs,
            max_output_bytes,
        }
    }

    pub fn definition() -> ToolDefinition {
        ToolDefinition {
            name: "shell.exec".into(),
            description: "Execute one application-configured program with a literal argv array; no shell parsing".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "program": {"type": "string", "minLength": 1, "maxLength": 64, "pattern": "^[A-Za-z0-9_.-]+$"},
                    "args": {"type": "array", "items": {"type": "string", "maxLength": 4096}, "maxItems": 64},
                    "cwd": {"type": "string", "maxLength": 1024},
                    "stdin": {"type": "string", "maxLength": 65536}
                },
                "required": ["program"],
                "additionalProperties": false
            }),
            risk: ToolRisk::Destructive,
            timeout_ms: 30_000,
            max_result_bytes: 256 * 1024,
        }
    }
}

#[async_trait]
impl ToolHandler for ShellExecTool {
    async fn execute(
        &self,
        _: ToolExecutionContext,
        arguments: Value,
    ) -> Result<String, ToolHandlerError> {
        let alias = arguments
            .get("program")
            .and_then(Value::as_str)
            .ok_or(ToolHandlerError)?;
        let executable = self.programs.get(alias).ok_or(ToolHandlerError)?;
        let cwd = self
            .scope
            .existing_directory(arguments.get("cwd").and_then(Value::as_str).unwrap_or(""))
            .map_err(|_| ToolHandlerError)?;
        let args = arguments
            .get("args")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .map(|value| value.as_str().map(str::to_owned).ok_or(ToolHandlerError))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let stdin = arguments
            .get("stdin")
            .and_then(Value::as_str)
            .map(str::as_bytes)
            .map(<[u8]>::to_vec);

        let mut command = tokio::process::Command::new(executable);
        command
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|_| ToolHandlerError)?;
        if let (Some(mut child_stdin), Some(input)) = (child.stdin.take(), stdin) {
            tokio::spawn(async move {
                let _ = child_stdin.write_all(&input).await;
                let _ = child_stdin.shutdown().await;
            });
        }

        let stdout = child.stdout.take().ok_or(ToolHandlerError)?;
        let stderr = child.stderr.take().ok_or(ToolHandlerError)?;
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<(bool, Vec<u8>)>(16);
        tokio::spawn(pump_output(stdout, true, sender.clone()));
        tokio::spawn(pump_output(stderr, false, sender.clone()));
        drop(sender);

        let mut stdout_bytes = Vec::new();
        let mut stderr_bytes = Vec::new();
        let mut total = 0usize;
        while let Some((is_stdout, chunk)) = receiver.recv().await {
            total = total.saturating_add(chunk.len());
            if total > self.max_output_bytes {
                let _ = child.kill().await;
                return Err(ToolHandlerError);
            }
            if is_stdout {
                stdout_bytes.extend_from_slice(&chunk);
            } else {
                stderr_bytes.extend_from_slice(&chunk);
            }
        }
        let status = child.wait().await.map_err(|_| ToolHandlerError)?;
        Ok(json!({
            "exit_code": status.code(),
            "success": status.success(),
            "stdout": String::from_utf8_lossy(&stdout_bytes),
            "stderr": String::from_utf8_lossy(&stderr_bytes)
        })
        .to_string())
    }

    fn summarize_arguments(&self, arguments: &Value) -> Option<String> {
        let program = arguments.get("program").and_then(Value::as_str)?;
        let cwd = arguments
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or("workspace root");
        let args = arguments
            .get("args")
            .and_then(Value::as_array)
            .map(|args| {
                args.iter()
                    .filter_map(Value::as_str)
                    .map(|arg| format!("{arg:?}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        Some(format!("Run configured program {program} {args} in {cwd}"))
    }
}

async fn pump_output<R>(
    mut reader: R,
    is_stdout: bool,
    sender: tokio::sync::mpsc::Sender<(bool, Vec<u8>)>,
) where
    R: AsyncRead + Unpin,
{
    let mut buffer = vec![0_u8; 8192];
    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        if sender
            .send((is_stdout, buffer[..read].to_vec()))
            .await
            .is_err()
        {
            break;
        }
    }
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
            cancellation: Arc::new(NeverCancel),
        }
    }

    #[test]
    fn delayed_marker_subprocess_helper() {
        if !std::path::Path::new(".agent-tools-process-helper").exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(800));
        std::fs::write("late-marker", "child survived").unwrap();
    }

    #[tokio::test]
    async fn dropping_execution_terminates_the_child() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".agent-tools-process-helper"), "ready").unwrap();
        let tool = ShellExecTool::new(
            PathScope::new(root.path(), true).unwrap(),
            HashMap::from([("helper".into(), std::env::current_exe().unwrap())]),
            4096,
        );
        let execution = tool.execute(
            context(),
            json!({
                "program": "helper",
                "args": ["--exact", "shell::tests::delayed_marker_subprocess_helper", "--nocapture"]
            }),
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), execution)
                .await
                .is_err()
        );
        tokio::time::sleep(std::time::Duration::from_millis(1_000)).await;
        assert!(!root.path().join("late-marker").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn passes_metacharacters_as_literal_argv_and_clears_environment() {
        let root = tempfile::tempdir().unwrap();
        let scope = PathScope::new(root.path(), true).unwrap();
        let programs = HashMap::from([("echo".into(), PathBuf::from("/bin/echo"))]);
        let tool = ShellExecTool::new(scope, programs, 4096);
        let marker = root.path().join("should-not-exist");
        let argument = format!("; touch {}", marker.display());
        let output = tool
            .execute(context(), json!({"program":"echo", "args":[argument]}))
            .await
            .unwrap();
        assert!(output.contains("; touch"));
        assert!(!marker.exists());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn executes_only_a_configured_absolute_program() {
        let root = tempfile::tempdir().unwrap();
        let whoami = PathBuf::from(r"C:\Windows\System32\whoami.exe");
        if !whoami.is_file() {
            return;
        }
        let tool = ShellExecTool::new(
            PathScope::new(root.path(), true).unwrap(),
            HashMap::from([("whoami".into(), whoami)]),
            4096,
        );
        let output = tool
            .execute(context(), json!({"program":"whoami"}))
            .await
            .unwrap();
        assert!(output.contains("\"success\":true"));
        assert!(tool
            .execute(
                context(),
                json!({"program":"cmd", "args":["/C", "echo bad"]})
            )
            .await
            .is_err());
        let bounded = ShellExecTool::new(
            PathScope::new(root.path(), true).unwrap(),
            HashMap::from([(
                "whoami".into(),
                PathBuf::from(r"C:\Windows\System32\whoami.exe"),
            )]),
            1,
        );
        assert!(bounded
            .execute(context(), json!({"program":"whoami"}))
            .await
            .is_err());
    }
}
