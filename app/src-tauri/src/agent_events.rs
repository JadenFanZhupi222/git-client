use async_trait::async_trait;
use ipc_types::{AgentEventDto, IpcError, ToolApprovalDecisionDto, ToolApprovalResolutionDto};
use review_agent::{
    AgentEvent, AgentEventSink, PermissionDecision, ToolApprovalRequest, ToolApprovalResolver,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Emitter;

pub(crate) struct AppAgentEventEmitter(pub tauri::AppHandle);

impl AgentEventSink for AppAgentEventEmitter {
    fn emit(&self, event: AgentEvent) {
        let _ = self.0.emit("agent-event", AgentEventDto::from(event));
    }
}

struct PendingApproval {
    run_id: String,
    sender: tokio::sync::oneshot::Sender<PermissionDecision>,
}

#[derive(Clone, Default)]
pub(crate) struct ToolApprovalRegistry {
    pending: Arc<Mutex<HashMap<String, PendingApproval>>>,
}

struct PendingGuard {
    approval_id: String,
    pending: Arc<Mutex<HashMap<String, PendingApproval>>>,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.approval_id);
    }
}

#[async_trait]
impl ToolApprovalResolver for ToolApprovalRegistry {
    async fn resolve(&self, request: ToolApprovalRequest) -> PermissionDecision {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let inserted = self
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                request.approval_id.clone(),
                PendingApproval {
                    run_id: request.run_id,
                    sender,
                },
            )
            .is_none();
        if !inserted {
            return PermissionDecision::Deny;
        }
        let _guard = PendingGuard {
            approval_id: request.approval_id,
            pending: Arc::clone(&self.pending),
        };
        receiver.await.unwrap_or(PermissionDecision::Deny)
    }
}

impl ToolApprovalRegistry {
    fn resolve_pending(&self, resolution: ToolApprovalResolutionDto) -> Result<(), IpcError> {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let Some(waiter) = pending.get(&resolution.approval_id) else {
            return Err(approval_error(
                "TOOL_APPROVAL_EXPIRED",
                "Tool approval is unknown, expired, or already resolved",
            ));
        };
        if waiter.run_id != resolution.run_id {
            return Err(approval_error(
                "TOOL_APPROVAL_RUN_MISMATCH",
                "Tool approval does not belong to this run",
            ));
        }
        let Some(waiter) = pending.remove(&resolution.approval_id) else {
            return Err(approval_error(
                "TOOL_APPROVAL_EXPIRED",
                "Tool approval is no longer pending",
            ));
        };
        let decision = match resolution.decision {
            ToolApprovalDecisionDto::Allow => PermissionDecision::Allow,
            ToolApprovalDecisionDto::Deny => PermissionDecision::Deny,
        };
        waiter.sender.send(decision).map_err(|_| {
            approval_error(
                "TOOL_APPROVAL_EXPIRED",
                "Tool approval is no longer waiting for a decision",
            )
        })
    }
}

fn approval_error(code: &str, message: &str) -> IpcError {
    IpcError {
        code: code.into(),
        message: message.into(),
        recoverable: false,
    }
}

#[tauri::command]
pub(crate) async fn resolve_tool_approval(
    registry: tauri::State<'_, ToolApprovalRegistry>,
    resolution: ToolApprovalResolutionDto,
) -> Result<(), IpcError> {
    registry.resolve_pending(resolution)
}

#[cfg(test)]
mod tests {
    use super::*;
    use review_agent::ToolRisk;

    fn request() -> ToolApprovalRequest {
        ToolApprovalRequest {
            run_id: "run-1".into(),
            approval_id: "approval-1".into(),
            call_id: "call-1".into(),
            tool_name: "filesystem.write".into(),
            risk: ToolRisk::Write,
            summary: Some("write one file".into()),
        }
    }

    #[tokio::test]
    async fn approval_is_run_bound_and_one_shot() {
        let registry = ToolApprovalRegistry::default();
        let resolver = registry.clone();
        let waiter = tokio::spawn(async move { resolver.resolve(request()).await });
        tokio::task::yield_now().await;
        let mismatch = registry.resolve_pending(ToolApprovalResolutionDto {
            run_id: "other-run".into(),
            approval_id: "approval-1".into(),
            decision: ToolApprovalDecisionDto::Allow,
        });
        assert_eq!(mismatch.unwrap_err().code, "TOOL_APPROVAL_RUN_MISMATCH");
        registry
            .resolve_pending(ToolApprovalResolutionDto {
                run_id: "run-1".into(),
                approval_id: "approval-1".into(),
                decision: ToolApprovalDecisionDto::Allow,
            })
            .unwrap();
        assert_eq!(waiter.await.unwrap(), PermissionDecision::Allow);
        let replay = registry.resolve_pending(ToolApprovalResolutionDto {
            run_id: "run-1".into(),
            approval_id: "approval-1".into(),
            decision: ToolApprovalDecisionDto::Deny,
        });
        assert_eq!(replay.unwrap_err().code, "TOOL_APPROVAL_EXPIRED");
    }

    #[tokio::test]
    async fn dropped_waiter_removes_pending_approval() {
        let registry = ToolApprovalRegistry::default();
        let resolver = registry.clone();
        let waiter = tokio::spawn(async move { resolver.resolve(request()).await });
        tokio::task::yield_now().await;
        waiter.abort();
        let _ = waiter.await;
        tokio::task::yield_now().await;
        assert!(registry.pending.lock().unwrap().is_empty());
    }
}
