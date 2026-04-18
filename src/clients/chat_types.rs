use serde::{Deserialize, Serialize};
use std::borrow::Cow;

// =============================================================================
// Chat Completions API Request DTOs (serialization only - can borrow)
// =============================================================================

#[derive(Serialize)]
pub(super) struct ChatRequest<'a> {
    pub(super) model: &'a str,
    pub(super) messages: Vec<ChatMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tools: Option<Vec<ChatTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reasoning_effort: Option<String>,
}

/// Request message type that borrows strings from history to avoid cloning.
#[derive(Serialize, Clone, Debug)]
pub(super) struct ChatMessage<'a> {
    pub(super) role: Cow<'a, str>,
    pub(super) content: Option<Cow<'a, str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_calls: Option<Vec<ChatToolCallRef<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_call_id: Option<Cow<'a, str>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(super) struct ChatTool {
    #[serde(rename = "type")]
    pub(super) type_: String,
    pub(super) function: ChatFunction,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(super) struct ChatFunction {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) parameters: serde_json::Value,
}

/// Borrowed tool call type for request serialization.
#[derive(Serialize, Clone, Debug)]
pub(super) struct ChatToolCallRef<'a> {
    pub(super) id: Cow<'a, str>,
    #[serde(rename = "type")]
    pub(super) type_: Cow<'a, str>,
    pub(super) function: ChatFunctionCallRef<'a>,
}

/// Borrowed function call type for request serialization.
#[derive(Serialize, Clone, Debug)]
pub(super) struct ChatFunctionCallRef<'a> {
    pub(super) name: Cow<'a, str>,
    pub(super) arguments: Cow<'a, str>,
}

// =============================================================================
// Chat Completions API Response DTOs (deserialization - owned types)
// =============================================================================

#[derive(Deserialize, Debug)]
pub(super) struct ChatResponse {
    pub(super) id: Option<String>,
    pub(super) choices: Vec<ChatChoice>,
    pub(super) usage: Option<ChatUsage>,
}

#[derive(Deserialize, Debug)]
pub(super) struct ChatChoice {
    #[expect(dead_code)]
    pub(super) index: u32,
    pub(super) message: ChatResponseMessage,
    #[expect(dead_code)]
    pub(super) finish_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
pub(super) struct ChatResponseMessage {
    #[expect(dead_code)]
    pub(super) role: Option<String>,
    pub(super) content: Option<String>,
    pub(super) tool_calls: Option<Vec<ChatToolCall>>,
}

/// Owned tool call type for response deserialization.
#[derive(Deserialize, Clone, Debug)]
pub(super) struct ChatToolCall {
    pub(super) id: String,
    #[serde(rename = "type")]
    #[expect(dead_code)]
    pub(super) type_: String,
    pub(super) function: ChatFunctionCall,
}

/// Owned function call type for response deserialization.
#[derive(Deserialize, Clone, Debug)]
pub(super) struct ChatFunctionCall {
    pub(super) name: String,
    pub(super) arguments: String,
}

#[derive(Deserialize, Debug, Default)]
pub(super) struct CompletionTokensDetails {
    pub(super) reasoning_tokens: Option<u64>,
}

#[derive(Deserialize, Debug, Default)]
pub(super) struct PromptTokensDetails {
    pub(super) cached_tokens: Option<u64>,
}

#[derive(Deserialize, Debug)]
#[allow(clippy::struct_field_names)]
pub(super) struct ChatUsage {
    pub(super) prompt_tokens: Option<u64>,
    pub(super) completion_tokens: Option<u64>,
    pub(super) total_tokens: Option<u64>,
    pub(super) prompt_tokens_details: Option<PromptTokensDetails>,
    pub(super) completion_tokens_details: Option<CompletionTokensDetails>,
}
