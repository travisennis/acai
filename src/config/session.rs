use std::{
    fs,
    io::{BufRead, BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::Context;
use chrono::Utc;
use serde::Deserialize;

use crate::clients::{ConversationItem, SessionRecord};

/// Session format version for v3 (unified JSONL schema).
const CURRENT_FORMAT_VERSION: u32 = 3;

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
    /// Full record history (Init, Messages, `FunctionCall`, etc.)
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
            records: Vec::new(),
        }
    }

    /// Returns the conversation items from this session's records,
    /// filtering out Init and Result records.
    pub fn messages(&self) -> Vec<ConversationItem> {
        self.records
            .iter()
            .filter_map(SessionRecord::to_conversation_item)
            .collect()
    }

    /// Loads a session from a JSONL file (v2 or v3 format).
    ///
    /// The method auto-detects the format version:
    /// - v3: first line is `{"type":"init", ...}`. Parsed directly.
    /// - v2: first line is `{"type":"session_start", ...}`. Converted in memory.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened, or if any line
    /// cannot be parsed as valid JSON.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let file = fs::File::open(path)
            .with_context(|| format!("Failed to open session file: {}", path.display()))?;
        let reader = std::io::BufReader::new(file);

        let mut lines = reader.lines().enumerate().peekable();

        // Peek at the first non-empty line to detect format version
        let first_line = loop {
            match lines.next() {
                Some((_, Ok(line))) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        break trimmed.to_string();
                    }
                },
                Some((_, Err(e))) => {
                    return Err(e).context("Failed to read session file");
                },
                None => return Err(anyhow::anyhow!("Session file is empty")),
            }
        };

        // Detect v2 vs v3
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&first_line) {
            let type_field = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if type_field == "session_start" {
                // v2 format
                return Self::load_format_v2(path, &first_line, lines);
            }
            if type_field == "init" {
                // v3 format
                return Self::load_format_v3(path, &first_line, lines);
            }
        }

        Err(anyhow::anyhow!(
            "Unable to detect session format in: {}",
            path.display()
        ))
    }

    /// Parse a v3 format session file.
    fn load_format_v3(
        path: &Path,
        first_line: &str,
        lines: std::iter::Peekable<impl Iterator<Item = (usize, Result<String, std::io::Error>)>>,
    ) -> anyhow::Result<Self> {
        let id;
        let working_dir;
        let model;
        let mut records = Vec::new();

        // Parse the Init record from the first line
        let init: SessionRecord = serde_json::from_str(first_line.trim()).with_context(|| {
            format!(
                "Failed to parse Init record of session file: {}",
                path.display()
            )
        })?;

        match &init {
            SessionRecord::Init {
                session_id,
                working_directory,
                model: m,
                ..
            } => {
                id = uuid::Uuid::parse_str(session_id)
                    .with_context(|| format!("Invalid session UUID '{session_id}'"))?;
                working_dir = working_directory.clone();
                model = m.clone();
            },
            _ => {
                return Err(anyhow::anyhow!(
                    "Expected Init record as first line in v3 session file: {}",
                    path.display()
                ));
            },
        }
        records.push(init);

        // Parse remaining lines
        for (line_num, line) in lines {
            let line = line.with_context(|| {
                format!(
                    "Failed to read line {} of session file: {}",
                    line_num + 1,
                    path.display()
                )
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let record: SessionRecord = serde_json::from_str(trimmed).with_context(|| {
                format!(
                    "Failed to parse line {} of session file: {}",
                    line_num + 1,
                    path.display()
                )
            })?;
            records.push(record);
        }

        // Drop any trailing Result record from in-memory resumable history.
        // The result record is metadata about a prior completed run, not
        // an input for the next run.
        if matches!(records.last(), Some(SessionRecord::Result { .. })) {
            records.pop();
        }

        Ok(Self {
            id,
            working_dir,
            model,
            records,
        })
    }

    /// Parse a v2 format session file and convert to v3 in memory.
    fn load_format_v2(
        path: &Path,
        first_line: &str,
        lines: std::iter::Peekable<impl Iterator<Item = (usize, Result<String, std::io::Error>)>>,
    ) -> anyhow::Result<Self> {
        // Parse the v2 header
        #[derive(Deserialize)]
        struct V2Header {
            session_id: String,
            working_directory: PathBuf,
            #[serde(default)]
            model: Option<String>,
        }

        let header: V2Header = serde_json::from_str(first_line.trim()).with_context(|| {
            format!(
                "Failed to parse v2 header of session file: {}",
                path.display()
            )
        })?;
        let id = uuid::Uuid::parse_str(&header.session_id)
            .with_context(|| format!("Invalid session UUID '{}'", header.session_id))?;
        let working_dir = header.working_directory;
        let model = header.model;
        let mut records = Vec::new();

        // Create an Init record from the v2 header data
        let now = Utc::now();
        records.push(SessionRecord::Init {
            format_version: CURRENT_FORMAT_VERSION,
            session_id: id.to_string(),
            timestamp: now,
            working_directory: working_dir.clone(),
            model: model.clone(),
            tools: vec![], // v2 didn't store tools; leave empty
        });

        // Parse v2 session lines
        #[allow(clippy::items_after_statements)]
        #[derive(Deserialize)]
        struct V2Line {
            #[serde(flatten)]
            item: ConversationItem,
        }

        for (line_num, line) in lines {
            let line = line.with_context(|| {
                format!(
                    "Failed to read line {} of session file: {}",
                    line_num + 1,
                    path.display()
                )
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Handle v2 duplicate timestamp field
            let entry: V2Line = if trimmed.contains("\"timestamp\"") {
                let mut value: serde_json::Value =
                    serde_json::from_str(trimmed).with_context(|| {
                        format!(
                            "Failed to parse line {} of session file: {}",
                            line_num + 1,
                            path.display()
                        )
                    })?;
                if let Some(obj) = value.as_object_mut() {
                    let has_item_fields = obj.contains_key("role")
                        || obj.contains_key("type")
                        || obj.contains_key("call_id");
                    if has_item_fields && obj.contains_key("timestamp") {
                        obj.remove("timestamp");
                    }
                }
                serde_json::from_value(value).with_context(|| {
                    format!(
                        "Failed to parse line {} of session file: {}",
                        line_num + 1,
                        path.display()
                    )
                })?
            } else {
                serde_json::from_str(trimmed).with_context(|| {
                    format!(
                        "Failed to parse line {} of session file: {}",
                        line_num + 1,
                        path.display()
                    )
                })?
            };

            records.push(SessionRecord::from_conversation_item(&entry.item));
        }

        Ok(Self {
            id,
            working_dir,
            model,
            records,
        })
    }

    /// Saves the session to a JSONL file atomically.
    ///
    /// Writes to a temporary file first, then renames to ensure atomic writes.
    /// The file is written in v3 format with exactly one Init record at the top,
    /// zero or more conversation records, and at most one Result record at the end.
    ///
    /// # Errors
    ///
    /// Returns an error if the session directory cannot be created, or if
    /// the file cannot be written.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create session directory: {}", parent.display())
            })?;
        }

        let temp_path = path.with_extension("tmp");
        let file = fs::File::create(&temp_path).with_context(|| {
            format!(
                "Failed to create temp session file: {}",
                temp_path.display()
            )
        })?;
        let mut writer = BufWriter::new(file);

        // Ensure the session always has an Init record at the top.
        // If the first record is not an Init, write one automatically.
        let needs_init = !matches!(self.records.first(), Some(SessionRecord::Init { .. }));
        if needs_init {
            let init = SessionRecord::Init {
                format_version: CURRENT_FORMAT_VERSION,
                session_id: self.id.to_string(),
                timestamp: Utc::now(),
                working_directory: self.working_dir.clone(),
                model: self.model.clone(),
                tools: vec![],
            };
            serde_json::to_writer(&mut writer, &init)
                .context("Failed to serialize session init record")?;
            writer.write_all(b"\n").context("Failed to write newline")?;
        }

        for record in &self.records {
            serde_json::to_writer(&mut writer, record)
                .context("Failed to serialize session record")?;
            writer.write_all(b"\n").context("Failed to write newline")?;
        }

        writer.flush().context("Failed to flush session file")?;
        drop(writer);

        fs::rename(&temp_path, path)
            .with_context(|| format!("Failed to rename temp file to: {}", path.display()))?;

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::clients::types::{ReasoningContent, ResultSubtype, Usage};
    use crate::models::Role;
    use tempfile::TempDir;

    /// Helper to create a minimal v3 session for testing.
    fn make_test_session() -> Session {
        let id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let mut session = Session::new(id, PathBuf::from("/work"));
        session.model = Some("test-model".to_string());
        session.records.push(SessionRecord::Init {
            format_version: 3,
            session_id: id.to_string(),
            timestamp: Utc::now(),
            working_directory: PathBuf::from("/work"),
            model: Some("test-model".to_string()),
            tools: vec!["bash".to_string(), "read".to_string()],
        });
        session
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
    fn test_session_save_and_load_roundtrip_v3() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let mut session = make_test_session();
        session.records.push(SessionRecord::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
            timestamp: None,
        });
        session.records.push(SessionRecord::Message {
            role: Role::Assistant,
            content: "Hi".to_string(),
            id: Some("msg-1".to_string()),
            status: Some("completed".to_string()),
            timestamp: None,
        });

        session.save(&path).unwrap();
        let loaded = Session::load(&path).unwrap();

        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.working_dir, session.working_dir);
        assert_eq!(loaded.model, session.model);
        // Init + 2 messages = 3 records
        assert_eq!(loaded.records.len(), 3);
    }

    #[test]
    fn test_session_jsonl_v3_format() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let mut session = make_test_session();
        session.records.push(SessionRecord::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
            timestamp: None,
        });

        session.save(&path).unwrap();

        // Verify it's valid JSONL: Init + 1 message = 2 lines
        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        // First line is the Init record
        let init: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(init["type"], "init");
        assert_eq!(init["format_version"], 3);
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

        let mut session = make_test_session();
        session.records.push(SessionRecord::Message {
            role: Role::User,
            content: "list files".to_string(),
            id: None,
            status: None,
            timestamp: None,
        });
        session.records.push(SessionRecord::FunctionCall {
            id: "fc-1".to_string(),
            call_id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: r#"{"cmd":"ls"}"#.to_string(),
            timestamp: None,
        });
        session.records.push(SessionRecord::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: "file.txt".to_string(),
            timestamp: None,
        });
        session.records.push(SessionRecord::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["thinking...".to_string()],
            encrypted_content: None,
            content: None,
            timestamp: None,
        });

        session.save(&path).unwrap();
        let loaded = Session::load(&path).unwrap();

        // Init + 4 items = 5 records
        assert_eq!(loaded.records.len(), 5);
    }

    #[test]
    fn test_session_empty_messages() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let id = uuid::Uuid::new_v4();
        let session = Session::new(id, PathBuf::from("/tmp"));
        // No Init record, no messages - save should produce empty file
        session.save(&path).unwrap();

        // An empty session file with no records is technically valid but
        // won't load back (no Init record to detect format).
        // In practice, sessions always have at least an Init record.
    }

    #[test]
    fn test_session_no_model() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let id = uuid::Uuid::new_v4();
        let mut session = Session::new(id, PathBuf::from("/tmp"));
        session.records.push(SessionRecord::Init {
            format_version: 3,
            session_id: id.to_string(),
            timestamp: Utc::now(),
            working_directory: PathBuf::from("/tmp"),
            model: None,
            tools: vec![],
        });
        session.records.push(SessionRecord::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
            timestamp: None,
        });

        session.save(&path).unwrap();

        // Verify model field is absent in JSON
        let content = fs::read_to_string(&path).unwrap();
        let line = content.lines().next().unwrap();
        let val: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(val.get("model").is_none());

        let loaded = Session::load(&path).unwrap();
        assert!(loaded.model.is_none());
    }

    #[test]
    fn test_session_v2_backward_compat() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");
        let test_uuid = "550e8400-e29b-41d4-a716-446655440002";

        // Manually create a v2 session file
        let v2_content = format!(
            r#"{{"format_version":2,"session_id":"{test_uuid}","timestamp":"2026-04-04T15:51:54.474459Z","working_directory":"/tmp/test","model":"gpt-4","type":"session_start"}}
{{"format_version":2,"session_id":"{test_uuid}","working_directory":"/tmp/test","model":"gpt-4","type":"message","role":"user","content":"Hello","id":null,"status":null}}
{{"format_version":2,"session_id":"{test_uuid}","working_directory":"/tmp/test","model":"gpt-4","type":"message","role":"assistant","content":"Hi there","id":"msg-1","status":"completed"}}"#
        );

        fs::write(&path, v2_content).unwrap();

        let loaded = Session::load(&path).unwrap();
        assert_eq!(loaded.id.to_string(), test_uuid);
        assert_eq!(loaded.working_dir, PathBuf::from("/tmp/test"));
        assert_eq!(loaded.model, Some("gpt-4".to_string()));
        // Init (generated) + 2 messages = 3 records
        assert_eq!(loaded.records.len(), 3);

        // Verify the Init record was generated
        assert!(matches!(
            &loaded.records[0],
            SessionRecord::Init { session_id, .. } if session_id == test_uuid
        ));

        // Verify messages were parsed correctly
        match &loaded.records[1] {
            SessionRecord::Message { role, content, .. } => {
                assert_eq!(*role, Role::User);
                assert_eq!(content, "Hello");
            },
            _ => panic!("Expected Message record"),
        }
        match &loaded.records[2] {
            SessionRecord::Message { role, content, .. } => {
                assert_eq!(*role, Role::Assistant);
                assert_eq!(content, "Hi there");
            },
            _ => panic!("Expected Message record"),
        }
    }

    #[test]
    fn test_session_v2_duplicate_timestamp_compat() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");
        let test_uuid = "550e8400-e29b-41d4-a716-446655440003";

        // Simulate v2 file with duplicate timestamp fields
        let v2_content = format!(
            r#"{{"format_version":2,"session_id":"{test_uuid}","timestamp":"2026-04-04T15:51:54.474459Z","working_directory":"/tmp/test","model":"gpt-4","type":"session_start"}}
{{"format_version":2,"session_id":"{test_uuid}","timestamp":"2026-04-04T15:51:18.873738Z","working_directory":"/tmp/test","model":"gpt-4","type":"message","role":"user","content":"Hello","id":null,"status":null,"timestamp":"2026-04-04T15:51:18.873738+00:00"}}"#
        );

        fs::write(&path, v2_content).unwrap();

        let loaded = Session::load(&path).unwrap();
        assert_eq!(loaded.id.to_string(), test_uuid);
        assert_eq!(loaded.records.len(), 2); // Init + 1 message
    }

    #[test]
    fn test_session_result_record_stripped_on_load() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let mut session = make_test_session();
        session.records.push(SessionRecord::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
            timestamp: None,
        });
        // Add a Result record (as if a completed run saved it)
        session.records.push(SessionRecord::Result {
            subtype: ResultSubtype::Success,
            success: true,
            is_error: false,
            duration_ms: 1500,
            turn_count: 1,
            num_turns: 1,
            session_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            result: Some("Done!".to_string()),
            error: None,
            usage: Usage::default(),
            permission_denials: None,
        });

        session.save(&path).unwrap();

        // Loading should strip the trailing Result record
        let loaded = Session::load(&path).unwrap();
        assert_eq!(loaded.records.len(), 2); // Init + message
        assert!(matches!(loaded.records[1], SessionRecord::Message { .. }));
    }

    #[test]
    fn test_session_v3_roundtrip_with_reasoning() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let mut session = make_test_session();
        session.records.push(SessionRecord::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["thinking...".to_string()],
            encrypted_content: Some("gAAAAABencrypted...".to_string()),
            content: Some(vec![ReasoningContent {
                content_type: "reasoning_text".to_string(),
                text: Some("deep thoughts".to_string()),
            }]),
            timestamp: None,
        });

        session.save(&path).unwrap();
        let loaded = Session::load(&path).unwrap();

        assert_eq!(loaded.records.len(), 2); // Init + Reasoning
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
    fn test_session_save_writes_v3() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let session = make_test_session();
        session.save(&path).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let first_line = content.lines().next().unwrap();
        let val: serde_json::Value = serde_json::from_str(first_line).unwrap();
        assert_eq!(val["type"], "init");
        assert_eq!(val["format_version"], 3);
    }
}
