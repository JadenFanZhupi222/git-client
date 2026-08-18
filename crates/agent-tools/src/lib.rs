mod artifact;
mod filesystem;
mod path_scope;
mod search;
mod shell;
mod web;

use agent_runtime::{
    PermissionDecision, PermissionPolicy, PermissionRule, ToolMatcher, ToolRegistrationError,
    ToolRegistry, ToolRisk,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub use artifact::ArtifactWriteTool;
pub use filesystem::{FilesystemListTool, FilesystemReadTool, FilesystemWriteTool, PatchApplyTool};
pub use path_scope::{PathScope, PathScopeError};
pub use search::SearchTextTool;
pub use shell::ShellExecTool;
pub use web::{is_public_ip, WebFetchTool};

pub(crate) fn content_digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

pub fn digest_content(bytes: &[u8]) -> String {
    content_digest(bytes)
}

#[derive(Debug, Clone)]
pub struct ShellProgram {
    pub alias: String,
    pub executable: PathBuf,
}

#[derive(Debug, Clone)]
pub struct WebToolPolicy {
    pub allowed_domains: Vec<String>,
    pub allow_subdomains: bool,
    pub allow_http: bool,
    pub allow_private_network: bool,
    pub max_response_bytes: usize,
}

impl Default for WebToolPolicy {
    fn default() -> Self {
        Self {
            allowed_domains: Vec::new(),
            allow_subdomains: false,
            allow_http: false,
            allow_private_network: false,
            max_response_bytes: 256 * 1024,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuiltinToolConfig {
    pub workspace_root: PathBuf,
    pub artifact_root: PathBuf,
    pub shell_programs: Vec<ShellProgram>,
    pub web: WebToolPolicy,
    pub max_file_bytes: usize,
    pub max_shell_output_bytes: usize,
}

impl BuiltinToolConfig {
    pub fn local_only(workspace_root: PathBuf, artifact_root: PathBuf) -> Self {
        Self {
            workspace_root,
            artifact_root,
            shell_programs: Vec::new(),
            web: WebToolPolicy::default(),
            max_file_bytes: 512 * 1024,
            max_shell_output_bytes: 256 * 1024,
        }
    }
}

pub struct BuiltinToolPack {
    pub registry: Arc<ToolRegistry>,
    pub policy: PermissionPolicy,
}

#[derive(Debug)]
pub enum BuiltinToolError {
    Scope(PathScopeError),
    InvalidConfig(&'static str),
    Registration(ToolRegistrationError),
}

impl From<PathScopeError> for BuiltinToolError {
    fn from(value: PathScopeError) -> Self {
        Self::Scope(value)
    }
}

impl From<ToolRegistrationError> for BuiltinToolError {
    fn from(value: ToolRegistrationError) -> Self {
        Self::Registration(value)
    }
}

pub fn build_builtin_tool_pack(
    config: BuiltinToolConfig,
) -> Result<BuiltinToolPack, BuiltinToolError> {
    if config.max_file_bytes == 0 || config.max_file_bytes > 1024 * 1024 {
        return Err(BuiltinToolError::InvalidConfig("max_file_bytes"));
    }
    if config.max_shell_output_bytes == 0 || config.max_shell_output_bytes > 1024 * 1024 {
        return Err(BuiltinToolError::InvalidConfig("max_shell_output_bytes"));
    }
    if config.web.max_response_bytes == 0 || config.web.max_response_bytes > 1024 * 1024 {
        return Err(BuiltinToolError::InvalidConfig("max_web_response_bytes"));
    }

    let workspace = PathScope::new(&config.workspace_root, true)?;
    let artifacts = PathScope::new(&config.artifact_root, false)?;
    let programs = validate_programs(config.shell_programs)?;
    let web = WebFetchTool::new(config.web)?;

    let mut registry = ToolRegistry::default();
    registry.register(
        FilesystemReadTool::definition(config.max_file_bytes),
        Arc::new(FilesystemReadTool::new(
            workspace.clone(),
            config.max_file_bytes,
        )),
    )?;
    registry.register(
        FilesystemListTool::definition(),
        Arc::new(FilesystemListTool::new(workspace.clone(), 500)),
    )?;
    registry.register(
        FilesystemWriteTool::definition(config.max_file_bytes),
        Arc::new(FilesystemWriteTool::new(
            workspace.clone(),
            config.max_file_bytes,
        )),
    )?;
    registry.register(
        SearchTextTool::definition(),
        Arc::new(SearchTextTool::new(
            workspace.clone(),
            config.max_file_bytes,
        )),
    )?;
    registry.register(
        PatchApplyTool::definition(config.max_file_bytes),
        Arc::new(PatchApplyTool::new(
            workspace.clone(),
            config.max_file_bytes,
        )),
    )?;
    registry.register(
        ShellExecTool::definition(),
        Arc::new(ShellExecTool::new(
            workspace,
            programs,
            config.max_shell_output_bytes,
        )),
    )?;
    registry.register(WebFetchTool::definition(), Arc::new(web))?;
    registry.register(
        ArtifactWriteTool::definition(config.max_file_bytes),
        Arc::new(ArtifactWriteTool::new(artifacts, config.max_file_bytes)),
    )?;

    let policy = PermissionPolicy::new(vec![
        exact_rule(
            "filesystem.read",
            ToolRisk::ReadOnly,
            PermissionDecision::Allow,
        ),
        exact_rule(
            "filesystem.list",
            ToolRisk::ReadOnly,
            PermissionDecision::Allow,
        ),
        exact_rule("search.text", ToolRisk::ReadOnly, PermissionDecision::Allow),
        exact_rule("filesystem.write", ToolRisk::Write, PermissionDecision::Ask),
        exact_rule("patch.apply", ToolRisk::Write, PermissionDecision::Ask),
        exact_rule("artifact.write", ToolRisk::Write, PermissionDecision::Ask),
        exact_rule("shell.exec", ToolRisk::Destructive, PermissionDecision::Ask),
        exact_rule("web.fetch", ToolRisk::External, PermissionDecision::Ask),
    ]);
    Ok(BuiltinToolPack {
        registry: Arc::new(registry),
        policy,
    })
}

fn exact_rule(name: &str, risk: ToolRisk, decision: PermissionDecision) -> PermissionRule {
    PermissionRule {
        matcher: ToolMatcher::Exact(name.into()),
        risk: Some(risk),
        decision,
    }
}

fn validate_programs(
    programs: Vec<ShellProgram>,
) -> Result<HashMap<String, PathBuf>, BuiltinToolError> {
    let mut output = HashMap::new();
    for program in programs {
        let valid_alias = !program.alias.is_empty()
            && program.alias.len() <= 64
            && program
                .alias
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
        if !valid_alias || output.contains_key(&program.alias) {
            return Err(BuiltinToolError::InvalidConfig("shell_alias"));
        }
        let executable = program
            .executable
            .canonicalize()
            .map_err(|_| BuiltinToolError::InvalidConfig("shell_executable"))?;
        if !executable.is_file() {
            return Err(BuiltinToolError::InvalidConfig("shell_executable"));
        }
        let executable_name = executable
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_ascii_lowercase)
            .ok_or(BuiltinToolError::InvalidConfig("shell_executable"))?;
        if matches!(
            executable_name.as_str(),
            "cmd"
                | "cmd.exe"
                | "powershell"
                | "powershell.exe"
                | "pwsh"
                | "pwsh.exe"
                | "sh"
                | "bash"
                | "zsh"
                | "fish"
                | "dash"
                | "ksh"
                | "csh"
                | "tcsh"
        ) {
            return Err(BuiltinToolError::InvalidConfig("shell_interpreter"));
        }
        output.insert(program.alias, executable);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_registers_stable_tools_and_exact_default_policy() {
        let workspace = tempfile::tempdir().unwrap();
        let artifacts = tempfile::tempdir().unwrap();
        let pack = build_builtin_tool_pack(BuiltinToolConfig::local_only(
            workspace.path().into(),
            artifacts.path().into(),
        ))
        .unwrap();
        let definitions = pack.registry.definitions();
        assert_eq!(
            definitions
                .iter()
                .map(|definition| definition.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "artifact.write",
                "filesystem.list",
                "filesystem.read",
                "filesystem.write",
                "patch.apply",
                "search.text",
                "shell.exec",
                "web.fetch",
            ]
        );
        assert_eq!(
            pack.policy.evaluate("filesystem.read", ToolRisk::ReadOnly),
            PermissionDecision::Allow
        );
        assert_eq!(
            pack.policy.evaluate("patch.apply", ToolRisk::Write),
            PermissionDecision::Ask
        );
        assert_eq!(
            pack.policy.evaluate("unknown", ToolRisk::ReadOnly),
            PermissionDecision::Deny
        );
    }

    #[test]
    fn shell_interpreters_cannot_be_registered() {
        #[cfg(windows)]
        let executable = PathBuf::from(r"C:\Windows\System32\cmd.exe");
        #[cfg(not(windows))]
        let executable = PathBuf::from("/bin/sh");

        let error = validate_programs(vec![ShellProgram {
            alias: "interpreter".into(),
            executable,
        }])
        .unwrap_err();
        assert!(matches!(
            error,
            BuiltinToolError::InvalidConfig("shell_interpreter")
        ));
    }

    #[test]
    fn pack_rejects_unsafe_limits_domains_and_aliases() {
        let workspace = tempfile::tempdir().unwrap();
        let artifacts = tempfile::tempdir().unwrap();

        let mut invalid_limit =
            BuiltinToolConfig::local_only(workspace.path().into(), artifacts.path().into());
        invalid_limit.max_file_bytes = 0;
        assert!(matches!(
            build_builtin_tool_pack(invalid_limit),
            Err(BuiltinToolError::InvalidConfig("max_file_bytes"))
        ));

        let mut invalid_domain =
            BuiltinToolConfig::local_only(workspace.path().into(), artifacts.path().into());
        invalid_domain.web.allowed_domains = vec!["bad_domain.example".into()];
        assert!(matches!(
            build_builtin_tool_pack(invalid_domain),
            Err(BuiltinToolError::InvalidConfig("web_domain"))
        ));

        let mut invalid_alias =
            BuiltinToolConfig::local_only(workspace.path().into(), artifacts.path().into());
        invalid_alias.shell_programs = vec![ShellProgram {
            alias: "bad alias".into(),
            executable: std::env::current_exe().unwrap(),
        }];
        assert!(matches!(
            build_builtin_tool_pack(invalid_alias),
            Err(BuiltinToolError::InvalidConfig("shell_alias"))
        ));
    }
}
