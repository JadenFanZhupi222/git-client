use agent_runtime::{
    NeverCancel, PermissionDecision, ToolApprovalRequest, ToolApprovalResolver, ToolCall,
    ToolExecutionError, ToolExecutor, ToolOutcome, ToolRun, ToolRunLimits,
};
use agent_tools::{build_builtin_tool_pack, BuiltinToolConfig};
use async_trait::async_trait;
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct AllowAndRecord(Mutex<Vec<ToolApprovalRequest>>);

#[async_trait]
impl ToolApprovalResolver for AllowAndRecord {
    async fn resolve(&self, request: ToolApprovalRequest) -> PermissionDecision {
        self.0.lock().unwrap().push(request);
        PermissionDecision::Allow
    }
}

#[tokio::test]
async fn read_is_allowed_but_write_patch_and_artifact_require_one_shot_approval() {
    let workspace = tempfile::tempdir().unwrap();
    let artifacts = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("a.txt"), "before").unwrap();
    let pack = build_builtin_tool_pack(BuiltinToolConfig::local_only(
        workspace.path().into(),
        artifacts.path().into(),
    ))
    .unwrap();
    let approvals = Arc::new(AllowAndRecord::default());
    let executor = ToolExecutor::new(pack.registry, pack.policy).with_approvals(approvals.clone());
    let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel));

    let read = executor
        .execute(
            &run,
            ToolCall::with_call_id("filesystem.read", "read", json!({"path":"a.txt"})),
        )
        .await
        .unwrap();
    assert_eq!(read.outcome, ToolOutcome::Success);
    assert_eq!(read.content, "before");
    assert!(approvals.0.lock().unwrap().is_empty());

    executor
        .execute(
            &run,
            ToolCall::with_call_id(
                "filesystem.write",
                "write",
                json!({"path":"b.txt", "content":"created"}),
            ),
        )
        .await
        .unwrap();
    executor
        .execute(
            &run,
            ToolCall::with_call_id(
                "patch.apply",
                "patch",
                json!({"path":"a.txt", "expected":"before", "replacement":"after"}),
            ),
        )
        .await
        .unwrap();
    executor
        .execute(
            &run,
            ToolCall::with_call_id(
                "artifact.write",
                "artifact",
                json!({"name":"report.md", "media_type":"text/markdown", "content":"report"}),
            ),
        )
        .await
        .unwrap();

    let requests = approvals.0.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].tool_name, "filesystem.write");
    assert_eq!(requests[1].tool_name, "patch.apply");
    assert_eq!(requests[2].tool_name, "artifact.write");
    assert!(requests.iter().all(|request| request.summary.is_some()));
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("a.txt")).unwrap(),
        "after"
    );
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("b.txt")).unwrap(),
        "created"
    );
}

#[tokio::test]
async fn schema_and_scope_fail_before_any_outside_mutation() {
    let parent = tempfile::tempdir().unwrap();
    let workspace_path = parent.path().join("workspace");
    let artifact_path = parent.path().join("artifacts");
    std::fs::create_dir(&workspace_path).unwrap();
    std::fs::create_dir(&artifact_path).unwrap();
    let outside = parent.path().join("outside.txt");
    let pack =
        build_builtin_tool_pack(BuiltinToolConfig::local_only(workspace_path, artifact_path))
            .unwrap();
    let approvals = Arc::new(AllowAndRecord::default());
    let executor = ToolExecutor::new(pack.registry, pack.policy).with_approvals(approvals.clone());
    let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel));

    let schema_error = executor
        .execute(
            &run,
            ToolCall::with_call_id(
                "filesystem.write",
                "bad-schema",
                json!({"path":"ok.txt", "content":"x", "unexpected":true}),
            ),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        schema_error,
        ToolExecutionError::InvalidInput {
            code: "additional_property",
            ..
        }
    ));
    assert!(approvals.0.lock().unwrap().is_empty());

    let escaped = executor
        .execute(
            &run,
            ToolCall::with_call_id(
                "filesystem.write",
                "escape",
                json!({"path":"../outside.txt", "content":"x"}),
            ),
        )
        .await
        .unwrap();
    assert_eq!(escaped.outcome, ToolOutcome::Failed);
    assert!(!outside.exists());
}
