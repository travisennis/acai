use std::env;

use log::debug;

use crate::models::{Message, Role};

use super::tools::{Tool, bash_tool, execute_tool};
use super::types::{ApiResponse, ApiUsage, ConversationItem, Request, Usage};

const BASE_URL: &str = "https://openrouter.ai/api/v1/responses";

// =============================================================================
// Responses Client
// =============================================================================

pub struct Responses {
    model: String,
    token: String,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_output_tokens: Option<u32>,
    /// Conversation history using typed items (Responses API format)
    history: Vec<ConversationItem>,
    tools: Vec<Tool>,
    /// Callback for streaming JSON output
    #[allow(clippy::type_complexity)]
    streaming_callback: Option<Box<dyn Fn(&str) + Send + Sync>>,
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
    pub fn new(model: String, system_prompt: &str) -> anyhow::Result<Self> {
        let token = env::var("OPENROUTER_API_KEY").map_err(|e| {
            anyhow::anyhow!("OPENROUTER_API_KEY environment variable is not set: {e}")
        })?;

        Ok(Self {
            model,
            token,
            temperature: Some(0.8),
            top_p: None,
            max_output_tokens: Some(8000),
            history: vec![ConversationItem::Message {
                role: Role::System,
                content: system_prompt.to_string(),
                id: None,
                status: None,
            }],
            tools: vec![bash_tool()],
            streaming_callback: None,
            session_id: uuid::Uuid::new_v4().to_string(),
            total_usage: Usage::default(),
            turn_count: 0,
            client: reqwest::Client::new(),
        })
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

    /// Get a reference to the typed conversation history.
    pub fn get_history(&self) -> &[ConversationItem] {
        &self.history
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

    #[allow(clippy::too_many_lines)]
    pub async fn send(&mut self, message: Message) -> anyhow::Result<Option<Message>> {
        // Emit init message with session info, cwd, and tools
        self.emit_init_message();

        // Stream system message if not already done
        if let Some(ref callback) = self.streaming_callback
            && let Some(ConversationItem::Message {
                role: Role::System,
                content,
                ..
            }) = self.history.first()
        {
            let json = serde_json::json!({
                "type": "message",
                "role": "system",
                "content": content
            });
            if let Ok(json_str) = serde_json::to_string(&json) {
                callback(&json_str);
            }
        }

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

        // Agent loop: continue until model stops making tool calls
        loop {
            let prompt = Request {
                model: self.model.clone(),
                input: build_input(&self.history),
                temperature: self.temperature,
                top_p: self.top_p,
                max_output_tokens: self.max_output_tokens,
                tools: Some(self.tools.clone()),
                tool_choice: Some("auto".to_string()),
            };

            debug!(target: "acai", "{BASE_URL}");
            let prompt_json = serde_json::to_string(&prompt)?;
            debug!(target: "acai", "{prompt_json}");

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

                // Add all output items to history
                self.history.extend(items.clone());

                // Collect function calls from the items
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

                if function_calls.is_empty() {
                    // No tool calls, we're done - extract assistant message text
                    let msg = items.iter().find_map(|item| {
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
                    });
                    return Ok(msg);
                }

                // Execute each tool call and add function_call_output to history
                for (_id, call_id, name, arguments) in &function_calls {
                    let tool_result = match execute_tool(name, arguments).await {
                        Ok(r) => r.output,
                        Err(e) => format!("Error: {e}"),
                    };

                    self.history.push(ConversationItem::FunctionCallOutput {
                        call_id: call_id.clone(),
                        output: tool_result,
                    });
                }

                // Loop continues - send next request with tool results included
            } else {
                let error_text = response.text().await?;
                debug!(target: "acai", "{error_text}");

                return match serde_json::from_str::<serde_json::Value>(&error_text) {
                    Ok(resp_json) => match serde_json::to_string_pretty(&resp_json) {
                        Ok(resp_formatted) => {
                            Err(anyhow::anyhow!("{}\n\n{}", self.model, resp_formatted))
                        },
                        Err(e) => Err(anyhow::anyhow!("Failed to format response JSON: {e}")),
                    },
                    Err(_) => Err(anyhow::anyhow!("{}\n\n{}", self.model, error_text)),
                };
            }
        }
    }
}

/// Build the input array for the Responses API from conversation history
fn build_input(history: &[ConversationItem]) -> Vec<serde_json::Value> {
    history.iter().map(ConversationItem::to_api_input).collect()
}

fn parse_output_items(api_response: &ApiResponse) -> Vec<ConversationItem> {
    let mut items = Vec::new();

    for output in &api_response.output {
        match output.msg_type.as_str() {
            "reasoning" => {
                // Extract reasoning text from content (content_type: "reasoning_text")
                let reasoning_text = output
                    .content
                    .as_ref()
                    .and_then(|c| c.iter().find(|item| item.content_type == "reasoning_text"))
                    .and_then(|item| item.text.clone());

                if let (Some(id), Some(text)) = (&output.id, reasoning_text) {
                    items.push(ConversationItem::Reasoning {
                        id: id.clone(),
                        summary: vec![text],
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
