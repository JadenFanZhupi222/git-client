use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

use crate::ToolCall;

const HARD_MAX_RESULT_BYTES: usize = 1024 * 1024;
const TRUNCATION_MARKER: &str = "\n[tool result truncated]";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    #[default]
    ReadOnly,
    Write,
    Destructive,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    Success,
    Denied,
    Failed,
    Timeout,
    Cancelled,
    InvalidInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub outcome: ToolOutcome,
    pub content: String,
    pub truncated: bool,
    pub content_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolMatcher {
    Exact(String),
    Prefix(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRule {
    pub matcher: ToolMatcher,
    pub risk: Option<ToolRisk>,
    pub decision: PermissionDecision,
}

#[derive(Debug, Clone, Default)]
pub struct PermissionPolicy {
    rules: Vec<PermissionRule>,
}

impl PermissionPolicy {
    pub fn new(rules: Vec<PermissionRule>) -> Self {
        Self { rules }
    }

    pub fn evaluate(&self, name: &str, risk: ToolRisk) -> PermissionDecision {
        self.rules
            .iter()
            .find(|rule| {
                rule.risk.is_none_or(|expected| expected == risk)
                    && match &rule.matcher {
                        ToolMatcher::Exact(expected) => expected == name,
                        ToolMatcher::Prefix(prefix) => name.starts_with(prefix),
                    }
            })
            .map(|rule| rule.decision)
            .unwrap_or(PermissionDecision::Deny)
    }
}

fn restrict_decision(
    application: PermissionDecision,
    run: Option<PermissionDecision>,
) -> PermissionDecision {
    match (application, run) {
        (PermissionDecision::Deny, _) | (_, Some(PermissionDecision::Deny)) => {
            PermissionDecision::Deny
        }
        (PermissionDecision::Ask, _) | (_, Some(PermissionDecision::Ask)) => {
            PermissionDecision::Ask
        }
        (PermissionDecision::Allow, _) => PermissionDecision::Allow,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolApprovalRequest {
    pub run_id: String,
    pub approval_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub risk: ToolRisk,
    pub summary: Option<String>,
}

#[async_trait]
pub trait ToolCancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;

    async fn cancelled(&self) {
        while !self.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[derive(Debug, Default)]
pub struct NeverCancel;

#[async_trait]
impl ToolCancellation for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[async_trait]
pub trait ToolApprovalResolver: Send + Sync {
    async fn resolve(&self, request: ToolApprovalRequest) -> PermissionDecision;
}

#[derive(Debug, Default)]
pub struct DenyAllApprovals;

#[async_trait]
impl ToolApprovalResolver for DenyAllApprovals {
    async fn resolve(&self, _: ToolApprovalRequest) -> PermissionDecision {
        PermissionDecision::Deny
    }
}

#[derive(Clone)]
pub struct ToolExecutionContext {
    pub run_id: String,
    pub call_id: String,
    pub cancellation: Arc<dyn ToolCancellation>,
}

#[derive(Debug, Error)]
#[error("tool handler failed")]
pub struct ToolHandlerError;

#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn execute(
        &self,
        context: ToolExecutionContext,
        arguments: Value,
    ) -> Result<String, ToolHandlerError>;

    fn summarize_arguments(&self, _: &Value) -> Option<String> {
        None
    }

    fn sanitize_result(&self, content: String) -> String {
        content
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ToolRegistrationError {
    #[error("invalid tool definition: {0}")]
    InvalidDefinition(&'static str),
    #[error("duplicate tool name")]
    DuplicateName,
    #[error("invalid tool input schema: {0}")]
    InvalidSchema(&'static str),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ToolExecutionError {
    #[error("invalid tool call: {0}")]
    InvalidCall(&'static str),
    #[error("unknown tool")]
    UnknownTool,
    #[error("invalid tool input at {path}: {code}")]
    InvalidInput { path: String, code: &'static str },
    #[error("tool run budget exceeded: {0}")]
    BudgetExceeded(&'static str),
    #[error("tool run cancelled")]
    Cancelled,
    #[error("tool execution timed out")]
    Timeout,
}

#[derive(Clone)]
struct RegisteredTool {
    definition: crate::ToolDefinition,
    schema: CompiledSchema,
    handler: Arc<dyn ToolHandler>,
}

#[derive(Default)]
pub struct ToolRegistry {
    entries: HashMap<String, RegisteredTool>,
}

impl ToolRegistry {
    pub fn register(
        &mut self,
        definition: crate::ToolDefinition,
        handler: Arc<dyn ToolHandler>,
    ) -> Result<(), ToolRegistrationError> {
        validate_definition(&definition)?;
        if self.entries.contains_key(&definition.name) {
            return Err(ToolRegistrationError::DuplicateName);
        }
        let schema = CompiledSchema::compile(&definition.input_schema)?;
        self.entries.insert(
            definition.name.clone(),
            RegisteredTool {
                definition,
                schema,
                handler,
            },
        );
        Ok(())
    }

    pub fn definitions(&self) -> Vec<crate::ToolDefinition> {
        let mut definitions = self
            .entries
            .values()
            .map(|entry| entry.definition.clone())
            .collect::<Vec<_>>();
        definitions.sort_by(|left, right| left.name.cmp(&right.name));
        definitions
    }

    fn get(&self, name: &str) -> Option<&RegisteredTool> {
        self.entries.get(name)
    }
}

fn validate_definition(definition: &crate::ToolDefinition) -> Result<(), ToolRegistrationError> {
    let valid_name = !definition.name.is_empty()
        && definition.name.len() <= 128
        && definition.name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':' | b'/')
        });
    if !valid_name {
        return Err(ToolRegistrationError::InvalidDefinition("name"));
    }
    if definition.description.trim().is_empty() || definition.description.len() > 4096 {
        return Err(ToolRegistrationError::InvalidDefinition("description"));
    }
    if definition.timeout_ms == 0 {
        return Err(ToolRegistrationError::InvalidDefinition("timeout"));
    }
    if definition.max_result_bytes == 0 || definition.max_result_bytes > HARD_MAX_RESULT_BYTES {
        return Err(ToolRegistrationError::InvalidDefinition("result_limit"));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct CompiledSchema(Value);

impl CompiledSchema {
    fn compile(schema: &Value) -> Result<Self, ToolRegistrationError> {
        audit_schema(schema)?;
        Ok(Self(schema.clone()))
    }

    fn validate(&self, value: &Value) -> Result<(), ToolExecutionError> {
        validate_value(&self.0, value, "$")
    }
}

fn audit_schema(schema: &Value) -> Result<(), ToolRegistrationError> {
    let object = schema
        .as_object()
        .ok_or(ToolRegistrationError::InvalidSchema("schema_not_object"))?;
    const ALLOWED: &[&str] = &[
        "$schema",
        "title",
        "description",
        "type",
        "properties",
        "required",
        "additionalProperties",
        "minProperties",
        "maxProperties",
        "items",
        "minItems",
        "maxItems",
        "uniqueItems",
        "minLength",
        "maxLength",
        "pattern",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
        "enum",
        "const",
        "oneOf",
    ];
    if object.keys().any(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(ToolRegistrationError::InvalidSchema("unsupported_keyword"));
    }
    if let Some(kind) = object.get("type") {
        let Some(kind) = kind.as_str() else {
            return Err(ToolRegistrationError::InvalidSchema("invalid_type"));
        };
        if !matches!(
            kind,
            "object" | "array" | "string" | "integer" | "number" | "boolean" | "null"
        ) {
            return Err(ToolRegistrationError::InvalidSchema("unsupported_type"));
        }
    }
    for annotation in ["$schema", "title", "description"] {
        if object
            .get(annotation)
            .is_some_and(|value| !value.is_string())
        {
            return Err(ToolRegistrationError::InvalidSchema("invalid_annotation"));
        }
    }
    if object.contains_key("$ref") {
        return Err(ToolRegistrationError::InvalidSchema("external_reference"));
    }
    if let Some(properties) = object.get("properties") {
        let properties = properties
            .as_object()
            .ok_or(ToolRegistrationError::InvalidSchema("invalid_properties"))?;
        for child in properties.values() {
            audit_schema(child)?;
        }
    }
    if let Some(required) = object.get("required") {
        let required = required
            .as_array()
            .ok_or(ToolRegistrationError::InvalidSchema("invalid_required"))?;
        let mut names = HashSet::new();
        for name in required {
            let name = name
                .as_str()
                .ok_or(ToolRegistrationError::InvalidSchema("invalid_required"))?;
            if !names.insert(name) {
                return Err(ToolRegistrationError::InvalidSchema("duplicate_required"));
            }
        }
    }
    if object
        .get("additionalProperties")
        .is_some_and(|value| !value.is_boolean())
    {
        return Err(ToolRegistrationError::InvalidSchema(
            "invalid_additional_properties",
        ));
    }
    for key in [
        "minProperties",
        "maxProperties",
        "minItems",
        "maxItems",
        "minLength",
        "maxLength",
    ] {
        if object
            .get(key)
            .is_some_and(|value| value.as_u64().is_none())
        {
            return Err(ToolRegistrationError::InvalidSchema(
                "invalid_unsigned_limit",
            ));
        }
    }
    for (min, max) in [
        ("minProperties", "maxProperties"),
        ("minItems", "maxItems"),
        ("minLength", "maxLength"),
    ] {
        if let (Some(min), Some(max)) = (
            object.get(min).and_then(Value::as_u64),
            object.get(max).and_then(Value::as_u64),
        ) {
            if min > max {
                return Err(ToolRegistrationError::InvalidSchema("contradictory_limits"));
            }
        }
    }
    if object
        .get("uniqueItems")
        .is_some_and(|value| !value.is_boolean())
    {
        return Err(ToolRegistrationError::InvalidSchema("invalid_unique_items"));
    }
    if let Some(items) = object.get("items") {
        audit_schema(items)?;
    }
    if let Some(pattern) = object.get("pattern") {
        let pattern = pattern
            .as_str()
            .ok_or(ToolRegistrationError::InvalidSchema("invalid_pattern"))?;
        Regex::new(pattern).map_err(|_| ToolRegistrationError::InvalidSchema("invalid_pattern"))?;
    }
    for key in ["minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"] {
        if object
            .get(key)
            .is_some_and(|value| value.as_f64().is_none())
        {
            return Err(ToolRegistrationError::InvalidSchema("invalid_number_limit"));
        }
    }
    if object
        .get("multipleOf")
        .is_some_and(|value| value.as_f64().is_none_or(|number| number <= 0.0))
    {
        return Err(ToolRegistrationError::InvalidSchema("invalid_multiple_of"));
    }
    if object
        .get("enum")
        .is_some_and(|value| value.as_array().is_none_or(Vec::is_empty))
    {
        return Err(ToolRegistrationError::InvalidSchema("invalid_enum"));
    }
    if let Some(one_of) = object.get("oneOf") {
        let one_of = one_of
            .as_array()
            .filter(|choices| !choices.is_empty())
            .ok_or(ToolRegistrationError::InvalidSchema("invalid_one_of"))?;
        for child in one_of {
            audit_schema(child)?;
        }
    }
    Ok(())
}

fn invalid(path: &str, code: &'static str) -> ToolExecutionError {
    ToolExecutionError::InvalidInput {
        path: path.chars().take(256).collect(),
        code,
    }
}

fn validate_value(schema: &Value, value: &Value, path: &str) -> Result<(), ToolExecutionError> {
    let schema = schema.as_object().expect("schema audited at registration");
    if let Some(expected) = schema.get("const") {
        if value != expected {
            return Err(invalid(path, "const"));
        }
    }
    if let Some(choices) = schema.get("enum").and_then(Value::as_array) {
        if !choices.contains(value) {
            return Err(invalid(path, "enum"));
        }
    }
    if let Some(choices) = schema.get("oneOf").and_then(Value::as_array) {
        let matches = choices
            .iter()
            .filter(|choice| validate_value(choice, value, path).is_ok())
            .count();
        if matches != 1 {
            return Err(invalid(path, "one_of"));
        }
    }
    if let Some(kind) = schema.get("type").and_then(Value::as_str) {
        let valid = match kind {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => false,
        };
        if !valid {
            return Err(invalid(path, "type"));
        }
    }
    if let Some(object) = value.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    return Err(invalid(path, "required"));
                }
            }
        }
        let additional = schema
            .get("additionalProperties")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        for (name, child) in object {
            match properties.and_then(|properties| properties.get(name)) {
                Some(child_schema) => {
                    validate_value(child_schema, child, &format!("{path}.{name}"))?
                }
                None if !additional => return Err(invalid(path, "additional_property")),
                None => {}
            }
        }
        let len = object.len() as u64;
        if schema
            .get("minProperties")
            .and_then(Value::as_u64)
            .is_some_and(|min| len < min)
        {
            return Err(invalid(path, "min_properties"));
        }
        if schema
            .get("maxProperties")
            .and_then(Value::as_u64)
            .is_some_and(|max| len > max)
        {
            return Err(invalid(path, "max_properties"));
        }
    }
    if let Some(array) = value.as_array() {
        let len = array.len() as u64;
        if schema
            .get("minItems")
            .and_then(Value::as_u64)
            .is_some_and(|min| len < min)
        {
            return Err(invalid(path, "min_items"));
        }
        if schema
            .get("maxItems")
            .and_then(Value::as_u64)
            .is_some_and(|max| len > max)
        {
            return Err(invalid(path, "max_items"));
        }
        if schema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
            for (index, item) in array.iter().enumerate() {
                if array[..index].contains(item) {
                    return Err(invalid(path, "unique_items"));
                }
            }
        }
        if let Some(items) = schema.get("items") {
            for (index, item) in array.iter().enumerate() {
                validate_value(items, item, &format!("{path}[{index}]"))?;
            }
        }
    }
    if let Some(text) = value.as_str() {
        let len = text.chars().count() as u64;
        if schema
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|min| len < min)
        {
            return Err(invalid(path, "min_length"));
        }
        if schema
            .get("maxLength")
            .and_then(Value::as_u64)
            .is_some_and(|max| len > max)
        {
            return Err(invalid(path, "max_length"));
        }
        if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
            if !Regex::new(pattern).expect("pattern audited").is_match(text) {
                return Err(invalid(path, "pattern"));
            }
        }
    }
    if let Some(number) = value.as_f64() {
        if schema
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|min| number < min)
            || schema
                .get("exclusiveMinimum")
                .and_then(Value::as_f64)
                .is_some_and(|min| number <= min)
            || schema
                .get("maximum")
                .and_then(Value::as_f64)
                .is_some_and(|max| number > max)
            || schema
                .get("exclusiveMaximum")
                .and_then(Value::as_f64)
                .is_some_and(|max| number >= max)
        {
            return Err(invalid(path, "number_range"));
        }
        if let Some(multiple) = schema.get("multipleOf").and_then(Value::as_f64) {
            let quotient = number / multiple;
            if (quotient - quotient.round()).abs() > f64::EPSILON * quotient.abs().max(1.0) * 8.0 {
                return Err(invalid(path, "multiple_of"));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ToolRunLimits {
    pub max_model_rounds: u32,
    pub max_tool_calls: u32,
    pub max_result_bytes: usize,
    pub max_tool_timeout: Duration,
    pub deadline: Option<Instant>,
}

impl Default for ToolRunLimits {
    fn default() -> Self {
        Self {
            max_model_rounds: 10,
            max_tool_calls: 8,
            max_result_bytes: 300_000,
            max_tool_timeout: Duration::from_secs(30),
            deadline: None,
        }
    }
}

#[derive(Default)]
struct ToolRunCounters {
    model_rounds: u32,
    tool_calls: u32,
    result_bytes: usize,
    call_ids: HashSet<String>,
    next_approval_id: u64,
}

pub struct ToolRun {
    run_id: String,
    limits: ToolRunLimits,
    cancellation: Arc<dyn ToolCancellation>,
    run_policy: Option<PermissionPolicy>,
    counters: Mutex<ToolRunCounters>,
}

impl ToolRun {
    pub fn new(
        run_id: impl Into<String>,
        limits: ToolRunLimits,
        cancellation: Arc<dyn ToolCancellation>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            limits,
            cancellation,
            run_policy: None,
            counters: Mutex::new(ToolRunCounters::default()),
        }
    }

    pub fn with_policy(mut self, policy: PermissionPolicy) -> Self {
        self.run_policy = Some(policy);
        self
    }

    pub fn begin_model_round(&self) -> Result<(), ToolExecutionError> {
        self.check_active()?;
        let mut counters = self
            .counters
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if counters.model_rounds >= self.limits.max_model_rounds {
            return Err(ToolExecutionError::BudgetExceeded("model_rounds"));
        }
        counters.model_rounds += 1;
        Ok(())
    }

    fn check_active(&self) -> Result<(), ToolExecutionError> {
        if self.cancellation.is_cancelled() {
            return Err(ToolExecutionError::Cancelled);
        }
        if self
            .limits
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(ToolExecutionError::Timeout);
        }
        Ok(())
    }

    fn effective_timeout(&self, cap: Duration) -> Result<Duration, ToolExecutionError> {
        self.check_active()?;
        let timeout = self
            .limits
            .deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()).min(cap))
            .unwrap_or(cap);
        if timeout.is_zero() {
            Err(ToolExecutionError::Timeout)
        } else {
            Ok(timeout)
        }
    }

    fn reserve_call(&self, call_id: &str) -> Result<(), ToolExecutionError> {
        self.check_active()?;
        let mut counters = self
            .counters
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if counters.tool_calls >= self.limits.max_tool_calls {
            return Err(ToolExecutionError::BudgetExceeded("tool_calls"));
        }
        if !counters.call_ids.insert(call_id.to_owned()) {
            return Err(ToolExecutionError::InvalidCall("duplicate_call_id"));
        }
        counters.tool_calls += 1;
        Ok(())
    }

    fn next_approval_id(&self) -> String {
        let mut counters = self
            .counters
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        counters.next_approval_id = counters.next_approval_id.saturating_add(1);
        format!(
            "approval-{}-{}",
            stable_hash(&self.run_id),
            counters.next_approval_id
        )
    }

    fn remaining_result_bytes(&self) -> usize {
        let counters = self
            .counters
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.limits
            .max_result_bytes
            .saturating_sub(counters.result_bytes)
    }

    fn commit_result_bytes(&self, bytes: usize) -> Result<(), ToolExecutionError> {
        let mut counters = self
            .counters
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let next = counters.result_bytes.saturating_add(bytes);
        if next > self.limits.max_result_bytes {
            return Err(ToolExecutionError::BudgetExceeded("result_bytes"));
        }
        counters.result_bytes = next;
        Ok(())
    }
}

fn stable_hash(value: &str) -> String {
    let hash = value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        hash.wrapping_mul(0x100000001b3) ^ u64::from(byte)
    });
    format!("{hash:016x}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolExecutionEvent {
    ValidationFailed {
        call_id: String,
        code: &'static str,
    },
    ApprovalRequested {
        request: ToolApprovalRequest,
    },
    ApprovalResolved {
        approval_id: String,
        call_id: String,
        decision: PermissionDecision,
    },
    Started {
        call_id: String,
        name: String,
        risk: ToolRisk,
    },
    Completed {
        call_id: String,
        name: String,
        outcome: ToolOutcome,
        duration_ms: u64,
        content_bytes: usize,
        truncated: bool,
    },
}

pub trait ToolExecutionEventSink: Send + Sync {
    fn emit(&self, event: ToolExecutionEvent);
}

#[derive(Debug, Default)]
pub struct NoopToolExecutionEventSink;

impl ToolExecutionEventSink for NoopToolExecutionEventSink {
    fn emit(&self, _: ToolExecutionEvent) {}
}

pub struct ToolExecutor {
    registry: Arc<ToolRegistry>,
    policy: PermissionPolicy,
    approvals: Arc<dyn ToolApprovalResolver>,
    events: Arc<dyn ToolExecutionEventSink>,
    secret_literals: Vec<String>,
}

impl ToolExecutor {
    pub fn new(registry: Arc<ToolRegistry>, policy: PermissionPolicy) -> Self {
        Self {
            registry,
            policy,
            approvals: Arc::new(DenyAllApprovals),
            events: Arc::new(NoopToolExecutionEventSink),
            secret_literals: Vec::new(),
        }
    }

    pub fn with_approvals(mut self, approvals: Arc<dyn ToolApprovalResolver>) -> Self {
        self.approvals = approvals;
        self
    }

    pub fn with_events(mut self, events: Arc<dyn ToolExecutionEventSink>) -> Self {
        self.events = events;
        self
    }

    pub fn with_secret_literals(mut self, secrets: Vec<String>) -> Self {
        self.secret_literals = secrets
            .into_iter()
            .filter(|secret| !secret.is_empty())
            .collect();
        self
    }

    pub async fn execute(
        &self,
        run: &ToolRun,
        call: ToolCall,
    ) -> Result<ToolResult, ToolExecutionError> {
        if call.call_id.trim().is_empty() || call.name.trim().is_empty() {
            return Err(ToolExecutionError::InvalidCall("identity"));
        }
        let Some(tool) = self.registry.get(&call.name) else {
            return Err(ToolExecutionError::UnknownTool);
        };
        run.reserve_call(&call.call_id)?;
        if let Err(error) = tool.schema.validate(&call.arguments) {
            let code = match &error {
                ToolExecutionError::InvalidInput { code, .. } => *code,
                _ => "invalid_input",
            };
            self.events.emit(ToolExecutionEvent::ValidationFailed {
                call_id: call.call_id,
                code,
            });
            return Err(error);
        }

        let app_decision = self.policy.evaluate(&call.name, tool.definition.risk);
        let run_decision = run
            .run_policy
            .as_ref()
            .map(|policy| policy.evaluate(&call.name, tool.definition.risk));
        let mut decision = restrict_decision(app_decision, run_decision);
        if decision == PermissionDecision::Ask {
            let approval_timeout = run.effective_timeout(
                Duration::from_millis(tool.definition.timeout_ms).min(run.limits.max_tool_timeout),
            )?;
            let request = ToolApprovalRequest {
                run_id: run.run_id.clone(),
                approval_id: run.next_approval_id(),
                call_id: call.call_id.clone(),
                tool_name: call.name.clone(),
                risk: tool.definition.risk,
                summary: tool
                    .handler
                    .summarize_arguments(&call.arguments)
                    .map(|summary| redact_secrets(summary, &self.secret_literals))
                    .map(|summary| truncate_utf8(summary, 512).0),
            };
            self.events.emit(ToolExecutionEvent::ApprovalRequested {
                request: request.clone(),
            });
            let approval_id = request.approval_id.clone();
            decision = tokio::select! {
                biased;
                _ = run.cancellation.cancelled() => return Err(ToolExecutionError::Cancelled),
                resolved = tokio::time::timeout(approval_timeout, self.approvals.resolve(request)) => {
                    resolved.map_err(|_| ToolExecutionError::Timeout)?
                },
            };
            if decision == PermissionDecision::Ask {
                decision = PermissionDecision::Deny;
            }
            self.events.emit(ToolExecutionEvent::ApprovalResolved {
                approval_id,
                call_id: call.call_id.clone(),
                decision,
            });
        }
        if decision != PermissionDecision::Allow {
            let result = ToolResult {
                call_id: call.call_id,
                name: call.name,
                outcome: ToolOutcome::Denied,
                content: "Tool permission denied.".into(),
                truncated: false,
                content_bytes: "Tool permission denied.".len(),
            };
            self.events.emit(ToolExecutionEvent::Completed {
                call_id: result.call_id.clone(),
                name: result.name.clone(),
                outcome: ToolOutcome::Denied,
                duration_ms: 0,
                content_bytes: result.content_bytes,
                truncated: false,
            });
            return Ok(result);
        }

        run.check_active()?;
        self.events.emit(ToolExecutionEvent::Started {
            call_id: call.call_id.clone(),
            name: call.name.clone(),
            risk: tool.definition.risk,
        });
        let started = Instant::now();
        let timeout = run.effective_timeout(
            Duration::from_millis(tool.definition.timeout_ms).min(run.limits.max_tool_timeout),
        )?;
        let context = ToolExecutionContext {
            run_id: run.run_id.clone(),
            call_id: call.call_id.clone(),
            cancellation: Arc::clone(&run.cancellation),
        };
        let arguments = call.arguments.clone();
        let output = tokio::select! {
            biased;
            _ = run.cancellation.cancelled() => {
                self.emit_terminal(&call, started, ToolOutcome::Cancelled, 0, false);
                return Err(ToolExecutionError::Cancelled);
            },
            result = tokio::time::timeout(timeout, tool.handler.execute(context, arguments)) => {
                match result {
                    Ok(result) => result,
                    Err(_) => {
                        self.emit_terminal(&call, started, ToolOutcome::Timeout, 0, false);
                        return Err(ToolExecutionError::Timeout);
                    },
                }
            }
        };
        let (outcome, content) = match output {
            Ok(content) => (ToolOutcome::Success, tool.handler.sanitize_result(content)),
            Err(_) => (ToolOutcome::Failed, "Tool execution failed.".into()),
        };
        let content = redact_secrets(content, &self.secret_literals);
        let cap = tool
            .definition
            .max_result_bytes
            .min(run.remaining_result_bytes())
            .min(HARD_MAX_RESULT_BYTES);
        if cap == 0 {
            return Err(ToolExecutionError::BudgetExceeded("result_bytes"));
        }
        let (content, truncated) = truncate_utf8_with_marker(content, cap);
        let content_bytes = content.len();
        run.commit_result_bytes(content_bytes)?;
        self.emit_terminal(&call, started, outcome, content_bytes, truncated);
        Ok(ToolResult {
            call_id: call.call_id,
            name: call.name,
            outcome,
            content,
            truncated,
            content_bytes,
        })
    }

    fn emit_terminal(
        &self,
        call: &ToolCall,
        started: Instant,
        outcome: ToolOutcome,
        content_bytes: usize,
        truncated: bool,
    ) {
        let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        self.events.emit(ToolExecutionEvent::Completed {
            call_id: call.call_id.clone(),
            name: call.name.clone(),
            outcome,
            duration_ms,
            content_bytes,
            truncated,
        });
    }
}

impl ToolExecutionEvent {
    pub fn into_agent_event_kind(self) -> crate::AgentEventKind {
        match self {
            Self::ValidationFailed { call_id, code } => {
                crate::AgentEventKind::ToolValidationFailed {
                    call_id,
                    tool_name: None,
                    error: code.into(),
                }
            }
            Self::ApprovalRequested { request } => crate::AgentEventKind::ToolApprovalRequested {
                approval_id: request.approval_id,
                call_id: request.call_id,
                tool_name: request.tool_name,
                risk: request.risk,
                summary: request.summary,
            },
            Self::ApprovalResolved {
                approval_id,
                call_id,
                decision,
            } => crate::AgentEventKind::ToolApprovalResolved {
                approval_id,
                call_id,
                decision,
            },
            Self::Started {
                call_id,
                name,
                risk,
            } => crate::AgentEventKind::ToolExecutionStarted {
                call_id,
                tool_name: name,
                risk,
            },
            Self::Completed {
                call_id,
                name,
                outcome,
                duration_ms,
                content_bytes,
                truncated,
            } => crate::AgentEventKind::ToolExecutionCompleted {
                call_id,
                tool_name: name,
                outcome,
                duration_ms,
                content_bytes,
                truncated,
            },
        }
    }
}

fn redact_secrets(mut content: String, secrets: &[String]) -> String {
    content.retain(|character| character == '\n' || character == '\t' || !character.is_control());
    for secret in secrets {
        content = content.replace(secret, "[REDACTED]");
    }
    let sensitive = Regex::new(
        r"(?im)(authorization\s*[:=]\s*|api[_-]?key\s*[:=]\s*|token\s*[:=]\s*)([^\s,;]+)",
    )
    .expect("static redaction regex");
    sensitive.replace_all(&content, "$1[REDACTED]").into_owned()
}

fn truncate_utf8(mut content: String, max_bytes: usize) -> (String, bool) {
    if content.len() <= max_bytes {
        return (content, false);
    }
    let mut end = max_bytes.min(content.len());
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    content.truncate(end);
    (content, true)
}

fn truncate_utf8_with_marker(content: String, max_bytes: usize) -> (String, bool) {
    if content.len() <= max_bytes {
        return (content, false);
    }
    if max_bytes <= TRUNCATION_MARKER.len() {
        return truncate_utf8(content, max_bytes);
    }
    let (mut content, _) = truncate_utf8(content, max_bytes - TRUNCATION_MARKER.len());
    content.push_str(TRUNCATION_MARKER);
    (content, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct EchoHandler(Arc<AtomicUsize>);

    #[async_trait]
    impl ToolHandler for EchoHandler {
        async fn execute(
            &self,
            _: ToolExecutionContext,
            arguments: Value,
        ) -> Result<String, ToolHandlerError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(arguments["text"].as_str().unwrap_or_default().to_owned())
        }

        fn summarize_arguments(&self, _: &Value) -> Option<String> {
            Some("sanitized summary".into())
        }
    }

    fn definition() -> crate::ToolDefinition {
        crate::ToolDefinition {
            name: "test.echo".into(),
            description: "Echo validated text".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": {"type": "string", "minLength": 1, "maxLength": 40},
                    "count": {"type": "integer", "minimum": 1, "maximum": 3},
                    "tags": {"type": "array", "items": {"type": "string"}, "uniqueItems": true}
                },
                "required": ["text"],
                "additionalProperties": false
            }),
            risk: ToolRisk::ReadOnly,
            timeout_ms: 100,
            max_result_bytes: 64,
        }
    }

    fn allow_policy() -> PermissionPolicy {
        PermissionPolicy::new(vec![PermissionRule {
            matcher: ToolMatcher::Exact("test.echo".into()),
            risk: Some(ToolRisk::ReadOnly),
            decision: PermissionDecision::Allow,
        }])
    }

    #[test]
    fn tool_contracts_round_trip_with_first_class_call_id() {
        let call = ToolCall::with_call_id("test.echo", "call-1", json!({"text":"ok"}));
        let encoded = serde_json::to_string(&call).unwrap();
        assert!(!encoded.contains("_call_id"));
        assert_eq!(serde_json::from_str::<ToolCall>(&encoded).unwrap(), call);
        let result = ToolResult {
            call_id: "call-1".into(),
            name: "test.echo".into(),
            outcome: ToolOutcome::Success,
            content: "ok".into(),
            truncated: false,
            content_bytes: 2,
        };
        assert_eq!(
            serde_json::from_str::<ToolResult>(&serde_json::to_string(&result).unwrap()).unwrap(),
            result
        );
    }

    #[test]
    fn registry_rejects_duplicates_and_unsupported_schema_keywords() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::default();
        registry
            .register(definition(), Arc::new(EchoHandler(Arc::clone(&calls))))
            .unwrap();
        assert_eq!(
            registry
                .register(definition(), Arc::new(EchoHandler(calls)))
                .unwrap_err(),
            ToolRegistrationError::DuplicateName
        );
        let mut invalid = definition();
        invalid.name = "test.invalid".into();
        invalid.input_schema["$ref"] = json!("https://example.test/schema");
        assert_eq!(
            registry
                .register(
                    invalid,
                    Arc::new(EchoHandler(Arc::new(AtomicUsize::new(0))))
                )
                .unwrap_err(),
            ToolRegistrationError::InvalidSchema("unsupported_keyword")
        );
    }

    #[test]
    fn controlled_schema_profile_validates_composition_and_all_value_families() {
        let schema = CompiledSchema::compile(&json!({
            "type": "object",
            "properties": {
                "mode": {"enum": ["read", "write"]},
                "enabled": {"const": true},
                "target": {"oneOf": [
                    {"type": "string", "pattern": "^[a-z]+$", "minLength": 2, "maxLength": 8},
                    {"type": "integer", "minimum": 2, "exclusiveMaximum": 10, "multipleOf": 2}
                ]},
                "items": {"type": "array", "items": {"type": "boolean"}, "minItems": 1, "maxItems": 2, "uniqueItems": true},
                "none": {"type": "null"}
            },
            "required": ["mode", "enabled", "target", "items", "none"],
            "minProperties": 5,
            "maxProperties": 5,
            "additionalProperties": false
        })).unwrap();
        schema.validate(&json!({
            "mode": "read", "enabled": true, "target": "repo", "items": [true, false], "none": null
        })).unwrap();
        schema
            .validate(&json!({
                "mode": "write", "enabled": true, "target": 4, "items": [true], "none": null
            }))
            .unwrap();
        for invalid_value in [
            json!({"mode":"other", "enabled":true, "target":"repo", "items":[true], "none":null}),
            json!({"mode":"read", "enabled":false, "target":"repo", "items":[true], "none":null}),
            json!({"mode":"read", "enabled":true, "target":"R", "items":[true], "none":null}),
            json!({"mode":"read", "enabled":true, "target":3, "items":[true], "none":null}),
            json!({"mode":"read", "enabled":true, "target":"repo", "items":[true,true], "none":null}),
        ] {
            assert!(schema.validate(&invalid_value).is_err());
        }
    }

    #[test]
    fn policy_uses_first_match_and_run_restriction_never_broadens() {
        let policy = PermissionPolicy::new(vec![
            PermissionRule {
                matcher: ToolMatcher::Exact("filesystem.delete".into()),
                risk: None,
                decision: PermissionDecision::Deny,
            },
            PermissionRule {
                matcher: ToolMatcher::Prefix("filesystem.".into()),
                risk: None,
                decision: PermissionDecision::Allow,
            },
        ]);
        assert_eq!(
            policy.evaluate("filesystem.delete", ToolRisk::Destructive),
            PermissionDecision::Deny
        );
        assert_eq!(
            policy.evaluate("filesystem.read", ToolRisk::ReadOnly),
            PermissionDecision::Allow
        );
        assert_eq!(
            policy.evaluate("web.fetch", ToolRisk::External),
            PermissionDecision::Deny
        );
        assert_eq!(
            restrict_decision(PermissionDecision::Allow, Some(PermissionDecision::Ask)),
            PermissionDecision::Ask
        );
        assert_eq!(
            restrict_decision(PermissionDecision::Deny, Some(PermissionDecision::Allow)),
            PermissionDecision::Deny
        );
    }

    #[tokio::test]
    async fn invalid_input_never_reaches_handler_or_permission_execution() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::default();
        registry
            .register(definition(), Arc::new(EchoHandler(Arc::clone(&calls))))
            .unwrap();
        let executor = ToolExecutor::new(Arc::new(registry), allow_policy());
        let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel));
        let error = executor
            .execute(
                &run,
                ToolCall::with_call_id("test.echo", "bad", json!({"text":"ok", "secret":true})),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ToolExecutionError::InvalidInput {
                code: "additional_property",
                ..
            }
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn policy_defaults_to_deny_and_run_policy_cannot_broaden_it() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::default();
        registry
            .register(definition(), Arc::new(EchoHandler(Arc::clone(&calls))))
            .unwrap();
        let executor = ToolExecutor::new(Arc::new(registry), PermissionPolicy::default());
        let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel))
            .with_policy(allow_policy());
        let result = executor
            .execute(
                &run,
                ToolCall::with_call_id("test.echo", "denied", json!({"text":"ok"})),
            )
            .await
            .unwrap();
        assert_eq!(result.outcome, ToolOutcome::Denied);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    struct Approval(PermissionDecision);

    #[async_trait]
    impl ToolApprovalResolver for Approval {
        async fn resolve(&self, _: ToolApprovalRequest) -> PermissionDecision {
            self.0
        }
    }

    #[tokio::test]
    async fn ask_requires_one_shot_allow_before_handler_execution() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::default();
        registry
            .register(definition(), Arc::new(EchoHandler(Arc::clone(&calls))))
            .unwrap();
        let ask = PermissionPolicy::new(vec![PermissionRule {
            matcher: ToolMatcher::Prefix("test.".into()),
            risk: None,
            decision: PermissionDecision::Ask,
        }]);
        let executor = ToolExecutor::new(Arc::new(registry), ask)
            .with_approvals(Arc::new(Approval(PermissionDecision::Allow)));
        let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel));
        let result = executor
            .execute(
                &run,
                ToolCall::with_call_id("test.echo", "approved", json!({"text":"ok"})),
            )
            .await
            .unwrap();
        assert_eq!(result.outcome, ToolOutcome::Success);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    struct ToggleCancel(AtomicBool);

    #[async_trait]
    impl ToolCancellation for ToggleCancel {
        fn is_cancelled(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    struct SlowHandler;

    #[async_trait]
    impl ToolHandler for SlowHandler {
        async fn execute(
            &self,
            _: ToolExecutionContext,
            _: Value,
        ) -> Result<String, ToolHandlerError> {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok("late".into())
        }
    }

    #[tokio::test]
    async fn timeout_and_budgets_fail_closed() {
        let mut slow = definition();
        slow.name = "test.slow".into();
        slow.timeout_ms = 5;
        let mut registry = ToolRegistry::default();
        registry.register(slow, Arc::new(SlowHandler)).unwrap();
        let policy = PermissionPolicy::new(vec![PermissionRule {
            matcher: ToolMatcher::Exact("test.slow".into()),
            risk: None,
            decision: PermissionDecision::Allow,
        }]);
        let executor = ToolExecutor::new(Arc::new(registry), policy);
        let run = ToolRun::new(
            "run",
            ToolRunLimits {
                max_tool_calls: 1,
                ..ToolRunLimits::default()
            },
            Arc::new(ToggleCancel(AtomicBool::new(false))),
        );
        assert_eq!(
            executor
                .execute(
                    &run,
                    ToolCall::with_call_id("test.slow", "one", json!({"text":"ok"}))
                )
                .await
                .unwrap_err(),
            ToolExecutionError::Timeout
        );
        assert_eq!(
            executor
                .execute(
                    &run,
                    ToolCall::with_call_id("test.slow", "two", json!({"text":"ok"}))
                )
                .await
                .unwrap_err(),
            ToolExecutionError::BudgetExceeded("tool_calls")
        );
        for _ in 0..run.limits.max_model_rounds {
            run.begin_model_round().unwrap();
        }
        assert_eq!(
            run.begin_model_round().unwrap_err(),
            ToolExecutionError::BudgetExceeded("model_rounds")
        );
    }

    #[tokio::test]
    async fn result_is_redacted_then_utf8_safely_truncated_and_counted() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut definition = definition();
        definition.max_result_bytes = 32;
        let mut registry = ToolRegistry::default();
        registry
            .register(definition, Arc::new(EchoHandler(calls)))
            .unwrap();
        let executor = ToolExecutor::new(Arc::new(registry), allow_policy())
            .with_secret_literals(vec!["super-secret".into()]);
        let run = ToolRun::new("run", ToolRunLimits::default(), Arc::new(NeverCancel));
        let result = executor
            .execute(
                &run,
                ToolCall::with_call_id(
                    "test.echo",
                    "redact",
                    json!({"text":"super-secret你好你好你好你好"}),
                ),
            )
            .await
            .unwrap();
        assert!(!result.content.contains("super-secret"));
        assert!(result.truncated);
        assert!(result.content_bytes <= 32);
        assert!(std::str::from_utf8(result.content.as_bytes()).is_ok());
    }
}
