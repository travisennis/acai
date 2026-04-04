use std::{
    fs,
    io::{BufRead, BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::clients::ConversationItem;

/// Session format version for forward compatibility.
/// Increment when the session JSONL schema changes.
const CURRENT_FORMAT_VERSION: u32 = 2;

/// A single line in a JSONL session file.
/// Contains per-line metadata plus the flattened `ConversationItem`.
/// Note: The timestamp is stored within the `ConversationItem` itself to avoid
/// duplicate field issues during serialization.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionLine {
    pub format_version: u32,
    pub session_id: String,
    pub working_directory: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(flatten)]
    pub item: ConversationItem,
}

/// A metadata-only line written as the first entry in every session file.
/// Ensures session identity is preserved even when there are no messages.
#[derive(Serialize, Deserialize, Debug, Clone)]
struct SessionHeader {
    format_version: u32,
    session_id: String,
    timestamp: DateTime<Utc>,
    working_directory: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(rename = "type")]
    line_type: String,
}

/// In-memory session state reconstructed from a JSONL file.
///
/// A session represents a conversation with the AI, including its unique ID,
/// working directory, model used, and message history.
///
/// # Examples
///
/// ```
/// use acai::config::Session;
/// use std::path::PathBuf;
///
/// let session = Session::new("uuid-here".to_string(), PathBuf::from("/project"));
/// assert!(session.messages.is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct Session {
    /// Unique session identifier (UUID v4)
    pub id: String,
    /// Working directory where session was created
    pub working_dir: PathBuf,
    /// Model used for the session
    pub model: Option<String>,
    /// Conversation history
    pub messages: Vec<ConversationItem>,
}

impl Session {
    /// Creates a new empty session.
    ///
    /// # Examples
    ///
    /// ```
    /// use acai::config::Session;
    /// use std::path::PathBuf;
    ///
    /// let session = Session::new("uuid".to_string(), PathBuf::from("/project"));
    /// assert_eq!(session.id, "uuid");
    /// ```
    pub const fn new(id: String, working_dir: PathBuf) -> Self {
        Self {
            id,
            working_dir,
            model: None,
            messages: Vec::new(),
        }
    }

    /// Loads a session from a JSONL file.
    ///
    /// The first line is a `SessionHeader`, subsequent lines are `SessionLine` entries.
    /// Each line is a valid JSON object.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use acai::config::Session;
    /// use std::path::Path;
    ///
    /// let session = Session::load(Path::new("session.jsonl"))?;
    /// println!("Loaded session: {}", session.id);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened, or if any line
    /// cannot be parsed as valid JSON.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let file = fs::File::open(path)
            .with_context(|| format!("Failed to open session file: {}", path.display()))?;
        let reader = std::io::BufReader::new(file);

        let mut id = String::new();
        let mut working_dir = PathBuf::new();
        let mut model = None;
        let mut messages = Vec::new();

        for (line_num, line) in reader.lines().enumerate() {
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

            // First line is the session header
            if line_num == 0 {
                let header: SessionHeader = serde_json::from_str(trimmed).with_context(|| {
                    format!("Failed to parse header of session file: {}", path.display())
                })?;
                id = header.session_id;
                working_dir = header.working_directory;
                model = header.model;
                continue;
            }

            // Handle backward compatibility: old session files may have a duplicate
            // `timestamp` field at the SessionLine level (in addition to the one in
            // the flattened ConversationItem). We need to remove the duplicate.
            let entry: SessionLine = if trimmed.contains("\"timestamp\"") {
                // Parse as generic Value to handle potential duplicates
                let mut value: serde_json::Value =
                    serde_json::from_str(trimmed).with_context(|| {
                        format!(
                            "Failed to parse line {} of session file: {}",
                            line_num + 1,
                            path.display()
                        )
                    })?;

                // If there's a top-level timestamp field, remove it
                // (the ConversationItem already has its own timestamp)
                if let Some(obj) = value.as_object_mut() {
                    // Check if this looks like a SessionLine (has 'item' type fields)
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

            if entry.model.is_some() {
                model.clone_from(&entry.model);
            }

            messages.push(entry.item);
        }

        Ok(Self {
            id,
            working_dir,
            model,
            messages,
        })
    }

    /// Saves the session to a JSONL file atomically.
    ///
    /// Writes to a temporary file first, then renames to ensure atomic writes.
    /// The file contains one JSON object per line (JSONL format).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use acai::config::Session;
    /// use std::path::PathBuf;
    ///
    /// let mut session = Session::new("uuid".to_string(), PathBuf::from("/project"));
    /// session.save(Path::new("session.jsonl"))?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
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

        let now = Utc::now();

        // Write header line
        let header = SessionHeader {
            format_version: CURRENT_FORMAT_VERSION,
            session_id: self.id.clone(),
            timestamp: now,
            working_directory: self.working_dir.clone(),
            model: self.model.clone(),
            line_type: "session_start".to_string(),
        };
        serde_json::to_writer(&mut writer, &header)
            .context("Failed to serialize session header")?;
        writer.write_all(b"\n").context("Failed to write newline")?;

        // Write conversation items with their individual timestamps
        for item in &self.messages {
            // Ensure the item has a timestamp for serialization
            let mut item_with_timestamp = item.clone();
            match &mut item_with_timestamp {
                ConversationItem::Message { timestamp, .. }
                | ConversationItem::FunctionCall { timestamp, .. }
                | ConversationItem::FunctionCallOutput { timestamp, .. }
                | ConversationItem::Reasoning { timestamp, .. } => {
                    if timestamp.is_none() {
                        *timestamp = Some(now.to_rfc3339());
                    }
                },
            }

            let line = SessionLine {
                format_version: CURRENT_FORMAT_VERSION,
                session_id: self.id.clone(),
                working_directory: self.working_dir.clone(),
                model: self.model.clone(),
                item: item_with_timestamp,
            };
            serde_json::to_writer(&mut writer, &line)
                .context("Failed to serialize session line")?;
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
    use crate::models::Role;
    use tempfile::TempDir;

    #[test]
    fn test_session_new_defaults() {
        let session = Session::new("test-id".to_string(), PathBuf::from("/tmp/test"));
        assert_eq!(session.id, "test-id");
        assert_eq!(session.working_dir, PathBuf::from("/tmp/test"));
        assert!(session.messages.is_empty());
        assert!(session.model.is_none());
    }

    #[test]
    fn test_session_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let mut session = Session::new("abc-123".to_string(), PathBuf::from("/tmp/test"));
        session.model = Some("gpt-4".to_string());
        session.messages.push(ConversationItem::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
            timestamp: None,
        });

        session.save(&path).unwrap();
        let loaded = Session::load(&path).unwrap();

        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.working_dir, session.working_dir);
        assert_eq!(loaded.model, session.model);
        assert_eq!(loaded.messages.len(), 1);
    }

    #[test]
    fn test_session_jsonl_format() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let mut session = Session::new("test-uuid".to_string(), PathBuf::from("/work"));
        session.model = Some("test-model".to_string());
        session.messages.push(ConversationItem::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
            timestamp: None,
        });
        session.messages.push(ConversationItem::Message {
            role: Role::Assistant,
            content: "Hi".to_string(),
            id: Some("msg-1".to_string()),
            status: Some("completed".to_string()),
            timestamp: None,
        });

        session.save(&path).unwrap();

        // Verify it's valid JSONL: 1 header + 2 message lines
        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);

        // First line is the session header
        let header: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(header["format_version"], 2);
        assert_eq!(header["session_id"], "test-uuid");
        assert_eq!(header["working_directory"], "/work");
        assert_eq!(header["model"], "test-model");
        assert_eq!(header["type"], "session_start");

        // Second line is the first message
        let first_msg: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(first_msg["format_version"], 2);
        assert_eq!(first_msg["session_id"], "test-uuid");
        assert_eq!(first_msg["type"], "message");
        assert_eq!(first_msg["role"], "user");
        assert_eq!(first_msg["content"], "Hello");
    }

    #[test]
    fn test_session_multiple_item_types() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let mut session = Session::new("multi-test".to_string(), PathBuf::from("/tmp"));
        session.messages.push(ConversationItem::Message {
            role: Role::User,
            content: "list files".to_string(),
            id: None,
            status: None,
            timestamp: None,
        });
        session.messages.push(ConversationItem::FunctionCall {
            id: "fc-1".to_string(),
            call_id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: r#"{"cmd":"ls"}"#.to_string(),
            timestamp: None,
        });
        session.messages.push(ConversationItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: "file.txt".to_string(),
            timestamp: None,
        });
        session.messages.push(ConversationItem::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["thinking...".to_string()],
            encrypted_content: None,
            content: None,
            timestamp: None,
        });

        session.save(&path).unwrap();
        let loaded = Session::load(&path).unwrap();

        assert_eq!(loaded.messages.len(), 4);
    }

    #[test]
    fn test_session_empty_messages() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let session = Session::new("empty".to_string(), PathBuf::from("/tmp"));
        session.save(&path).unwrap();

        // File should have exactly one line (the header)
        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);

        let header: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(header["type"], "session_start");
        assert_eq!(header["session_id"], "empty");

        // Round-trip should preserve metadata
        let loaded = Session::load(&path).unwrap();
        assert_eq!(loaded.id, "empty");
        assert_eq!(loaded.working_dir, PathBuf::from("/tmp"));
        assert!(loaded.messages.is_empty());
    }

    #[test]
    fn test_session_no_model() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        let mut session = Session::new("no-model".to_string(), PathBuf::from("/tmp"));
        session.messages.push(ConversationItem::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
            timestamp: None,
        });

        session.save(&path).unwrap();

        // Verify model field is absent in JSON
        let content = fs::read_to_string(&path).unwrap();
        let line: serde_json::Value =
            serde_json::from_str(content.lines().next().unwrap()).unwrap();
        assert!(line.get("model").is_none());

        let loaded = Session::load(&path).unwrap();
        assert!(loaded.model.is_none());
    }

    /// Test backward compatibility: session files with duplicate timestamp fields
    /// (one at `SessionLine` level and one in `ConversationItem`) should load correctly.
    /// This was a bug in versions prior to the fix where both `SessionLine` and
    /// `ConversationItem` had timestamp fields, causing duplicate fields in JSON.
    #[test]
    fn test_session_backward_compatibility_duplicate_timestamp() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("session.jsonl");

        // Manually create a corrupted session file with duplicate timestamps
        // (simulating the bug where SessionLine.timestamp existed alongside
        // ConversationItem.timestamp)
        let corrupted_content = r#"{"format_version":2,"session_id":"test-session","timestamp":"2026-04-04T15:51:54.474459Z","working_directory":"/tmp/test","model":"gpt-4","type":"session_start"}
{"format_version":2,"session_id":"test-session","timestamp":"2026-04-04T15:51:18.873738Z","working_directory":"/tmp/test","model":"gpt-4","type":"message","role":"user","content":"Hello","id":null,"status":null,"timestamp":"2026-04-04T15:51:18.873738+00:00"}
{"format_version":2,"session_id":"test-session","timestamp":"2026-04-04T15:51:20.000000Z","working_directory":"/tmp/test","model":"gpt-4","type":"message","role":"assistant","content":"Hi there","id":"msg-1","status":"completed","timestamp":"2026-04-04T15:51:20.000000+00:00"}"#;

        fs::write(&path, corrupted_content).unwrap();

        // Should load without error despite duplicate timestamps
        let loaded = Session::load(&path).unwrap();
        assert_eq!(loaded.id, "test-session");
        assert_eq!(loaded.working_dir, PathBuf::from("/tmp/test"));
        assert_eq!(loaded.model, Some("gpt-4".to_string()));
        assert_eq!(loaded.messages.len(), 2);

        // Verify the messages were parsed correctly
        match &loaded.messages[0] {
            ConversationItem::Message { role, content, .. } => {
                assert_eq!(*role, Role::User);
                assert_eq!(content, "Hello");
            },
            _ => panic!("Expected Message"),
        }

        match &loaded.messages[1] {
            ConversationItem::Message { role, content, .. } => {
                assert_eq!(*role, Role::Assistant);
                assert_eq!(content, "Hi there");
            },
            _ => panic!("Expected Message"),
        }
    }
}
