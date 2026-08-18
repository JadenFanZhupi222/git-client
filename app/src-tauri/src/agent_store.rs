use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_session::{DurableAgentSession, GoalError, GoalPersistence};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use fs2::FileExt;

const MAGIC: &[u8; 8] = b"VAGOAL01";
const FORMAT_VERSION: u16 = 1;
const HEADER_BYTES: usize = MAGIC.len() + 2 + 24;
const KEYRING_SERVICE: &str = "com.versionarc.desktop";
const KEYRING_USER: &str = "agent-checkpoint-key-v1";

pub(crate) trait AgentStoreKeyProvider: Send + Sync {
    fn load_or_create_key(&self) -> Result<[u8; 32], GoalError>;
}

#[derive(Debug, Default)]
pub(crate) struct KeyringAgentStoreKeyProvider;

impl AgentStoreKeyProvider for KeyringAgentStoreKeyProvider {
    fn load_or_create_key(&self) -> Result<[u8; 32], GoalError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|_| GoalError::KeyUnavailable)?;
        match entry.get_password() {
            Ok(encoded) => decode_key(&encoded),
            Err(keyring::Error::NoEntry) => {
                let mut key = [0_u8; 32];
                getrandom::fill(&mut key).map_err(|_| GoalError::KeyUnavailable)?;
                entry
                    .set_password(&hex::encode(key))
                    .map_err(|_| GoalError::KeyUnavailable)?;
                Ok(key)
            }
            Err(_) => Err(GoalError::KeyUnavailable),
        }
    }
}

fn decode_key(encoded: &str) -> Result<[u8; 32], GoalError> {
    let bytes = hex::decode(encoded).map_err(|_| GoalError::KeyUnavailable)?;
    bytes.try_into().map_err(|_| GoalError::KeyUnavailable)
}

#[derive(Clone)]
pub(crate) struct EncryptedAgentStore {
    root: PathBuf,
    keys: Arc<dyn AgentStoreKeyProvider>,
}

impl EncryptedAgentStore {
    pub(crate) fn new(root: impl Into<PathBuf>, keys: Arc<dyn AgentStoreKeyProvider>) -> Self {
        Self {
            root: root.into(),
            keys,
        }
    }

    pub(crate) fn production(app_data_dir: &Path) -> Self {
        Self::new(
            app_data_dir.join("agent-sessions").join("v1"),
            Arc::new(KeyringAgentStoreKeyProvider),
        )
    }

    fn state_path(&self, session_id: &str) -> Result<PathBuf, GoalError> {
        validate_opaque_id(session_id)?;
        Ok(self.root.join(format!("{session_id}.state")))
    }

    fn lock_path(&self, session_id: &str) -> Result<PathBuf, GoalError> {
        validate_opaque_id(session_id)?;
        Ok(self.root.join(format!("{session_id}.lock")))
    }

    fn with_lock<T>(
        &self,
        session_id: &str,
        operation: impl FnOnce(&Path) -> Result<T, GoalError>,
    ) -> Result<T, GoalError> {
        std::fs::create_dir_all(&self.root).map_err(|_| GoalError::StorageUnavailable)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.lock_path(session_id)?)
            .map_err(|_| GoalError::StorageUnavailable)?;
        lock.try_lock_exclusive()
            .map_err(|_| GoalError::StorageLocked)?;
        let result = operation(&self.state_path(session_id)?);
        let _ = FileExt::unlock(&lock);
        result
    }

    fn encrypt(&self, session: &DurableAgentSession) -> Result<Vec<u8>, GoalError> {
        let plaintext = serde_json::to_vec(session).map_err(|_| GoalError::StorageUnavailable)?;
        let key = self.keys.load_or_create_key()?;
        let cipher = XChaCha20Poly1305::new((&key).into());
        let mut nonce = [0_u8; 24];
        getrandom::fill(&mut nonce).map_err(|_| GoalError::StorageUnavailable)?;
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: session.session_id.as_bytes(),
                },
            )
            .map_err(|_| GoalError::StorageUnavailable)?;
        let mut output = Vec::with_capacity(HEADER_BYTES + ciphertext.len());
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        output.extend_from_slice(&nonce);
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }

    fn decrypt(&self, session_id: &str, bytes: &[u8]) -> Result<DurableAgentSession, GoalError> {
        if bytes.len() <= HEADER_BYTES || bytes.get(..MAGIC.len()) != Some(MAGIC) {
            return Err(GoalError::CheckpointCorrupt);
        }
        let version = u16::from_le_bytes(
            bytes[MAGIC.len()..MAGIC.len() + 2]
                .try_into()
                .map_err(|_| GoalError::CheckpointCorrupt)?,
        );
        if version != FORMAT_VERSION {
            return Err(GoalError::UnsupportedVersion);
        }
        let nonce_start = MAGIC.len() + 2;
        let nonce_end = nonce_start + 24;
        let key = self.keys.load_or_create_key()?;
        let cipher = XChaCha20Poly1305::new((&key).into());
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(&bytes[nonce_start..nonce_end]),
                Payload {
                    msg: &bytes[nonce_end..],
                    aad: session_id.as_bytes(),
                },
            )
            .map_err(|_| GoalError::CheckpointCorrupt)?;
        let session: DurableAgentSession =
            serde_json::from_slice(&plaintext).map_err(|_| GoalError::CheckpointCorrupt)?;
        if session.format_version != DurableAgentSession::FORMAT_VERSION
            || session.session_id != session_id
        {
            return Err(GoalError::CheckpointCorrupt);
        }
        Ok(session)
    }
}

impl GoalPersistence for EncryptedAgentStore {
    fn load(&self, session_id: &str) -> Result<Option<DurableAgentSession>, GoalError> {
        self.with_lock(session_id, |path| match std::fs::read(path) {
            Ok(bytes) => self.decrypt(session_id, &bytes).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(GoalError::StorageUnavailable),
        })
    }

    fn save(&self, session: &DurableAgentSession) -> Result<(), GoalError> {
        session.checkpoint_validate()?;
        let bytes = self.encrypt(session)?;
        self.with_lock(&session.session_id, |path| {
            let parent = path.parent().ok_or(GoalError::StorageUnavailable)?;
            let mut temporary = tempfile::NamedTempFile::new_in(parent)
                .map_err(|_| GoalError::StorageUnavailable)?;
            temporary
                .write_all(&bytes)
                .and_then(|_| temporary.as_file_mut().sync_all())
                .map_err(|_| GoalError::StorageUnavailable)?;
            temporary
                .persist(path)
                .map_err(|_| GoalError::StorageUnavailable)?;
            sync_directory(parent);
            Ok(())
        })
    }

    fn remove(&self, session_id: &str) -> Result<(), GoalError> {
        self.with_lock(session_id, |path| match std::fs::remove_file(path) {
            Ok(()) => {
                if let Some(parent) = path.parent() {
                    sync_directory(parent);
                }
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(GoalError::StorageUnavailable),
        })
    }
}

fn sync_directory(path: &Path) {
    if let Ok(directory) = File::open(path) {
        let _ = directory.sync_all();
    }
}

fn validate_opaque_id(value: &str) -> Result<(), GoalError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(GoalError::InvalidId)
    }
}

trait CheckpointValidation {
    fn checkpoint_validate(&self) -> Result<(), GoalError>;
}

impl CheckpointValidation for DurableAgentSession {
    fn checkpoint_validate(&self) -> Result<(), GoalError> {
        if self.format_version != DurableAgentSession::FORMAT_VERSION {
            return Err(GoalError::UnsupportedVersion);
        }
        validate_opaque_id(&self.session_id)?;
        validate_opaque_id(&self.repository_identity)?;
        if let Some(goal) = &self.active_goal {
            goal.checkpoint.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_session::{AgentBudgetAccount, GoalRepository, ModelBudgetLimit, PriceSnapshot};
    use review_agent::{ProcessReplayPolicy, ToolIntent, ToolRisk, TranscriptItem};

    #[derive(Debug)]
    struct StaticKey([u8; 32]);

    impl AgentStoreKeyProvider for StaticKey {
        fn load_or_create_key(&self) -> Result<[u8; 32], GoalError> {
            Ok(self.0)
        }
    }

    #[derive(Debug)]
    struct MissingKey;

    impl AgentStoreKeyProvider for MissingKey {
        fn load_or_create_key(&self) -> Result<[u8; 32], GoalError> {
            Err(GoalError::KeyUnavailable)
        }
    }

    fn store(root: &Path, byte: u8) -> EncryptedAgentStore {
        EncryptedAgentStore::new(root, Arc::new(StaticKey([byte; 32])))
    }

    fn budget() -> AgentBudgetAccount {
        AgentBudgetAccount::new(
            "deepseek-v4-flash",
            Some(PriceSnapshot {
                currency: "CNY".into(),
                input_cache_hit_per_million_micros: 20_000,
                input_cache_miss_per_million_micros: 1_000_000,
                output_per_million_micros: 2_000_000,
                source_url: "official".into(),
                source_version: "v1".into(),
                checked_at: "2026-08-19".into(),
            }),
            ModelBudgetLimit::CostMicros {
                currency: "CNY".into(),
                limit_micros: 1_000_000,
            },
        )
        .unwrap()
    }

    #[test]
    fn checkpoint_is_authenticated_encrypted_and_round_trips_without_plaintext() {
        let root = tempfile::tempdir().unwrap();
        let store = store(root.path(), 7);
        let repository = GoalRepository::new(store.clone());
        repository.load_or_create("repo-a", "identity-a").unwrap();
        repository
            .create_goal(
                "repo-a",
                "identity-a",
                "goal-a",
                "PROMPT_PLAINTEXT_MARKER".into(),
                "deepseek-v4-flash".into(),
                budget(),
                "workspace-digest".into(),
                1,
            )
            .unwrap();
        repository
            .mutate_goal_current("repo-a", 2, |goal| {
                goal.checkpoint
                    .recent_transcript
                    .push(TranscriptItem::ToolResult {
                        name: "filesystem.read".into(),
                        call_id: "call-a".into(),
                        content: "SANITIZED_TOOL_CONTENT_MARKER".into(),
                        counts_toward_budget: true,
                    });
                goal.checkpoint.pending_intents.push(ToolIntent {
                    execution_id: "exec-a".into(),
                    run_id: "goal-a".into(),
                    call_id: "call-b".into(),
                    tool_name: "filesystem.write".into(),
                    risk: ToolRisk::Write,
                    arguments: serde_json::json!({"content":"API_KEY_TEST_MARKER"}),
                    approval_id: Some("approval-a".into()),
                    approved: true,
                    resource: Some("safe.txt".into()),
                    before_digest: Some("absent".into()),
                    expected_after_digest: Some("digest".into()),
                    replay_policy: Some(ProcessReplayPolicy::Never),
                });
                Ok(())
            })
            .unwrap();
        let bytes = std::fs::read(root.path().join("repo-a.state")).unwrap();
        assert!(bytes.starts_with(MAGIC));
        let encoded = String::from_utf8_lossy(&bytes);
        for marker in [
            "PROMPT_PLAINTEXT_MARKER",
            "SANITIZED_TOOL_CONTENT_MARKER",
            "API_KEY_TEST_MARKER",
        ] {
            assert!(!encoded.contains(marker));
        }
        let loaded = store.load("repo-a").unwrap().unwrap();
        assert_eq!(
            loaded.active_goal.unwrap().objective,
            "PROMPT_PLAINTEXT_MARKER"
        );
    }

    #[test]
    fn wrong_key_tamper_version_and_missing_key_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let first = store(root.path(), 1);
        first
            .save(&DurableAgentSession::new("repo-a", "identity-a"))
            .unwrap();
        assert_eq!(
            store(root.path(), 2).load("repo-a").unwrap_err(),
            GoalError::CheckpointCorrupt
        );

        let path = root.path().join("repo-a.state");
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[8] = 99;
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(
            first.load("repo-a").unwrap_err(),
            GoalError::UnsupportedVersion
        );

        let missing = EncryptedAgentStore::new(root.path(), Arc::new(MissingKey));
        assert_eq!(
            missing
                .save(&DurableAgentSession::new("repo-b", "identity-b"))
                .unwrap_err(),
            GoalError::KeyUnavailable
        );
        assert!(!root.path().join("repo-b.state").exists());
    }

    #[test]
    fn corrupt_checkpoint_is_not_overwritten_by_load() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("repo-a.state");
        std::fs::write(&path, b"corrupt-marker").unwrap();
        let store = store(root.path(), 1);
        assert_eq!(
            store.load("repo-a").unwrap_err(),
            GoalError::CheckpointCorrupt
        );
        assert_eq!(std::fs::read(path).unwrap(), b"corrupt-marker");
    }
}
