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
pub(crate) struct Tool {
    #[serde(rename = "type")]
    tool_type: String,
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
        tool_type: "function".to_string(),
        name: "shell".to_string(),
        description: "Execute a shell command in the host machine's terminal. \
            Returns the stdout/stderr output. Use for running build commands, \
            git operations, file manipulation, etc. Does not support interactive commands.".to_string(),
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
fn execute_tool(name: &str, arguments: &str) -> Result<ToolResult, String> {
    match name {
        "shell" => execute_shell(arguments),
        _ => Err(format!("Unknown tool: {}", name)),
    }
}

fn execute_shell(arguments: &str) -> Result<ToolResult, String> {
    #[derive(Deserialize)]
    struct ShellArgs {
        command: String,
        #[allow(dead_code)]
        timeout: Option<u64>,
    }

    let args: ShellArgs = serde_json::from_str(arguments)
        .map_err(|e| format!("Invalid shell arguments: {}", e))?;

    let _timeout = args.timeout;

    let output = std::process::Command::new("bash")
        .arg("-c")
        .arg(&args.command)
        .output()
        .map_err(|e| format!("Failed to execute command: {}", e))?;

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
}

impl Responses {
    pub fn new(model: String, system_prompt: &str) -> Self {
        let token = env::var("OPENROUTER_API_KEY")
            .unwrap_or_else(|_error| panic!("Error: OPENROUTER_API_KEY not set."));

        Self {
            model,
            token,
            temperature: Some(0.0),
            top_p: None,
            max_output_tokens: None,
            system: system_prompt.to_string(),
            history: vec![ConversationItem::Message {
                role: Role::System,
                content: system_prompt.to_string(),
                id: None,
                status: None,
            }],
            stream: false,
            tools: vec![shell_tool()],
        }
    }

    /// Add custom tools or override defaults
    #[allow(dead_code)]
    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = tools;
        self
    }

    pub fn temperature(mut self, temperature: Option<f32>) -> Self {
        if let Some(temperature) = temperature {
            self.temperature = Some(temperature);
        }
        self
    }

    pub fn top_p(mut self, top_p: Option<f32>) -> Self {
        if let Some(top_p) = top_p {
            self.top_p = Some(top_p);
        }
        self
    }

    pub fn max_output_tokens(mut self, max_output_tokens: Option<u32>) -> Self {
        if let Some(max_output_tokens) = max_output_tokens {
            self.max_output_tokens = Some(max_output_tokens);
        }
        self
    }

    #[allow(dead_code)]
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    #[allow(dead_code)]
    pub async fn send(
        &mut self,
        message: Message,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        // Add user message to history
        self.history.push(ConversationItem::Message {
            role: Role::User,
            content: message.content,
            id: None,
            status: None,
        });

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

            debug!(target: "acai", "{}", BASE_URL);
            let prompt_json = serde_json::to_string(&prompt)?;
            debug!(target: "acai", "{}", prompt_json);

            let response = client
                .post(BASE_URL)
                .json(&prompt)
                .header("content-type", "application/json")
                .bearer_auth(self.token.clone())
                .send()
                .await?;

            if response.status().is_success() {
                let api_response = response.json::<ApiResponse>().await?;
                debug!(target: "acai", "{:?}", api_response);

                // Parse ALL output items (reasoning, function_calls, messages)
                let items = parse_output_items(&api_response);

                // Add all output items to history
                self.history.extend(items.clone());

                // Collect function calls from the items
                let function_calls: Vec<_> = items.iter()
                    .filter_map(|item| {
                        if let ConversationItem::FunctionCall { id, call_id, name, arguments } = item {
                            Some((id.clone(), call_id.clone(), name.clone(), arguments.clone()))
                        } else {
                            None
                        }
                    })
                    .collect();

                if function_calls.is_empty() {
                    // No tool calls, we're done - extract assistant message text
                    let msg = items.iter().find_map(|item| {
                        if let ConversationItem::Message { role: Role::Assistant, content, .. } = item {
                            Some(Message { role: Role::Assistant, content: content.clone() })
                        } else {
                            None
                        }
                    });
                    return Ok(msg);
                }

                // Execute each tool call and add function_call_output to history
                for (_id, call_id, name, arguments) in &function_calls {
                    let tool_result = match execute_tool(name, arguments) {
                        Ok(r) => r.output,
                        Err(e) => format!("Error: {}", e),
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
                    Ok(resp_json) => {
                        match serde_json::to_string_pretty(&resp_json) {
                            Ok(resp_formatted) => {
                                Err(format!("{}\n\n{}", self.model, resp_formatted).into())
                            }
                            Err(e) => Err(format!("Failed to format response JSON: {e}").into()),
                        }
                    }
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
    history.iter().map(|item| {
        match item {
            ConversationItem::Message { role, content, id, status } => {
                let use_output_format = matches!(role, Role::Assistant);

                let mut msg = serde_json::json!({
                    "type": "message",
                    "role": match role {
                        Role::System => "system",
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        _ => "user",
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
            ConversationItem::FunctionCall { id, call_id, name, arguments } => {
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
    }).collect()
}

/// Parse all output items from API response into ConversationItems
fn parse_output_items(api_response: &ApiResponse) -> Vec<ConversationItem> {
    let mut items = Vec::new();
    
    for output in &api_response.output {
        match output.msg_type.as_str() {
            "reasoning" => {
                // Extract reasoning text from content (content_type: "reasoning_text")
                let reasoning_text = output.content.as_ref()
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
                let text = output.content.as_ref()
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
    usage: Option<Usage>,
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

#[derive(Deserialize, Debug)]
struct Usage {
    #[serde(rename = "input_tokens")]
    #[allow(dead_code)]
    input_tokens: Option<u32>,
    #[serde(rename = "output_tokens")]
    #[allow(dead_code)]
    output_tokens: Option<u32>,
    #[serde(rename = "total_tokens")]
    #[allow(dead_code)]
    total_tokens: Option<u32>,
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
