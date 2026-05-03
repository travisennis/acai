use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Context;
use fs4::{FileExt, TryLockError};

use crate::clients::{ConversationItem, GitState, SessionRecord};

/// Session format version for append-only task event logs.
pub const CURRENT_FORMAT_VERSION: u32 = 4;

/// In-memory session state reconstructed from a JSONL file.
///
/// A session represents a conversation with the AI, including its unique ID,
/// working directory, model used, and full record history.
///
/// # Examples
///
/// ```
/// use cake::config::Session;
/// use std::path::PathBuf;
///
/// let session = Session::new(uuid::Uuid::new_v4(), PathBuf::from("/project"));
/// assert!(session.records.is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct Session {
    /// Unique session identifier (UUID v4)
    pub id: uuid::Uuid,
    /// Working directory where session was created
    pub working_dir: PathBuf,
    /// Model used for the session
    pub model: Option<String>,
    /// Full system prompt used when the session was created.
    pub system_prompt: Option<String>,
    /// Git repository state captured when the session was created.
    pub git: Option<GitState>,
    /// Full record history (`SessionMeta`, task boundaries, messages, tool calls, etc.)
    pub records: Vec<SessionRecord>,
}

impl Session {
    /// Creates a new empty session.
    ///
    /// # Examples
    ///
    /// ```
    /// use cake::config::Session;
    /// use std::path::PathBuf;
    ///
    /// let id = uuid::Uuid::new_v4();
    /// let session = Session::new(id, PathBuf::from("/project"));
    /// assert_eq!(session.id, id);
    /// ```
    pub const fn new(id: uuid::Uuid, working_dir: PathBuf) -> Self {
        Self {
            id,
            working_dir,
            model: None,
            system_prompt: None,
            git: None,
            records: Vec::new(),
        }
    }

    /// Returns the conversation items from this session's records,
    /// filtering out session metadata and task boundary records.
    pub fn messages(&self) -> Vec<ConversationItem> {
        self.records
            .iter()
            .filter_map(SessionRecord::to_conversation_item)
            .collect()
    }

    /// Loads a v4 append-only session from a JSONL file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened, or if any line
    /// cannot be parsed as valid JSON.
    #[allow(clippy::too_many_lines)]
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to open session file: {}", path.display()))?;
        let mut lines = content.lines().enumerate().peekable();

        let first_line = lines
            .by_ref()
            .find_map(|(_, line)| {
                let trimmed = line.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .ok_or_else(|| anyhow::anyhow!("Session file is empty"))?;

        let id;
        let working_dir;
        let model;
        let system_prompt;
        let git;
        let mut records = Vec::new();

        if let Ok(value) = serde_json::from_str::<serde_json::Value>(first_line.trim())
            && value.get("type").and_then(|value| value.as_str()) != Some("session_meta")
        {
            anyhow::bail!(
                "Unsupported or legacy session file format: expected session_meta as first record in {}",
                path.display()
            );
        }

        let meta: SessionRecord = serde_json::from_str(first_line.trim()).with_context(|| {
            format!(
                "Failed to parse session_meta record of session file: {}",
                path.display()
            )
        })?;

        match &meta {
            SessionRecord::SessionMeta {
                format_version,
                session_id,
                working_directory,
                model: m,
                system_prompt: sp,
                git: git_state,
                ..
            } => {
                if *format_version != CURRENT_FORMAT_VERSION {
                    anyhow::bail!(
                        "Unsupported session format_version: {} (expected {}). Session file: {}",
                        format_version,
                        CURRENT_FORMAT_VERSION,
                        path.display()
                    );
                }
                id = uuid::Uuid::parse_str(session_id)
                    .with_context(|| format!("Invalid session UUID '{session_id}'"))?;
                working_dir = working_directory.clone();
                model = m.clone();
                system_prompt = sp.clone();
                git = Some(git_state.clone());
            },
            _ => {
                return Err(anyhow::anyhow!(
                    "Unsupported or legacy session file format: expected session_meta as first record in {}",
                    path.display()
                ));
            },
        }
        records.push(meta);

        while let Some((line_num, line)) = lines.next() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<SessionRecord>(trimmed) {
                Ok(record) => records.push(record),
                Err(error) if lines.peek().is_none() && !content.ends_with('\n') => {
                    tracing::warn!(
                        "Ignoring partial final session record in {}: {}",
                        path.display(),
                        error
                    );
                    break;
                },
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "Failed to parse line {} of session file: {}",
                            line_num + 1,
                            path.display()
                        )
                    });
                },
            }
        }

        Ok(Self {
            id,
            working_dir,
            model,
            system_prompt,
            git,
            records,
        })
    }

    /// Create a new session file, write its metadata record, and return a locked append handle.
    pub fn create_on_disk(path: &Path, meta: &SessionRecord) -> anyhow::Result<File> {
        if !matches!(meta, SessionRecord::SessionMeta { .. }) {
            anyhow::bail!("First record in a new session file must be session_meta");
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create session directory: {}", parent.display())
            })?;
        }

        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("Failed to create session file: {}", path.display()))?;
        lock_session_file(&file, path)?;
        Self::append_record(&mut file, meta)?;
        Ok(file)
    }

    /// Open an existing session file for append and acquire an advisory lock.
    pub fn open_for_append(path: &Path) -> anyhow::Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(path)
            .with_context(|| format!("Failed to open session file: {}", path.display()))?;
        lock_session_file(&file, path)?;
        Ok(file)
    }

    /// Append one JSONL session record and flush it to disk.
    pub fn append_record(file: &mut File, record: &SessionRecord) -> anyhow::Result<()> {
        serde_json::to_writer(&mut *file, record).context("Failed to serialize session record")?;
        file.write_all(b"\n").context("Failed to write newline")?;
        file.flush().context("Failed to flush session file")
    }

    /// Append multiple JSONL session records.
    pub fn append_records(file: &mut File, records: &[SessionRecord]) -> anyhow::Result<()> {
        for record in records {
            Self::append_record(file, record)?;
        }
        Ok(())
    }
}

fn lock_session_file(file: &File, path: &Path) -> anyhow::Result<()> {
    match FileExt::try_lock(file) {
        Ok(()) => Ok(()),
        Err(TryLockError::WouldBlock) => {
            let id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("<unknown>");
            anyhow::bail!(
                "Another cake invocation is currently writing to session {id}. Wait for it to finish or run in a different directory."
            );
        },
        Err(TryLockError::Error(error)) => {
            Err(error).with_context(|| format!("Failed to lock session file: {}", path.display()))
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::clients::types::{ReasoningContent, TaskCompleteSubtype, Usage};
    use crate::models::Role;
    use tempfile::TempDir;

    /// Helper to create a minimal v4 session for testing.
    fn make_test_session() -> Session {
        let id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let mut session = Session::new(id, PathBuf::from("/work"));
        session.model = Some("test-model".to_string());
        session
    }

    fn meta_record(session: &Session) -> SessionRecord {
        SessionRecord::SessionMeta {
            format_version: CURRENT_FORMAT_VERSION,
            session_id: session.id.to_string(),
            timestamp: chrono::Utc::now(),
            working_directory: session.working_dir.clone(),
            model: session.model.clone(),
            tools: vec!["bash".to_string(), "read".to_string()],
            cake_version: Some("test".to_string()),
            system_prompt: Some("test system prompt".to_string()),
            git: GitState {
                repository_url: Some("https://example.com/repo.git".to_string()),
                branch: Some("main".to_string()),
                commit_hash: Some("abc123".to_string()),
            },
        }
    }

    fn task_start(session: &Session, task_id: &str) -> SessionRecord {
        SessionRecord::TaskStart {
            session_id: session.id.to_string(),
            task_id: task_id.to_string(),
            timestamp: chrono::Utc::now(),
        }
    }

    fn task_complete(session: &Session, task_id: &str) -> SessionRecord {
        SessionRecord::TaskComplete {
            subtype: TaskCompleteSubtype::Success,
            success: true,
            is_error: false,
            duration_ms: 100,
            turn_count: 1,
            num_turns: 1,
            session_id: session.id.to_string(),
            task_id: task_id.to_string(),
            result: Some("Done".to_string()),
            error: None,
            usage: Usage::default(),
            permission_denials: None,
        }
    }

    #[test]
    fn test_session_new_defaults() {
        let id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
        let session = Session::new(id, PathBuf::from("/tmp/test"));
        assert_eq!(session.id, id);
        assert_eq!(session.working_dir, PathBuf::from("/tmp/test"));
        assert!(session.records.is_empty());
        assert!(session.model.is_none());
    }

    #[test]
    fn test_session_create_and_load_roundtrip_v4() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let session = make_test_session();
        let mut file = Session::create_on_disk(&path, &meta_record(&session)).unwrap();
        Session::append_record(&mut file, &task_start(&session, "task-1")).unwrap();
        Session::append_record(
            &mut file,
            &SessionRecord::Message {
                role: Role::User,
                content: "Hello".to_string(),
                id: None,
                status: None,
                timestamp: None,
            },
        )
        .unwrap();
        Session::append_record(
            &mut file,
            &SessionRecord::Message {
                role: Role::Assistant,
                content: "Hi".to_string(),
                id: Some("msg-1".to_string()),
                status: Some("completed".to_string()),
                timestamp: None,
            },
        )
        .unwrap();
        Session::append_record(&mut file, &task_complete(&session, "task-1")).unwrap();
        drop(file);

        let loaded = Session::load(&path).unwrap();

        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.working_dir, session.working_dir);
        assert_eq!(loaded.model, session.model);
        assert_eq!(loaded.records.len(), 5);
        assert_eq!(loaded.messages().len(), 2);
    }

    #[test]
    fn test_session_jsonl_v4_format() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let session = make_test_session();
        let mut file = Session::create_on_disk(&path, &meta_record(&session)).unwrap();
        Session::append_record(
            &mut file,
            &SessionRecord::Message {
                role: Role::User,
                content: "Hello".to_string(),
                id: None,
                status: None,
                timestamp: None,
            },
        )
        .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let init: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(init["type"], "session_meta");
        assert_eq!(init["format_version"], CURRENT_FORMAT_VERSION);
        assert_eq!(init["session_id"], "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(init["working_directory"], "/work");
        assert_eq!(init["model"], "test-model");
        assert!(init["tools"].is_array());

        // Second line is the message
        let msg: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(msg["type"], "message");
        assert_eq!(msg["role"], "user");
        assert_eq!(msg["content"], "Hello");
    }

    #[test]
    fn test_session_multiple_item_types() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let session = make_test_session();
        let mut file = Session::create_on_disk(&path, &meta_record(&session)).unwrap();
        let records = vec![
            SessionRecord::Message {
                role: Role::User,
                content: "list files".to_string(),
                id: None,
                status: None,
                timestamp: None,
            },
            SessionRecord::FunctionCall {
                id: "fc-1".to_string(),
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: r#"{"cmd":"ls"}"#.to_string(),
                timestamp: None,
            },
            SessionRecord::FunctionCallOutput {
                call_id: "call-1".to_string(),
                output: "file.txt".to_string(),
                timestamp: None,
            },
            SessionRecord::Reasoning {
                id: "r-1".to_string(),
                summary: vec!["thinking...".to_string()],
                encrypted_content: None,
                content: None,
                timestamp: None,
            },
        ];

        Session::append_records(&mut file, &records).unwrap();
        drop(file);
        let loaded = Session::load(&path).unwrap();

        assert_eq!(loaded.records.len(), 5);
        assert_eq!(loaded.messages().len(), 4);
    }

    #[test]
    fn test_session_append_second_task_does_not_add_second_meta() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let session = make_test_session();
        let mut file = Session::create_on_disk(&path, &meta_record(&session)).unwrap();
        Session::append_records(
            &mut file,
            &[
                task_start(&session, "task-1"),
                task_complete(&session, "task-1"),
            ],
        )
        .unwrap();
        drop(file);

        let mut file = Session::open_for_append(&path).unwrap();
        Session::append_records(
            &mut file,
            &[
                task_start(&session, "task-2"),
                task_complete(&session, "task-2"),
            ],
        )
        .unwrap();
        drop(file);

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content.matches(r#""type":"session_meta""#).count(), 1);
        assert_eq!(content.matches(r#""type":"task_start""#).count(), 2);
        assert_eq!(content.matches(r#""type":"task_complete""#).count(), 2);
    }

    #[test]
    fn test_session_no_model() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let session = Session::new(uuid::Uuid::new_v4(), PathBuf::from("/tmp"));
        let mut file = Session::create_on_disk(&path, &meta_record(&session)).unwrap();
        Session::append_record(
            &mut file,
            &SessionRecord::Message {
                role: Role::User,
                content: "Hello".to_string(),
                id: None,
                status: None,
                timestamp: None,
            },
        )
        .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let line = content.lines().next().unwrap();
        let val: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(val.get("model").is_none());

        let loaded = Session::load(&path).unwrap();
        assert!(loaded.model.is_none());
    }

    #[test]
    fn test_session_legacy_first_record_errors() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");
        fs::write(
            &path,
            r#"{"format_version":3,"session_id":"550e8400-e29b-41d4-a716-446655440000","timestamp":"2026-04-04T15:51:54Z","working_directory":"/tmp/test","type":"init","tools":[]}"#,
        )
        .unwrap();

        let error = Session::load(&path).unwrap_err().to_string();
        assert!(error.contains("Unsupported or legacy session file format"));
    }

    #[test]
    fn test_session_format_version_mismatch_errors() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");
        fs::write(
            &path,
            r#"{"type":"session_meta","format_version":5,"session_id":"550e8400-e29b-41d4-a716-446655440000","timestamp":"2026-04-04T15:51:54Z","working_directory":"/tmp/test","tools":[]}"#,
        )
        .unwrap();

        let error = Session::load(&path).unwrap_err().to_string();
        assert!(error.contains("Unsupported session format_version: 5 (expected 4)"));
    }

    #[test]
    fn test_session_loads_trailing_task_start() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");
        let session = make_test_session();
        let mut file = Session::create_on_disk(&path, &meta_record(&session)).unwrap();
        Session::append_record(&mut file, &task_start(&session, "task-1")).unwrap();
        drop(file);

        let loaded = Session::load(&path).unwrap();
        assert!(matches!(
            loaded.records.last(),
            Some(SessionRecord::TaskStart { .. })
        ));
    }

    #[test]
    fn test_session_v4_roundtrip_with_reasoning() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let session = make_test_session();
        let mut file = Session::create_on_disk(&path, &meta_record(&session)).unwrap();
        Session::append_record(
            &mut file,
            &SessionRecord::Reasoning {
                id: "r-1".to_string(),
                summary: vec!["thinking...".to_string()],
                encrypted_content: Some("gAAAAABencrypted...".to_string()),
                content: Some(vec![ReasoningContent {
                    content_type: "reasoning_text".to_string(),
                    text: Some("deep thoughts".to_string()),
                }]),
                timestamp: None,
            },
        )
        .unwrap();
        drop(file);

        let loaded = Session::load(&path).unwrap();

        assert_eq!(loaded.records.len(), 2);
        match &loaded.records[1] {
            SessionRecord::Reasoning {
                encrypted_content, ..
            } => {
                assert_eq!(encrypted_content.as_deref(), Some("gAAAAABencrypted..."));
            },
            _ => panic!("Expected Reasoning record"),
        }
    }

    #[test]
    fn test_session_create_writes_v4() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let session = make_test_session();
        let _file = Session::create_on_disk(&path, &meta_record(&session)).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let first_line = content.lines().next().unwrap();
        let val: serde_json::Value = serde_json::from_str(first_line).unwrap();
        assert_eq!(val["type"], "session_meta");
        assert_eq!(val["format_version"], CURRENT_FORMAT_VERSION);
    }

    #[test]
    fn test_session_lock_error_is_user_facing() {
        let dir = TempDir::new().unwrap();
        let path = dir
            .path()
            .join("550e8400-e29b-41d4-a716-446655440000.jsonl");
        let session = make_test_session();
        let _locked = Session::create_on_disk(&path, &meta_record(&session)).unwrap();

        let error = Session::open_for_append(&path).unwrap_err().to_string();
        assert!(error.contains(
            "Another cake invocation is currently writing to session 550e8400-e29b-41d4-a716-446655440000"
        ));
    }
}
