use std::borrow::Cow;
use std::env;
use std::time::Duration;

use log::debug;
use tokio::time::sleep;

use crate::models::{Message, Role};

use super::tools::{Tool, bash_tool, edit_tool, execute_tool, read_tool, write_tool};
use super::types::{ApiResponse, ApiUsage, ConversationItem, Request, Usage};
use crate::config::defaults::DEFAULT_PROVIDERS;

/// Callback type for streaming JSON output
type StreamingCallback = Box<dyn Fn(&str) + Send + Sync>;

const BASE_URL: &str = "https://openrouter.ai/api/v1/responses";

/// Maximum number of retries for transient API errors
const MAX_RETRIES: u32 = 3;
/// Initial delay in seconds for exponential backoff
const INITIAL_DELAY_SECS: u64 = 1;

/// Determines if an HTTP status code represents a transient error that should be retried
const fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 500 | 503)
}

// =============================================================================
// Responses Client
// =============================================================================

pub struct Responses {
    model: Cow<'static, str>,
    token: String,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_output_tokens: Option<u32>,
    providers: Vec<String>,
    /// Conversation history using typed items (Responses API format)
    history: Vec<ConversationItem>,
    tools: Vec<Tool>,
    /// Callback for streaming JSON output
    streaming_callback: Option<StreamingCallback>,
    /// Session ID for tracking
    pub session_id: String,
    /// Accumulated usage across all API calls
    pub total_usage: Usage,
    /// Number of API calls made
    pub turn_count: u32,
    /// Reusable HTTP client for connection pooling
    client: reqwest::Client,
}

impl Responses {
    pub fn new(model: impl Into<Cow<'static, str>>, system_prompt: &str) -> anyhow::Result<Self> {
        let token = env::var("OPENROUTER_API_KEY").map_err(|e| {
            anyhow::anyhow!("OPENROUTER_API_KEY environment variable is not set: {e}")
        })?;

        Ok(Self {
            model: model.into(),
            token,
            temperature: Some(0.8),
            top_p: None,
            max_output_tokens: Some(8000),
            providers: DEFAULT_PROVIDERS.iter().map(|s| (*s).to_string()).collect(),
            history: vec![ConversationItem::Message {
                role: Role::System,
                content: system_prompt.to_string(),
                id: None,
                status: None,
            }],
            tools: vec![bash_tool(), edit_tool(), read_tool(), write_tool()],
            streaming_callback: None,
            session_id: uuid::Uuid::new_v4().to_string(),
            total_usage: Usage::default(),
            turn_count: 0,
            client: reqwest::Client::new(),
        })
    }

    /// Get the model name.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Replace the auto-generated session ID with a restored session's ID.
    pub fn with_session_id(mut self, id: String) -> Self {
        self.session_id = id;
        self
    }

    /// Replace the conversation history with restored messages.
    pub fn with_history(mut self, messages: Vec<ConversationItem>) -> Self {
        self.history = messages;
        self
    }

    /// Get conversation history without the system message (for saving to session files).
    /// Returns all messages except the first one if it's a system message.
    pub fn get_history_without_system(&self) -> Vec<ConversationItem> {
        let skip = usize::from(self.history.first().is_some_and(|item| {
            matches!(
                item,
                ConversationItem::Message {
                    role: Role::System,
                    ..
                }
            )
        }));
        self.history.iter().skip(skip).cloned().collect()
    }

    /// Enable streaming JSON output - callback receives JSON string for each message
    pub fn with_streaming_json(mut self, callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.streaming_callback = Some(Box::new(callback));
        self
    }

    /// Emit the init message with session info, cwd, and tools
    pub fn emit_init_message(&self) {
        if let Some(ref callback) = self.streaming_callback {
            // Get current working directory
            let cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            // Get tool names only
            let tools: Vec<String> = self.tools.iter().map(|tool| tool.name.clone()).collect();

            let json = serde_json::json!({
                "type": "init",
                "session_id": self.session_id,
                "cwd": cwd,
                "tools": tools
            });

            if let Ok(json_str) = serde_json::to_string(&json) {
                callback(&json_str);
            }
        }
    }

    /// Accumulate usage from an API response
    #[allow(clippy::ref_option)]
    fn accumulate_usage(&mut self, usage: Option<&ApiUsage>) {
        if let Some(api_usage) = usage {
            self.total_usage.input_tokens += api_usage.input_tokens.unwrap_or(0);
            self.total_usage.input_tokens_details.cached_tokens += api_usage
                .input_tokens_details
                .as_ref()
                .map_or(0, |d| d.cached_tokens.unwrap_or(0));
            self.total_usage.output_tokens += api_usage.output_tokens.unwrap_or(0);
            self.total_usage.output_tokens_details.reasoning_tokens += api_usage
                .output_tokens_details
                .as_ref()
                .map_or(0, |d| d.reasoning_tokens.unwrap_or(0));
            self.total_usage.total_tokens += api_usage.total_tokens.unwrap_or(0);
            self.turn_count += 1;
        }
    }

    /// Emit the result message with success/error, duration, and usage stats
    pub fn emit_result_message(
        &self,
        success: bool,
        duration_ms: u64,
        error_message: Option<&str>,
    ) {
        if let Some(ref callback) = self.streaming_callback {
            let mut json = serde_json::json!({
                "type": "result",
                "success": success,
                "duration_ms": duration_ms,
                "turn_count": self.turn_count,
                "usage": self.total_usage
            });

            if success {
                json["subtype"] = serde_json::json!("success");
            } else {
                json["subtype"] = serde_json::json!("error");
                if let Some(err_msg) = error_message {
                    json["error"] = serde_json::json!(err_msg);
                }
            }

            if let Ok(json_str) = serde_json::to_string(&json) {
                callback(&json_str);
            }
        }
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn temperature(mut self, temperature: Option<f32>) -> Self {
        if let Some(temperature) = temperature {
            self.temperature = Some(temperature);
        }
        self
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn top_p(mut self, top_p: Option<f32>) -> Self {
        if let Some(top_p) = top_p {
            self.top_p = Some(top_p);
        }
        self
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn max_output_tokens(mut self, max_output_tokens: Option<u32>) -> Self {
        if let Some(max_output_tokens) = max_output_tokens {
            self.max_output_tokens = Some(max_output_tokens);
        }
        self
    }

    pub fn providers(mut self, providers: Vec<String>) -> Self {
        if !providers.is_empty() {
            self.providers = providers;
        }
        self
    }

    #[allow(clippy::too_many_lines)]
    pub async fn send(&mut self, message: Message) -> anyhow::Result<Option<Message>> {
        // Add user message to history
        let user_content = message.content.clone();
        self.history.push(ConversationItem::Message {
            role: Role::User,
            content: user_content.clone(),
            id: None,
            status: None,
        });

        // Stream user message
        if let Some(ref callback) = self.streaming_callback {
            let json = serde_json::json!({
                "type": "message",
                "role": "user",
                "content": user_content
            });
            if let Ok(json_str) = serde_json::to_string(&json) {
                callback(&json_str);
            }
        }

        let provider_config = if self.providers.is_empty()
            || (self.providers.len() == 1 && self.providers[0] == "all")
        {
            None
        } else {
            Some(super::types::ProviderConfig {
                only: self.providers.clone(),
            })
        };

        // Agent loop: continue until model stops making tool calls
        loop {
            let prompt = Request {
                model: self.model.as_ref(),
                input: build_input(&self.history),
                temperature: self.temperature,
                top_p: self.top_p,
                max_output_tokens: self.max_output_tokens,
                tools: Some(self.tools.clone()),
                tool_choice: Some("auto".to_string()),
                provider: provider_config.clone(),
            };

            debug!(target: "acai", "{BASE_URL}");
            let prompt_json = serde_json::to_string(&prompt)?;
            debug!(target: "acai", "{prompt_json}");

            let mut attempt = 0;
            let response = loop {
                let response = self
                    .client
                    .post(BASE_URL)
                    .json(&prompt)
                    .header("content-type", "application/json")
                    .header("HTTP-Referer", "https://github.com/travisennis/acai")
                    .header("X-Title", "acai")
                    .bearer_auth(self.token.clone())
                    .send()
                    .await?;

                let status = response.status();

                if status.is_success() || !is_retryable_status(status) {
                    // Success or non-retryable error - break out of retry loop
                    break response;
                }

                // Transient error - check if we should retry
                attempt += 1;
                if attempt > MAX_RETRIES {
                    // Exhausted retries - return the error response
                    break response;
                }

                // Calculate exponential backoff delay
                let delay_secs = INITIAL_DELAY_SECS * 2_u64.pow(attempt - 1);
                let delay = Duration::from_secs(delay_secs);

                debug!(
                    target: "acai",
                    "API request failed with status {status}, retrying in {delay_secs}s (attempt {attempt}/{MAX_RETRIES})"
                );

                sleep(delay).await;
            };

            if response.status().is_success() {
                let api_response = response.json::<ApiResponse>().await?;
                debug!(target: "acai", "{api_response:?}");

                // Accumulate usage from this API response
                self.accumulate_usage(api_response.usage.as_ref());

                // Parse ALL output items (reasoning, function_calls, messages)
                let items = parse_output_items(&api_response);

                // Stream each item as JSON if callback is set
                if let Some(ref callback) = self.streaming_callback {
                    for item in &items {
                        if let Ok(json) = serde_json::to_string(&item.to_streaming_json()) {
                            callback(&json);
                        }
                    }
                }

                // Collect function calls from the items (borrowing from items)
                let function_calls: Vec<_> = items
                    .iter()
                    .filter_map(|item| {
                        if let ConversationItem::FunctionCall {
                            id,
                            call_id,
                            name,
                            arguments,
                        } = item
                        {
                            Some((id.clone(), call_id.clone(), name.clone(), arguments.clone()))
                        } else {
                            None
                        }
                    })
                    .collect();

                // Move items into history BEFORE checking for function calls
                self.history.extend(items);

                // If no function calls, resolve and return the message
                if function_calls.is_empty() {
                    return Ok(Some(resolve_assistant_message(&self.history)));
                }

                // Execute tool calls concurrently using join_all
                let futures = function_calls
                    .iter()
                    .map(|(_id, call_id, name, arguments)| {
                        let call_id = call_id.clone();
                        async move {
                            let result = match execute_tool(name, arguments).await {
                                Ok(r) => r.output,
                                Err(e) => format!("Error: {e}"),
                            };
                            (call_id, result)
                        }
                    });

                let results = futures::future::join_all(futures).await;

                // Add results to history in order
                for (call_id, output) in results {
                    let item = ConversationItem::FunctionCallOutput { call_id, output };
                    self.history.push(item.clone());

                    // Stream the function call output
                    if let (Some(ref callback), Ok(json)) = (
                        self.streaming_callback.as_ref(),
                        serde_json::to_string(&item.to_streaming_json()),
                    ) {
                        callback(&json);
                    }
                }

                // Loop continues - send next request with tool results included
            } else {
                let error_text = response.text().await?;
                debug!(target: "acai", "{error_text}");

                return match serde_json::from_str::<serde_json::Value>(&error_text) {
                    Ok(resp_json) => match serde_json::to_string_pretty(&resp_json) {
                        Ok(resp_formatted) => Err(anyhow::anyhow!(
                            "{}\n\n{}",
                            self.model.as_ref(),
                            resp_formatted
                        )),
                        Err(e) => Err(anyhow::anyhow!("Failed to format response JSON: {e}")),
                    },
                    Err(_) => Err(anyhow::anyhow!("{}\n\n{}", self.model.as_ref(), error_text)),
                };
            }
        }
    }
}

/// Build the input array for the Responses API from conversation history
fn build_input(history: &[ConversationItem]) -> Vec<serde_json::Value> {
    history.iter().map(ConversationItem::to_api_input).collect()
}

/// Extract the assistant message from output items, or return a meaningful
/// fallback when the response was truncated or empty.
fn resolve_assistant_message(items: &[ConversationItem]) -> Message {
    // Look for an assistant message in the output
    if let Some(msg) = items.iter().find_map(|item| {
        if let ConversationItem::Message {
            role: Role::Assistant,
            content,
            ..
        } = item
        {
            Some(Message {
                role: Role::Assistant,
                content: content.clone(),
            })
        } else {
            None
        }
    }) {
        return msg;
    }

    // No assistant message — response was incomplete
    let content = if items.is_empty() {
        "No response was received from the model.".to_string()
    } else if items
        .iter()
        .any(|item| matches!(item, ConversationItem::Reasoning { .. }))
    {
        "The model's response was incomplete. The task may have been partially completed but was cut off during reasoning.".to_string()
    } else {
        "The model's response was incomplete. No final message was received.".to_string()
    };

    Message {
        role: Role::Assistant,
        content,
    }
}

fn parse_output_items(api_response: &ApiResponse) -> Vec<ConversationItem> {
    let mut items = Vec::new();

    for output in &api_response.output {
        match output.msg_type.as_str() {
            "reasoning" => {
                if let Some(id) = &output.id {
                    // Extract summary: prefer top-level summary array, fall back to
                    // content-based reasoning_text items
                    let summary = output
                        .summary
                        .clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| {
                            output
                                .content
                                .as_ref()
                                .map(|c| {
                                    c.iter()
                                        .filter(|item| item.content_type == "reasoning_text")
                                        .filter_map(|item| item.text.clone())
                                        .collect()
                                })
                                .unwrap_or_default()
                        });

                    // Preserve original content array for echoing back to the API
                    let content = output.content.as_ref().map(|c| {
                        c.iter()
                            .map(|item| super::types::ReasoningContent {
                                content_type: item.content_type.clone(),
                                text: item.text.clone(),
                            })
                            .collect()
                    });

                    items.push(ConversationItem::Reasoning {
                        id: id.clone(),
                        summary,
                        encrypted_content: output.encrypted_content.clone(),
                        content,
                    });
                }
            },
            "function_call" => {
                items.push(ConversationItem::FunctionCall {
                    id: output.id.clone().unwrap_or_default(),
                    call_id: output.call_id.clone().unwrap_or_default(),
                    name: output.name.clone().unwrap_or_default(),
                    arguments: output.arguments.clone().unwrap_or_default(),
                });
            },
            "message" => {
                // Extract text content
                let text = output
                    .content
                    .as_ref()
                    .and_then(|c| c.iter().find(|item| item.content_type == "output_text"))
                    .and_then(|item| item.text.clone())
                    .unwrap_or_default();

                items.push(ConversationItem::Message {
                    role: Role::Assistant,
                    content: text,
                    id: output.id.clone(),
                    status: output.status.clone(),
                });
            },
            _ => {}, // ignore unknown types
        }
    }

    items
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::super::types::{
        ApiInputTokensDetails, ApiOutputTokensDetails, OutputContent, OutputMessage,
    };
    use super::*;

    fn test_client() -> Responses {
        Responses {
            model: Cow::Owned("test-model".to_string()),
            token: "test-token".to_string(),
            temperature: Some(0.8),
            top_p: None,
            max_output_tokens: Some(8000),
            providers: DEFAULT_PROVIDERS.iter().map(|s| (*s).to_string()).collect(),
            history: vec![],
            tools: vec![],
            streaming_callback: None,
            session_id: "test-session".to_string(),
            total_usage: Usage::default(),
            turn_count: 0,
            client: reqwest::Client::new(),
        }
    }

    #[test]
    fn accumulate_usage_adds_tokens() {
        let mut client = test_client();
        let usage = ApiUsage {
            input_tokens: Some(100),
            output_tokens: Some(50),
            total_tokens: Some(150),
            input_tokens_details: Some(ApiInputTokensDetails {
                cached_tokens: Some(10),
            }),
            output_tokens_details: Some(ApiOutputTokensDetails {
                reasoning_tokens: Some(5),
            }),
        };
        client.accumulate_usage(Some(&usage));
        assert_eq!(client.total_usage.input_tokens, 100);
        assert_eq!(client.total_usage.output_tokens, 50);
        assert_eq!(client.total_usage.total_tokens, 150);
        assert_eq!(client.total_usage.input_tokens_details.cached_tokens, 10);
        assert_eq!(client.total_usage.output_tokens_details.reasoning_tokens, 5);
        assert_eq!(client.turn_count, 1);
    }

    #[test]
    fn accumulate_usage_none_is_noop() {
        let mut client = test_client();
        client.accumulate_usage(None);
        assert_eq!(client.total_usage.input_tokens, 0);
        assert_eq!(client.turn_count, 0);
    }

    #[test]
    fn accumulate_usage_accumulates_across_calls() {
        let mut client = test_client();
        let usage = ApiUsage {
            input_tokens: Some(100),
            output_tokens: Some(50),
            total_tokens: Some(150),
            input_tokens_details: None,
            output_tokens_details: None,
        };
        client.accumulate_usage(Some(&usage));
        client.accumulate_usage(Some(&usage));
        assert_eq!(client.total_usage.input_tokens, 200);
        assert_eq!(client.total_usage.output_tokens, 100);
        assert_eq!(client.total_usage.total_tokens, 300);
        assert_eq!(client.turn_count, 2);
    }

    #[test]
    fn accumulate_usage_handles_partial_usage() {
        let mut client = test_client();
        let usage = ApiUsage {
            input_tokens: Some(100),
            output_tokens: None,
            total_tokens: None,
            input_tokens_details: None,
            output_tokens_details: None,
        };
        client.accumulate_usage(Some(&usage));
        assert_eq!(client.total_usage.input_tokens, 100);
        assert_eq!(client.total_usage.output_tokens, 0);
        assert_eq!(client.total_usage.total_tokens, 0);
    }

    #[test]
    fn parse_output_items_message() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "message".to_string(),
                id: Some("msg-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: Some("assistant".to_string()),
                status: Some("completed".to_string()),
                content: Some(vec![OutputContent {
                    content_type: "output_text".to_string(),
                    text: Some("Hello!".to_string()),
                }]),
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ConversationItem::Message {
            role: Role::Assistant, content, ..
        } if content == "Hello!"));
    }

    #[test]
    fn parse_output_items_function_call() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "function_call".to_string(),
                id: Some("fc-1".to_string()),
                call_id: Some("call-1".to_string()),
                name: Some("bash".to_string()),
                arguments: Some(r#"{"cmd":"ls"}"#.to_string()),
                role: None,
                status: None,
                content: None,
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ConversationItem::FunctionCall {
            name, ..
        } if name == "bash"));
    }

    #[test]
    fn parse_output_items_reasoning() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "reasoning".to_string(),
                id: Some("r-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: Some(vec![OutputContent {
                    content_type: "reasoning_text".to_string(),
                    text: Some("thinking...".to_string()),
                }]),
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ConversationItem::Reasoning { .. }));
    }

    #[test]
    fn parse_output_items_reasoning_with_encrypted_content() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "reasoning".to_string(),
                id: Some("r-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: None,
                encrypted_content: Some("gAAAAABencrypted...".to_string()),
                summary: Some(vec!["step 1".to_string(), "step 2".to_string()]),
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        if let ConversationItem::Reasoning {
            summary,
            encrypted_content,
            ..
        } = &items[0]
        {
            assert_eq!(summary.len(), 2);
            assert_eq!(summary[0], "step 1");
            assert_eq!(encrypted_content.as_deref(), Some("gAAAAABencrypted..."));
        } else {
            panic!("Expected Reasoning item");
        }
    }

    #[test]
    fn parse_output_items_reasoning_preserves_content_for_roundtrip() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "reasoning".to_string(),
                id: Some("r-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: Some(vec![OutputContent {
                    content_type: "reasoning_text".to_string(),
                    text: Some("deep reasoning here".to_string()),
                }]),
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        // Verify content is preserved for API round-tripping
        let api_input = items[0].to_api_input();
        assert_eq!(api_input["content"][0]["type"], "reasoning_text");
        assert_eq!(api_input["content"][0]["text"], "deep reasoning here");
    }

    #[test]
    fn parse_output_items_unknown_type_ignored() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "unknown_type".to_string(),
                id: None,
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: None,
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert!(items.is_empty());
    }

    #[test]
    fn parse_output_items_multiple_items() {
        let response = ApiResponse {
            id: None,
            output: vec![
                OutputMessage {
                    msg_type: "reasoning".to_string(),
                    id: Some("r-1".to_string()),
                    call_id: None,
                    name: None,
                    arguments: None,
                    role: None,
                    status: None,
                    content: Some(vec![OutputContent {
                        content_type: "reasoning_text".to_string(),
                        text: Some("thinking...".to_string()),
                    }]),
                    encrypted_content: None,
                    summary: None,
                },
                OutputMessage {
                    msg_type: "message".to_string(),
                    id: Some("msg-1".to_string()),
                    call_id: None,
                    name: None,
                    arguments: None,
                    role: None,
                    status: None,
                    content: Some(vec![OutputContent {
                        content_type: "output_text".to_string(),
                        text: Some("Hello!".to_string()),
                    }]),
                    encrypted_content: None,
                    summary: None,
                },
            ],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn parse_output_items_message_without_content() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "message".to_string(),
                id: Some("msg-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: None,
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        // Should have empty content
        assert!(matches!(&items[0], ConversationItem::Message {
            content, ..
        } if content.is_empty()));
    }

    #[test]
    fn builder_with_session_id() {
        let client = test_client().with_session_id("custom-id".to_string());
        assert_eq!(client.session_id, "custom-id");
    }

    #[test]
    fn builder_with_history() {
        let history = vec![ConversationItem::Message {
            role: Role::User,
            content: "hi".to_string(),
            id: None,
            status: None,
        }];
        let client = test_client().with_history(history);
        assert_eq!(client.history.len(), 1);
    }

    #[test]
    fn builder_temperature() {
        let client = test_client().temperature(Some(0.5));
        assert_eq!(client.temperature, Some(0.5));
    }

    #[test]
    fn builder_temperature_none_keeps_default() {
        let client = test_client().temperature(None);
        assert_eq!(client.temperature, Some(0.8));
    }

    #[test]
    fn builder_top_p() {
        let client = test_client().top_p(Some(0.9));
        assert_eq!(client.top_p, Some(0.9));
    }

    #[test]
    fn builder_max_output_tokens() {
        let client = test_client().max_output_tokens(Some(4000));
        assert_eq!(client.max_output_tokens, Some(4000));
    }

    #[test]
    fn build_input_converts_history() {
        let history = vec![ConversationItem::Message {
            role: Role::User,
            content: "hi".to_string(),
            id: None,
            status: None,
        }];
        let input = build_input(&history);
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["type"], "message");
    }

    #[test]
    fn build_input_empty_history() {
        let history: Vec<ConversationItem> = vec![];
        let input = build_input(&history);
        assert!(input.is_empty());
    }

    #[test]
    fn emit_result_message_success() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured_clone = captured.clone();
        let client = test_client().with_streaming_json(move |json| {
            *captured_clone.lock().unwrap() = json.to_string();
        });
        client.emit_result_message(true, 1000, None);
        drop(client);
        let json: serde_json::Value = serde_json::from_str(&captured.lock().unwrap()).unwrap();
        assert_eq!(json["type"], "result");
        assert_eq!(json["subtype"], "success");
        assert_eq!(json["success"], true);
        assert_eq!(json["duration_ms"], 1000);
    }

    #[test]
    fn emit_result_message_error() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured_clone = captured.clone();
        let client = test_client().with_streaming_json(move |json| {
            *captured_clone.lock().unwrap() = json.to_string();
        });
        client.emit_result_message(false, 500, Some("boom"));
        drop(client);
        let json: serde_json::Value = serde_json::from_str(&captured.lock().unwrap()).unwrap();
        assert_eq!(json["subtype"], "error");
        assert_eq!(json["error"], "boom");
    }

    #[test]
    fn emit_result_message_no_callback() {
        let client = test_client();
        // Should not panic when no callback is set
        client.emit_result_message(true, 1000, None);
    }

    #[test]
    fn emit_init_message() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured_clone = captured.clone();
        let client = test_client().with_streaming_json(move |json| {
            *captured_clone.lock().unwrap() = json.to_string();
        });
        client.emit_init_message();
        drop(client);
        let json: serde_json::Value = serde_json::from_str(&captured.lock().unwrap()).unwrap();
        assert_eq!(json["type"], "init");
        assert_eq!(json["session_id"], "test-session");
    }

    #[test]
    fn get_history_without_system_excludes_system_message() {
        let mut client = test_client();
        client.history.push(ConversationItem::Message {
            role: Role::System,
            content: "system prompt".to_string(),
            id: None,
            status: None,
        });
        client.history.push(ConversationItem::Message {
            role: Role::User,
            content: "user message".to_string(),
            id: None,
            status: None,
        });
        let history = client.get_history_without_system();
        assert_eq!(history.len(), 1);
        assert!(matches!(
            &history[0],
            ConversationItem::Message {
                role: Role::User,
                ..
            }
        ));
    }

    #[test]
    fn get_history_without_system_no_system_message() {
        let mut client = test_client();
        client.history.push(ConversationItem::Message {
            role: Role::User,
            content: "user message".to_string(),
            id: None,
            status: None,
        });
        let history = client.get_history_without_system();
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn get_history_without_system_empty_history() {
        let client = test_client();
        let history = client.get_history_without_system();
        assert!(history.is_empty());
    }

    #[test]
    fn resolve_assistant_message_with_assistant_message() {
        let items = vec![ConversationItem::Message {
            role: Role::Assistant,
            content: "Hello!".to_string(),
            id: Some("msg-1".to_string()),
            status: Some("completed".to_string()),
        }];
        let msg = resolve_assistant_message(&items);
        assert_eq!(msg.content, "Hello!");
    }

    #[test]
    fn resolve_assistant_message_truncated_with_reasoning() {
        let items = vec![ConversationItem::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["thinking...".to_string()],
            encrypted_content: None,
            content: None,
        }];
        let msg = resolve_assistant_message(&items);
        assert!(msg.content.contains("cut off during reasoning"));
    }

    #[test]
    fn resolve_assistant_message_no_output_items() {
        let items: Vec<ConversationItem> = vec![];
        let msg = resolve_assistant_message(&items);
        assert_eq!(msg.content, "No response was received from the model.");
    }

    #[test]
    fn resolve_assistant_message_items_but_no_message_or_reasoning() {
        let items = vec![ConversationItem::FunctionCall {
            id: "fc-1".to_string(),
            call_id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: "{}".to_string(),
        }];
        let msg = resolve_assistant_message(&items);
        assert_eq!(
            msg.content,
            "The model's response was incomplete. No final message was received."
        );
    }

    #[test]
    fn builder_providers_sets_custom_providers() {
        let client = test_client().providers(vec!["OpenAI".to_string(), "Anthropic".to_string()]);
        assert_eq!(client.providers, vec!["OpenAI", "Anthropic"]);
    }

    #[test]
    fn builder_providers_empty_uses_default() {
        let client = test_client().providers(vec![]);
        let expected: Vec<String> = DEFAULT_PROVIDERS.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(client.providers, expected);
    }

    #[test]
    fn providers_default_in_new() {
        // Set a dummy API key for testing if not already set
        if std::env::var("OPENROUTER_API_KEY").is_err() {
            #[allow(unused_unsafe)]
            unsafe {
                std::env::set_var("OPENROUTER_API_KEY", "test-api-key");
            }
        }
        let client =
            Responses::new("test-model", "system prompt").expect("Failed to create client");
        let expected: Vec<String> = DEFAULT_PROVIDERS.iter().map(|s| (*s).to_string()).collect();
        assert_eq!(client.providers, expected);
    }

    #[test]
    fn provider_config_with_all_returns_none() {
        let providers = vec!["all".to_string()];
        let config = if providers.is_empty() || (providers.len() == 1 && providers[0] == "all") {
            None
        } else {
            Some(super::super::types::ProviderConfig { only: providers })
        };
        assert!(config.is_none());
    }

    #[test]
    fn is_retryable_status_correctly_identifies_transient_errors() {
        // Retryable status codes
        assert!(is_retryable_status(reqwest::StatusCode::TOO_MANY_REQUESTS)); // 429
        assert!(is_retryable_status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR
        )); // 500
        assert!(is_retryable_status(
            reqwest::StatusCode::SERVICE_UNAVAILABLE
        )); // 503

        // Non-retryable status codes
        assert!(!is_retryable_status(reqwest::StatusCode::BAD_REQUEST)); // 400
        assert!(!is_retryable_status(reqwest::StatusCode::UNAUTHORIZED)); // 401
        assert!(!is_retryable_status(reqwest::StatusCode::FORBIDDEN)); // 403
        assert!(!is_retryable_status(reqwest::StatusCode::NOT_FOUND)); // 404
        assert!(!is_retryable_status(reqwest::StatusCode::OK)); // 200
        assert!(!is_retryable_status(reqwest::StatusCode::CREATED)); // 201
    }

    #[test]
    fn streaming_json_includes_function_call_output() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured_clone = captured.clone();

        let mut client = test_client().with_streaming_json(move |json| {
            captured_clone.lock().unwrap().push(json.to_string());
        });

        // Add a function call to history
        client.history.push(ConversationItem::FunctionCall {
            id: "call-1".to_string(),
            call_id: "call-1".to_string(),
            name: "bash".to_string(),
            arguments: r#"{"cmd":"echo hello"}"#.to_string(),
        });

        // Simulate tool execution results
        let results = vec![("call-1".to_string(), "hello world".to_string())];

        // Add results to history (this should trigger streaming)
        for (call_id, output) in results {
            let item = ConversationItem::FunctionCallOutput { call_id, output };
            client.history.push(item.clone());

            // Stream the function call output
            if let (Some(callback), Ok(json)) = (
                client.streaming_callback.as_ref(),
                serde_json::to_string(&item.to_streaming_json()),
            ) {
                callback(&json);
            }
        }

        drop(client);
        let messages: Vec<serde_json::Value> = captured
            .lock()
            .unwrap()
            .iter()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["type"], "function_call_output");
        assert_eq!(messages[0]["call_id"], "call-1");
        assert_eq!(messages[0]["output"], "hello world");
    }
}
