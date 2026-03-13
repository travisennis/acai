use serde::{Deserialize, Serialize};

use crate::models::Role;

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
    },
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
    FunctionCallOutput {
        call_id: String,
        output: String,
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
            } => {
                serde_json::json!({
                    "type": "function_call",
                    "id": id,
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments
                })
            },
            Self::FunctionCallOutput { call_id, output } => {
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
    pub fn to_streaming_json(&self) -> serde_json::Value {
        match self {
            Self::Message {
                role,
                content,
                id,
                status,
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
            } => {
                serde_json::json!({
                    "type": "function_call",
                    "id": id,
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments
                })
            },
            Self::FunctionCallOutput { call_id, output } => {
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
// API Request/Response DTOs (internal to clients)
// =============================================================================

#[derive(Clone, Serialize)]
pub(super) struct ProviderConfig {
    pub(super) only: Vec<String>,
}

#[derive(Serialize)]
pub(super) struct Request<'a> {
    pub(super) model: &'a str,
    pub(super) input: Vec<serde_json::Value>,
    pub(super) temperature: Option<f32>,
    pub(super) top_p: Option<f32>,
    pub(super) max_output_tokens: Option<u32>,
    pub(super) tools: Option<Vec<super::tools::Tool>>,
    pub(super) tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) provider: Option<ProviderConfig>,
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
    pub input_tokens: u32,
    pub input_tokens_details: InputTokensDetails,
    pub output_tokens: u32,
    pub output_tokens_details: OutputTokensDetails,
    pub total_tokens: u32,
}

/// Details about input tokens
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct InputTokensDetails {
    pub cached_tokens: u32,
}

/// Details about output tokens
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct OutputTokensDetails {
    pub reasoning_tokens: u32,
}

/// Internal usage struct for API response deserialization (with optional fields)
#[derive(Deserialize, Debug, Clone, Default)]
pub(super) struct ApiUsage {
    pub(super) input_tokens: Option<u32>,
    pub(super) input_tokens_details: Option<ApiInputTokensDetails>,
    pub(super) output_tokens: Option<u32>,
    pub(super) output_tokens_details: Option<ApiOutputTokensDetails>,
    pub(super) total_tokens: Option<u32>,
}

/// Internal input tokens details for API response deserialization
#[derive(Deserialize, Debug, Clone, Default)]
pub(super) struct ApiInputTokensDetails {
    pub(super) cached_tokens: Option<u32>,
}

/// Internal output tokens details for API response deserialization
#[derive(Deserialize, Debug, Clone, Default)]
pub(super) struct ApiOutputTokensDetails {
    pub(super) reasoning_tokens: Option<u32>,
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
            },
            ConversationItem::FunctionCall {
                id: "fc".to_string(),
                call_id: "call".to_string(),
                name: "tool".to_string(),
                arguments: "{}".to_string(),
            },
            ConversationItem::FunctionCallOutput {
                call_id: "call".to_string(),
                output: "out".to_string(),
            },
            ConversationItem::Reasoning {
                id: "r".to_string(),
                summary: vec!["s".to_string()],
                encrypted_content: None,
                content: None,
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
}
