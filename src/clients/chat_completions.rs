use std::borrow::Cow;
use tracing::{debug, trace};

use crate::config::model::ResolvedModelConfig;
use crate::models::Role;

use crate::clients::agent::TurnResult;
use crate::clients::chat_types::{
    ChatFunction, ChatFunctionCallRef, ChatMessage, ChatRequest, ChatResponse, ChatTool,
    ChatToolCallRef,
};
use crate::clients::tools::Tool;
use crate::clients::types::{ConversationItem, InputTokensDetails, OutputTokensDetails, Usage};

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
        reasoning_effort: config.config.reasoning_effort.clone(),
    };

    let url = format!(
        "{}/chat/completions",
        config.config.base_url.trim_end_matches('/')
    );
    debug!(target: "cake", "{url}");
    if tracing::enabled!(tracing::Level::TRACE) {
        let request_json = serde_json::to_string(&request)?;
        trace!(target: "cake", "{request_json}");
    }

    let response = client
        .post(&url)
        .json(&request)
        .header("HTTP-Referer", "https://github.com/travisennis/cake")
        .header("X-Title", "cake")
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
    trace!(target: "cake", "{chat_response:?}");

    let usage = chat_response.usage.as_ref().map(|u| Usage {
        input_tokens: u.prompt_tokens.unwrap_or(0),
        output_tokens: u.completion_tokens.unwrap_or(0),
        total_tokens: u.total_tokens.unwrap_or(0),
        input_tokens_details: InputTokensDetails {
            cached_tokens: u
                .prompt_tokens_details
                .as_ref()
                .and_then(|d| d.cached_tokens)
                .unwrap_or(0),
        },
        output_tokens_details: OutputTokensDetails {
            reasoning_tokens: u
                .completion_tokens_details
                .as_ref()
                .and_then(|d| d.reasoning_tokens)
                .unwrap_or(0),
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
/// - `Reasoning` → preserved as provider-specific `reasoning_content` on the
///   next assistant message for providers like Moonshot/Kimi
fn build_messages(history: &[ConversationItem]) -> Vec<ChatMessage<'_>> {
    let mut messages: Vec<ChatMessage<'_>> = Vec::new();
    let mut pending_tool_calls: Vec<ChatToolCallRef<'_>> = Vec::new();
    let mut pending_reasoning_content: Option<Cow<'_, str>> = None;

    for item in history {
        match item {
            ConversationItem::Message { role, content, .. } => {
                // Flush any pending tool calls as an assistant message
                flush_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning_content,
                );

                let role_str = match role {
                    Role::System => "system",
                    Role::Assistant => "assistant",
                    Role::User => "user",
                    Role::Tool => "tool",
                };

                messages.push(ChatMessage {
                    role: Cow::Borrowed(role_str),
                    content: Some(Cow::Borrowed(content)),
                    reasoning_content: matches!(role, Role::Assistant)
                        .then(|| pending_reasoning_content.take())
                        .flatten(),
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
                pending_tool_calls.push(ChatToolCallRef {
                    id: Cow::Borrowed(call_id),
                    type_: Cow::Borrowed("function"),
                    function: ChatFunctionCallRef {
                        name: Cow::Borrowed(name),
                        arguments: Cow::Borrowed(arguments),
                    },
                });
            },
            ConversationItem::FunctionCallOutput {
                call_id, output, ..
            } => {
                // Flush any pending tool calls first
                flush_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning_content,
                );

                messages.push(ChatMessage {
                    role: Cow::Borrowed("tool"),
                    content: Some(Cow::Borrowed(output)),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: Some(Cow::Borrowed(call_id)),
                });
            },
            ConversationItem::Reasoning { content, .. } => {
                pending_reasoning_content =
                    extract_reasoning_content(content.as_ref()).map(Cow::Borrowed);
            },
        }
    }

    // Flush any remaining tool calls
    flush_tool_calls(
        &mut messages,
        &mut pending_tool_calls,
        &mut pending_reasoning_content,
    );

    messages
}

fn extract_reasoning_content(
    content: Option<&Vec<super::types::ReasoningContent>>,
) -> Option<&str> {
    content.and_then(|items| items.iter().find_map(|item| item.text.as_deref()))
}

/// Flush accumulated tool calls into an assistant message.
fn flush_tool_calls<'a>(
    messages: &mut Vec<ChatMessage<'a>>,
    tool_calls: &mut Vec<ChatToolCallRef<'a>>,
    reasoning_content: &mut Option<Cow<'a, str>>,
) {
    if tool_calls.is_empty() {
        return;
    }

    messages.push(ChatMessage {
        role: Cow::Borrowed("assistant"),
        content: None,
        reasoning_content: reasoning_content.take(),
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
    let timestamp = chrono::Utc::now().to_rfc3339();

    if let Some(reasoning_content) = &message.reasoning_content {
        items.push(ConversationItem::Reasoning {
            id: response.id.clone().unwrap_or_default(),
            summary: vec!["Thinking...".to_string()],
            encrypted_content: None,
            content: Some(vec![super::types::ReasoningContent {
                content_type: "reasoning_text".to_string(),
                text: Some(reasoning_content.clone()),
            }]),
            timestamp: Some(timestamp.clone()),
        });
    }

    // Extract tool calls first
    if let Some(tool_calls) = &message.tool_calls {
        for tc in tool_calls {
            items.push(ConversationItem::FunctionCall {
                id: tc.id.clone(),
                call_id: tc.id.clone(),
                name: tc.function.name.clone(),
                arguments: tc.function.arguments.clone(),
                timestamp: Some(timestamp.clone()),
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
            timestamp: Some(timestamp.clone()),
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
            timestamp: Some(timestamp),
        });
    }

    items
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::clients::chat_types::{
        ChatChoice, ChatFunctionCall, ChatResponse, ChatResponseMessage, ChatToolCall, ChatUsage,
        PromptTokensDetails,
    };
    use crate::clients::types::ReasoningContent;

    #[test]
    fn build_messages_simple_conversation() {
        let history = vec![
            ConversationItem::Message {
                role: Role::System,
                content: "You are helpful.".to_string(),
                id: None,
                status: None,
                timestamp: None,
            },
            ConversationItem::Message {
                role: Role::User,
                content: "Hello".to_string(),
                id: None,
                status: None,
                timestamp: None,
            },
        ];
        let msgs = build_messages(&history);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "system");
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
                timestamp: None,
            },
            ConversationItem::FunctionCall {
                id: "fc-1".to_string(),
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: r#"{"cmd":"ls"}"#.to_string(),
                timestamp: None,
            },
            ConversationItem::FunctionCall {
                id: "fc-2".to_string(),
                call_id: "call-2".to_string(),
                name: "read".to_string(),
                arguments: r#"{"path":"foo.txt"}"#.to_string(),
                timestamp: None,
            },
            ConversationItem::FunctionCallOutput {
                call_id: "call-1".to_string(),
                output: "file.txt".to_string(),
                timestamp: None,
            },
            ConversationItem::FunctionCallOutput {
                call_id: "call-2".to_string(),
                output: "contents".to_string(),
                timestamp: None,
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
        assert!(msgs[1].reasoning_content.is_none());
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
    fn build_messages_preserves_reasoning_content_for_assistant_messages() {
        let history = vec![
            ConversationItem::Message {
                role: Role::User,
                content: "think".to_string(),
                id: None,
                status: None,
                timestamp: None,
            },
            ConversationItem::Reasoning {
                id: "r-1".to_string(),
                summary: vec!["thinking...".to_string()],
                encrypted_content: None,
                content: Some(vec![ReasoningContent {
                    content_type: "reasoning_text".to_string(),
                    text: Some("internal reasoning".to_string()),
                }]),
                timestamp: None,
            },
            ConversationItem::Message {
                role: Role::Assistant,
                content: "done".to_string(),
                id: None,
                status: None,
                timestamp: None,
            },
        ];
        let msgs = build_messages(&history);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(
            msgs[1].reasoning_content.as_deref(),
            Some("internal reasoning")
        );
    }

    #[test]
    fn build_messages_preserves_reasoning_content_for_assistant_tool_calls() {
        let history = vec![
            ConversationItem::Message {
                role: Role::User,
                content: "do stuff".to_string(),
                id: None,
                status: None,
                timestamp: None,
            },
            ConversationItem::Reasoning {
                id: "r-1".to_string(),
                summary: vec!["thinking...".to_string()],
                encrypted_content: None,
                content: Some(vec![ReasoningContent {
                    content_type: "reasoning_text".to_string(),
                    text: Some("preserved reasoning".to_string()),
                }]),
                timestamp: None,
            },
            ConversationItem::FunctionCall {
                id: "fc-1".to_string(),
                call_id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: r#"{"cmd":"ls"}"#.to_string(),
                timestamp: None,
            },
        ];

        let msgs = build_messages(&history);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(
            msgs[1].reasoning_content.as_deref(),
            Some("preserved reasoning")
        );
        assert!(msgs[1].tool_calls.is_some());
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
                    reasoning_content: None,
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
                    reasoning_content: None,
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
    fn parse_choices_preserves_reasoning_content_for_tool_calls() {
        let response = ChatResponse {
            id: Some("chatcmpl-456".to_string()),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".to_string()),
                    content: None,
                    reasoning_content: Some("preserved reasoning".to_string()),
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
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], ConversationItem::Reasoning {
            content: Some(content), ..
        } if content[0].text.as_deref() == Some("preserved reasoning")));
        assert!(matches!(&items[1], ConversationItem::FunctionCall {
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
                    reasoning_content: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(ChatUsage {
                prompt_tokens: Some(100),
                completion_tokens: Some(50),
                total_tokens: Some(150),
                prompt_tokens_details: None,
                completion_tokens_details: None,
            }),
        };
        // parse_choices doesn't handle usage — the caller does
        let items = parse_choices(&response);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn parse_response_extracts_cached_tokens() {
        let usage = ChatUsage {
            prompt_tokens: Some(200),
            completion_tokens: Some(80),
            total_tokens: Some(280),
            prompt_tokens_details: Some(PromptTokensDetails {
                cached_tokens: Some(150),
            }),
            completion_tokens_details: None,
        };
        let mapped = Usage {
            input_tokens: usage.prompt_tokens.unwrap_or(0),
            output_tokens: usage.completion_tokens.unwrap_or(0),
            total_tokens: usage.total_tokens.unwrap_or(0),
            input_tokens_details: InputTokensDetails {
                cached_tokens: usage
                    .prompt_tokens_details
                    .as_ref()
                    .and_then(|d| d.cached_tokens)
                    .unwrap_or(0),
            },
            output_tokens_details: OutputTokensDetails {
                reasoning_tokens: usage
                    .completion_tokens_details
                    .as_ref()
                    .and_then(|d| d.reasoning_tokens)
                    .unwrap_or(0),
            },
        };
        assert_eq!(mapped.input_tokens_details.cached_tokens, 150);
    }

    #[test]
    fn parse_response_defaults_cached_tokens_when_missing() {
        let usage = ChatUsage {
            prompt_tokens: Some(100),
            completion_tokens: Some(50),
            total_tokens: Some(150),
            prompt_tokens_details: None,
            completion_tokens_details: None,
        };
        let cached = usage
            .prompt_tokens_details
            .as_ref()
            .and_then(|d| d.cached_tokens)
            .unwrap_or(0);
        assert_eq!(cached, 0);
    }

    #[test]
    fn build_messages_empty_history() {
        let history: Vec<ConversationItem> = vec![];
        let msgs = build_messages(&history);
        assert!(msgs.is_empty());
    }

    // =========================================================================
    // Malformed Response Tests
    // =========================================================================

    #[test]
    fn parse_choices_empty_message_content() {
        let response = ChatResponse {
            id: Some("chatcmpl-123".to_string()),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".to_string()),
                    content: Some(String::new()), // Empty content
                    reasoning_content: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: None,
        };
        let items = parse_choices(&response);
        // Empty content should not create a message item
        assert_eq!(items.len(), 1);
        // But it should create an empty assistant message
        assert!(matches!(&items[0], ConversationItem::Message {
            role: Role::Assistant,
            content,
            ..
        } if content.is_empty()));
    }

    #[test]
    fn parse_choices_none_content_creates_empty_message() {
        let response = ChatResponse {
            id: Some("chatcmpl-123".to_string()),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".to_string()),
                    content: None, // No content
                    reasoning_content: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: None,
        };
        let items = parse_choices(&response);
        // Should create an empty assistant message
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ConversationItem::Message {
            role: Role::Assistant,
            content,
            ..
        } if content.is_empty()));
    }

    #[test]
    fn parse_choices_multiple_tool_calls() {
        let response = ChatResponse {
            id: Some("chatcmpl-456".to_string()),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".to_string()),
                    content: None,
                    reasoning_content: None,
                    tool_calls: Some(vec![
                        ChatToolCall {
                            id: "call-1".to_string(),
                            type_: "function".to_string(),
                            function: ChatFunctionCall {
                                name: "bash".to_string(),
                                arguments: r#"{"cmd":"ls"}"#.to_string(),
                            },
                        },
                        ChatToolCall {
                            id: "call-2".to_string(),
                            type_: "function".to_string(),
                            function: ChatFunctionCall {
                                name: "read".to_string(),
                                arguments: r#"{"path":"file.txt"}"#.to_string(),
                            },
                        },
                    ]),
                },
                finish_reason: Some("tool_calls".to_string()),
            }],
            usage: None,
        };
        let items = parse_choices(&response);
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], ConversationItem::FunctionCall {
            name, ..
        } if name == "bash"));
        assert!(matches!(&items[1], ConversationItem::FunctionCall {
            name, ..
        } if name == "read"));
    }

    #[test]
    fn parse_choices_tool_calls_with_text_content() {
        // Some models return both tool calls and text content
        let response = ChatResponse {
            id: Some("chatcmpl-789".to_string()),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".to_string()),
                    content: Some("Let me help you with that.".to_string()),
                    reasoning_content: None,
                    tool_calls: Some(vec![ChatToolCall {
                        id: "call-1".to_string(),
                        type_: "function".to_string(),
                        function: ChatFunctionCall {
                            name: "bash".to_string(),
                            arguments: "{}".to_string(),
                        },
                    }]),
                },
                finish_reason: Some("tool_calls".to_string()),
            }],
            usage: None,
        };
        let items = parse_choices(&response);
        // Should have both tool call and message
        assert_eq!(items.len(), 2);
        // Tool call comes first
        assert!(matches!(&items[0], ConversationItem::FunctionCall { .. }));
        // Then the message
        assert!(matches!(&items[1], ConversationItem::Message {
            content,
            ..
        } if content == "Let me help you with that."));
    }

    #[test]
    fn parse_choices_missing_id_defaults_to_none() {
        let response = ChatResponse {
            id: None, // Missing id
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: Some("assistant".to_string()),
                    content: Some("Hello".to_string()),
                    reasoning_content: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: None,
        };
        let items = parse_choices(&response);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ConversationItem::Message {
            id,
            ..
        } if id.is_none()));
    }

    #[test]
    fn parse_choices_missing_role_defaults_to_none() {
        let response = ChatResponse {
            id: Some("chatcmpl-123".to_string()),
            choices: vec![ChatChoice {
                index: 0,
                message: ChatResponseMessage {
                    role: None, // Missing role
                    content: Some("Hello".to_string()),
                    reasoning_content: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: None,
        };
        let items = parse_choices(&response);
        // Should still create a message item
        assert_eq!(items.len(), 1);
    }
}

/// Tests for parsing raw HTTP responses
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod response_parsing_tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Create a minimal valid Chat Completions response
    fn minimal_valid_response() -> serde_json::Value {
        serde_json::json!({
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop"
            }]
        })
    }

    #[tokio::test]
    async fn parse_response_valid_json() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_valid_response()))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/chat/completions", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        assert!(result.is_ok());
        let turn_result = result.unwrap();
        assert_eq!(turn_result.items.len(), 1);
    }

    #[tokio::test]
    async fn parse_response_invalid_json() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json{broken"))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/chat/completions", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn parse_response_empty_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(""))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/chat/completions", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn parse_response_missing_choices_fails() {
        let mock_server = MockServer::start().await;

        // Response without "choices" field - should fail because choices is required
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-123",
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 5
                }
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/chat/completions", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        // Should fail because "choices" is a required field
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn parse_response_with_usage() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-123",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Hello!"
                    },
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 100,
                    "completion_tokens": 50,
                    "total_tokens": 150
                }
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/chat/completions", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        assert!(result.is_ok());
        let turn_result = result.unwrap();
        assert!(turn_result.usage.is_some());
        let usage = turn_result.usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[tokio::test]
    async fn parse_response_partial_usage() {
        let mock_server = MockServer::start().await;

        // Response with partial usage (some fields missing)
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-123",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Hello!"
                    },
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 100,
                    "completion_tokens": 50
                    // total_tokens is missing
                }
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/chat/completions", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        assert!(result.is_ok());
        let turn_result = result.unwrap();
        let usage = turn_result.usage.unwrap();
        // Should use defaults for missing fields
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.total_tokens, 0); // Default
    }

    #[tokio::test]
    async fn parse_response_with_tool_calls() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-123",
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "reasoning_content": "preserved reasoning",
                        "tool_calls": [{
                            "id": "call-abc",
                            "type": "function",
                            "function": {
                                "name": "bash",
                                "arguments": "{\"cmd\":\"ls\"}"
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }]
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/chat/completions", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        assert!(result.is_ok());
        let turn_result = result.unwrap();
        assert_eq!(turn_result.items.len(), 2);
        assert!(
            matches!(&turn_result.items[0], ConversationItem::Reasoning {
            content: Some(content),
            ..
        } if content[0].text.as_deref() == Some("preserved reasoning"))
        );
        assert!(
            matches!(&turn_result.items[1], ConversationItem::FunctionCall {
            name,
            call_id,
            ..
        } if name == "bash" && call_id == "call-abc")
        );
    }
}
