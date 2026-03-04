use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};

use crate::clients::ConversationItem;

/// Session format version for forward compatibility.
/// Increment when the session JSON schema changes.
const CURRENT_FORMAT_VERSION: u32 = 1;

/// Session metadata and messages
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Session {
    /// Schema version for forward compatibility
    pub format_version: u32,
    /// Unique session identifier (UUID v4)
    pub id: String,
    /// Working directory where session was created
    pub working_dir: PathBuf,
    /// System prompt used for this session
    pub system_prompt: String,
    /// Unix timestamp in milliseconds when session was created
    pub created_at: u64,
    /// Unix timestamp in milliseconds when session was last updated
    pub updated_at: u64,
    /// Conversation history (typed, serializable)
    pub messages: Vec<ConversationItem>,
}

/// Returns the current time as Unix timestamp in milliseconds.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Session {
    /// Create a new session with the given working directory and system prompt.
    /// Sets `created_at` and `updated_at` to now.
    pub fn new(id: String, working_dir: PathBuf, system_prompt: String) -> Self {
        let now = now_ms();
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            id,
            working_dir,
            system_prompt,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        }
    }

    /// Load session from a file. Returns error if `format_version` is unsupported.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let data = fs::read_to_string(path)
            .with_context(|| format!("Failed to read session file: {}", path.display()))?;
        let session: Self = serde_json::from_str(&data)
            .with_context(|| format!("Failed to parse session file: {}", path.display()))?;
        if session.format_version > CURRENT_FORMAT_VERSION {
            return Err(anyhow!(
                "Unsupported session format version {} (max supported: {})",
                session.format_version,
                CURRENT_FORMAT_VERSION
            ));
        }
        Ok(session)
    }

    /// Save session atomically (write to temp file, then rename).
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create session directory: {}", parent.display()))?;
        }

        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize session")?;

        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, json)
            .with_context(|| format!("Failed to write temp session file: {}", temp_path.display()))?;
        fs::rename(&temp_path, path)
            .with_context(|| format!("Failed to rename temp file to: {}", path.display()))?;

        Ok(())
    }

    /// Update the `updated_at` timestamp to now.
    pub fn touch(&mut self) {
        self.updated_at = now_ms();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Role;
    use tempfile::TempDir;

    #[test]
    fn test_session_new_defaults() {
        let session = Session::new(
            "test-id".to_string(),
            PathBuf::from("/tmp/test"),
            "system prompt".to_string(),
        );
        assert_eq!(session.format_version, CURRENT_FORMAT_VERSION);
        assert_eq!(session.id, "test-id");
        assert_eq!(session.working_dir, PathBuf::from("/tmp/test"));
        assert_eq!(session.system_prompt, "system prompt");
        assert!(session.created_at > 0);
        assert_eq!(session.created_at, session.updated_at);
        assert!(session.messages.is_empty());
    }

    #[test]
    fn test_session_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.json");

        let mut session = Session::new(
            "abc-123".to_string(),
            PathBuf::from("/tmp/test"),
            "test prompt".to_string(),
        );
        session.messages.push(ConversationItem::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
        });

        session.save(&path).unwrap();
        let loaded = Session::load(&path).unwrap();

        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.format_version, session.format_version);
        assert_eq!(loaded.working_dir, session.working_dir);
        assert_eq!(loaded.system_prompt, session.system_prompt);
        assert_eq!(loaded.messages.len(), 1);
    }

    #[test]
    fn test_session_load_rejects_unsupported_version() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.json");

        let mut session = Session::new(
            "test".to_string(),
            PathBuf::from("/tmp"),
            "prompt".to_string(),
        );
        session.format_version = 999;
        // Write directly, bypassing save()
        let json = serde_json::to_string_pretty(&session).unwrap();
        fs::write(&path, json).unwrap();

        let result = Session::load(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported session format version"));
    }
}
