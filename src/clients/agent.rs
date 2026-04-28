use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use tokio::time::sleep;
use tracing::debug;

use crate::clients::chat_completions;
use crate::clients::responses;
use crate::clients::retry::{self, HttpFailure, RequestOverrides, RetryStatus};
use crate::clients::tools::{Tool, bash_tool, edit_tool, execute_tool, read_tool, write_tool};
use crate::clients::types::{ConversationItem, ResultSubtype, SessionRecord, Usage};
use crate::config::model::{ApiType, ResolvedModelConfig};
use crate::models::{Message, Role};

/// Callback type for streaming JSON output
type StreamingCallback = Box<dyn Fn(&str) + Send + Sync>;

/// Callback type for progress reporting (receives conversation items as they occur)
type ProgressCallback = Box<dyn Fn(&ConversationItem) + Send + Sync>;
/// Callback type for retry wait reporting
type RetryCallback = Box<dyn Fn(&RetryStatus) + Send + Sync>;

/// Result of a single API turn (one request/response cycle).
#[derive(Debug)]
pub(super) struct TurnResult {
    pub(super) items: Vec<ConversationItem>,
    pub(super) usage: Option<Usage>,
}

fn build_http_client(disable_connection_reuse: bool) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_mins(5));

    if disable_connection_reuse {
        builder = builder.pool_max_idle_per_host(0);
    }

    builder.build().unwrap_or_default()
}

// =============================================================================
// Agent (shared loop over any backend)
// =============================================================================

/// Orchestrates conversation loops, tool execution, and API communication.
///
/// The `Agent` manages the conversation history, executes tool calls from the
/// AI model, and handles streaming output. It supports multiple API backends
/// through the `ApiType` configuration.
pub struct Agent {
    config: ResolvedModelConfig,
    /// Conversation history using typed items
    history: Vec<ConversationItem>,
    tools: Vec<Tool>,
    /// Callback for streaming JSON output
    streaming_callback: Option<StreamingCallback>,
    /// Callback for human-readable progress reporting
    progress_callback: Option<ProgressCallback>,
    /// Callback for retry wait reporting
    retry_callback: Option<RetryCallback>,
    /// Session ID for tracking
    pub session_id: uuid::Uuid,
    /// Accumulated usage across all API calls
    pub total_usage: Usage,
    /// Number of API calls made
    pub turn_count: u32,
    /// Reusable HTTP client for connection pooling
    client: reqwest::Client,
    /// Buffer of all session records emitted during this run.
    /// Populated by `emit_init_message`, `stream_item`, and `emit_result_message`.
    /// Used to reconstruct the session file on save.
    stream: Vec<SessionRecord>,
    /// Maps SKILL.md paths to skill names for activation deduplication.
    /// When the Read tool targets one of these paths, the agent checks if the
    /// skill has already been activated and returns a lightweight message instead.
    skill_locations: HashMap<PathBuf, String>,
    /// Names of skills that have been activated (read) in this session.
    /// Shared between tool executions for concurrent access.
    activated_skills: Arc<Mutex<HashSet<String>>>,
}

impl Agent {
    /// Creates a new agent with the given configuration and system prompt.
    ///
    /// The agent is initialized with four default tools: Bash, Read, Edit, and Write.
    /// A new session ID is generated automatically.
    pub fn new(config: ResolvedModelConfig, system_prompt: &str) -> Self {
        let timestamp = chrono::Utc::now().to_rfc3339();
        Self {
            config,
            history: vec![ConversationItem::Message {
                role: Role::System,
                content: system_prompt.to_string(),
                id: None,
                status: None,
                timestamp: Some(timestamp),
            }],
            tools: vec![bash_tool(), edit_tool(), read_tool(), write_tool()],
            streaming_callback: None,
            progress_callback: None,
            retry_callback: None,
            session_id: uuid::Uuid::new_v4(),
            total_usage: Usage::default(),
            turn_count: 0,
            client: build_http_client(false),
            stream: Vec::new(),
            skill_locations: HashMap::new(),
            activated_skills: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Returns the model name from the configuration.
    pub fn model(&self) -> &str {
        &self.config.config.model
    }

    /// Sets the session ID for a restored session.
    ///
    /// Use this when continuing a previous session to preserve the session ID.
    pub const fn with_session_id(mut self, id: uuid::Uuid) -> Self {
        self.session_id = id;
        self
    }

    /// Sets the conversation history for a restored session.
    ///
    /// Use this when continuing a previous session to restore the conversation context.
    pub fn with_history(mut self, messages: Vec<ConversationItem>) -> Self {
        // Preserve the system message (first item set by Agent::new),
        // then append the restored conversation history.
        debug_assert!(
            !self.history.is_empty(),
            "with_history requires Agent::new() to have set a system message"
        );
        self.history.truncate(1);
        self.history.extend(messages);
        self
    }

    /// Seed the stream buffer with prior session records when resuming a session.
    ///
    /// This ensures that the full prior conversation is preserved in the stream
    /// output so it can be saved to the session file.
    ///
    /// Drops any trailing Result and leading Init records: the agent will
    /// always emit its own Init on start, and the next save writes a fresh Result.
    pub fn with_stream_records(mut self, records: Vec<SessionRecord>) -> Self {
        self.stream = records
            .into_iter()
            .filter(|r| {
                !matches!(r, SessionRecord::Result { .. })
                    && !matches!(r, SessionRecord::Init { .. })
            })
            .collect();
        self
    }

    /// Set the skill locations for deduplication.
    ///
    /// These paths are checked when the Read tool is used. If the model reads
    /// a SKILL.md file that was already read in this session, a lightweight
    /// "already activated" message is returned instead of the full content.
    pub fn with_skill_locations(mut self, locations: HashMap<PathBuf, String>) -> Self {
        self.skill_locations = locations;
        self
    }

    /// Set the initially activated skills (used when resuming a session).
    ///
    /// These skills are pre-seeded into the activated set so they are not
    /// re-read during the resumed session.
    pub fn with_activated_skills(self, skills: HashSet<String>) -> Self {
        if let Ok(mut guard) = self.activated_skills.lock() {
            *guard = skills;
        }
        self
    }

    /// Returns the names of skills that have been activated in this session.
    #[allow(dead_code)]
    pub fn activated_skills(&self) -> HashSet<String> {
        self.activated_skills
            .lock()
            .map_or_else(|_| HashSet::new(), |guard| guard.clone())
    }

    /// Drains and returns the full stream buffer including Init and any Result record.
    ///
    /// The caller is responsible for appending a Result record before draining
    /// if they want one in the output.
    pub fn drain_stream(&mut self) -> Vec<SessionRecord> {
        std::mem::take(&mut self.stream)
    }

    /// Enables streaming JSON output for each message.
    ///
    /// The callback receives a JSON string for each message, tool call, and result.
    /// This is useful for integrating with other tools or TUIs.
    pub fn with_streaming_json(mut self, callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.streaming_callback = Some(Box::new(callback));
        self
    }

    /// Enables progress reporting for tool execution.
    ///
    /// The callback receives conversation items as they occur, useful for
    /// displaying human-readable progress during long-running operations.
    pub fn with_progress_callback(
        mut self,
        callback: impl Fn(&ConversationItem) + Send + Sync + 'static,
    ) -> Self {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    /// Enables retry wait reporting.
    pub fn with_retry_callback(
        mut self,
        callback: impl Fn(&RetryStatus) + Send + Sync + 'static,
    ) -> Self {
        self.retry_callback = Some(Box::new(callback));
        self
    }

    /// Report a conversation item via the progress callback, if set.
    fn report_progress(&self, item: &ConversationItem) {
        if let Some(ref callback) = self.progress_callback {
            callback(item);
        }
    }

    /// Report a retry status via the retry callback, if set.
    fn report_retry(&self, status: &RetryStatus) {
        if let Some(ref callback) = self.retry_callback {
            callback(status);
        }
    }

    /// Stream a session record via the streaming callback, if set.
    /// Also appends the record to the internal stream buffer.
    fn stream_record(&mut self, record: SessionRecord) {
        if let Some(ref callback) = self.streaming_callback
            && let Ok(json) = serde_json::to_string(&record.to_streaming_json())
        {
            callback(&json);
        }
        self.stream.push(record);
    }

    /// Stream a conversation item as JSON via the streaming callback, if set.
    fn stream_item(&mut self, item: &ConversationItem) {
        self.stream_record(SessionRecord::from_conversation_item(item));
    }

    /// Emit the init message with session info, cwd, and tools.
    /// Appends an Init record to the stream buffer.
    pub fn emit_init_message(&mut self) {
        let cwd: PathBuf = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let tools: Vec<String> = self.tools.iter().map(|tool| tool.name.clone()).collect();

        let record = SessionRecord::Init {
            format_version: 3,
            session_id: self.session_id.to_string(),
            timestamp: Utc::now(),
            working_directory: cwd,
            model: Some(self.config.config.model.clone()),
            tools,
        };

        self.stream_record(record);
    }

    /// Accumulate usage from an API turn
    const fn accumulate_usage(&mut self, turn_usage: Option<&Usage>) {
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

    /// Emit the result message with success/error, duration, usage stats.
    /// Appends a Result record to the stream buffer.
    pub fn emit_result_message(
        &mut self,
        success: bool,
        duration_ms: u64,
        result_text: Option<String>,
        error_message: Option<String>,
    ) {
        let subtype = if success {
            ResultSubtype::Success
        } else {
            ResultSubtype::ErrorDuringExecution
        };

        let record = SessionRecord::Result {
            subtype,
            success,
            is_error: !success,
            duration_ms,
            turn_count: self.turn_count,
            num_turns: self.turn_count,
            session_id: self.session_id.to_string(),
            result: result_text,
            error: error_message,
            usage: self.total_usage.clone(),
            permission_denials: None,
        };

        self.stream_record(record);
    }

    /// Sends a message and runs the agent loop until completion.
    ///
    /// The agent will process the message, execute any tool calls, and continue
    /// until the model produces a final response without requesting more tools.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails, the response cannot be parsed,
    /// or a tool execution fails critically.
    pub async fn send(&mut self, message: Message) -> anyhow::Result<Option<Message>> {
        let user_item = ConversationItem::Message {
            role: Role::User,
            content: message.content.clone(),
            id: None,
            status: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        };

        // Stream user message before updating history so callers see it immediately.
        self.stream_item(&user_item);
        self.history.push(user_item);

        // Agent loop: continue until model stops making tool calls
        loop {
            let turn_result = self.complete_turn().await?;

            // Accumulate usage
            self.accumulate_usage(turn_result.usage.as_ref());

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
                        ..
                    } = item
                    {
                        Some((id.clone(), call_id.clone(), name.clone(), arguments.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            // Stream each item as JSON if callback is set
            for item in &turn_result.items {
                self.stream_item(item);
            }

            // Report progress for items, but skip assistant messages on the final turn
            // (they're already printed to stdout, so we'd duplicate)
            let has_tool_calls = !function_calls.is_empty();
            for item in &turn_result.items {
                // Skip assistant messages on the final turn (no tool calls)
                if !has_tool_calls
                    && matches!(
                        item,
                        ConversationItem::Message {
                            role: Role::Assistant,
                            ..
                        }
                    )
                {
                    continue;
                }
                self.report_progress(item);
            }

            // Move items into history
            self.history.extend(turn_result.items);

            // If no function calls, resolve and return the message
            if function_calls.is_empty() {
                return Ok(Some(resolve_assistant_message(&self.history)));
            }

            // Execute tool calls concurrently
            let skill_locations = self.skill_locations.clone();
            let activated_skills = Arc::clone(&self.activated_skills);
            let futures = function_calls
                .iter()
                .map(|(_id, call_id, name, arguments)| {
                    let call_id = call_id.clone();
                    let name = name.clone();
                    let arguments = arguments.clone();
                    let skill_locations = skill_locations.clone();
                    let activated_skills = Arc::clone(&activated_skills);
                    async move {
                        let result = execute_tool_with_skill_dedup(
                            &name,
                            &arguments,
                            &skill_locations,
                            &activated_skills,
                        )
                        .await;
                        (call_id, result)
                    }
                });

            let results = futures::future::join_all(futures).await;

            // Add results to history in order
            for (call_id, output) in results {
                let timestamp = chrono::Utc::now().to_rfc3339();
                let item = ConversationItem::FunctionCallOutput {
                    call_id,
                    output,
                    timestamp: Some(timestamp),
                };
                self.stream_item(&item);
                self.history.push(item);
            }

            // Loop continues - send next request with tool results included
        }
    }

    /// Execute a single API turn with retry logic.
    async fn complete_turn(&mut self) -> anyhow::Result<TurnResult> {
        let mut attempt = 1;
        let mut request_overrides = RequestOverrides {
            max_output_tokens: self.config.config.max_output_tokens,
            reasoning_max_tokens: self.config.config.reasoning_max_tokens,
            context_overflow_retry_used: false,
        };
        let mut disable_connection_reuse = false;

        loop {
            let request_result = match self.config.config.api_type {
                ApiType::Responses => {
                    responses::send_request(
                        &self.client,
                        &self.config,
                        &self.history,
                        &self.tools,
                        &request_overrides,
                    )
                    .await
                },
                ApiType::ChatCompletions => {
                    chat_completions::send_request(
                        &self.client,
                        &self.config,
                        &self.history,
                        &self.tools,
                        &request_overrides,
                    )
                    .await
                },
            };

            match request_result {
                Ok(response) => {
                    if response.status().is_success() {
                        if disable_connection_reuse {
                            self.client = build_http_client(false);
                        }

                        return match self.config.config.api_type {
                            ApiType::Responses => responses::parse_response(response).await,
                            ApiType::ChatCompletions => {
                                chat_completions::parse_response(response).await
                            },
                        };
                    }

                    let failure = HttpFailure {
                        status: response.status().as_u16(),
                        headers: response.headers().clone(),
                        body: response.text().await?,
                    };

                    match retry::classify_http_failure(
                        &failure,
                        attempt,
                        self.session_id,
                        &request_overrides,
                    ) {
                        retry::RetryDecision::Retry { status } => {
                            self.wait_for_retry(&status).await;
                            attempt += 1;
                        },
                        retry::RetryDecision::RetryWithOverrides { status, overrides } => {
                            request_overrides = overrides;
                            self.wait_for_retry(&status).await;
                            attempt += 1;
                        },
                        retry::RetryDecision::DoNotRetry => {
                            return Err(api_error_from_failure(
                                &self.config.config.model,
                                &failure,
                            )
                            .into());
                        },
                    }
                },
                Err(error) => {
                    match retry::classify_transport_error(&error, attempt, self.session_id) {
                        retry::RetryDecision::Retry { status } => {
                            if retry::should_disable_connection_reuse(&error)
                                && !disable_connection_reuse
                            {
                                self.client = build_http_client(true);
                                disable_connection_reuse = true;
                            }

                            self.wait_for_retry(&status).await;
                            attempt += 1;
                        },
                        retry::RetryDecision::RetryWithOverrides { status, overrides } => {
                            request_overrides = overrides;
                            self.wait_for_retry(&status).await;
                            attempt += 1;
                        },
                        retry::RetryDecision::DoNotRetry => return Err(error),
                    }
                },
            }
        }
    }

    async fn wait_for_retry(&self, status: &RetryStatus) {
        self.report_retry(status);
        debug!(
            target: "cake",
            reason = ?status.reason,
            detail = %status.detail,
            delay_ms = status.delay.as_millis(),
            attempt = status.attempt,
            max_attempts = status.max_retries,
            "Retrying API request"
        );

        if !status.delay.is_zero() {
            sleep(status.delay).await;
        }
    }
}

async fn execute_tool_output(name: &str, arguments: &str) -> String {
    match execute_tool(name, arguments).await {
        Ok(result) => result.output,
        Err(error) => format!("Error: {error}"),
    }
}

async fn execute_tool_with_skill_dedup(
    name: &str,
    arguments: &str,
    skill_locations: &HashMap<PathBuf, String>,
    activated_skills: &Arc<Mutex<HashSet<String>>>,
) -> String {
    if name != "Read" {
        return execute_tool_output(name, arguments).await;
    }

    let Some(path_str) = crate::clients::tools::read::extract_path(arguments) else {
        return execute_tool_output(name, arguments).await;
    };

    let Ok(path) = PathBuf::from(&path_str).canonicalize() else {
        return execute_tool_output(name, arguments).await;
    };

    let Some(skill_name) = skill_locations.get(&path) else {
        return execute_tool_output(name, arguments).await;
    };

    let already_active = activated_skills
        .lock()
        .is_ok_and(|guard| guard.contains(skill_name));
    if already_active {
        tracing::info!("Skill '{skill_name}' already activated, skipping re-read");
        return format!(
            "Skill '{skill_name}' is already active in this session. \
             Its instructions are already in the conversation context."
        );
    }

    let result = execute_tool_output(name, arguments).await;
    if let Ok(mut guard) = activated_skills.lock() {
        guard.insert(skill_name.clone());
    }
    tracing::info!("Skill '{}' activated", skill_name);
    result
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

fn api_error_from_failure(model: &str, failure: &HttpFailure) -> crate::exit_code::ApiError {
    debug!(target: "cake", "{}", failure.body);

    crate::exit_code::ApiError {
        status: failure.status,
        body: format_api_error_body(model, &failure.body),
    }
}

fn format_api_error_body(model: &str, error_text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(error_text).map_or_else(
        |_err| format!("{model}\n\n{error_text}"),
        |resp_json| {
            serde_json::to_string_pretty(&resp_json).map_or_else(
                |_| format!("{model}\n\n{error_text}"),
                |formatted| format!("{model}\n\n{formatted}"),
            )
        },
    )
}

#[cfg(test)]
fn test_resolved_model_config(api_type: ApiType, base_url: &str) -> ResolvedModelConfig {
    ResolvedModelConfig {
        config: crate::config::model::ModelConfig {
            model: "test-model".to_string(),
            api_type,
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
    }
}

#[cfg(test)]
fn test_agent_for(api_type: ApiType, base_url: &str) -> Agent {
    let mut agent = Agent::new(
        test_resolved_model_config(api_type, base_url),
        "test system prompt",
    );
    agent.session_id = uuid::uuid!("550e8400-e29b-41d4-a716-446655440000");
    agent.tools = vec![];
    agent
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::clients::types::{InputTokensDetails, OutputTokensDetails};
    use crate::config::model::ApiType;

    fn test_agent() -> Agent {
        test_agent_for(ApiType::ChatCompletions, "https://api.example.com")
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
        let mut agent = test_agent().with_streaming_json(move |json| {
            *captured_clone.lock().unwrap() = json.to_string();
        });
        agent.emit_result_message(true, 1000, None, None);
        drop(agent);
        let json: serde_json::Value = serde_json::from_str(&captured.lock().unwrap()).unwrap();
        assert_eq!(json["type"], "result");
        assert_eq!(json["subtype"], "success");
        assert_eq!(json["success"], true);
        assert_eq!(json["is_error"], false);
        assert_eq!(json["duration_ms"], 1000);
    }

    #[test]
    fn emit_result_message_error() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured_clone = captured.clone();
        let mut agent = test_agent().with_streaming_json(move |json| {
            *captured_clone.lock().unwrap() = json.to_string();
        });
        agent.emit_result_message(false, 500, None, Some("boom".to_string()));
        drop(agent);
        let json: serde_json::Value = serde_json::from_str(&captured.lock().unwrap()).unwrap();
        assert_eq!(json["subtype"], "error_during_execution");
        assert_eq!(json["error"], "boom");
        assert_eq!(json["is_error"], true);
    }

    #[test]
    fn emit_result_message_no_callback() {
        let mut agent = test_agent();
        agent.emit_result_message(true, 1000, None, None);
    }

    #[test]
    fn emit_init_message() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured_clone = captured.clone();
        let mut agent = test_agent().with_streaming_json(move |json| {
            *captured_clone.lock().unwrap() = json.to_string();
        });
        agent.emit_init_message();
        drop(agent);
        let json: serde_json::Value = serde_json::from_str(&captured.lock().unwrap()).unwrap();
        assert_eq!(json["type"], "init");
        assert_eq!(json["session_id"], "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(json["format_version"], 3);
    }

    #[test]
    fn resolve_assistant_message_with_assistant_message() {
        let items = vec![ConversationItem::Message {
            role: Role::Assistant,
            content: "Hello!".to_string(),
            id: Some("msg-1".to_string()),
            status: Some("completed".to_string()),
            timestamp: None,
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
            timestamp: None,
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
            timestamp: None,
        }];
        let msg = resolve_assistant_message(&items);
        assert_eq!(
            msg.content,
            "The model's response was incomplete. No final message was received."
        );
    }

    #[test]
    fn builder_with_session_id() {
        let id = uuid::uuid!("6ba7b810-9dad-11d1-80b4-00c04fd430c8");
        let agent = test_agent().with_session_id(id);
        assert_eq!(agent.session_id, id);
    }

    #[test]
    fn builder_with_history() {
        let history = vec![ConversationItem::Message {
            role: Role::User,
            content: "hi".to_string(),
            id: None,
            status: None,
            timestamp: None,
        }];
        let agent = test_agent().with_history(history);
        // 1 system message (from test_agent) + 1 user message from with_history
        assert_eq!(agent.history.len(), 2);
        assert!(matches!(
            &agent.history[0],
            ConversationItem::Message {
                role: Role::System,
                ..
            }
        ));
        assert!(matches!(
            &agent.history[1],
            ConversationItem::Message {
                role: Role::User,
                ..
            }
        ));
    }

    #[test]
    fn stream_item_emits_function_call_output() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured_clone = captured.clone();

        let mut agent = test_agent().with_streaming_json(move |json| {
            captured_clone.lock().unwrap().push(json.to_string());
        });

        let item = ConversationItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: "hello world".to_string(),
            timestamp: None,
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
}

/// Error handling tests using wiremock for HTTP mocking
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod error_tests {
    use super::*;
    use crate::config::model::ApiType;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Create a test agent configured to use the Responses API with a mock server URL
    fn test_agent_with_url(base_url: &str) -> Agent {
        test_agent_for(ApiType::Responses, base_url)
    }

    /// Create a test agent configured to use the Chat Completions API with a mock server URL
    fn test_agent_chat_completions(base_url: &str) -> Agent {
        test_agent_for(ApiType::ChatCompletions, base_url)
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
            timestamp: None,
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
            timestamp: None,
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
            timestamp: None,
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
            timestamp: None,
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
            timestamp: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_ok());
        let turn_result = result.unwrap();
        assert_eq!(turn_result.items.len(), 1);
    }

    #[tokio::test]
    async fn test_429_retry_after_header_is_honored() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", "1")
                    .set_body_json(serde_json::json!({
                        "error": {
                            "message": "Rate limit exceeded",
                            "type": "rate_limit_error"
                        }
                    })),
            )
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_response()))
            .mount(&mock_server)
            .await;

        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = Arc::clone(&captured);
        let mut agent =
            test_agent_with_url(&mock_server.uri()).with_retry_callback(move |status| {
                captured_clone.lock().unwrap().push(status.clone());
            });
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "test".to_string(),
            id: None,
            status: None,
            timestamp: None,
        });

        let start = Instant::now();
        let result = agent.complete_turn().await;
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        assert!(elapsed >= Duration::from_millis(900));
        let status = {
            let statuses = captured.lock().unwrap();
            assert_eq!(statuses.len(), 1);
            statuses[0].clone()
        };
        assert_eq!(status.delay, Duration::from_secs(1));
        assert_eq!(status.detail, "429 rate limit");
        assert_eq!(status.attempt, 2);
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
            timestamp: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_502_bad_gateway_retries_and_succeeds() {
        let mock_server = MockServer::start().await;

        // First request returns 502
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(502).set_body_json(serde_json::json!({
                "error": {
                    "message": "Bad gateway",
                    "type": "bad_gateway"
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
            timestamp: None,
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
            timestamp: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_503_x_should_retry_false_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(
                ResponseTemplate::new(503)
                    .insert_header("x-should-retry", "false")
                    .set_body_json(serde_json::json!({
                        "error": {
                            "message": "Service temporarily unavailable",
                            "type": "server_error"
                        }
                    })),
            )
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

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
            timestamp: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_529_overloaded_retries_and_succeeds() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(529).set_body_json(serde_json::json!({
                "error": {
                    "message": "Provider overloaded",
                    "type": "server_error"
                }
            })))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

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
            timestamp: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_overloaded_error_body_retries_and_succeeds() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": {
                    "message": "provider overloaded",
                    "type": "overloaded_error"
                }
            })))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

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
            timestamp: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_504_gateway_timeout_retries_and_succeeds() {
        let mock_server = MockServer::start().await;

        // First request returns 504
        Mock::given(method("POST"))
            .and(path("/responses"))
            .respond_with(ResponseTemplate::new(504).set_body_json(serde_json::json!({
                "error": {
                    "message": "Gateway timeout",
                    "type": "gateway_timeout"
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
            timestamp: None,
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
            timestamp: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("test-model"));
    }

    #[tokio::test]
    async fn test_context_overflow_reduces_max_output_tokens_once() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .and(body_partial_json(serde_json::json!({
                "max_output_tokens": 5000,
                "reasoning": {
                    "max_tokens": 4000
                }
            })))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": {
                    "message": "input length and max_tokens exceed context limit: 12000 + 5000 > 16384",
                    "type": "invalid_request_error"
                }
            })))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/responses"))
            .and(body_partial_json(serde_json::json!({
                "max_output_tokens": 3360,
                "reasoning": {
                    "max_tokens": 3359
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_response()))
            .mount(&mock_server)
            .await;

        let mut agent = test_agent_with_url(&mock_server.uri());
        agent.config.config.max_output_tokens = Some(5000);
        agent.config.config.reasoning_max_tokens = Some(4000);
        agent.history.push(ConversationItem::Message {
            role: Role::User,
            content: "test".to_string(),
            id: None,
            status: None,
            timestamp: None,
        });

        let result = agent.complete_turn().await;
        assert!(result.is_ok());
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
            timestamp: None,
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
            timestamp: None,
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
            timestamp: None,
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
            timestamp: None,
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
