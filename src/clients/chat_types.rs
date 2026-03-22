use serde::{Deserialize, Serialize};

// =============================================================================
// Chat Completions API Request DTOs
// =============================================================================

#[derive(Serialize)]
pub(super) struct ChatRequest<'a> {
    pub(super) model: &'a str,
    pub(super) messages: Vec<ChatMessage>,
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
}

#[derive(Serialize, Clone, Debug)]
pub(super) struct ChatMessage {
    pub(super) role: String,
    pub(super) content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_call_id: Option<String>,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(super) struct ChatToolCall {
    pub(super) id: String,
    #[serde(rename = "type")]
    pub(super) type_: String,
    pub(super) function: ChatFunctionCall,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(super) struct ChatFunctionCall {
    pub(super) name: String,
    pub(super) arguments: String,
}

// =============================================================================
// Chat Completions API Response DTOs
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

#[derive(Deserialize, Debug)]
#[allow(clippy::struct_field_names)]
pub(super) struct ChatUsage {
    pub(super) prompt_tokens: Option<u64>,
    pub(super) completion_tokens: Option<u64>,
    pub(super) total_tokens: Option<u64>,
}
