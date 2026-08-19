use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use review_agent::{PermissionDecision, PermissionPolicy, PermissionRule, ToolMatcher, ToolRisk};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceStateError;

pub(crate) fn local_agent_policy() -> PermissionPolicy {
    PermissionPolicy::new(vec![
        rule(
            "filesystem.read",
            ToolRisk::ReadOnly,
            PermissionDecision::Allow,
        ),
        rule(
            "filesystem.list",
            ToolRisk::ReadOnly,
            PermissionDecision::Allow,
        ),
        rule("search.text", ToolRisk::ReadOnly, PermissionDecision::Allow),
        rule("filesystem.write", ToolRisk::Write, PermissionDecision::Ask),
        rule("patch.apply", ToolRisk::Write, PermissionDecision::Ask),
        rule("artifact.write", ToolRisk::Write, PermissionDecision::Ask),
        rule(
            "shell.exec",
            ToolRisk::Destructive,
            PermissionDecision::Deny,
        ),
        rule("web.fetch", ToolRisk::External, PermissionDecision::Deny),
    ])
}

fn rule(name: &str, risk: ToolRisk, decision: PermissionDecision) -> PermissionRule {
    PermissionRule {
        matcher: ToolMatcher::Exact(name.into()),
        risk: Some(risk),
        decision,
    }
}

pub(crate) async fn workspace_digest(repository: &Path) -> Result<String, WorkspaceStateError> {
    let output = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repository)
        .args([
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ])
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "safe.directory")
        .env("GIT_CONFIG_VALUE_0", repository)
        .output()
        .await
        .map_err(|_| WorkspaceStateError)?;
    if !output.status.success() {
        return Err(WorkspaceStateError);
    }
    let hash = output
        .stdout
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            hash.wrapping_mul(0x100000001b3) ^ u64::from(*byte)
        });
    Ok(format!("workspace-{hash:016x}"))
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_policy_keeps_the_expected_security_boundary() {
        let policy = local_agent_policy();
        assert_eq!(
            policy.evaluate("filesystem.read", ToolRisk::ReadOnly),
            PermissionDecision::Allow
        );
        assert_eq!(
            policy.evaluate("filesystem.write", ToolRisk::Write),
            PermissionDecision::Ask
        );
        assert_eq!(
            policy.evaluate("shell.exec", ToolRisk::Destructive),
            PermissionDecision::Deny
        );
        assert_eq!(
            policy.evaluate("unknown", ToolRisk::External),
            PermissionDecision::Deny
        );
    }

    #[tokio::test]
    async fn workspace_digest_changes_with_status_and_rejects_non_repositories() {
        let repository = tempfile::tempdir().unwrap();
        let initialized = std::process::Command::new("git")
            .arg("init")
            .arg(repository.path())
            .status()
            .unwrap();
        assert!(initialized.success());
        let clean = workspace_digest(repository.path()).await.unwrap();
        std::fs::write(repository.path().join("untracked.txt"), "changed").unwrap();
        let dirty = workspace_digest(repository.path()).await.unwrap();
        assert_ne!(clean, dirty);

        let not_repository = tempfile::tempdir().unwrap();
        assert_eq!(
            workspace_digest(not_repository.path()).await,
            Err(WorkspaceStateError)
        );
    }
}
