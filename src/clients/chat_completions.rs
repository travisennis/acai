use log::debug;

use crate::config::model::ResolvedModelConfig;
use crate::models::Role;

use super::agent::TurnResult;
use super::chat_types::{
    ChatFunction, ChatFunctionCall, ChatMessage, ChatRequest, ChatResponse, ChatTool, ChatToolCall,
};
use super::tools::Tool;
use super::types::{ConversationItem, InputTokensDetails, OutputTokensDetails, Usage};

// =============================================================================
// Chat Completions API Backend
// =============================================================================

/// Send a request to the Chat Completions API, returning the raw HTTP response.
///
/// # Errors
///
/// Returns an error if the HTTP request fails.
pub(super) async fn send_request(
    client: &reqwest::Client,
    config: &ResolvedModelConfig,
    history: &[ConversationItem],
    tools: &[Tool],
) -> anyhow::Result<reqwest::Response> {
    let messages = build_messages(history);
    let chat_tools = convert_tools(tools);

    let request = ChatRequest {
        model: &config.config.model,
        messages,
        temperature: config.config.temperature,
        top_p: config.config.top_p,
        max_completion_tokens: config.config.max_output_tokens,
        tools: if chat_tools.is_empty() {
            None
        } else {
            Some(chat_tools)
        },
        tool_choice: if tools.is_empty() {
            None
        } else {
            Some("auto".to_string())
        },
    };

    let url = format!(
        "{}/chat/completions",
        config.config.base_url.trim_end_matches('/')
    );
    debug!(target: "acai", "{url}");
    let request_json = serde_json::to_string(&request)?;
    debug!(target: "acai", "{request_json}");

    let response = client
        .post(&url)
        .json(&request)
        .header("content-type", "application/json")
        .header("HTTP-Referer", "https://github.com/travisennis/acai")
        .header("X-Title", "acai")
        .bearer_auth(&config.api_key)
        .send()
        .await?;

    Ok(response)
}

/// Parse an HTTP response from the Chat Completions API into a `TurnResult`.
///
/// # Errors
///
/// Returns an error if the response body cannot be deserialized.
pub(super) async fn parse_response(response: reqwest::Response) -> anyhow::Result<TurnResult> {
    let chat_response = response.json::<ChatResponse>().await?;
    debug!(target: "acai", "{chat_response:?}");

    #[allow(clippy::cast_possible_truncation)]
    let usage = chat_response.usage.as_ref().map(|u| Usage {
        input_tokens: u.prompt_tokens.unwrap_or(0) as u32,
        output_tokens: u.completion_tokens.unwrap_or(0) as u32,
        total_tokens: u.total_tokens.unwrap_or(0) as u32,
        input_tokens_details: InputTokensDetails { cached_tokens: 0 },
        output_tokens_details: OutputTokensDetails {
            reasoning_tokens: 0,
        },
    });

    let items = parse_choices(&chat_response);

    Ok(TurnResult { items, usage })
}

/// Convert internal conversation history to Chat Completions messages.
///
/// This handles the key translation:
/// - `ConversationItem::Message` → `ChatMessage` with role/content
/// - Consecutive `FunctionCall` items → one assistant message with `tool_calls`
/// - `FunctionCallOutput` → tool role message with `tool_call_id`
/// - `Reasoning` → skipped (not supported by Chat Completions API)
fn build_messages(history: &[ConversationItem]) -> Vec<ChatMessage> {
    let mut messages: Vec<ChatMessage> = Vec::new();
    let mut pending_tool_calls: Vec<ChatToolCall> = Vec::new();

    for item in history {
        match item {
            ConversationItem::Message { role, content, .. } => {
                // Flush any pending tool calls as an assistant message
                flush_tool_calls(&mut messages, &mut pending_tool_calls);

                let role_str = match role {
                    Role::System => "developer",
                    Role::Assistant => "assistant",
                    Role::User => "user",
                    Role::Tool => "tool",
                };

                messages.push(ChatMessage {
                    role: role_str.to_string(),
                    content: Some(content.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                });
            },
            ConversationItem::FunctionCall {
                call_id,
                name,
                arguments,
                ..
            } => {
                pending_tool_calls.push(ChatToolCall {
                    id: call_id.clone(),
                    type_: "function".to_string(),
                    function: ChatFunctionCall {
                        name: name.clone(),
                        arguments: arguments.clone(),
                    },
                });
            },
            ConversationItem::FunctionCallOutput { call_id, output } => {
                // Flush any pending tool calls first
                flush_tool_calls(&mut messages, &mut pending_tool_calls);

                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: Some(output.clone()),
                    tool_calls: None,
                    tool_call_id: Some(call_id.clone()),
                });
            },
            ConversationItem::Reasoning { .. } => {
                // Reasoning is not supported by Chat Completions API — skip
            },
        }
    }

    // Flush any remaining tool calls
    flush_tool_calls(&mut messages, &mut pending_tool_calls);

    messages
}

/// Flush accumulated tool calls into an assistant message.
fn flush_tool_calls(messages: &mut Vec<ChatMessage>, tool_calls: &mut Vec<ChatToolCall>) {
    if tool_calls.is_empty() {
        return;
    }

    messages.push(ChatMessage {
        role: "assistant".to_string(),
        content: None,
        tool_calls: Some(std::mem::take(tool_calls)),
        tool_call_id: None,
    });
}

/// Convert internal tool definitions to Chat Completions format.
fn convert_tools(tools: &[Tool]) -> Vec<ChatTool> {
    tools
        .iter()
        .map(|tool| ChatTool {
            type_: "function".to_string(),
            function: ChatFunction {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.parameters.clone(),
            },
        })
        .collect()
}

/// Parse the choices from a Chat Completions response into `ConversationItem` values.
fn parse_choices(response: &ChatResponse) -> Vec<ConversationItem> {
    let mut items = Vec::new();

    let Some(choice) = response.choices.first() else {
        return items;
    };

    let message = &choice.message;

    // Extract tool calls first
    if let Some(tool_calls) = &message.tool_calls {
        for tc in tool_calls {
            items.push(ConversationItem::FunctionCall {
                id: tc.id.clone(),
                call_id: tc.id.clone(),
                name: tc.function.name.clone(),
                arguments: tc.function.arguments.clone(),
            });
        }
    }

    // Extract text content (may coexist with tool calls)
    if let Some(content) = &message.content
        && !content.is_empty()
    {
        items.push(ConversationItem::Message {
            role: Role::Assistant,
            content: content.clone(),
            id: response.id.clone(),
            status: Some("completed".to_string()),
        });
    }

    // If we got tool calls but no text content, that's fine — the agent loop
    // will execute the tools and continue. But if we got neither, add an
    // empty assistant message so the caller knows the model responded.
    if items.is_empty() {
        items.push(ConversationItem::Message {
            role: Role::Assistant,
            content: String::new(),
            id: response.id.clone(),
            status: Some("completed".to_string()),
        });
    }

    items
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::super::chat_types::{ChatChoice, ChatResponse, ChatResponseMessage, ChatUsage};
    use super::*;

    #[test]
    fn build_messages_simple_conversation() {
        let history = vec![
            ConversationItem::Message {
                role: Role::System,
                content: "You are helpful.".to_string(),
                id: None,
                status: None,
            },
            ConversationItem::Message {
                role: Role::User,
                content: "Hello".to_string(),
                id: None,
                status: None,
            },
        ];
        let msgs = build_messages(&history);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "developer");
        assert_eq!(msgs[0].content.as_deref(), Some("You are helpful."));
        assert_eq!(msgs[1].role, "user");
        assert_eq!(msgs[1].content.as_deref(), Some("Hello"));
    }

    #[test]
    fn build_messages_groups_consecutive_function_calls() {
        let history = vec![
            ConversationItem::Message {
                role: Role::User,
                content: "do stuff".to_string(),
                id: None,
                status: None,
            },
            ConversationItem::FunctionCall {
                id: "fc-1".to_string(),
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: r#"{"cmd":"ls"}"#.to_string(),
            },
            ConversationItem::FunctionCall {
                id: "fc-2".to_string(),
                call_id: "call-2".to_string(),
                name: "read".to_string(),
                arguments: r#"{"path":"foo.txt"}"#.to_string(),
            },
            ConversationItem::FunctionCallOutput {
                call_id: "call-1".to_string(),
                output: "file.txt".to_string(),
            },
            ConversationItem::FunctionCallOutput {
                call_id: "call-2".to_string(),
                output: "contents".to_string(),
            },
        ];
        let msgs = build_messages(&history);
        // user + assistant(with 2 tool_calls) + tool + tool = 4 messages
        assert_eq!(msgs.len(), 4);

        // First: user message
        assert_eq!(msgs[0].role, "user");

        // Second: assistant with grouped tool_calls
        assert_eq!(msgs[1].role, "assistant");
        assert!(msgs[1].content.is_none());
        let tcs = msgs[1].tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 2);
        assert_eq!(tcs[0].function.name, "bash");
        assert_eq!(tcs[1].function.name, "read");

        // Third and fourth: tool results
        assert_eq!(msgs[2].role, "tool");
        assert_eq!(msgs[2].tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(msgs[3].role, "tool");
        assert_eq!(msgs[3].tool_call_id.as_deref(), Some("call-2"));
    }

    #[test]
    fn build_messages_skips_reasoning() {
        let history = vec![
            ConversationItem::Message {
                role: Role::User,
                content: "think".to_string(),
                id: None,
                status: None,
            },
            ConversationItem::Reasoning {
                id: "r-1".to_string(),
                summary: vec!["thinking...".to_string()],
                encrypted_content: None,
                content: None,
            },
            ConversationItem::Message {
                role: Role::Assistant,
                content: "done".to_string(),
                id: None,
                status: None,
            },
        ];
        let msgs = build_messages(&history);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "assistant");
    }

    #[test]
    fn convert_tools_wraps_under_function() {
        let tools = vec![Tool {
            type_: "function".to_string(),
            name: "bash".to_string(),
            description: "Run a command".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }];
        let chat_tools = convert_tools(&tools);
        assert_eq!(chat_tools.len(), 1);
        assert_eq!(chat_tools[0].type_, "function");
        assert_eq!(chat_tools[0].function.name, "bash");
        assert_eq!(chat_tools[0].function.description, "Run a command");
    }

    #[test]
    fn parse_choices_text_response() {
        let response = ChatResponse {
            id: Some("chatcmpl-123".to_string()),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".to_string()),
                    content: Some("Hello!".to_string()),
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: None,
        };
        let items = parse_choices(&response);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ConversationItem::Message {
            role: Role::Assistant,
            content,
            ..
        } if content == "Hello!"));
    }

    #[test]
    fn parse_choices_tool_calls() {
        let response = ChatResponse {
            id: Some("chatcmpl-456".to_string()),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".to_string()),
                    content: None,
                    tool_calls: Some(vec![ChatToolCall {
                        id: "call-abc".to_string(),
                        type_: "function".to_string(),
                        function: ChatFunctionCall {
                            name: "bash".to_string(),
                            arguments: r#"{"cmd":"ls"}"#.to_string(),
                        },
                    }]),
                },
                finish_reason: Some("tool_calls".to_string()),
            }],
            usage: None,
        };
        let items = parse_choices(&response);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ConversationItem::FunctionCall {
            name, call_id, ..
        } if name == "bash" && call_id == "call-abc"));
    }

    #[test]
    fn parse_choices_empty_response() {
        let response = ChatResponse {
            id: None,
            choices: vec![],
            usage: None,
        };
        let items = parse_choices(&response);
        assert!(items.is_empty());
    }

    #[test]
    fn parse_choices_with_usage() {
        let response = ChatResponse {
            id: None,
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".to_string()),
                    content: Some("Hi".to_string()),
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(ChatUsage {
                prompt_tokens: Some(100),
                completion_tokens: Some(50),
                total_tokens: Some(150),
            }),
        };
        // parse_choices doesn't handle usage — the caller does
        let items = parse_choices(&response);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn build_messages_empty_history() {
        let history: Vec<ConversationItem> = vec![];
        let msgs = build_messages(&history);
        assert!(msgs.is_empty());
    }
}
