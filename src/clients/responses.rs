use std::{env, error::Error};

use log::debug;
use serde::{Deserialize, Serialize};

use crate::models::{Message, Role};

const BASE_URL: &str = "https://openrouter.ai/api/v1/responses";

// =============================================================================
// Tool Types
// =============================================================================

/// Tool definition sent in API requests
#[derive(Serialize, Clone, Debug)]
struct Tool {
    #[serde(rename = "type")]
    tool_type: String,
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// Tool choice configuration
#[derive(Serialize, Clone, Debug)]
struct ToolChoice {
    #[serde(rename = "type")]
    tool_type: String,
    name: String,
}

/// Function call output from API response
#[derive(Deserialize, Debug, Clone)]
struct FunctionCall {
    #[serde(rename = "type")]
    msg_type: String,
    id: String,
    #[serde(rename = "call_id")]
    call_id: String,
    name: String,
    arguments: String,
}

/// Tool output to include in conversation
#[derive(Serialize, Clone)]
struct FunctionCallOutput {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(rename = "call_id")]
    call_id: String,
    output: String,
}

/// Tool output result (stored separately from messages)
#[derive(Clone, Debug)]
struct ToolOutput {
    call_id: String,
    output: String,
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
        timeout: Option<u64>,
    }

    let args: ShellArgs = serde_json::from_str(arguments)
        .map_err(|e| format!("Invalid shell arguments: {}", e))?;

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
    system: String,
    messages: Vec<Message>,
    stream: bool,
    tools: Vec<Tool>,
    /// Pending tool calls from the last assistant response
    pending_tool_calls: Vec<FunctionCall>,
    /// Pending tool outputs to send in next request
    pending_tool_outputs: Vec<ToolOutput>,
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
            messages: vec![Message {
                role: Role::System,
                content: system_prompt.to_string(),
            }],
            stream: false,
            tools: vec![shell_tool()],
            pending_tool_calls: vec![],
            pending_tool_outputs: vec![],
        }
    }

    /// Add custom tools or override defaults
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

    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    pub async fn send(
        &mut self,
        message: Message,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        self.messages.push(message);
        let client = reqwest::Client::new();

        // Agent loop: continue until model stops making tool calls
        loop {
            let prompt = Request {
                model: self.model.clone(),
                input: build_input(&self.messages, &self.pending_tool_calls),
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

                // Check for function calls
                let function_calls = parse_function_calls(api_response.output.clone());

                if function_calls.is_empty() {
                    // No tool calls, we're done - return the assistant message
                    let message = parse_response(api_response);

                    if let Some(ref msg) = message {
                        self.messages.push(msg.clone());
                    }

                    return Ok(message);
                }

                // Store tool calls for next request
                self.pending_tool_calls = function_calls.clone();

                // Add assistant message with tool_calls to history
                // Use empty content - tool_calls will be sent separately
                self.messages.push(Message {
                    role: Role::Assistant,
                    content: String::new(),
                });

                // Clear previous tool outputs and add new ones
                self.pending_tool_outputs.clear();

                // Execute each tool call and store results (NOT as messages)
                for call in &function_calls {
                    // Execute the tool
                    let result = execute_tool(&call.name, &call.arguments);

                    let tool_result = match result {
                        Ok(r) => r.output,
                        Err(e) => format!("Error: {}", e),
                    };

                    // The function_call_output approach is rejected by OpenAI's API.
                    // Use user message workaround with structured content
                    self.messages.push(Message {
                        role: Role::User,
                        content: format!(
                            "[Tool Result for call_id: {}]\nOutput: {}",
                            call.call_id, tool_result
                        ),
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

    pub fn get_message_history(&self) -> Vec<Message> {
        self.messages.clone()
    }
}

fn build_input(messages: &[Message], tool_calls: &[FunctionCall]) -> Vec<serde_json::Value> {
    let mut inputs = Vec::new();
    let mut last_assistant_idx: Option<usize> = None;
    
    // First, find the last assistant message (which should have tool_calls_summary)
    for (i, msg) in messages.iter().enumerate() {
        if msg.role == Role::Assistant {
            last_assistant_idx = Some(i);
        }
    }

    for (i, msg) in messages.iter().enumerate() {
        // Skip tool role messages - they're handled separately as function_call_output
        if msg.role == Role::Tool {
            continue;
        }

        match msg.role {
            Role::Tool => {
                // Tool results are now sent as user messages, so this shouldn't happen
                // But handle it just in case
                continue;
            }
            Role::Assistant => {
                // If this is the last assistant message and we have tool_calls, include them
                let is_last_assistant = last_assistant_idx == Some(i);
                
                if is_last_assistant && !tool_calls.is_empty() {
                    // Assistant message with tool_calls
                    let tool_calls_json: Vec<serde_json::Value> = tool_calls
                        .iter()
                        .map(|tc| {
                            serde_json::json!({
                                "id": tc.call_id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments
                                }
                            })
                        })
                        .collect();

                    // Assistant message with tool_calls - always include content field (even if empty)
                    // This is required by the API
                    if msg.content.is_empty() {
                        inputs.push(serde_json::json!({
                            "type": "message",
                            "role": "assistant",
                            "content": "",
                            "tool_calls": tool_calls_json
                        }));
                    } else {
                        inputs.push(serde_json::json!({
                            "type": "message",
                            "role": "assistant",
                            "content": [{
                                "type": "input_text",
                                "text": msg.content.clone()
                            }],
                            "tool_calls": tool_calls_json
                        }));
                    }
                } else {
                    // Regular assistant message
                    inputs.push(serde_json::json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "input_text",
                            "text": msg.content.clone()
                        }]
                    }));
                }
            }
            _ => {
                // System or User messages
                inputs.push(serde_json::json!({
                    "type": "message",
                    "role": match msg.role {
                        Role::System => "system",
                        Role::User => "user",
                        _ => "assistant",
                    },
                    "content": [{
                        "type": "input_text",
                        "text": msg.content.clone()
                    }]
                }));
            }
        }
    }

    // Note: Tool results are now sent as user messages instead of function_call_output
    // because OpenAI's API rejects function_call_output with the error:
    // "messages with role 'tool' must be a response to a preceding message with 'tool_calls'"
    // The workaround is to include the tool result as a user message with structured content.

    inputs
}

fn extract_call_id_and_output(content: &str) -> (String, String) {
    // Content format: "call_id: xxx\noutput: yyy"
    let mut call_id = String::new();
    let mut output = String::new();

    for line in content.split('\n') {
        if line.starts_with("call_id: ") {
            call_id = line[9..].to_string();
        } else if line.starts_with("output: ") {
            output = line[8..].to_string();
        }
    }

    (call_id, output)
}

fn extract_call_id(content: &str) -> String {
    // Extract call_id from content like "[Tool Call: shell id=xxx call_id=yyy]\nArguments: ..."
    if let Some(start) = content.find("call_id=") {
        let rest = &content[start + 8..];
        if let Some(end) = rest.find(|c: char| !c.is_alphanumeric() && c != '-') {
            return rest[..end].to_string();
        }
        return rest.to_string();
    }
    // If no call_id found, return empty string (API might reject this)
    "".to_string()
}

fn parse_function_calls(output: Vec<OutputMessage>) -> Vec<FunctionCall> {
    output
        .into_iter()
        .filter(|o| o.msg_type == "function_call")
        .map(|o| FunctionCall {
            msg_type: o.msg_type,
            id: o.id.unwrap_or_default(),
            call_id: o.call_id.unwrap_or_default(),
            name: o.name.unwrap_or_default(),
            arguments: o.arguments.unwrap_or_default(),
        })
        .collect()
}

fn parse_response(api_response: ApiResponse) -> Option<Message> {
    // Find the message output (skip reasoning blocks)
    // Note: When there are function calls, there might still be a message output
    // but it may not have content (the function call is the main output)
    let output = api_response.output.into_iter().find(|o| o.msg_type == "message")?;

    // Check if there's content - when there are function calls, the message might not have content
    let content_vec = match output.content {
        Some(c) => c,
        None => return None, // No content when there are function calls
    };
    let content = match content_vec.into_iter().next() {
        Some(c) => c,
        None => return None, // Empty content
    };

    if content.content_type == "output_text" {
        Some(Message {
            role: Role::Assistant,
            content: content.text.unwrap_or_default(),
        })
    } else {
        None
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

#[derive(Serialize)]
struct InputMessage {
    #[serde(rename = "type")]
    msg_type: String,
    role: String,
    content: Vec<ContentBlock>,
}

#[derive(Serialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

#[derive(Deserialize, Debug)]
struct ApiResponse {
    id: Option<String>,
    output: Vec<OutputMessage>,
    usage: Option<Usage>,
    status: Option<String>,
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
    input_tokens: Option<u32>,
    #[serde(rename = "output_tokens")]
    output_tokens: Option<u32>,
    #[serde(rename = "total_tokens")]
    total_tokens: Option<u32>,
}

#[derive(Deserialize, Debug)]
struct ApiError {
    code: Option<String>,
    message: String,
    metadata: Option<serde_json::Value>,
}
