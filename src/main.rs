//! acai - AI coding assistant CLI

mod cli;
mod clients;
mod config;
mod logger;
mod models;
mod prompts;

use std::time::Instant;

use crate::cli::CmdRunner;
use crate::clients::{Agent, ConversationItem, set_additional_dirs};
use std::collections::HashMap;
use std::io::Write;

use crate::config::{
    AgentsFile, DataDir, ModelConfig, ModelDefinition, ResolvedModelConfig, Session,
    SettingsLoader, worktree,
};
use crate::models::{Message, Role};
use crate::prompts::build_system_prompt;
use clap::{Parser, ValueEnum};
use tracing::info;

/// Output format for the response
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Plain text output
    #[default]
    Text,
    /// Stream each message as JSON as it's received
    StreamJson,
}

/// AI coding assistant CLI
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct CodingAssistant {
    /// The prompt to send to the AI (use `-` to read from stdin)
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,

    /// Sets the max tokens value
    #[arg(long)]
    pub max_tokens: Option<u32>,

    /// Output format for the response (text or stream-json)
    #[arg(long, value_enum, default_value = "text")]
    pub output_format: OutputFormat,

    /// Continue the most recent session for this directory
    #[arg(long = "continue")]
    pub continue_session: bool,

    /// Resume a specific session by UUID
    #[arg(long, value_name = "UUID")]
    pub resume: Option<String>,

    /// Fork a session: copy its history into a new session with a fresh ID.
    /// Use without a value to fork the latest session, or provide a UUID.
    #[arg(long, num_args = 0..=1, default_missing_value = "", value_name = "UUID")]
    pub fork: Option<String>,

    /// Do not save the session to disk
    #[arg(long)]
    pub no_session: bool,

    /// Run in an isolated git worktree (optionally provide a name)
    #[arg(short, long, num_args = 0..=1, default_missing_value = "", value_name = "NAME")]
    pub worktree: Option<String>,

    /// Select a model by name from settings.toml
    #[arg(long)]
    pub model: Option<String>,

    /// Show tool call progress on stderr (only applies to text output format)
    #[arg(long)]
    pub verbose: bool,

    /// Override reasoning effort level (none, low, medium, high, xhigh)
    #[arg(long, value_name = "EFFORT")]
    pub reasoning_effort: Option<String>,

    /// Override reasoning token budget
    #[arg(long, value_name = "TOKENS")]
    pub reasoning_budget: Option<u32>,

    /// Add a directory to the sandbox config (read-only access). Can be repeated.
    #[arg(long, value_name = "DIR")]
    pub add_dir: Vec<String>,
}

impl CodingAssistant {
    /// Read content from stdin if available (non-terminal)
    ///
    /// This function only reads from stdin if:
    /// 1. Stdin is not a terminal (i.e., piped input)
    /// 2. There is data available to read within a short timeout
    ///
    /// This prevents hanging when stdin is a pipe but no data is being sent,
    /// which can happen when the CLI is invoked from another process (e.g.,
    /// from a TUI or another CLI instance) that doesn't close stdin properly.
    fn read_stdin_content() -> Option<String> {
        use std::io::IsTerminal;
        use std::sync::mpsc;
        use std::thread;
        use std::time::Duration;

        if std::io::stdin().is_terminal() {
            return None;
        }

        // Use a thread with a timeout to read from stdin.
        // This prevents hanging when stdin is a pipe but no data is being sent.
        let (tx, rx) = mpsc::channel();
        let thread = thread::spawn(move || {
            let stdin = std::io::stdin();
            match std::io::read_to_string(stdin) {
                Ok(s) if !s.is_empty() => {
                    let _ = tx.send(Some(s));
                },
                _ => {
                    let _ = tx.send(None);
                },
            }
        });

        // Wait for stdin with a short timeout (100ms)
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Some(content)) => {
                // Wait for the thread to finish
                let _ = thread.join();
                Some(content)
            },
            Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = thread.join();
                None
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // No data available within timeout - assume stdin is empty
                // The thread will continue running in the background but we don't wait for it
                None
            },
        }
    }

    /// Build the final content from prompt and stdin according to codex-style rules
    fn build_content(
        prompt: Option<&str>,
        stdin_content: Option<String>,
    ) -> anyhow::Result<String> {
        let stdin_content = stdin_content.filter(|s| !s.is_empty());

        match (prompt, stdin_content) {
            (Some("-"), None) => Err(anyhow::anyhow!("No input provided via stdin")),
            (Some("-") | None, Some(stdin)) => Ok(stdin),
            (Some(prompt), Some(stdin)) => Ok(format!("{prompt}\n\n{stdin}")),
            (Some(prompt), None) => Ok(prompt.to_string()),
            (None, None) => Err(anyhow::anyhow!(
                "No input provided. Provide a prompt as an argument, use 'acai -' for stdin, or pipe input to acai."
            )),
        }
    }

    /// Resolve the effective `ModelConfig`, applying CLI overrides.
    fn resolve_model_config(
        &self,
        settings: &HashMap<String, ModelDefinition>,
    ) -> anyhow::Result<ModelConfig> {
        let mut config = if let Some(ref model_name) = self.model {
            // Validate model name format
            if let Err(e) = ModelDefinition::validate_name(model_name) {
                anyhow::bail!(
                    "Invalid model name '{model_name}': {e}. Model names must contain only lowercase letters, numbers, and hyphens."
                );
            }

            // Look up the model in settings
            if let Some(def) = settings.get(model_name) {
                def.to_model_config()
            } else {
                let available: Vec<_> = settings.keys().cloned().collect();
                let available_str = if available.is_empty() {
                    String::new()
                } else {
                    format!(": {}", available.join(", "))
                };
                anyhow::bail!(
                    "Unknown model '{model_name}'{available_str}.- Use a model name from settings.toml, or omit --model to use the default."
                );
            }
        } else {
            // Use default config (current behavior)
            ModelConfig::default()
        };

        // Apply CLI overrides
        if let Some(max_tokens) = self.max_tokens {
            config.max_output_tokens = Some(max_tokens);
        }
        if let Some(ref effort) = self.reasoning_effort {
            config.reasoning_effort = Some(effort.clone());
        }
        if let Some(budget) = self.reasoning_budget {
            config.reasoning_max_tokens = Some(budget);
        }

        Ok(config)
    }

    fn build_client_and_session(
        &self,
        data_dir: &DataDir,
        current_dir: std::path::PathBuf,
        agents_files: &[AgentsFile],
        settings: &HashMap<String, ModelDefinition>,
    ) -> anyhow::Result<(Agent, Session)> {
        let system_prompt = build_system_prompt(&current_dir, agents_files);
        let model_config = self.resolve_model_config(settings)?;
        let resolved = ResolvedModelConfig::resolve(model_config)?;

        if self.continue_session {
            let restored = data_dir
                .load_latest_session(&current_dir)?
                .ok_or_else(|| anyhow::anyhow!("No previous session found for this directory"))?;
            let agent = Agent::new(resolved, &system_prompt)
                .with_session_id(restored.id.clone())
                .with_history(restored.messages.clone());
            Ok((agent, restored))
        } else if let Some(ref uuid) = self.resume {
            uuid::Uuid::parse_str(uuid)
                .map_err(|e| anyhow::anyhow!("Invalid session UUID '{uuid}': {e}"))?;
            let restored = data_dir
                .load_session(&current_dir, uuid)?
                .ok_or_else(|| anyhow::anyhow!("Session {uuid} not found in this directory"))?;
            let agent = Agent::new(resolved, &system_prompt)
                .with_session_id(restored.id.clone())
                .with_history(restored.messages.clone());
            Ok((agent, restored))
        } else if let Some(ref fork_id) = self.fork {
            let restored = if fork_id.is_empty() {
                data_dir.load_latest_session(&current_dir)?.ok_or_else(|| {
                    anyhow::anyhow!("No previous session found for this directory")
                })?
            } else {
                uuid::Uuid::parse_str(fork_id)
                    .map_err(|e| anyhow::anyhow!("Invalid session UUID '{fork_id}': {e}"))?;
                data_dir
                    .load_session(&current_dir, fork_id)?
                    .ok_or_else(|| {
                        anyhow::anyhow!("Session {fork_id} not found in this directory")
                    })?
            };
            let agent = Agent::new(resolved, &system_prompt).with_history(restored.messages);
            let s = Session::new(agent.session_id.clone(), current_dir);
            Ok((agent, s))
        } else {
            let agent = Agent::new(resolved, &system_prompt);
            let s = Session::new(agent.session_id.clone(), current_dir);
            Ok((agent, s))
        }
    }

    /// Set up a worktree if `--worktree` was provided.
    fn setup_worktree(
        &self,
        original_dir: &std::path::Path,
    ) -> anyhow::Result<Option<worktree::Worktree>> {
        let Some(ref wt_name) = self.worktree else {
            return Ok(None);
        };

        let name = if wt_name.is_empty() {
            None
        } else {
            Some(wt_name.as_str())
        };

        let wt = worktree::create(original_dir, name)?;
        eprintln!("Working in worktree '{}' ({})", wt.name, wt.path.display());
        std::env::set_current_dir(&wt.path)
            .map_err(|e| anyhow::anyhow!("Failed to cd into worktree: {e}"))?;
        Ok(Some(wt))
    }

    /// Clean up a worktree after the session ends.
    fn cleanup_worktree(wt: &worktree::Worktree, original_dir: &std::path::Path) {
        if let Err(e) = std::env::set_current_dir(original_dir) {
            tracing::warn!(
                "Failed to restore original directory '{}': {e}",
                original_dir.display()
            );
        }

        match worktree::has_changes(&wt.path) {
            Ok(false) => {
                eprintln!("No changes in worktree '{}', removing.", wt.name);
                if let Err(e) = worktree::remove(original_dir, &wt.name, false) {
                    tracing::warn!("Failed to clean up worktree '{}': {e}", wt.name);
                }
            },
            Ok(true) => {
                eprintln!(
                    "Worktree '{}' has changes, keeping at {}",
                    wt.name,
                    wt.path.display()
                );
            },
            Err(e) => {
                tracing::warn!(
                    "Could not check worktree '{}' for changes, keeping it: {e}",
                    wt.name
                );
            },
        }
    }
}

impl CmdRunner for CodingAssistant {
    async fn run(&self, data_dir: &DataDir) -> anyhow::Result<()> {
        let original_dir = std::env::current_dir()?;
        let wt = self.setup_worktree(&original_dir)?;

        let stdin_content = Self::read_stdin_content();
        let content = Self::build_content(self.prompt.as_deref(), stdin_content)?;

        let current_dir = std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get current directory: {e}"))?;

        // Load settings from TOML files
        let settings = SettingsLoader::load(Some(&current_dir), &data_dir.get_cache_dir())?;

        let agents_files = data_dir.read_agents_files(&current_dir);

        let (mut client, mut session) =
            self.build_client_and_session(data_dir, current_dir, &agents_files, &settings)?;

        if self.output_format == OutputFormat::StreamJson {
            client = client.with_streaming_json(|json| {
                println!("{json}");
            });
        }

        let verbose = self.verbose && self.output_format == OutputFormat::Text;

        if verbose {
            let model = client.model().to_string();
            let tool_count = client.tool_count();
            let start_time = Instant::now();

            client = client.with_progress_callback(move |item| {
                let elapsed = start_time.elapsed().as_secs_f64();
                let line = format_progress_item(item, elapsed);
                if !line.is_empty() {
                    let _ = writeln!(std::io::stderr(), "{line}");
                }
            });

            eprintln!(
                "\x1b[1;36m--\x1b[0m dir: {}, session: {}, model: {model}, tools: {tool_count}",
                original_dir.display(),
                session.id
            );
        }

        let msg = Message {
            role: Role::User,
            content,
        };

        let start = Instant::now();

        client.emit_init_message();

        let result: anyhow::Result<Option<Message>> = client.send(msg).await;

        #[allow(clippy::cast_possible_truncation)]
        let duration_ms = start.elapsed().as_millis() as u64;

        if self.output_format == OutputFormat::StreamJson {
            match &result {
                Ok(_) => {
                    client.emit_result_message(true, duration_ms, None);
                },
                Err(e) => {
                    client.emit_result_message(false, duration_ms, Some(e.to_string().as_ref()));
                },
            }
        }

        if verbose {
            #[allow(clippy::cast_precision_loss)]
            let secs = duration_ms as f64 / 1000.0;
            let turns = client.turn_count;
            let tokens = client.total_usage.total_tokens;
            eprintln!("\x1b[1;36m--\x1b[0m done: {secs:.1}s, {turns} turns, {tokens} tokens");
        }

        if !self.no_session {
            session.messages = client.get_history_without_system();
            session.model = Some(client.model().to_string());
            if let Err(e) = data_dir.save_session(&session) {
                tracing::error!("Failed to save session: {e}");
            }
        }

        let response = result?;

        if self.output_format == OutputFormat::Text {
            if let Some(response_msg) = response {
                println!("{}", response_msg.content);
            } else {
                eprintln!(
                    "Warning: No response received from the model. The task may be incomplete."
                );
            }
        }

        if let Some(ref wt) = wt {
            Self::cleanup_worktree(wt, &original_dir);
        }

        Ok(())
    }
}

/// Format a conversation item as a human-readable progress line for verbose mode.
fn format_progress_item(item: &ConversationItem, elapsed_secs: f64) -> String {
    // ANSI: dim white for timestamp, bold cyan for separator, default for content
    let ts = format!("\x1b[2m[{elapsed_secs:>6.1}s]\x1b[0m");

    match item {
        ConversationItem::FunctionCall {
            name, arguments, ..
        } => {
            let summary = clients::summarize_tool_args(name, arguments);
            format!("{ts} \x1b[1;36m>\x1b[0m {name}: {summary}")
        },
        // Skip message, reasoning, and function output items
        _ => String::new(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir = DataDir::new()?;

    let _ = logger::configure(&data_dir.get_cache_dir());

    info!("data dir: {}", data_dir.get_cache_dir().display());

    let args = CodingAssistant::parse();

    // Process --add-dir flags and set them in thread-local storage
    let additional_dirs: Vec<std::path::PathBuf> = args
        .add_dir
        .iter()
        .filter_map(|dir| {
            let path = std::path::PathBuf::from(dir);
            if path.exists() && path.is_dir() {
                Some(path)
            } else {
                tracing::warn!(
                    "--add-dir path '{dir}' does not exist or is not a directory, ignoring"
                );
                None
            }
        })
        .collect();
    set_additional_dirs(additional_dirs);

    args.run(&data_dir).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::ApiType;

    #[test]
    fn test_cli_parsing_positional_prompt() {
        let args = CodingAssistant::parse_from(["acai", "test prompt"]);
        assert_eq!(args.prompt, Some("test prompt".to_string()));
    }

    #[test]
    fn test_cli_parsing_dash_for_stdin() {
        let args = CodingAssistant::parse_from(["acai", "-"]);
        assert_eq!(args.prompt, Some("-".to_string()));
    }

    #[test]
    fn test_cli_parsing_no_prompt() {
        let args = CodingAssistant::parse_from(["acai"]);
        assert_eq!(args.prompt, None);
    }

    #[test]
    fn test_cli_parsing_model_flag() {
        let args = CodingAssistant::parse_from(["acai", "--model", "claude", "test prompt"]);
        assert_eq!(args.model, Some("claude".to_string()));
        assert_eq!(args.prompt, Some("test prompt".to_string()));
    }

    #[test]
    fn test_cli_parsing_model_flag_without_prompt() {
        let args = CodingAssistant::parse_from(["acai", "--model", "deepseek"]);
        assert_eq!(args.model, Some("deepseek".to_string()));
        assert_eq!(args.prompt, None);
    }

    #[test]
    fn test_cli_parsing_no_model_flag() {
        let args = CodingAssistant::parse_from(["acai", "test prompt"]);
        assert_eq!(args.model, None);
    }

    #[test]
    fn test_cli_parsing_add_dir_single() {
        let args =
            CodingAssistant::parse_from(["acai", "--add-dir", "/path/to/dir", "test prompt"]);
        assert_eq!(args.add_dir, vec!["/path/to/dir"]);
        assert_eq!(args.prompt, Some("test prompt".to_string()));
    }

    #[test]
    fn test_cli_parsing_add_dir_multiple() {
        let args = CodingAssistant::parse_from([
            "acai",
            "--add-dir",
            "/path/to/dir1",
            "--add-dir",
            "/path/to/dir2",
            "test prompt",
        ]);
        assert_eq!(args.add_dir, vec!["/path/to/dir1", "/path/to/dir2"]);
    }

    #[test]
    fn test_cli_parsing_add_dir_none() {
        let args = CodingAssistant::parse_from(["acai", "test prompt"]);
        assert!(args.add_dir.is_empty());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_resolve_model_config_default() {
        let args = CodingAssistant::parse_from(["acai", "test prompt"]);
        let settings = HashMap::new();
        let config = args.resolve_model_config(&settings).unwrap();
        assert_eq!(config.model, "glm-5");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_resolve_model_config_unknown_model() {
        let mut args = CodingAssistant::parse_from(["acai", "test prompt"]);
        args.model = Some("nonexistent".to_string());

        let settings = HashMap::new();
        let result = args.resolve_model_config(&settings);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown model 'nonexistent'"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_resolve_model_config_invalid_name_format() {
        let mut args = CodingAssistant::parse_from(["acai", "test prompt"]);
        args.model = Some("Invalid Name!".to_string());

        let settings = HashMap::new();
        let result = args.resolve_model_config(&settings);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid model name 'Invalid Name!'"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_resolve_model_config_from_settings() {
        let args = CodingAssistant::parse_from(["acai", "--model", "claude", "test"]);

        let mut settings = HashMap::new();
        settings.insert(
            "claude".to_string(),
            ModelDefinition {
                name: "claude".to_string(),
                model: "anthropic/claude-3-sonnet".to_string(),
                base_url: "https://openrouter.ai/api/v1/".to_string(),
                api_key_env: "OPENROUTER_API_KEY".to_string(),
                api_type: ApiType::Responses,
                temperature: Some(0.7),
                top_p: Some(0.9),
                max_output_tokens: Some(8000),
                reasoning_effort: None,
                reasoning_summary: None,
                reasoning_max_tokens: None,
                providers: vec![],
            },
        );

        let config = args.resolve_model_config(&settings).unwrap();
        assert_eq!(config.model, "anthropic/claude-3-sonnet");
        assert_eq!(config.api_type, ApiType::Responses);
        assert_eq!(config.temperature, Some(0.7));
        assert_eq!(config.top_p, Some(0.9));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_build_content_prompt_only() {
        let result = CodingAssistant::build_content(Some("hello"), None);
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_build_content_stdin_only() {
        let result = CodingAssistant::build_content(None, Some("stdin content".to_string()));
        assert_eq!(result.unwrap(), "stdin content");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_build_content_dash_with_stdin() {
        let result = CodingAssistant::build_content(Some("-"), Some("stdin content".to_string()));
        assert_eq!(result.unwrap(), "stdin content");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_build_content_dash_without_stdin() {
        let result = CodingAssistant::build_content(Some("-"), None);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No input provided via stdin")
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_build_content_prompt_and_stdin() {
        let result =
            CodingAssistant::build_content(Some("instructions"), Some("file content".to_string()));
        assert_eq!(result.unwrap(), "instructions\n\nfile content");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_build_content_no_input() {
        let result = CodingAssistant::build_content(None, None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("No input provided"));
        assert!(err_msg.contains("acai -"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_build_content_empty_prompt() {
        let result = CodingAssistant::build_content(Some(""), None);
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_build_content_empty_stdin() {
        let result = CodingAssistant::build_content(None, Some(String::new()));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("No input provided")
        );
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_build_content_prompt_with_empty_stdin() {
        let result = CodingAssistant::build_content(Some("my prompt"), Some(String::new()));
        assert_eq!(result.unwrap(), "my prompt");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_build_content_multiline_prompt() {
        let result = CodingAssistant::build_content(Some("line 1\nline 2"), None);
        assert_eq!(result.unwrap(), "line 1\nline 2");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_build_content_multiline_stdin() {
        let result =
            CodingAssistant::build_content(None, Some("stdin line 1\nstdin line 2".to_string()));
        assert_eq!(result.unwrap(), "stdin line 1\nstdin line 2");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_build_content_multiline_both() {
        let result = CodingAssistant::build_content(
            Some("prompt line 1\nprompt line 2"),
            Some("stdin line 1\nstdin line 2".to_string()),
        );
        assert_eq!(
            result.unwrap(),
            "prompt line 1\nprompt line 2\n\nstdin line 1\nstdin line 2"
        );
    }
}
