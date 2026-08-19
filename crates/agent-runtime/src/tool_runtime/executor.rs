use super::*;

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
    journal: Arc<dyn ToolIntentJournal>,
    secret_literals: Vec<String>,
}

impl ToolExecutor {
    pub fn new(registry: Arc<ToolRegistry>, policy: PermissionPolicy) -> Self {
        Self {
            registry,
            policy,
            approvals: Arc::new(DenyAllApprovals),
            events: Arc::new(NoopToolExecutionEventSink),
            journal: Arc::new(NoopToolIntentJournal),
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

    pub fn with_journal(mut self, journal: Arc<dyn ToolIntentJournal>) -> Self {
        self.journal = journal;
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
        let mut resolved_approval_id = None;
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
                    .map(|summary| redact_sensitive_text(summary, &self.secret_literals))
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
                approval_id: approval_id.clone(),
                call_id: call.call_id.clone(),
                decision,
            });
            resolved_approval_id = Some(approval_id);
        }
        if decision != PermissionDecision::Allow {
            let result = ToolResult {
                call_id: call.call_id,
                name: call.name,
                outcome: ToolOutcome::Denied,
                content: "Tool permission denied.".into(),
                truncated: false,
                content_bytes: "Tool permission denied.".len(),
                receipt: None,
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
        let execution_id = format!(
            "exec-{}",
            stable_hash(&format!(
                "{}\0{}\0{}",
                run.execution_namespace, call.call_id, call.name
            ))
        );
        let preparation = match tool.handler.prepare_intent(
            &ToolExecutionContext {
                run_id: run.run_id.clone(),
                call_id: call.call_id.clone(),
                execution_id: execution_id.clone(),
                cancellation: Arc::clone(&run.cancellation),
            },
            &call.arguments,
        ) {
            Ok(preparation) => preparation,
            Err(_) => {
                let content = "Tool preparation failed.".to_owned();
                run.commit_result_bytes(content.len())?;
                self.events.emit(ToolExecutionEvent::Completed {
                    call_id: call.call_id.clone(),
                    name: call.name.clone(),
                    outcome: ToolOutcome::Failed,
                    duration_ms: 0,
                    content_bytes: content.len(),
                    truncated: false,
                });
                return Ok(ToolResult {
                    call_id: call.call_id,
                    name: call.name,
                    outcome: ToolOutcome::Failed,
                    content_bytes: content.len(),
                    content,
                    truncated: false,
                    receipt: None,
                });
            }
        };
        let intent = ToolIntent {
            execution_id: execution_id.clone(),
            run_id: run.run_id.clone(),
            call_id: call.call_id.clone(),
            tool_name: call.name.clone(),
            risk: tool.definition.risk,
            arguments: call.arguments.clone(),
            approval_id: resolved_approval_id,
            approved: true,
            resource: preparation.resource,
            before_digest: preparation.before_digest,
            expected_after_digest: preparation.expected_after_digest,
            replay_policy: preparation.replay_policy,
        };
        self.journal
            .record_intent(&intent)
            .map_err(|_| ToolExecutionError::IntentPersistence)?;
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
            execution_id,
            cancellation: Arc::clone(&run.cancellation),
        };
        let arguments = call.arguments.clone();
        let output = tokio::select! {
            biased;
            _ = run.cancellation.cancelled() => {
                self.resolve_read_only_without_effect(&intent, ToolOutcome::Cancelled)?;
                self.emit_terminal(&call, started, ToolOutcome::Cancelled, 0, false);
                return Err(ToolExecutionError::Cancelled);
            },
            result = tokio::time::timeout(timeout, tool.handler.execute(context, arguments)) => {
                match result {
                    Ok(result) => result,
                    Err(_) => {
                        self.resolve_read_only_without_effect(&intent, ToolOutcome::Timeout)?;
                        self.emit_terminal(&call, started, ToolOutcome::Timeout, 0, false);
                        return Err(ToolExecutionError::Timeout);
                    },
                }
            }
        };
        let (outcome, content, receipt) = match output {
            Ok(output) => {
                self.journal
                    .record_receipt(&intent, &output.receipt)
                    .map_err(|_| ToolExecutionError::ReceiptPersistence)?;
                (
                    ToolOutcome::Success,
                    tool.handler.sanitize_result(output.sanitized_content),
                    Some(output.receipt),
                )
            }
            Err(error) => {
                self.resolve_read_only_without_effect(&intent, ToolOutcome::Failed)?;
                (
                    ToolOutcome::Failed,
                    error
                        .sanitized_content()
                        .unwrap_or("Tool execution failed.")
                        .to_owned(),
                    None,
                )
            }
        };
        let content = redact_sensitive_text(content, &self.secret_literals);
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
            receipt,
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

    fn resolve_read_only_without_effect(
        &self,
        intent: &ToolIntent,
        outcome: ToolOutcome,
    ) -> Result<(), ToolExecutionError> {
        if intent.risk != ToolRisk::ReadOnly {
            return Ok(());
        }
        self.journal
            .record_no_effect(intent, outcome)
            .map_err(|_| ToolExecutionError::IntentResolutionPersistence)
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

pub fn redact_sensitive_text(mut content: String, secrets: &[String]) -> String {
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

pub(super) fn truncate_utf8(mut content: String, max_bytes: usize) -> (String, bool) {
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
