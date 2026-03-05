use serde::{Deserialize, Serialize};

use crate::models::Role;

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
                    "role": match role {
                        Role::System => "system",
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::Tool => "tool",
                    },
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
            Self::Reasoning { id, summary } => {
                serde_json::json!({
                    "type": "reasoning",
                    "id": id,
                    "summary": summary.iter().map(|s| {
                        serde_json::json!({"type": "summary_text", "text": s})
                    }).collect::<Vec<_>>()
                })
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
                let role_str = match role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };
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
            Self::Reasoning { id, summary } => {
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

#[derive(Serialize)]
pub(super) struct Request {
    pub(super) model: String,
    pub(super) input: Vec<serde_json::Value>,
    pub(super) temperature: Option<f32>,
    pub(super) top_p: Option<f32>,
    pub(super) max_output_tokens: Option<u32>,
    pub(super) tools: Option<Vec<super::tools::Tool>>,
    pub(super) tool_choice: Option<String>,
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
    #[serde(rename = "call_id")]
    pub(super) call_id: Option<String>,
    pub(super) name: Option<String>,
    pub(super) arguments: Option<String>,
    #[expect(dead_code)]
    pub(super) role: Option<String>,
    pub(super) status: Option<String>,
    pub(super) content: Option<Vec<OutputContent>>,
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
    #[serde(rename = "input_tokens")]
    pub input_tokens: u32,
    #[serde(rename = "input_tokens_details")]
    pub input_tokens_details: InputTokensDetails,
    #[serde(rename = "output_tokens")]
    pub output_tokens: u32,
    #[serde(rename = "output_tokens_details")]
    pub output_tokens_details: OutputTokensDetails,
    #[serde(rename = "total_tokens")]
    pub total_tokens: u32,
}

/// Details about input tokens
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct InputTokensDetails {
    #[serde(rename = "cached_tokens")]
    pub cached_tokens: u32,
}

/// Details about output tokens
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct OutputTokensDetails {
    #[serde(rename = "reasoning_tokens")]
    pub reasoning_tokens: u32,
}

/// Internal usage struct for API response deserialization (with optional fields)
#[derive(Deserialize, Debug, Clone, Default)]
pub(super) struct ApiUsage {
    #[serde(rename = "input_tokens")]
    pub(super) input_tokens: Option<u32>,
    #[serde(rename = "input_tokens_details")]
    pub(super) input_tokens_details: Option<ApiInputTokensDetails>,
    #[serde(rename = "output_tokens")]
    pub(super) output_tokens: Option<u32>,
    #[serde(rename = "output_tokens_details")]
    pub(super) output_tokens_details: Option<ApiOutputTokensDetails>,
    #[serde(rename = "total_tokens")]
    pub(super) total_tokens: Option<u32>,
}

/// Internal input tokens details for API response deserialization
#[derive(Deserialize, Debug, Clone, Default)]
pub(super) struct ApiInputTokensDetails {
    #[serde(rename = "cached_tokens")]
    pub(super) cached_tokens: Option<u32>,
}

/// Internal output tokens details for API response deserialization
#[derive(Deserialize, Debug, Clone, Default)]
pub(super) struct ApiOutputTokensDetails {
    #[serde(rename = "reasoning_tokens")]
    pub(super) reasoning_tokens: Option<u32>,
}

#[expect(dead_code)]
#[derive(Deserialize, Debug)]
pub(super) struct ApiError {
    pub(super) code: Option<String>,
    pub(super) message: String,
    pub(super) metadata: Option<serde_json::Value>,
}
