use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::Role;

/// Snapshot of git repository state captured when a session file is created.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct GitState {
    pub repository_url: Option<String>,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
}

// =============================================================================
// Reasoning Content (preserved for API round-tripping)
// =============================================================================

/// A content item within a reasoning output, preserved verbatim for echoing
/// back to the API in multi-turn conversations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningContent {
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

// =============================================================================
// Conversation Item Enum (for Responses API input/output)
// =============================================================================

/// Represents a single item in the conversation history, mapping directly to
/// the Responses API input/output array format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationItem {
    Message {
        role: Role,
        content: String,
        /// Assistant message ID (required for assistant messages in input)
        id: Option<String>,
        /// "completed" or "incomplete" (required for assistant messages in input)
        status: Option<String>,
        /// Timestamp when this item was created
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: String,
        /// Timestamp when this item was created
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },
    FunctionCallOutput {
        call_id: String,
        output: String,
        /// Timestamp when this item was created
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },
    Reasoning {
        id: String,
        summary: Vec<String>,
        /// Opaque encrypted reasoning content that must be echoed back to the
        /// API for multi-turn conversations with reasoning models.
        #[serde(skip_serializing_if = "Option::is_none")]
        encrypted_content: Option<String>,
        /// Original content array from the API response (e.g., `reasoning_text` items).
        /// Must be echoed back so the router can reconstruct `reasoning_content`
        /// for Chat Completions providers like Moonshot AI.
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<Vec<ReasoningContent>>,
        /// Timestamp when this item was created
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },
}

impl ConversationItem {
    /// Convert this item to JSON format for API input
    pub fn to_api_input(&self) -> serde_json::Value {
        match self {
            Self::Message {
                role,
                content,
                id,
                status,
                ..
            } => {
                let use_output_format = matches!(role, Role::Assistant);

                let mut msg = serde_json::json!({
                    "type": "message",
                    "role": role.as_str(),
                });

                // Content format depends on role
                if use_output_format {
                    msg["content"] = serde_json::json!([{
                        "type": "output_text",
                        "text": content,
                        "annotations": []
                    }]);
                } else {
                    msg["content"] = serde_json::json!([{
                        "type": "input_text",
                        "text": content
                    }]);
                }

                // Include id and status for assistant messages
                if let Some(id) = id {
                    msg["id"] = serde_json::json!(id);
                }
                if let Some(status) = status {
                    msg["status"] = serde_json::json!(status);
                }

                msg
            },
            Self::FunctionCall {
                id,
                call_id,
                name,
                arguments,
                ..
            } => {
                serde_json::json!({
                    "type": "function_call",
                    "id": id,
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments
                })
            },
            Self::FunctionCallOutput {
                call_id, output, ..
            } => {
                serde_json::json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": output
                })
            },
            Self::Reasoning {
                id,
                summary,
                encrypted_content,
                content,
                ..
            } => {
                let mut obj = serde_json::json!({
                    "type": "reasoning",
                    "id": id,
                    "summary": summary.iter().map(|s| {
                        serde_json::json!({"type": "summary_text", "text": s})
                    }).collect::<Vec<_>>()
                });
                if let Some(enc) = encrypted_content {
                    obj["encrypted_content"] = serde_json::json!(enc);
                }
                if let Some(content) = content {
                    obj["content"] = serde_json::json!(content);
                }
                obj
            },
        }
    }

    /// Convert this item to JSON format for streaming output
    #[allow(dead_code)]
    pub fn to_streaming_json(&self) -> serde_json::Value {
        match self {
            Self::Message {
                role,
                content,
                id,
                status,
                ..
            } => {
                let role_str = role.as_str();
                let mut obj = serde_json::json!({
                    "type": "message",
                    "role": role_str,
                });
                obj["content"] = serde_json::json!(content);
                if let Some(id) = id {
                    obj["id"] = serde_json::json!(id);
                }
                if let Some(status) = status {
                    obj["status"] = serde_json::json!(status);
                }
                obj
            },
            Self::FunctionCall {
                id,
                call_id,
                name,
                arguments,
                ..
            } => {
                serde_json::json!({
                    "type": "function_call",
                    "id": id,
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments
                })
            },
            Self::FunctionCallOutput {
                call_id, output, ..
            } => {
                serde_json::json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": output
                })
            },
            Self::Reasoning { id, summary, .. } => {
                serde_json::json!({
                    "type": "reasoning",
                    "id": id,
                    "summary": summary
                })
            },
        }
    }
}

// =============================================================================
// Session Record Enum (for unified JSONL schema)
// =============================================================================

/// Subtype of a task completion record.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskCompleteSubtype {
    Success,
    ErrorDuringExecution,
    ErrorMaxTurns,
}

/// A single line in an append-only JSONL session file.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionRecord {
    /// First line of every persisted session file.
    SessionMeta {
        format_version: u32,
        session_id: String,
        /// Timestamp when the session was created.
        timestamp: DateTime<Utc>,
        working_directory: PathBuf,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        tools: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cake_version: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system_prompt: Option<String>,
        #[serde(default)]
        git: GitState,
    },

    TaskStart {
        session_id: String,
        task_id: String,
        timestamp: DateTime<Utc>,
    },

    /// Initial prompt context used for one invocation.
    ///
    /// These records are append-only audit entries. They are intentionally not
    /// replayed from session history; each invocation rebuilds fresh prompt
    /// context from current AGENTS.md files, skills, and environment state.
    PromptContext {
        session_id: String,
        task_id: String,
        role: Role,
        content: String,
        timestamp: DateTime<Utc>,
    },

    Message {
        role: Role,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },

    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },

    FunctionCallOutput {
        call_id: String,
        output: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },

    Reasoning {
        id: String,
        summary: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        encrypted_content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<Vec<ReasoningContent>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },

    TaskComplete {
        subtype: TaskCompleteSubtype,
        success: bool,
        is_error: bool,
        duration_ms: u64,
        turn_count: u32,
        num_turns: u32,
        session_id: String,
        task_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        usage: Usage,
        #[serde(skip_serializing_if = "Option::is_none")]
        permission_denials: Option<Vec<String>>,
    },
}

/// A single line in `--output-format stream-json` output for the current task.
///
/// This intentionally excludes `session_meta`, so live stream output cannot be
/// mistaken for a complete resumable session file.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamRecord {
    TaskStart {
        session_id: String,
        task_id: String,
        timestamp: DateTime<Utc>,
    },

    Message {
        role: Role,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },

    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },

    FunctionCallOutput {
        call_id: String,
        output: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },

    Reasoning {
        id: String,
        summary: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        encrypted_content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<Vec<ReasoningContent>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
    },

    TaskComplete {
        subtype: TaskCompleteSubtype,
        success: bool,
        is_error: bool,
        duration_ms: u64,
        turn_count: u32,
        num_turns: u32,
        session_id: String,
        task_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        usage: Usage,
        #[serde(skip_serializing_if = "Option::is_none")]
        permission_denials: Option<Vec<String>>,
    },
}

impl From<StreamRecord> for SessionRecord {
    fn from(record: StreamRecord) -> Self {
        match record {
            StreamRecord::TaskStart {
                session_id,
                task_id,
                timestamp,
            } => Self::TaskStart {
                session_id,
                task_id,
                timestamp,
            },
            StreamRecord::Message {
                role,
                content,
                id,
                status,
                timestamp,
            } => Self::Message {
                role,
                content,
                id,
                status,
                timestamp,
            },
            StreamRecord::FunctionCall {
                id,
                call_id,
                name,
                arguments,
                timestamp,
            } => Self::FunctionCall {
                id,
                call_id,
                name,
                arguments,
                timestamp,
            },
            StreamRecord::FunctionCallOutput {
                call_id,
                output,
                timestamp,
            } => Self::FunctionCallOutput {
                call_id,
                output,
                timestamp,
            },
            StreamRecord::Reasoning {
                id,
                summary,
                encrypted_content,
                content,
                timestamp,
            } => Self::Reasoning {
                id,
                summary,
                encrypted_content,
                content,
                timestamp,
            },
            StreamRecord::TaskComplete {
                subtype,
                success,
                is_error,
                duration_ms,
                turn_count,
                num_turns,
                session_id,
                task_id,
                result,
                error,
                usage,
                permission_denials,
            } => Self::TaskComplete {
                subtype,
                success,
                is_error,
                duration_ms,
                turn_count,
                num_turns,
                session_id,
                task_id,
                result,
                error,
                usage,
                permission_denials,
            },
        }
    }
}

impl StreamRecord {
    /// Convert a `ConversationItem` into its corresponding `StreamRecord` variant.
    pub fn from_conversation_item(item: &ConversationItem) -> Self {
        match item {
            ConversationItem::Message {
                role,
                content,
                id,
                status,
                timestamp,
            } => Self::Message {
                role: *role,
                content: content.clone(),
                id: id.clone(),
                status: status.clone(),
                timestamp: timestamp.clone(),
            },
            ConversationItem::FunctionCall {
                id,
                call_id,
                name,
                arguments,
                timestamp,
            } => Self::FunctionCall {
                id: id.clone(),
                call_id: call_id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
                timestamp: timestamp.clone(),
            },
            ConversationItem::FunctionCallOutput {
                call_id,
                output,
                timestamp,
            } => Self::FunctionCallOutput {
                call_id: call_id.clone(),
                output: output.clone(),
                timestamp: timestamp.clone(),
            },
            ConversationItem::Reasoning {
                id,
                summary,
                encrypted_content,
                content,
                timestamp,
            } => Self::Reasoning {
                id: id.clone(),
                summary: summary.clone(),
                encrypted_content: encrypted_content.clone(),
                content: content.clone(),
                timestamp: timestamp.clone(),
            },
        }
    }
}

impl SessionRecord {
    /// Convert a `SessionRecord` back into a `ConversationItem`, if applicable.
    /// Returns `None` for session metadata and task boundary records, which have no
    /// `ConversationItem` equivalent.
    pub fn to_conversation_item(&self) -> Option<ConversationItem> {
        match self {
            Self::Message {
                role,
                content,
                id,
                status,
                timestamp,
            } => Some(ConversationItem::Message {
                role: *role,
                content: content.clone(),
                id: id.clone(),
                status: status.clone(),
                timestamp: timestamp.clone(),
            }),
            Self::FunctionCall {
                id,
                call_id,
                name,
                arguments,
                timestamp,
            } => Some(ConversationItem::FunctionCall {
                id: id.clone(),
                call_id: call_id.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
                timestamp: timestamp.clone(),
            }),
            Self::FunctionCallOutput {
                call_id,
                output,
                timestamp,
            } => Some(ConversationItem::FunctionCallOutput {
                call_id: call_id.clone(),
                output: output.clone(),
                timestamp: timestamp.clone(),
            }),
            Self::Reasoning {
                id,
                summary,
                encrypted_content,
                content,
                timestamp,
            } => Some(ConversationItem::Reasoning {
                id: id.clone(),
                summary: summary.clone(),
                encrypted_content: encrypted_content.clone(),
                content: content.clone(),
                timestamp: timestamp.clone(),
            }),
            Self::SessionMeta { .. }
            | Self::TaskStart { .. }
            | Self::PromptContext { .. }
            | Self::TaskComplete { .. } => None,
        }
    }

    /// Convert this record to a JSON value suitable for streaming output.
    #[allow(dead_code)]
    pub fn to_streaming_json(&self) -> serde_json::Value {
        // We serialize the whole record; serde handles the tag.
        serde_json::to_value(self).unwrap_or_else(
            |_| serde_json::json!({"type": "error", "error": "serialization failed"}),
        )
    }
}

// =============================================================================
// API Request/Response DTOs (internal to clients)
// =============================================================================

#[derive(Clone, Serialize)]
pub(super) struct ProviderConfig {
    pub(super) only: Vec<String>,
}

#[derive(Clone, Serialize)]
pub(super) struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_tokens: Option<u32>,
}

#[derive(Serialize)]
pub(super) struct Request<'a> {
    pub(super) model: &'a str,
    pub(super) input: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) instructions: Option<&'a str>,
    pub(super) temperature: Option<f32>,
    pub(super) top_p: Option<f32>,
    pub(super) max_output_tokens: Option<u32>,
    pub(super) tools: Option<Vec<super::tools::Tool>>,
    pub(super) tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) provider: Option<ProviderConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reasoning: Option<ReasoningConfig>,
}

#[derive(Deserialize, Debug)]
pub(super) struct ApiResponse {
    #[expect(dead_code)]
    pub(super) id: Option<String>,
    pub(super) output: Vec<OutputMessage>,
    pub(super) usage: Option<ApiUsage>,
    #[expect(dead_code)]
    pub(super) status: Option<String>,
    #[expect(dead_code)]
    pub(super) error: Option<ApiError>,
}

#[derive(Deserialize, Debug, Clone)]
pub(super) struct OutputMessage {
    #[serde(rename = "type")]
    pub(super) msg_type: String,
    pub(super) id: Option<String>,
    pub(super) call_id: Option<String>,
    pub(super) name: Option<String>,
    pub(super) arguments: Option<String>,
    #[expect(dead_code)]
    pub(super) role: Option<String>,
    pub(super) status: Option<String>,
    pub(super) content: Option<Vec<OutputContent>>,
    /// Opaque encrypted reasoning content returned by reasoning models.
    pub(super) encrypted_content: Option<String>,
    /// Top-level summary strings on reasoning items (alternative to content-based summaries).
    pub(super) summary: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone)]
pub(super) struct OutputContent {
    #[serde(rename = "type")]
    pub(super) content_type: String,
    pub(super) text: Option<String>,
}

/// Usage statistics for API calls
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub input_tokens_details: InputTokensDetails,
    pub output_tokens: u64,
    pub output_tokens_details: OutputTokensDetails,
    pub total_tokens: u64,
}

/// Details about input tokens
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct InputTokensDetails {
    pub cached_tokens: u64,
}

/// Details about output tokens
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct OutputTokensDetails {
    pub reasoning_tokens: u64,
}

/// Internal usage struct for API response deserialization (with optional fields)
#[derive(Deserialize, Debug, Clone, Default)]
pub(super) struct ApiUsage {
    pub(super) input_tokens: Option<u64>,
    pub(super) input_tokens_details: Option<ApiInputTokensDetails>,
    pub(super) output_tokens: Option<u64>,
    pub(super) output_tokens_details: Option<ApiOutputTokensDetails>,
    pub(super) total_tokens: Option<u64>,
}

/// Internal input tokens details for API response deserialization
#[derive(Deserialize, Debug, Clone, Default)]
pub(super) struct ApiInputTokensDetails {
    pub(super) cached_tokens: Option<u64>,
}

/// Internal output tokens details for API response deserialization
#[derive(Deserialize, Debug, Clone, Default)]
pub(super) struct ApiOutputTokensDetails {
    pub(super) reasoning_tokens: Option<u64>,
}

#[expect(dead_code)]
#[derive(Deserialize, Debug)]
pub(super) struct ApiError {
    pub(super) code: Option<String>,
    pub(super) message: String,
    pub(super) metadata: Option<serde_json::Value>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn to_api_input_user_message() {
        let item = ConversationItem::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
            timestamp: None,
        };
        let json = item.to_api_input();
        assert_eq!(json["type"], "message");
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"][0]["type"], "input_text");
        assert_eq!(json["content"][0]["text"], "Hello");
    }

    #[test]
    fn to_api_input_assistant_message_uses_output_text() {
        let item = ConversationItem::Message {
            role: Role::Assistant,
            content: "Hi".to_string(),
            id: Some("msg-1".to_string()),
            status: Some("completed".to_string()),
            timestamp: None,
        };
        let json = item.to_api_input();
        assert_eq!(json["role"], "assistant");
        assert_eq!(json["content"][0]["type"], "output_text");
        assert_eq!(json["content"][0]["text"], "Hi");
        assert_eq!(json["id"], "msg-1");
        assert_eq!(json["status"], "completed");
    }

    #[test]
    fn to_api_input_system_message() {
        let item = ConversationItem::Message {
            role: Role::System,
            content: "You are helpful".to_string(),
            id: None,
            status: None,
            timestamp: None,
        };
        let json = item.to_api_input();
        assert_eq!(json["role"], "system");
        assert_eq!(json["content"][0]["type"], "input_text");
    }

    #[test]
    fn to_api_input_tool_message() {
        let item = ConversationItem::Message {
            role: Role::Tool,
            content: "tool result".to_string(),
            id: None,
            status: None,
            timestamp: None,
        };
        let json = item.to_api_input();
        assert_eq!(json["role"], "tool");
        assert_eq!(json["content"][0]["type"], "input_text");
    }

    #[test]
    fn to_api_input_function_call() {
        let item = ConversationItem::FunctionCall {
            id: "fc-1".to_string(),
            call_id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: r#"{"cmd":"ls"}"#.to_string(),
            timestamp: None,
        };
        let json = item.to_api_input();
        assert_eq!(json["type"], "function_call");
        assert_eq!(json["id"], "fc-1");
        assert_eq!(json["call_id"], "call-1");
        assert_eq!(json["name"], "bash");
        assert_eq!(json["arguments"], r#"{"cmd":"ls"}"#);
    }

    #[test]
    fn to_api_input_function_call_output() {
        let item = ConversationItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: "file.txt".to_string(),
            timestamp: None,
        };
        let json = item.to_api_input();
        assert_eq!(json["type"], "function_call_output");
        assert_eq!(json["call_id"], "call-1");
        assert_eq!(json["output"], "file.txt");
    }

    #[test]
    fn to_api_input_reasoning() {
        let item = ConversationItem::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["thinking...".to_string()],
            encrypted_content: None,
            content: None,
            timestamp: None,
        };
        let json = item.to_api_input();
        assert_eq!(json["type"], "reasoning");
        assert_eq!(json["id"], "r-1");
        assert_eq!(json["summary"][0]["type"], "summary_text");
        assert_eq!(json["summary"][0]["text"], "thinking...");
    }

    #[test]
    fn to_api_input_reasoning_multiple_summaries() {
        let item = ConversationItem::Reasoning {
            id: "r-2".to_string(),
            summary: vec!["step 1".to_string(), "step 2".to_string()],
            encrypted_content: None,
            content: None,
            timestamp: None,
        };
        let json = item.to_api_input();
        assert_eq!(json["summary"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn to_api_input_reasoning_with_encrypted_content() {
        let item = ConversationItem::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["thinking...".to_string()],
            encrypted_content: Some("gAAAAABencrypted...".to_string()),
            content: None,
            timestamp: None,
        };
        let json = item.to_api_input();
        assert_eq!(json["type"], "reasoning");
        assert_eq!(json["encrypted_content"], "gAAAAABencrypted...");
    }

    #[test]
    fn to_api_input_reasoning_without_encrypted_content_omits_field() {
        let item = ConversationItem::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["thinking...".to_string()],
            encrypted_content: None,
            content: None,
            timestamp: None,
        };
        let json = item.to_api_input();
        assert!(json.get("encrypted_content").is_none());
    }

    #[test]
    fn to_api_input_reasoning_with_content() {
        let item = ConversationItem::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["thinking...".to_string()],
            encrypted_content: None,
            timestamp: None,
            content: Some(vec![ReasoningContent {
                content_type: "reasoning_text".to_string(),
                text: Some("deep thoughts".to_string()),
            }]),
        };
        let json = item.to_api_input();
        assert_eq!(json["content"][0]["type"], "reasoning_text");
        assert_eq!(json["content"][0]["text"], "deep thoughts");
    }

    #[test]
    fn to_streaming_json_message() {
        let item = ConversationItem::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
            timestamp: None,
        };
        let json = item.to_streaming_json();
        assert_eq!(json["type"], "message");
        assert_eq!(json["content"], "Hello");
    }

    #[test]
    fn to_streaming_json_message_with_id_and_status() {
        let item = ConversationItem::Message {
            role: Role::Assistant,
            content: "Response".to_string(),
            id: Some("msg-123".to_string()),
            status: Some("completed".to_string()),
            timestamp: None,
        };
        let json = item.to_streaming_json();
        assert_eq!(json["id"], "msg-123");
        assert_eq!(json["status"], "completed");
    }

    #[test]
    fn to_streaming_json_reasoning_uses_plain_summary() {
        let item = ConversationItem::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["step 1".to_string()],
            encrypted_content: None,
            content: None,
            timestamp: None,
        };
        let json = item.to_streaming_json();
        assert_eq!(json["type"], "reasoning");
        // Streaming format uses plain strings, not objects
        assert_eq!(json["summary"][0], "step 1");
    }

    #[test]
    fn to_streaming_json_function_call() {
        let item = ConversationItem::FunctionCall {
            id: "fc-1".to_string(),
            call_id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: r#"{"cmd":"ls"}"#.to_string(),
            timestamp: None,
        };
        let json = item.to_streaming_json();
        assert_eq!(json["type"], "function_call");
        assert_eq!(json["name"], "bash");
    }

    #[test]
    fn to_streaming_json_function_call_output() {
        let item = ConversationItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: "result".to_string(),
            timestamp: None,
        };
        let json = item.to_streaming_json();
        assert_eq!(json["type"], "function_call_output");
        assert_eq!(json["output"], "result");
    }

    #[test]
    fn conversation_item_serialization_roundtrip() {
        let items = vec![
            ConversationItem::Message {
                role: Role::User,
                content: "test".to_string(),
                id: None,
                status: None,
                timestamp: None,
            },
            ConversationItem::FunctionCall {
                id: "fc".to_string(),
                call_id: "call".to_string(),
                name: "tool".to_string(),
                arguments: "{}".to_string(),
                timestamp: None,
            },
            ConversationItem::FunctionCallOutput {
                call_id: "call".to_string(),
                output: "out".to_string(),
                timestamp: None,
            },
            ConversationItem::Reasoning {
                id: "r".to_string(),
                summary: vec!["s".to_string()],
                encrypted_content: None,
                content: None,
                timestamp: None,
            },
        ];

        for item in items {
            let json = serde_json::to_string(&item).unwrap();
            let deserialized: ConversationItem = serde_json::from_str(&json).unwrap();
            assert_eq!(json, serde_json::to_string(&deserialized).unwrap());
        }
    }

    #[test]
    fn usage_default_values() {
        let usage = Usage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
        assert_eq!(usage.input_tokens_details.cached_tokens, 0);
        assert_eq!(usage.output_tokens_details.reasoning_tokens, 0);
    }

    #[test]
    fn usage_serialization() {
        let usage = Usage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            input_tokens_details: InputTokensDetails { cached_tokens: 20 },
            output_tokens_details: OutputTokensDetails {
                reasoning_tokens: 10,
            },
        };
        let json = serde_json::to_string(&usage).unwrap();
        assert!(json.contains("\"input_tokens\":100"));
        assert!(json.contains("\"output_tokens\":50"));
        assert!(json.contains("\"total_tokens\":150"));
    }

    #[test]
    fn provider_config_serialization() {
        let config = ProviderConfig {
            only: vec!["Fireworks".to_string(), "Moonshot AI".to_string()],
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"only\":["));
        assert!(json.contains("\"Fireworks\""));
        assert!(json.contains("\"Moonshot AI\""));
    }

    #[test]
    fn provider_config_single_provider() {
        let config = ProviderConfig {
            only: vec!["OpenAI".to_string()],
        };
        let json = serde_json::to_string(&config).unwrap();
        let expected = r#"{"only":["OpenAI"]}"#;
        assert_eq!(json, expected);
    }

    #[test]
    fn prompt_context_records_are_audit_only() {
        let record = SessionRecord::PromptContext {
            session_id: "session-1".to_string(),
            task_id: "task-1".to_string(),
            role: Role::Developer,
            content: "mutable context".to_string(),
            timestamp: DateTime::parse_from_rfc3339("2026-05-03T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["type"], "prompt_context");
        assert_eq!(json["role"], "developer");
        assert_eq!(json["content"], "mutable context");
        assert!(record.to_conversation_item().is_none());
    }

    #[test]
    fn snapshot_user_message() {
        let item = ConversationItem::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
            timestamp: None,
        };
        insta::assert_json_snapshot!("to_api_input_user_message", item.to_api_input());
    }

    #[test]
    fn snapshot_assistant_message_with_id_and_status() {
        let item = ConversationItem::Message {
            role: Role::Assistant,
            content: "Hi there".to_string(),
            id: Some("msg-1".to_string()),
            status: Some("completed".to_string()),
            timestamp: None,
        };
        insta::assert_json_snapshot!(
            "to_api_input_assistant_message_with_id_and_status",
            item.to_api_input()
        );
    }

    #[test]
    fn snapshot_system_message() {
        let item = ConversationItem::Message {
            role: Role::System,
            content: "You are cake".to_string(),
            id: None,
            status: None,
            timestamp: None,
        };
        insta::assert_json_snapshot!("to_api_input_system_message", item.to_api_input());
    }

    #[test]
    fn snapshot_function_call() {
        let item = ConversationItem::FunctionCall {
            id: "fc-1".to_string(),
            call_id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: r#"{"cmd":"ls"}"#.to_string(),
            timestamp: None,
        };
        insta::assert_json_snapshot!("to_api_input_function_call", item.to_api_input());
    }

    #[test]
    fn snapshot_function_call_output() {
        let item = ConversationItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: "file.txt\nother.txt".to_string(),
            timestamp: None,
        };
        insta::assert_json_snapshot!("to_api_input_function_call_output", item.to_api_input());
    }

    #[test]
    fn snapshot_reasoning_with_summary() {
        let item = ConversationItem::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["thinking...".to_string()],
            encrypted_content: None,
            content: None,
            timestamp: None,
        };
        insta::assert_json_snapshot!("to_api_input_reasoning_with_summary", item.to_api_input());
    }

    #[test]
    fn snapshot_reasoning_with_encrypted_content() {
        let item = ConversationItem::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["thinking...".to_string()],
            encrypted_content: Some("gAAAAABencrypted...".to_string()),
            content: None,
            timestamp: None,
        };
        insta::assert_json_snapshot!(
            "to_api_input_reasoning_with_encrypted_content",
            item.to_api_input()
        );
    }

    #[test]
    fn snapshot_reasoning_with_content_array() {
        let item = ConversationItem::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["thinking...".to_string()],
            encrypted_content: None,
            content: Some(vec![ReasoningContent {
                content_type: "reasoning_text".to_string(),
                text: Some("deep analysis".to_string()),
            }]),
            timestamp: None,
        };
        insta::assert_json_snapshot!(
            "to_api_input_reasoning_with_content_array",
            item.to_api_input()
        );
    }

    #[test]
    fn snapshot_to_streaming_json_message_with_id_and_status() {
        let item = ConversationItem::Message {
            role: Role::Assistant,
            content: "Response".to_string(),
            id: Some("msg-123".to_string()),
            status: Some("completed".to_string()),
            timestamp: None,
        };
        insta::assert_json_snapshot!(
            "to_streaming_json_message_with_id_and_status",
            item.to_streaming_json()
        );
    }

    #[test]
    fn snapshot_to_streaming_json_reasoning_plain_summary() {
        let item = ConversationItem::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["step 1".to_string(), "step 2".to_string()],
            encrypted_content: None,
            content: None,
            timestamp: None,
        };
        insta::assert_json_snapshot!(
            "to_streaming_json_reasoning_plain_summary",
            item.to_streaming_json()
        );
    }

    #[test]
    fn snapshot_to_streaming_json_function_call() {
        let item = ConversationItem::FunctionCall {
            id: "fc-1".to_string(),
            call_id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: r#"{"cmd":"ls"}"#.to_string(),
            timestamp: None,
        };
        insta::assert_json_snapshot!("to_streaming_json_function_call", item.to_streaming_json());
    }

    #[test]
    fn snapshot_to_streaming_json_function_call_output() {
        let item = ConversationItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: "result".to_string(),
            timestamp: None,
        };
        insta::assert_json_snapshot!(
            "to_streaming_json_function_call_output",
            item.to_streaming_json()
        );
    }
}
