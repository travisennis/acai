//! acai - AI coding assistant CLI

mod cli;
mod clients;
mod config;
mod logger;
mod models;
mod prompts;

use std::io::IsTerminal;
use std::time::Instant;

use crate::cli::CmdRunner;
use crate::clients::Responses;
use crate::config::{AgentsFile, DEFAULT_MODEL, DataDir, Session, worktree};
use crate::models::{Message, Role};
use crate::prompts::build_system_prompt;
use clap::{Parser, ValueEnum};
use log::info;

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

    /// Sets the model to use (e.g., "minimax/minimax-m2.5")
    #[arg(long, default_value = DEFAULT_MODEL)]
    pub model: String,

    /// Sets the temperature value
    #[arg(long)]
    pub temperature: Option<f32>,

    /// Sets the max tokens value
    #[arg(long)]
    pub max_tokens: Option<u32>,

    /// Sets the top-p value
    #[arg(long)]
    pub top_p: Option<f32>,

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

    /// Restrict which providers can serve requests (comma-separated or multiple flags).
    /// Use "all" to allow any provider. Defaults to "Fireworks,Moonshot AI".
    #[arg(long, num_args = 0.., value_delimiter = ',')]
    pub providers: Vec<String>,
}

/// Check if we should use quiet logging (no stderr output).
/// This is true when using machine-readable output formats like stream-json.
fn should_use_quiet_logging() -> bool {
    // Parse args to check for --output-format stream-json
    // We use try_parse to avoid panicking on invalid args (error will be shown later)
    CodingAssistant::try_parse()
        .map(|args| args.output_format == OutputFormat::StreamJson)
        .unwrap_or(false)
}

impl CodingAssistant {
    fn build_client_and_session(
        &self,
        data_dir: &DataDir,
        current_dir: std::path::PathBuf,
        agents_files: &[AgentsFile],
    ) -> anyhow::Result<(Responses, Session)> {
        let system_prompt = build_system_prompt(&current_dir, agents_files);

        if self.continue_session {
            let restored = data_dir
                .load_latest_session(&current_dir)?
                .ok_or_else(|| anyhow::anyhow!("No previous session found for this directory"))?;
            let c = Responses::new(self.model.clone(), &system_prompt)?
                .temperature(self.temperature)
                .top_p(self.top_p)
                .max_output_tokens(self.max_tokens)
                .providers(self.providers.clone())
                .with_session_id(restored.id.clone())
                .with_history(restored.messages.clone());
            Ok((c, restored))
        } else if let Some(ref uuid) = self.resume {
            uuid::Uuid::parse_str(uuid)
                .map_err(|e| anyhow::anyhow!("Invalid session UUID '{uuid}': {e}"))?;
            let restored = data_dir
                .load_session(&current_dir, uuid)?
                .ok_or_else(|| anyhow::anyhow!("Session {uuid} not found in this directory"))?;
            let c = Responses::new(self.model.clone(), &system_prompt)?
                .temperature(self.temperature)
                .top_p(self.top_p)
                .max_output_tokens(self.max_tokens)
                .providers(self.providers.clone())
                .with_session_id(restored.id.clone())
                .with_history(restored.messages.clone());
            Ok((c, restored))
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
            let c = Responses::new(self.model.clone(), &system_prompt)?
                .temperature(self.temperature)
                .top_p(self.top_p)
                .max_output_tokens(self.max_tokens)
                .providers(self.providers.clone())
                .with_history(restored.messages);
            let s = Session::new(c.session_id.clone(), current_dir);
            Ok((c, s))
        } else {
            let c = Responses::new(self.model.clone(), &system_prompt)?
                .temperature(self.temperature)
                .top_p(self.top_p)
                .max_output_tokens(self.max_tokens)
                .providers(self.providers.clone());
            let s = Session::new(c.session_id.clone(), current_dir);
            Ok((c, s))
        }
    }

    /// Set up a worktree if `--worktree` was provided. Returns the worktree
    /// handle and changes the process working directory into it.
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

    /// Clean up a worktree after the session ends. Removes it if there are no
    /// changes; keeps it otherwise.
    fn cleanup_worktree(wt: &worktree::Worktree, original_dir: &std::path::Path) {
        if let Err(e) = std::env::set_current_dir(original_dir) {
            log::warn!(
                "Failed to restore original directory '{}': {e}",
                original_dir.display()
            );
        }

        match worktree::has_changes(&wt.path) {
            Ok(false) => {
                eprintln!("No changes in worktree '{}', removing.", wt.name);
                if let Err(e) = worktree::remove(original_dir, &wt.name, false) {
                    log::warn!("Failed to clean up worktree '{}': {e}", wt.name);
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
                log::warn!(
                    "Could not check worktree '{}' for changes, keeping it: {e}",
                    wt.name
                );
            },
        }
    }
}

impl CmdRunner for CodingAssistant {
    async fn run(&self, data_dir: &DataDir) -> anyhow::Result<()> {
        // Validate mutually exclusive flags
        let session_flags = [
            self.continue_session,
            self.resume.is_some(),
            self.fork.is_some(),
        ];
        let active = session_flags.iter().filter(|&&f| f).count();
        if active > 1 {
            return Err(anyhow::anyhow!(
                "Cannot use more than one of --continue, --resume, and --fork at the same time"
            ));
        }

        // Handle stdin input
        let stdin_content: Option<String> = if std::io::stdin().is_terminal() {
            None
        } else {
            std::io::read_to_string(std::io::stdin()).ok()
        };

        // Build content from prompt and stdin
        // Error if neither prompt nor stdin input is provided
        let content = match (self.prompt.as_deref(), stdin_content) {
            (Some("-"), None) => {
                // acai - (with no piped input)
                return Err(anyhow::anyhow!("No input provided via stdin"));
            },
            (Some("-") | None, Some(stdin)) => stdin, // acai - < input.txt or just stdin
            (Some(prompt), Some(stdin)) => format!("{prompt}\n\n{stdin}"), // Both: prompt + stdin
            (Some(prompt), None) => prompt.to_string(), // Just prompt
            (None, None) => {
                // Nothing at all
                return Err(anyhow::anyhow!(
                    "No input provided. Provide a prompt as an argument, use 'acai -' for stdin, or pipe input to acai."
                ));
            },
        };

        let original_dir = std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get current directory: {e}"))?;

        let wt = self.setup_worktree(&original_dir)?;

        let current_dir = std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get current directory: {e}"))?;

        // Read AGENTS.md files from user-level and project-level
        let agents_files = data_dir.read_agents_files(&current_dir);

        // Build client and session, restoring from disk if requested
        let (mut client, mut session) =
            self.build_client_and_session(data_dir, current_dir, &agents_files)?;

        // Enable streaming JSON output if flag is set
        if self.output_format == OutputFormat::StreamJson {
            client = client.with_streaming_json(|json| {
                println!("{json}");
            });
        }

        let msg = Message {
            role: Role::User,
            content,
        };

        // Start timing
        let start = Instant::now();

        // Emit init message with session info, cwd, and tools
        client.emit_init_message();

        // Send message and handle result
        let result: anyhow::Result<Option<Message>> = client.send(msg).await;

        // Calculate duration
        #[allow(clippy::cast_possible_truncation)]
        let duration_ms = start.elapsed().as_millis() as u64;

        // Emit result message
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

        // Save session regardless of outcome (Phase 4: save on error)
        if !self.no_session {
            session.messages = client.get_history_without_system();
            session.model = Some(client.model().to_string());
            if let Err(e) = data_dir.save_session(&session) {
                log::error!("Failed to save session: {e}");
            }
        }

        // Propagate error after saving
        let response = result?;

        // Only print final response if NOT using streaming-json mode
        // (streaming mode already prints each message as JSON)
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir = DataDir::new()?;

    // Determine if we should use quiet logging before configuring the logger.
    // This ensures log messages don't pollute machine-readable output.
    let quiet = should_use_quiet_logging();

    let _ = logger::configure(&data_dir.get_cache_dir(), quiet);

    info!("data dir: {}", data_dir.get_cache_dir().display());

    let args = CodingAssistant::parse();
    args.run(&data_dir).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing_positional_prompt() {
        let args = CodingAssistant::parse_from(["acai", "test prompt"]);
        assert_eq!(args.prompt, Some("test prompt".to_string()));
    }

    #[test]
    fn test_cli_parsing_with_flags() {
        let args = CodingAssistant::parse_from([
            "acai",
            "--model",
            "gpt-4",
            "--temperature",
            "0.5",
            "prompt here",
        ]);
        assert_eq!(args.prompt, Some("prompt here".to_string()));
        assert_eq!(args.model, "gpt-4");
        assert_eq!(args.temperature, Some(0.5));
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
}
