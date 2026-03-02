use tokio::time::{Duration, timeout};

use std::{env, error::Error};

use log::debug;
use serde::{Deserialize, Serialize};

use crate::models::{Message, Role};

const BASE_URL: &str = "https://openrouter.ai/api/v1/responses";

// =============================================================================
// Conversation Item Enum (for Responses API input/output)
// =============================================================================

/// Represents a single item in the conversation history, mapping directly to
/// the Responses API input/output array format.
#[derive(Debug, Clone)]
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

// =============================================================================
// Tool Types
// =============================================================================

/// Tool definition sent in API requests
#[derive(Serialize, Clone, Debug)]
pub struct Tool {
    #[serde(rename = "type")]
    type_: String,
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// =============================================================================
// Shell Tool Definition
// =============================================================================

/// Returns the Shell tool definition
fn shell_tool() -> Tool {
    Tool {
        type_: "function".to_string(),
        name: "shell".to_string(),
        description: "Execute a shell command in the host machine's terminal. \
            Returns the stdout/stderr output. Use for running build commands, \
            git operations, file manipulation, etc. Does not support interactive commands."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout": {
                    "type": "number",
                    "description": "Optional timeout in seconds"
                }
            },
            "required": ["command"]
        }),
    }
}

// =============================================================================
// Tool Execution
// =============================================================================

/// Result of executing a tool
#[derive(Debug)]
pub struct ToolResult {
    #[allow(dead_code)]
    pub call_id: String,
    pub output: String,
}

/// Execute a tool call
async fn execute_tool(name: &str, arguments: &str) -> Result<ToolResult, String> {
    match name {
        "shell" => execute_shell(arguments).await,
        _ => Err(format!("Unknown tool: {name}")),
    }
}

async fn execute_shell(arguments: &str) -> Result<ToolResult, String> {
    #[derive(Deserialize)]
    struct ShellArgs {
        command: String,
        timeout: Option<u64>,
    }

    let args: ShellArgs =
        serde_json::from_str(arguments).map_err(|e| format!("Invalid shell arguments: {e}"))?;

    // Use default timeout of 60 seconds if not specified
    let timeout_secs = args.timeout.unwrap_or(60);

    // Run the shell command with timeout using tokio
    // timeout() returns Result<Result<Output, io::Error>, Elapsed>
    let output = match timeout(
        Duration::from_secs(timeout_secs),
        tokio::process::Command::new("bash")
            .arg("-c")
            .arg(&args.command)
            .kill_on_drop(true)
            .output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => return Err(format!("Failed to execute command: {e}")),
        Err(_) => return Err(format!("Command timed out after {timeout_secs} seconds")),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let result = if output.status.success() {
        stdout.to_string()
    } else {
        format!(
            "Exit code {}:\n{}{}",
            output.status.code().unwrap_or(-1),
            stdout,
            stderr
        )
    };

    Ok(ToolResult {
        call_id: String::new(),
        output: result,
    })
}

// =============================================================================
// Responses Client
// =============================================================================

pub struct Responses {
    model: String,
    token: String,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_output_tokens: Option<u32>,
    #[allow(dead_code)]
    system: String,
    /// Conversation history using typed items (Responses API format)
    history: Vec<ConversationItem>,
    #[allow(dead_code)]
    stream: bool,
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
}

impl Responses {
    pub fn new(model: String, system_prompt: &str) -> Self {
        let token = env::var("OPENROUTER_API_KEY")
            .unwrap_or_else(|_error| panic!("Error: OPENROUTER_API_KEY not set."));

        Self {
            model,
            token,
            temperature: Some(0.8),
            top_p: None,
            max_output_tokens: Some(8000),
            system: system_prompt.to_string(),
            history: vec![ConversationItem::Message {
                role: Role::System,
                content: system_prompt.to_string(),
                id: None,
                status: None,
            }],
            stream: false,
            tools: vec![shell_tool()],
            streaming_callback: None,
            session_id: uuid::Uuid::new_v4().to_string(),
            total_usage: Usage::default(),
            turn_count: 0,
        }
    }

    /// Add custom tools or override defaults
    #[allow(dead_code)]
    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = tools;
        self
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
            let tools: Vec<String> = self
                .tools
                .iter()
                .map(|tool| tool.name.clone())
                .collect();

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

    #[allow(clippy::missing_const_for_fn)]
    #[allow(dead_code)]
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    #[allow(clippy::too_many_lines)]
    #[allow(dead_code)]
    pub async fn send(
        &mut self,
        message: Message,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        // Emit init message with session info, cwd, and tools
        self.emit_init_message();

        // Stream system message if not already done
        if let Some(ref callback) = self.streaming_callback {
            if let Some(ConversationItem::Message {
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

        let client = reqwest::Client::new();

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

            let response = client
                .post(BASE_URL)
                .json(&prompt)
                .header("content-type", "application/json")
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
                        if let Ok(json) = serde_json::to_string(&conversation_item_to_json(item)) {
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
                            Err(format!("{}\n\n{}", self.model, resp_formatted).into())
                        }
                        Err(e) => Err(format!("Failed to format response JSON: {e}").into()),
                    },
                    Err(_) => Err(format!("{}\n\n{}", self.model, error_text).into()),
                };
            }
        }
    }

    pub fn get_message_history(&self) -> Vec<serde_json::Value> {
        build_input(&self.history)
    }
}

/// Build the input array for the Responses API from conversation history
fn build_input(history: &[ConversationItem]) -> Vec<serde_json::Value> {
    history
        .iter()
        .map(|item| {
            match item {
                ConversationItem::Message {
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
                }
                ConversationItem::FunctionCall {
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
                }
                ConversationItem::FunctionCallOutput { call_id, output } => {
                    serde_json::json!({
                        "type": "function_call_output",
                        "call_id": call_id,
                        "output": output
                    })
                }
                ConversationItem::Reasoning { id, summary } => {
                    serde_json::json!({
                        "type": "reasoning",
                        "id": id,
                        "summary": summary.iter().map(|s| {
                            serde_json::json!({"type": "summary_text", "text": s})
                        }).collect::<Vec<_>>()
                    })
                }
            }
        })
        .collect()
}

/// Parse all output items from API response into `ConversationItems`
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
            }
            "function_call" => {
                items.push(ConversationItem::FunctionCall {
                    id: output.id.clone().unwrap_or_default(),
                    call_id: output.call_id.clone().unwrap_or_default(),
                    name: output.name.clone().unwrap_or_default(),
                    arguments: output.arguments.clone().unwrap_or_default(),
                });
            }
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
            }
            _ => {} // ignore unknown types
        }
    }

    items
}

/// Convert a `ConversationItem` to a JSON-compatible `serde_json::Value` for streaming output
#[allow(clippy::type_complexity)]
fn conversation_item_to_json(item: &ConversationItem) -> serde_json::Value {
    match item {
        ConversationItem::Message {
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
        }
        ConversationItem::FunctionCall {
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
        }
        ConversationItem::FunctionCallOutput { call_id, output } => {
            serde_json::json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": output
            })
        }
        ConversationItem::Reasoning { id, summary } => {
            serde_json::json!({
                "type": "reasoning",
                "id": id,
                "summary": summary
            })
        }
    }
}

#[derive(Serialize)]
struct Request {
    model: String,
    input: Vec<serde_json::Value>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_output_tokens: Option<u32>,
    tools: Option<Vec<Tool>>,
    tool_choice: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ApiResponse {
    #[allow(dead_code)]
    id: Option<String>,
    output: Vec<OutputMessage>,
    #[allow(dead_code)]
    usage: Option<ApiUsage>,
    #[allow(dead_code)]
    status: Option<String>,
    #[allow(dead_code)]
    error: Option<ApiError>,
}

#[derive(Deserialize, Debug, Clone)]
struct OutputMessage {
    #[serde(rename = "type")]
    msg_type: String,
    id: Option<String>,
    #[serde(rename = "call_id")]
    call_id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
    #[allow(dead_code)]
    role: Option<String>,
    status: Option<String>,
    content: Option<Vec<OutputContent>>,
}

#[derive(Deserialize, Debug, Clone)]
struct OutputContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
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

/// Internal usage struct for API response deserialization (with optional fields)
#[derive(Deserialize, Debug, Clone, Default)]
struct ApiUsage {
    #[serde(rename = "input_tokens")]
    input_tokens: Option<u32>,
    #[serde(rename = "input_tokens_details")]
    input_tokens_details: Option<ApiInputTokensDetails>,
    #[serde(rename = "output_tokens")]
    output_tokens: Option<u32>,
    #[serde(rename = "output_tokens_details")]
    output_tokens_details: Option<ApiOutputTokensDetails>,
    #[serde(rename = "total_tokens")]
    total_tokens: Option<u32>,
}

/// Internal input tokens details for API response deserialization
#[derive(Deserialize, Debug, Clone, Default)]
struct ApiInputTokensDetails {
    #[serde(rename = "cached_tokens")]
    cached_tokens: Option<u32>,
}

/// Internal output tokens details for API response deserialization
#[derive(Deserialize, Debug, Clone, Default)]
struct ApiOutputTokensDetails {
    #[serde(rename = "reasoning_tokens")]
    reasoning_tokens: Option<u32>,
}

#[derive(Deserialize, Debug)]
struct ApiError {
    #[allow(dead_code)]
    code: Option<String>,
    #[allow(dead_code)]
    message: String,
    #[allow(dead_code)]
    metadata: Option<serde_json::Value>,
}
