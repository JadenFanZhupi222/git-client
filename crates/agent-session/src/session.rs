use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: SessionRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSession {
    pub session_id: String,
    pub revision: u64,
    pub system_instruction: String,
    pub memory_summary: Option<String>,
    pub recent_messages: Vec<SessionMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLease {
    pub session_id: String,
    pub run_id: String,
    pub base_revision: u64,
    pub snapshot: AgentSession,
}

#[derive(Debug, Clone)]
pub struct SessionStoreLimits {
    pub max_sessions: usize,
    pub max_system_bytes: usize,
    pub max_message_bytes: usize,
    pub max_recent_messages: usize,
    pub max_recent_bytes: usize,
    pub max_summary_bytes: usize,
}

impl Default for SessionStoreLimits {
    fn default() -> Self {
        Self {
            max_sessions: 32,
            max_system_bytes: 64 * 1024,
            max_message_bytes: 64 * 1024,
            max_recent_messages: 16,
            max_recent_bytes: 256 * 1024,
            max_summary_bytes: 32 * 1024,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SessionError {
    #[error("invalid session configuration")]
    InvalidConfig,
    #[error("invalid session identifier")]
    InvalidId,
    #[error("invalid session content")]
    InvalidContent,
    #[error("session already exists")]
    AlreadyExists,
    #[error("session not found")]
    NotFound,
    #[error("session capacity reached")]
    Capacity,
    #[error("session already has an active run")]
    Busy,
    #[error("session lease is stale")]
    StaleLease,
    #[error("memory compaction failed")]
    Compaction,
}

pub trait MemoryCompactor: Send + Sync {
    fn compact(
        &self,
        existing_summary: Option<&str>,
        messages: &[SessionMessage],
        max_bytes: usize,
    ) -> Result<String, SessionError>;
}

#[derive(Debug, Default)]
pub struct ExtractiveMemoryCompactor;

impl MemoryCompactor for ExtractiveMemoryCompactor {
    fn compact(
        &self,
        existing_summary: Option<&str>,
        messages: &[SessionMessage],
        max_bytes: usize,
    ) -> Result<String, SessionError> {
        if max_bytes == 0 {
            return Err(SessionError::Compaction);
        }
        let mut sections = Vec::new();
        if let Some(existing) = existing_summary.filter(|value| !value.is_empty()) {
            sections.push(existing.to_owned());
        }
        for message in messages {
            let role = match message.role {
                SessionRole::User => "user",
                SessionRole::Assistant => "assistant",
            };
            sections.push(format!(
                "{role}: {}",
                normalize_whitespace(&message.content)
            ));
        }
        let combined = sections.join("\n");
        if combined.len() <= max_bytes {
            return Ok(combined);
        }
        let marker = "[older memory truncated]\n";
        if max_bytes <= marker.len() {
            return Err(SessionError::Compaction);
        }
        let suffix = truncate_utf8_suffix(&combined, max_bytes - marker.len());
        Ok(format!("{marker}{suffix}"))
    }
}

struct SessionRecord {
    session: AgentSession,
    active_run: Option<String>,
}

#[derive(Default)]
struct StoreState {
    sessions: HashMap<String, SessionRecord>,
}

pub struct SessionStore {
    limits: SessionStoreLimits,
    compactor: Arc<dyn MemoryCompactor>,
    state: Mutex<StoreState>,
}

impl SessionStore {
    pub fn new(limits: SessionStoreLimits) -> Result<Self, SessionError> {
        Self::with_compactor(limits, Arc::new(ExtractiveMemoryCompactor))
    }

    pub fn with_compactor(
        limits: SessionStoreLimits,
        compactor: Arc<dyn MemoryCompactor>,
    ) -> Result<Self, SessionError> {
        if limits.max_sessions == 0
            || limits.max_system_bytes == 0
            || limits.max_message_bytes == 0
            || limits.max_recent_messages < 2
            || limits.max_recent_messages % 2 != 0
            || limits.max_recent_bytes < limits.max_message_bytes.saturating_mul(2)
            || limits.max_summary_bytes < 64
        {
            return Err(SessionError::InvalidConfig);
        }
        Ok(Self {
            limits,
            compactor,
            state: Mutex::new(StoreState::default()),
        })
    }

    pub fn create(
        &self,
        session_id: impl Into<String>,
        system_instruction: impl Into<String>,
    ) -> Result<AgentSession, SessionError> {
        let session_id = session_id.into();
        let system_instruction = system_instruction.into();
        validate_id(&session_id)?;
        validate_content(&system_instruction, self.limits.max_system_bytes, true)?;
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.sessions.contains_key(&session_id) {
            return Err(SessionError::AlreadyExists);
        }
        if state.sessions.len() >= self.limits.max_sessions {
            return Err(SessionError::Capacity);
        }
        let session = AgentSession {
            session_id: session_id.clone(),
            revision: 0,
            system_instruction,
            memory_summary: None,
            recent_messages: Vec::new(),
        };
        state.sessions.insert(
            session_id,
            SessionRecord {
                session: session.clone(),
                active_run: None,
            },
        );
        Ok(session)
    }

    pub fn get(&self, session_id: &str) -> Result<AgentSession, SessionError> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state
            .sessions
            .get(session_id)
            .map(|record| record.session.clone())
            .ok_or(SessionError::NotFound)
    }

    pub fn reset(&self, session_id: &str) -> Result<AgentSession, SessionError> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let record = state
            .sessions
            .get_mut(session_id)
            .ok_or(SessionError::NotFound)?;
        if record.active_run.is_some() {
            return Err(SessionError::Busy);
        }
        record.session.memory_summary = None;
        record.session.recent_messages.clear();
        record.session.revision = record
            .session
            .revision
            .checked_add(1)
            .ok_or(SessionError::Capacity)?;
        Ok(record.session.clone())
    }

    pub fn begin_turn(&self, session_id: &str, run_id: &str) -> Result<SessionLease, SessionError> {
        validate_id(session_id)?;
        validate_id(run_id)?;
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let record = state
            .sessions
            .get_mut(session_id)
            .ok_or(SessionError::NotFound)?;
        if record.active_run.is_some() {
            return Err(SessionError::Busy);
        }
        record.active_run = Some(run_id.to_owned());
        Ok(SessionLease {
            session_id: session_id.to_owned(),
            run_id: run_id.to_owned(),
            base_revision: record.session.revision,
            snapshot: record.session.clone(),
        })
    }

    pub fn commit_turn(
        &self,
        lease: &SessionLease,
        user: impl Into<String>,
        assistant: impl Into<String>,
    ) -> Result<AgentSession, SessionError> {
        let user = user.into();
        let assistant = assistant.into();
        validate_content(&user, self.limits.max_message_bytes, false)?;
        validate_content(&assistant, self.limits.max_message_bytes, false)?;

        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let record = state
            .sessions
            .get_mut(&lease.session_id)
            .ok_or(SessionError::NotFound)?;
        ensure_lease(record, lease)?;

        let mut next = record.session.clone();
        next.recent_messages.extend([
            SessionMessage {
                role: SessionRole::User,
                content: user,
            },
            SessionMessage {
                role: SessionRole::Assistant,
                content: assistant,
            },
        ]);
        while next.recent_messages.len() > self.limits.max_recent_messages
            || message_bytes(&next.recent_messages) > self.limits.max_recent_bytes
        {
            if next.recent_messages.len() <= 2 {
                return Err(SessionError::Compaction);
            }
            let compacted = next.recent_messages.drain(..2).collect::<Vec<_>>();
            let summary = self.compactor.compact(
                next.memory_summary.as_deref(),
                &compacted,
                self.limits.max_summary_bytes,
            )?;
            if summary.is_empty() || summary.len() > self.limits.max_summary_bytes {
                return Err(SessionError::Compaction);
            }
            next.memory_summary = Some(summary);
        }
        next.revision = next.revision.checked_add(1).ok_or(SessionError::Capacity)?;
        record.session = next.clone();
        record.active_run = None;
        Ok(next)
    }

    pub fn abort_turn(&self, lease: &SessionLease) -> Result<(), SessionError> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let record = state
            .sessions
            .get_mut(&lease.session_id)
            .ok_or(SessionError::NotFound)?;
        ensure_lease(record, lease)?;
        record.active_run = None;
        Ok(())
    }
}

fn ensure_lease(record: &SessionRecord, lease: &SessionLease) -> Result<(), SessionError> {
    if record.active_run.as_deref() != Some(&lease.run_id)
        || record.session.revision != lease.base_revision
    {
        Err(SessionError::StaleLease)
    } else {
        Ok(())
    }
}

fn validate_id(value: &str) -> Result<(), SessionError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(SessionError::InvalidId)
    }
}

fn validate_content(value: &str, max_bytes: usize, allow_empty: bool) -> Result<(), SessionError> {
    if value.len() > max_bytes || value.contains('\0') || (!allow_empty && value.trim().is_empty())
    {
        Err(SessionError::InvalidContent)
    } else {
        Ok(())
    }
}

fn message_bytes(messages: &[SessionMessage]) -> usize {
    messages.iter().fold(0usize, |total, message| {
        total.saturating_add(message.content.len())
    })
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_utf8_suffix(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut boundary = value.len() - max_bytes;
    while boundary < value.len() && !value.is_char_boundary(boundary) {
        boundary += 1;
    }
    &value[boundary..]
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingCompactor;

    impl MemoryCompactor for FailingCompactor {
        fn compact(
            &self,
            _: Option<&str>,
            _: &[SessionMessage],
            _: usize,
        ) -> Result<String, SessionError> {
            Err(SessionError::Compaction)
        }
    }

    fn limits() -> SessionStoreLimits {
        SessionStoreLimits {
            max_sessions: 1,
            max_system_bytes: 256,
            max_message_bytes: 128,
            max_recent_messages: 4,
            max_recent_bytes: 256,
            max_summary_bytes: 128,
        }
    }

    #[test]
    fn leases_commit_atomically_and_abort_without_memory_changes() {
        let store = SessionStore::new(limits()).unwrap();
        store.create("session", "system").unwrap();
        let lease = store.begin_turn("session", "run-1").unwrap();
        assert_eq!(
            store.begin_turn("session", "run-2").unwrap_err(),
            SessionError::Busy
        );
        store.abort_turn(&lease).unwrap();
        assert_eq!(store.get("session").unwrap().revision, 0);

        let lease = store.begin_turn("session", "run-2").unwrap();
        let committed = store.commit_turn(&lease, "question", "answer").unwrap();
        assert_eq!(committed.revision, 1);
        assert_eq!(committed.recent_messages.len(), 2);
        assert_eq!(
            store.abort_turn(&lease).unwrap_err(),
            SessionError::StaleLease
        );
    }

    #[test]
    fn capacity_ids_and_content_fail_closed() {
        let store = SessionStore::new(limits()).unwrap();
        assert_eq!(
            store.create("../bad", "system").unwrap_err(),
            SessionError::InvalidId
        );
        store.create("one", "system").unwrap();
        assert_eq!(
            store.create("two", "system").unwrap_err(),
            SessionError::Capacity
        );
        let lease = store.begin_turn("one", "run").unwrap();
        assert_eq!(
            store.commit_turn(&lease, "", "answer").unwrap_err(),
            SessionError::InvalidContent
        );
        store.abort_turn(&lease).unwrap();
    }

    #[test]
    fn old_complete_pairs_compact_and_recent_messages_remain_verbatim() {
        let store = SessionStore::new(limits()).unwrap();
        store.create("session", "system").unwrap();
        for index in 0..3 {
            let lease = store
                .begin_turn("session", &format!("run-{index}"))
                .unwrap();
            store
                .commit_turn(
                    &lease,
                    format!("question {index}"),
                    format!("answer {index}"),
                )
                .unwrap();
        }
        let session = store.get("session").unwrap();
        assert_eq!(session.recent_messages.len(), 4);
        assert_eq!(session.recent_messages[0].content, "question 1");
        let summary = session.memory_summary.unwrap();
        assert!(summary.contains("user: question 0"));
        assert!(summary.contains("assistant: answer 0"));
        assert!(!summary.contains("tool"));
        assert!(summary.len() <= limits().max_summary_bytes);
    }

    #[test]
    fn compactor_failure_does_not_partially_commit_memory() {
        let store = SessionStore::with_compactor(limits(), Arc::new(FailingCompactor)).unwrap();
        store.create("session", "system").unwrap();
        for index in 0..2 {
            let lease = store
                .begin_turn("session", &format!("run-{index}"))
                .unwrap();
            store.commit_turn(&lease, "question", "answer").unwrap();
        }
        let before = store.get("session").unwrap();
        let lease = store.begin_turn("session", "run-fail").unwrap();
        assert_eq!(
            store
                .commit_turn(&lease, "new question", "new answer")
                .unwrap_err(),
            SessionError::Compaction
        );
        assert_eq!(store.get("session").unwrap(), before);
        store.abort_turn(&lease).unwrap();
    }

    #[test]
    fn reset_clears_memory_but_never_an_active_turn() {
        let store = SessionStore::new(limits()).unwrap();
        store.create("session", "system").unwrap();
        let lease = store.begin_turn("session", "run").unwrap();
        store.commit_turn(&lease, "question", "answer").unwrap();
        let active = store.begin_turn("session", "active").unwrap();
        assert_eq!(store.reset("session").unwrap_err(), SessionError::Busy);
        store.abort_turn(&active).unwrap();
        let reset = store.reset("session").unwrap();
        assert_eq!(reset.revision, 2);
        assert!(reset.recent_messages.is_empty());
        assert!(reset.memory_summary.is_none());
    }
}
