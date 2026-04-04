use std::time::Duration;

use tokio::time::sleep;
use tracing::debug;

use crate::config::model::{ApiType, ResolvedModelConfig};
use crate::models::{Message, Role};

use crate::clients::chat_completions;
use crate::clients::responses;
use crate::clients::tools::{Tool, bash_tool, edit_tool, execute_tool, read_tool, write_tool};
use crate::clients::types::{ConversationItem, Usage};

/// Callback type for streaming JSON output
type StreamingCallback = Box<dyn Fn(&str) + Send + Sync>;

/// Callback type for progress reporting (receives conversation items as they occur)
type ProgressCallback = Box<dyn Fn(&ConversationItem) + Send + Sync>;

/// Maximum number of retries for transient API errors
const MAX_RETRIES: u32 = 3;
/// Initial delay in seconds for exponential backoff
const INITIAL_DELAY_SECS: u64 = 1;

/// Result of a single API turn (one request/response cycle).
#[derive(Debug)]
pub(super) struct TurnResult {
    pub(super) items: Vec<ConversationItem>,
    pub(super) usage: Option<Usage>,
}

/// Determines if an HTTP status code represents a transient error that should be retried
const fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 429 | 500 | 503)
}

// =============================================================================
// Agent (shared loop over any backend)
// =============================================================================

/// Orchestrates conversation loops, tool execution, and API communication.
///
/// The `Agent` manages the conversation history, executes tool calls from the
/// AI model, and handles streaming output. It supports multiple API backends
/// through the `ApiType` configuration.
///
/// # Example
///
/// ```no_run
/// use acai::clients::Agent;
/// use acai::config::ResolvedModelConfig;
///
/// // Create agent with resolved config and system prompt
/// // let agent = Agent::new(resolved_config, "You are a helpful assistant.");
/// ```
pub struct Agent {
    config: ResolvedModelConfig,
    /// Conversation history using typed items
    history: Vec<ConversationItem>,
    tools: Vec<Tool>,
    /// Callback for streaming JSON output
    streaming_callback: Option<StreamingCallback>,
    /// Callback for human-readable progress reporting
    progress_callback: Option<ProgressCallback>,
    /// Session ID for tracking
    pub session_id: String,
    /// Accumulated usage across all API calls
    pub total_usage: Usage,
    /// Number of API calls made
    pub turn_count: u32,
    /// Reusable HTTP client for connection pooling
    client: reqwest::Client,
}

impl Agent {
    /// Creates a new agent with the given configuration and system prompt.
    ///
    /// The agent is initialized with four default tools: Bash, Read, Edit, and Write.
    /// A new session ID is generated automatically.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use acai::clients::Agent;
    /// use acai::config::ResolvedModelConfig;
    ///
    /// // let resolved = ResolvedModelConfig::resolve(config)?;
    /// // let agent = Agent::new(resolved, "You are a helpful coding assistant.");
    /// ```
    pub fn new(config: ResolvedModelConfig, system_prompt: &str) -> Self {
        Self {
            config,
            history: vec![ConversationItem::Message {
                role: Role::System,
                content: system_prompt.to_string(),
                id: None,
                status: None,
            }],
            tools: vec![bash_tool(), edit_tool(), read_tool(), write_tool()],
            streaming_callback: None,
            progress_callback: None,
            session_id: uuid::Uuid::new_v4().to_string(),
            total_usage: Usage::default(),
            turn_count: 0,
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Returns the model name from the configuration.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use acai::clients::Agent;
    /// # use acai::config::ResolvedModelConfig;
    /// # let agent = Agent::new(ResolvedModelConfig::default(), "prompt");
    /// println!("Using model: {}", agent.model());
    /// ```
    pub fn model(&self) -> &str {
        &self.config.config.model
    }

    /// Returns the number of registered tools.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use acai::clients::Agent;
    /// # use acai::config::ResolvedModelConfig;
    /// # let agent = Agent::new(ResolvedModelConfig::default(), "prompt");
    /// println!("Agent has {} tools", agent.tool_count());
    /// ```
    #[allow(clippy::missing_const_for_fn)]
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    /// Sets the session ID for a restored session.
    ///
    /// Use this when continuing a previous session to preserve the session ID.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use acai::clients::Agent;
    /// # use acai::config::ResolvedModelConfig;
    /// # let resolved = ResolvedModelConfig::default();
    /// let agent = Agent::new(resolved, "prompt")
    ///     .with_session_id("existing-uuid".to_string());
    /// ```
    pub fn with_session_id(mut self, id: String) -> Self {
        self.session_id = id;
        self
    }

    /// Sets the conversation history for a restored session.
    ///
    /// Use this when continuing a previous session to restore the conversation context.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use acai::clients::Agent;
    /// # use acai::config::ResolvedModelConfig;
    /// # use acai::clients::ConversationItem;
    /// # let resolved = ResolvedModelConfig::default();
    /// let history = vec![]; // Previous conversation items
    /// let agent = Agent::new(resolved, "prompt")
    ///     .with_history(history);
    /// ```
    pub fn with_history(mut self, messages: Vec<ConversationItem>) -> Self {
        self.history = messages;
        self
    }

    /// Returns conversation history without the system message.
    ///
    /// This is used when saving sessions to disk, as the system prompt is
    /// regenerated on load rather than stored.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use acai::clients::Agent;
    /// # use acai::config::ResolvedModelConfig;
    /// # let agent = Agent::new(ResolvedModelConfig::default(), "system prompt");
    /// let history = agent.get_history_without_system();
    /// assert!(!history.iter().any(|item| matches!(item, acai::clients::ConversationItem::Message {
    ///     role: acai::models::Role::System, ..
    /// })));
    /// ```
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

    /// Enables streaming JSON output for each message.
    ///
    /// The callback receives a JSON string for each message, tool call, and result.
    /// This is useful for integrating with other tools or TUIs.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use acai::clients::Agent;
    /// # use acai::config::ResolvedModelConfig;
    /// # let resolved = ResolvedModelConfig::default();
    /// let agent = Agent::new(resolved, "prompt")
    ///     .with_streaming_json(|json| println!("{}", json));
    /// ```
    pub fn with_streaming_json(mut self, callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.streaming_callback = Some(Box::new(callback));
        self
    }

    /// Enables progress reporting for tool execution.
    ///
    /// The callback receives conversation items as they occur, useful for
    /// displaying human-readable progress during long-running operations.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use acai::clients::Agent;
    /// # use acai::config::ResolvedModelConfig;
    /// # use acai::clients::ConversationItem;
    /// # let resolved = ResolvedModelConfig::default();
    /// let agent = Agent::new(resolved, "prompt")
    ///     .with_progress_callback(|item| {
    ///         if let ConversationItem::FunctionCall { name, .. } = item {
    ///             eprintln!("Calling tool: {}", name);
    ///         }
    ///     });
    /// ```
    pub fn with_progress_callback(
        mut self,
        callback: impl Fn(&ConversationItem) + Send + Sync + 'static,
    ) -> Self {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    /// Report a conversation item via the progress callback, if set.
    fn report_progress(&self, item: &ConversationItem) {
        if let Some(ref callback) = self.progress_callback {
            callback(item);
        }
    }

    /// Stream a conversation item as JSON via the streaming callback, if set.
    fn stream_item(&self, item: &ConversationItem) {
        if let Some(ref callback) = self.streaming_callback
            && let Ok(json) = serde_json::to_string(&item.to_streaming_json())
        {
            callback(&json);
        }
    }

    /// Emit the init message with session info, cwd, and tools
    pub fn emit_init_message(&self) {
        if let Some(ref callback) = self.streaming_callback {
            let cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

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

    /// Accumulate usage from an API turn
    /// Note: clippy suggests `const fn` but this provides no benefit for runtime-only code.
    #[allow(clippy::missing_const_for_fn)]
    fn accumulate_usage(&mut self, turn_usage: Option<&Usage>) {
        if let Some(usage) = turn_usage {
            self.total_usage.input_tokens += usage.input_tokens;
            self.total_usage.input_tokens_details.cached_tokens +=
                usage.input_tokens_details.cached_tokens;
            self.total_usage.output_tokens += usage.output_tokens;
            self.total_usage.output_tokens_details.reasoning_tokens +=
                usage.output_tokens_details.reasoning_tokens;
            self.total_usage.total_tokens += usage.total_tokens;
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

    /// Sends a message and runs the agent loop until completion.
    ///
    /// The agent will process the message, execute any tool calls, and continue
    /// until the model produces a final response without requesting more tools.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use acai::clients::Agent;
    /// # use acai::config::ResolvedModelConfig;
    /// # use acai::models::Message;
    /// # use acai::models::Role;
    /// # async fn example() -> anyhow::Result<()> {
    /// # let agent = Agent::new(ResolvedModelConfig::default(), "prompt");
    /// let msg = Message { role: Role::User, content: "Hello!".to_string() };
    /// let response = agent.send(msg).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails, the response cannot be parsed,
    /// or a tool execution fails critically.
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

        // Agent loop: continue until model stops making tool calls
        loop {
            let turn_result = self.complete_turn().await?;

            // Accumulate usage
            self.accumulate_usage(turn_result.usage.as_ref());

            // Stream each item as JSON if callback is set, and report progress
            for item in &turn_result.items {
                self.stream_item(item);
                self.report_progress(item);
            }

            // Collect function calls from the items
            let function_calls: Vec<_> = turn_result
                .items
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

            // Move items into history
            self.history.extend(turn_result.items);

            // If no function calls, resolve and return the message
            if function_calls.is_empty() {
                return Ok(Some(resolve_assistant_message(&self.history)));
            }

            // Execute tool calls concurrently
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
                self.stream_item(&item);
                self.history.push(item);
            }

            // Loop continues - send next request with tool results included
        }
    }

    /// Execute a single API turn with retry logic.
    async fn complete_turn(&self) -> anyhow::Result<TurnResult> {
        let mut attempt = 0;
        let response = loop {
            let request_result = match self.config.config.api_type {
                ApiType::Responses => {
                    responses::send_request(&self.client, &self.config, &self.history, &self.tools)
                        .await
                },
                ApiType::ChatCompletions => {
                    chat_completions::send_request(
                        &self.client,
                        &self.config,
                        &self.history,
                        &self.tools,
                    )
                    .await
                },
            };

            match request_result {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() || !is_retryable_status(status) {
                        break resp;
                    }

                    attempt += 1;
                    if attempt > MAX_RETRIES {
                        break resp;
                    }

                    let delay_secs = INITIAL_DELAY_SECS * 2_u64.pow(attempt - 1);
                    let delay = Duration::from_secs(delay_secs);

                    debug!(
                        target: "acai",
                        "API request failed with status {status}, retrying in {delay_secs}s (attempt {attempt}/{MAX_RETRIES})"
                    );

                    sleep(delay).await;
                },
                Err(e) => {
                    // Retry on connection errors and timeouts
                    let is_network_error = e
                        .downcast_ref::<reqwest::Error>()
                        .is_some_and(|req_err| req_err.is_connect() || req_err.is_timeout());

                    if is_network_error {
                        attempt += 1;
                        if attempt > MAX_RETRIES {
                            return Err(e);
                        }

                        let delay_secs = INITIAL_DELAY_SECS * 2_u64.pow(attempt - 1);
                        let delay = Duration::from_secs(delay_secs);

                        debug!(
                            target: "acai",
                            "API request failed with network error: {e}, retrying in {delay_secs}s (attempt {attempt}/{MAX_RETRIES})"
                        );

                        sleep(delay).await;
                        continue;
                    }
                    return Err(e);
                },
            }
        };

        if response.status().is_success() {
            match self.config.config.api_type {
                ApiType::Responses => responses::parse_response(response).await,
                ApiType::ChatCompletions => chat_completions::parse_response(response).await,
            }
        } else {
            let model = &self.config.config.model;
            let error_text = response.text().await?;
            debug!(target: "acai", "{error_text}");

            serde_json::from_str::<serde_json::Value>(&error_text).map_or_else(
                |_err| Err(anyhow::anyhow!("{model}\n\n{error_text}")),
                |resp_json| match serde_json::to_string_pretty(&resp_json) {
                    Ok(resp_formatted) => Err(anyhow::anyhow!("{model}\n\n{resp_formatted}")),
                    Err(e) => Err(anyhow::anyhow!("Failed to format response JSON: {e}")),
                },
            )
        }
    }
}

/// Extract the assistant message from conversation history, or return a meaningful
/// fallback when the response was truncated or empty.
fn resolve_assistant_message(items: &[ConversationItem]) -> Message {
    if let Some(msg) = items.iter().rev().find_map(|item| {
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::clients::types::{InputTokensDetails, OutputTokensDetails};
    use crate::config::model::ModelConfig;

    fn test_config() -> ResolvedModelConfig {
        ResolvedModelConfig {
            config: ModelConfig::default(),
            api_key: "test-token".to_string(),
        }
    }

    fn test_agent() -> Agent {
        let config = test_config();
        Agent {
            config,
            history: vec![],
            tools: vec![],
            streaming_callback: None,
            progress_callback: None,
            session_id: "test-session".to_string(),
            total_usage: Usage::default(),
            turn_count: 0,
            client: reqwest::Client::new(),
        }
    }

    #[test]
    fn accumulate_usage_adds_tokens() {
        let mut agent = test_agent();
        let usage = Usage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            input_tokens_details: InputTokensDetails { cached_tokens: 10 },
            output_tokens_details: OutputTokensDetails {
                reasoning_tokens: 5,
            },
        };
        agent.accumulate_usage(Some(&usage));
        assert_eq!(agent.total_usage.input_tokens, 100);
        assert_eq!(agent.total_usage.output_tokens, 50);
        assert_eq!(agent.total_usage.total_tokens, 150);
        assert_eq!(agent.total_usage.input_tokens_details.cached_tokens, 10);
        assert_eq!(agent.total_usage.output_tokens_details.reasoning_tokens, 5);
        assert_eq!(agent.turn_count, 1);
    }

    #[test]
    fn accumulate_usage_none_is_noop() {
        let mut agent = test_agent();
        agent.accumulate_usage(None);
        assert_eq!(agent.total_usage.input_tokens, 0);
        assert_eq!(agent.turn_count, 0);
    }

    #[test]
    fn accumulate_usage_accumulates_across_calls() {
        let mut agent = test_agent();
        let usage = Usage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            input_tokens_details: InputTokensDetails { cached_tokens: 0 },
            output_tokens_details: OutputTokensDetails {
                reasoning_tokens: 0,
            },
        };
        agent.accumulate_usage(Some(&usage));
        agent.accumulate_usage(Some(&usage));
        assert_eq!(agent.total_usage.input_tokens, 200);
        assert_eq!(agent.total_usage.output_tokens, 100);
        assert_eq!(agent.total_usage.total_tokens, 300);
        assert_eq!(agent.turn_count, 2);
    }

    #[test]
    fn emit_result_message_success() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured_clone = captured.clone();
        let agent = test_agent().with_streaming_json(move |json| {
            *captured_clone.lock().unwrap() = json.to_string();
        });
        agent.emit_result_message(true, 1000, None);
        drop(agent);
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
        let agent = test_agent().with_streaming_json(move |json| {
            *captured_clone.lock().unwrap() = json.to_string();
        });
        agent.emit_result_message(false, 500, Some("boom"));
        drop(agent);
        let json: serde_json::Value = serde_json::from_str(&captured.lock().unwrap()).unwrap();
        assert_eq!(json["subtype"], "error");
        assert_eq!(json["error"], "boom");
    }

    #[test]
    fn emit_result_message_no_callback() {
        let agent = test_agent();
        agent.emit_result_message(true, 1000, None);
    }

    #[test]
    fn emit_init_message() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured_clone = captured.clone();
        let agent = test_agent().with_streaming_json(move |json| {
            *captured_clone.lock().unwrap() = json.to_string();
        });
        agent.emit_init_message();
        drop(agent);
        let json: serde_json::Value = serde_json::from_str(&captured.lock().unwrap()).unwrap();
        assert_eq!(json["type"], "init");
        assert_eq!(json["session_id"], "test-session");
    }

    #[test]
    fn get_history_without_system_excludes_system_message() {
        let mut agent = test_agent();
        agent.history.push(ConversationItem::Message {
            role: Role::System,
            content: "system prompt".to_string(),
            id: None,
            status: None,
        });
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "user message".to_string(),
            id: None,
            status: None,
        });
        let history = agent.get_history_without_system();
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
        let mut agent = test_agent();
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "user message".to_string(),
            id: None,
            status: None,
        });
        let history = agent.get_history_without_system();
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn get_history_without_system_empty_history() {
        let agent = test_agent();
        let history = agent.get_history_without_system();
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
    fn builder_with_session_id() {
        let agent = test_agent().with_session_id("custom-id".to_string());
        assert_eq!(agent.session_id, "custom-id");
    }

    #[test]
    fn builder_with_history() {
        let history = vec![ConversationItem::Message {
            role: Role::User,
            content: "hi".to_string(),
            id: None,
            status: None,
        }];
        let agent = test_agent().with_history(history);
        assert_eq!(agent.history.len(), 1);
    }

    #[test]
    fn stream_item_emits_function_call_output() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured_clone = captured.clone();

        let agent = test_agent().with_streaming_json(move |json| {
            captured_clone.lock().unwrap().push(json.to_string());
        });

        let item = ConversationItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: "hello world".to_string(),
        };

        agent.stream_item(&item);

        drop(agent);
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

    #[test]
    fn is_retryable_status_correctly_identifies_transient_errors() {
        assert!(is_retryable_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(is_retryable_status(
            reqwest::StatusCode::SERVICE_UNAVAILABLE
        ));

        assert!(!is_retryable_status(reqwest::StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(reqwest::StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(reqwest::StatusCode::FORBIDDEN));
        assert!(!is_retryable_status(reqwest::StatusCode::NOT_FOUND));
        assert!(!is_retryable_status(reqwest::StatusCode::OK));
        assert!(!is_retryable_status(reqwest::StatusCode::CREATED));
    }
}

/// Error handling tests using wiremock for HTTP mocking
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod error_tests {
    use super::*;
    use crate::config::model::{ApiType, ModelConfig};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Create a test agent configured to use the Responses API with a mock server URL
    fn test_agent_with_url(base_url: &str) -> Agent {
        let config = ResolvedModelConfig {
            config: ModelConfig {
                model: "test-model".to_string(),
                api_type: ApiType::Responses,
                base_url: base_url.to_string(),
                api_key_env: "TEST_API_KEY".to_string(),
                temperature: None,
                top_p: None,
                max_output_tokens: None,
                reasoning_effort: None,
                reasoning_summary: None,
                reasoning_max_tokens: None,
                providers: vec![],
            },
            api_key: "test-key".to_string(),
        };
        Agent {
            config,
            history: vec![],
            tools: vec![],
            streaming_callback: None,
            progress_callback: None,
            session_id: "test-session".to_string(),
            total_usage: Usage::default(),
            turn_count: 0,
            client: reqwest::Client::new(),
        }
    }

    /// Create a test agent configured to use the Chat Completions API with a mock server URL
    fn test_agent_chat_completions(base_url: &str) -> Agent {
        let config = ResolvedModelConfig {
            config: ModelConfig {
                model: "test-model".to_string(),
                api_type: ApiType::ChatCompletions,
                base_url: base_url.to_string(),
                api_key_env: "TEST_API_KEY".to_string(),
                temperature: None,
                top_p: None,
                max_output_tokens: None,
                reasoning_effort: None,
                reasoning_summary: None,
                reasoning_max_tokens: None,
                providers: vec![],
            },
            api_key: "test-key".to_string(),
        };
        Agent {
            config,
            history: vec![],
            tools: vec![],
            streaming_callback: None,
            progress_callback: None,
            session_id: "test-session".to_string(),
            total_usage: Usage::default(),
            turn_count: 0,
            client: reqwest::Client::new(),
        }
    }

    /// Create a successful Responses API response
    fn success_response() -> serde_json::Value {
        serde_json::json!({
            "id": "resp-123",
            "output": [
                {
                    "type": "message",
                    "id": "msg-1",
                    "status": "completed",
                    "content": [
                        {
                            "type": "output_text",
                            "text": "Hello!"
                        }
                    ]
                }
            ],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "total_tokens": 15
            }
        })
    }

    /// Create a successful Chat Completions API response
    fn success_chat_response() -> serde_json::Value {
        serde_json::json!({
            "id": "chatcmpl-123",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Hello!"
                    },
                    "finish_reason": "stop"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        })
    }

    // =========================================================================
    // HTTP Error Response Tests (Non-retryable 4xx errors)
    // =========================================================================

    #[tokio::test]
    async fn test_400_bad_request_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": {
                    "message": "Invalid request: missing required field",
                    "type": "invalid_request_error"
                }
            })))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_with_url(&mock_server.uri());
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "test".to_string(),
            id: None,
            status: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("test-model"));
    }

    #[tokio::test]
    async fn test_401_unauthorized_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": {
                    "message": "Invalid API key",
                    "type": "authentication_error"
                }
            })))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_with_url(&mock_server.uri());
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "test".to_string(),
            id: None,
            status: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("test-model"));
    }

    #[tokio::test]
    async fn test_403_forbidden_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "error": {
                    "message": "Access denied",
                    "type": "permission_error"
                }
            })))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_with_url(&mock_server.uri());
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "test".to_string(),
            id: None,
            status: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("test-model"));
    }

    #[tokio::test]
    async fn test_404_not_found_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": {
                    "message": "Model not found",
                    "type": "not_found_error"
                }
            })))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_with_url(&mock_server.uri());
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "test".to_string(),
            id: None,
            status: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("test-model"));
    }

    // =========================================================================
    // Retry Logic Tests (5xx and 429 errors should retry)
    // =========================================================================

    #[tokio::test]
    async fn test_429_too_many_requests_retries_and_succeeds() {
        let mock_server = MockServer::start().await;

        // First request returns 429
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": {
                    "message": "Rate limit exceeded",
                    "type": "rate_limit_error"
                }
            })))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        // Second request succeeds
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_response()))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_with_url(&mock_server.uri());
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "test".to_string(),
            id: None,
            status: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_ok());
        let turn_result = result.unwrap();
        assert_eq!(turn_result.items.len(), 1);
    }

    #[tokio::test]
    async fn test_500_internal_server_error_retries_and_succeeds() {
        let mock_server = MockServer::start().await;

        // First request returns 500
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": {
                    "message": "Internal server error",
                    "type": "server_error"
                }
            })))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        // Second request succeeds
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_response()))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_with_url(&mock_server.uri());
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "test".to_string(),
            id: None,
            status: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_503_service_unavailable_retries_and_succeeds() {
        let mock_server = MockServer::start().await;

        // First request returns 503
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({
                "error": {
                    "message": "Service temporarily unavailable",
                    "type": "service_unavailable"
                }
            })))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        // Second request succeeds
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_response()))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_with_url(&mock_server.uri());
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "test".to_string(),
            id: None,
            status: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_max_retries_exceeded_returns_error() {
        let mock_server = MockServer::start().await;

        // All requests return 429 (exceeds MAX_RETRIES)
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": {
                    "message": "Rate limit exceeded",
                    "type": "rate_limit_error"
                }
            })))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_with_url(&mock_server.uri());
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "test".to_string(),
            id: None,
            status: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("test-model"));
    }

    // =========================================================================
    // Chat Completions API Error Tests
    // =========================================================================

    #[tokio::test]
    async fn test_chat_completions_400_bad_request_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": {
                    "message": "Invalid request",
                    "type": "invalid_request_error"
                }
            })))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_chat_completions(&mock_server.uri());
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "test".to_string(),
            id: None,
            status: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_chat_completions_429_retries_and_succeeds() {
        let mock_server = MockServer::start().await;

        // First request returns 429
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": {
                    "message": "Rate limit exceeded",
                    "type": "rate_limit_error"
                }
            })))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        // Second request succeeds
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_chat_response()))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_chat_completions(&mock_server.uri());
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "test".to_string(),
            id: None,
            status: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_ok());
    }

    // =========================================================================
    // Successful Response Tests
    // =========================================================================

    #[tokio::test]
    async fn test_successful_responses_api_call() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_response()))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_with_url(&mock_server.uri());
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_ok());
        let turn_result = result.unwrap();
        assert_eq!(turn_result.items.len(), 1);
        assert!(matches!(&turn_result.items[0], ConversationItem::Message {
            role: Role::Assistant,
            content,
            ..
        } if content == "Hello!"));
        assert!(turn_result.usage.is_some());
        let usage = turn_result.usage.unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 5);
    }

    #[tokio::test]
    async fn test_successful_chat_completions_api_call() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_chat_response()))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_chat_completions(&mock_server.uri());
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_ok());
        let turn_result = result.unwrap();
        assert_eq!(turn_result.items.len(), 1);
        assert!(matches!(&turn_result.items[0], ConversationItem::Message {
            role: Role::Assistant,
            content,
            ..
        } if content == "Hello!"));
    }
}
