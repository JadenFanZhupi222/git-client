use super::*;

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
    pub(super) run_id: String,
    pub(super) execution_namespace: String,
    pub(super) limits: ToolRunLimits,
    pub(super) cancellation: Arc<dyn ToolCancellation>,
    pub(super) run_policy: Option<PermissionPolicy>,
    counters: Mutex<ToolRunCounters>,
}

impl ToolRun {
    pub fn new(
        run_id: impl Into<String>,
        limits: ToolRunLimits,
        cancellation: Arc<dyn ToolCancellation>,
    ) -> Self {
        let run_id = run_id.into();
        Self {
            execution_namespace: run_id.clone(),
            run_id,
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

    pub fn with_execution_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.execution_namespace = namespace.into();
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

    pub(super) fn check_active(&self) -> Result<(), ToolExecutionError> {
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

    pub(super) fn effective_timeout(&self, cap: Duration) -> Result<Duration, ToolExecutionError> {
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

    pub(super) fn reserve_call(&self, call_id: &str) -> Result<(), ToolExecutionError> {
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

    pub(super) fn next_approval_id(&self) -> String {
        let mut counters = self
            .counters
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        counters.next_approval_id = counters.next_approval_id.saturating_add(1);
        format!(
            "approval-{}-{}",
            stable_hash(&self.execution_namespace),
            counters.next_approval_id
        )
    }

    pub(super) fn remaining_result_bytes(&self) -> usize {
        let counters = self
            .counters
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.limits
            .max_result_bytes
            .saturating_sub(counters.result_bytes)
    }

    pub(super) fn commit_result_bytes(&self, bytes: usize) -> Result<(), ToolExecutionError> {
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

pub(super) fn stable_hash(value: &str) -> String {
    let hash = value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        hash.wrapping_mul(0x100000001b3) ^ u64::from(byte)
    });
    format!("{hash:016x}")
}
