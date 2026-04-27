//! cake - AI coding assistant CLI

mod cli;
mod clients;
mod config;
mod exit_code;
mod logger;
mod models;
mod prompts;

use std::time::{Duration, Instant};

use crate::cli::CmdRunner;
use crate::clients::{
    Agent, ConversationItem, set_additional_dirs, set_settings_dirs, set_skill_dirs,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::config::{
    AgentsFile, DataDir, DiagnosticLevel, ModelConfig, ModelDefinition, ResolvedModelConfig,
    Session, SettingsLoader, SkillCatalog, discover_skills, load_session_from_path,
    looks_like_uuid, worktree,
};
use crate::models::{Message, Role};
use crate::prompts::build_system_prompt;
use clap::{Parser, ValueEnum};
use indicatif::{ProgressBar, ProgressStyle};
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
#[allow(clippy::struct_excessive_bools)]
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

    /// Resume a specific session by UUID or file path.
    #[arg(long, value_name = "UUID_OR_PATH")]
    pub resume: Option<String>,

    /// Fork a session: copy its history into a new session with a fresh ID.
    /// Use without a value to fork the latest session, or provide a UUID or file path.
    #[arg(long, num_args = 0..=1, default_missing_value = "", value_name = "UUID_OR_PATH")]
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

    /// Override reasoning effort level (none, low, medium, high, xhigh)
    #[arg(long, value_name = "EFFORT")]
    pub reasoning_effort: Option<String>,

    /// Override reasoning token budget
    #[arg(long, value_name = "TOKENS")]
    pub reasoning_budget: Option<u32>,

    /// Add a directory to the sandbox config (read-only access). Can be repeated.
    #[arg(long, value_name = "DIR")]
    pub add_dir: Vec<String>,

    /// Disable all skills for this session
    #[arg(long)]
    pub no_skills: bool,

    /// Only load specific skills (comma-separated list of skill names)
    #[arg(long, value_name = "NAMES")]
    pub skills: Option<String>,
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
                tracing::warn!(
                    "Stdin read timed out after 100ms; piped input was ignored. \
                     Use '-' as the prompt to force stdin reading."
                );
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
                "No input provided. Provide a prompt as an argument, use 'cake -' for stdin, or pipe input to cake."
            )),
        }
    }

    /// Resolve the effective `ModelConfig`, applying CLI overrides.
    fn resolve_model_config(
        &self,
        models: &HashMap<String, ModelDefinition>,
        default_model: Option<&str>,
    ) -> anyhow::Result<ModelConfig> {
        let model_name = match self.model.as_deref() {
            Some(name) => name,
            None => match default_model {
                Some(name) => name,
                None => {
                    anyhow::bail!(
                        "No model specified. cake needs a model configuration to run.\n\n\
                        Set up ~/.config/cake/settings.toml with at least one model:\n\n\
                          default_model = \"zen\"\n\n\
                          [[models]]\n\
                          name = \"zen\"\n\
                          model = \"glm-5.1\"\n\
                          base_url = \"https://opencode.ai/zen/go/v1/\"\n\
                          api_key_env = \"OPENCODE_ZEN_API_TOKEN\"\n\n\
                        Then run 'cake <prompt>' or 'cake --model zen <prompt>'."
                    );
                },
            },
        };

        // Validate model name format
        if let Err(e) = ModelDefinition::validate_name(model_name) {
            anyhow::bail!(
                "Invalid model name '{model_name}': {e}. Model names must contain only lowercase letters, numbers, and hyphens."
            );
        }

        // Look up the model in settings
        let mut config = if let Some(def) = models.get(model_name) {
            def.to_model_config()
        } else {
            let available: Vec<_> = models.keys().cloned().collect();
            let available_str = if available.is_empty() {
                String::new()
            } else {
                format!(": {}", available.join(", "))
            };
            anyhow::bail!(
                "Unknown model '{model_name}'{available_str}. Use a model name from settings.toml, or set default_model and omit --model."
            );
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

    /// Resolve the model for a session restore (--continue, --resume, --fork).
    ///
    /// Policy (per the plan):
    /// - If `--model` is explicitly provided and the session has a stored model that
    ///   differs from it, error out with a clear message.
    /// - If `--model` is explicitly provided and matches the session model (or session
    ///   has no model), use the explicitly provided model.
    /// - If `--model` is not provided and the session has a stored model, use the
    ///   session model.
    /// - If `--model` is not provided and the session has no stored model, fall back
    ///   to default model resolution.
    fn resolve_model_for_session(
        &self,
        models: &HashMap<String, ModelDefinition>,
        default_model: Option<&str>,
        session_model: Option<&str>,
    ) -> anyhow::Result<ResolvedModelConfig> {
        let cli_model_explicit = self.model.is_some();

        if cli_model_explicit {
            // User explicitly passed --model. Resolve it and check against session.
            let config = self.resolve_model_config(models, default_model)?;
            let resolved = ResolvedModelConfig::resolve(config)?;
            let resolved_model = &resolved.config.model;

            if let Some(sm) = session_model
                && sm != resolved_model
            {
                anyhow::bail!(
                    "Session model mismatch: session uses '{sm}' but --model resolves to '{resolved_model}'. \
                     Use --model {sm} to continue with the session's model, or start a new session."
                );
            }

            Ok(resolved)
        } else if let Some(sm) = session_model {
            // No --model, but session has a stored model. Use it.
            // Look up the model name in settings to get provider config.
            // Try by name first (for sessions that store the config name),
            // then by model identifier (for backward compatibility with older
            // sessions that stored the API model string).
            let def = models
                .get(sm)
                .or_else(|| models.values().find(|d| d.model == sm));
            if let Some(def) = def {
                let resolved = ResolvedModelConfig::resolve(def.to_model_config())?;
                let resolved = self.apply_cli_overrides(resolved);
                Ok(resolved)
            } else {
                anyhow::bail!(
                    "Session model '{sm}' is not configured in settings.toml. \
                     Add a [[models]] entry for '{sm}' to continue this session, \
                     or start a new session."
                );
            }
        } else {
            // No --model, no session model. Fall back to default.
            let config = self.resolve_model_config(models, default_model)?;
            let resolved = ResolvedModelConfig::resolve(config)?;
            Ok(resolved)
        }
    }

    /// Apply CLI overrides (`max_tokens`, `reasoning_effort`, `reasoning_budget`) to a
    /// resolved model config.
    fn apply_cli_overrides(&self, mut resolved: ResolvedModelConfig) -> ResolvedModelConfig {
        if let Some(max_tokens) = self.max_tokens {
            resolved.config.max_output_tokens = Some(max_tokens);
        }
        if let Some(ref effort) = self.reasoning_effort {
            resolved.config.reasoning_effort = Some(effort.clone());
        }
        if let Some(budget) = self.reasoning_budget {
            resolved.config.reasoning_max_tokens = Some(budget);
        }
        resolved
    }

    /// Extract activated skills from session history by scanning for Read tool
    /// outputs that mention skill activation messages.
    fn extract_activated_skills(messages: &[ConversationItem]) -> HashSet<String> {
        let mut activated = HashSet::new();
        for item in messages {
            if let ConversationItem::FunctionCallOutput { output, .. } = item {
                // Check for our "already active" or activation log messages
                for line in output.lines() {
                    if let Some(start) = line.find("Skill '")
                        && let Some(end) = line[start + 7..].find('\'')
                    {
                        let name = &line[start + 7..start + 7 + end];
                        activated.insert(name.to_string());
                    }
                }
            }
        }
        activated
    }

    /// Build a map of skill file paths to skill names for activation deduplication.
    fn skill_locations(skill_catalog: &SkillCatalog) -> HashMap<PathBuf, String> {
        skill_catalog
            .skills
            .iter()
            .map(|s| (s.location.clone(), s.name.clone()))
            .collect()
    }

    /// Convert a restored session into the agent/session pair used for a continued run.
    fn restored_client_and_session(
        restored: Session,
        resolved: ResolvedModelConfig,
        system_prompt: &str,
        skill_locations: &HashMap<PathBuf, String>,
    ) -> (Agent, Session) {
        let messages = restored.messages();
        let prior_skills = Self::extract_activated_skills(&messages);

        let agent = Agent::new(resolved.clone(), system_prompt)
            .with_session_id(restored.id.to_string())
            .with_history(messages)
            .with_stream_records(restored.records)
            .with_skill_locations(skill_locations.clone())
            .with_activated_skills(prior_skills);
        let mut session = Session::new(restored.id, restored.working_dir);
        session.model = Some(resolved.config.model);
        (agent, session)
    }

    /// Validate that a session loaded from a file path belongs to the current directory.
    fn ensure_session_directory_matches(
        restored: &Session,
        current_dir: &std::path::Path,
        flag: &str,
    ) -> anyhow::Result<()> {
        if restored.working_dir == current_dir {
            return Ok(());
        }

        anyhow::bail!(
            "Working directory mismatch: session was created in '{}' but current directory is '{}'. \
             Run the command from the original directory, or use {flag} with a UUID to skip this check.",
            restored.working_dir.display(),
            current_dir.display()
        );
    }

    /// Build the agent/session pair for a new run using the agent-generated session id.
    fn new_client_and_session(
        resolved: ResolvedModelConfig,
        current_dir: PathBuf,
        system_prompt: &str,
        skill_locations: HashMap<PathBuf, String>,
    ) -> (Agent, Session) {
        let agent =
            Agent::new(resolved.clone(), system_prompt).with_skill_locations(skill_locations);
        #[allow(clippy::expect_used)]
        {
            let new_id =
                uuid::Uuid::parse_str(&agent.session_id).expect("agent generates valid UUIDs");
            info!(target: "cake", "New session: {new_id}");
            let mut session = Session::new(new_id, current_dir);
            session.model = Some(resolved.config.model);
            (agent, session)
        }
    }

    /// Build the agent/session pair for a forked run using a fresh agent session id.
    fn forked_client_and_session(
        restored: Session,
        resolved: ResolvedModelConfig,
        current_dir: PathBuf,
        system_prompt: &str,
        skill_locations: HashMap<PathBuf, String>,
    ) -> (Agent, Session) {
        let agent = Agent::new(resolved.clone(), system_prompt)
            .with_history(restored.messages())
            .with_stream_records(restored.records)
            .with_skill_locations(skill_locations);
        #[allow(clippy::expect_used)]
        {
            let new_id =
                uuid::Uuid::parse_str(&agent.session_id).expect("agent generates valid UUIDs");
            info!(target: "cake", "New forked session: {new_id}");
            let mut session = Session::new(new_id, current_dir);
            session.model = Some(resolved.config.model);
            (agent, session)
        }
    }

    fn build_client_and_session(
        &self,
        data_dir: &DataDir,
        current_dir: PathBuf,
        agents_files: &[AgentsFile],
        models: &HashMap<String, ModelDefinition>,
        default_model: Option<&str>,
        skill_catalog: &SkillCatalog,
    ) -> anyhow::Result<(Agent, Session)> {
        let system_prompt = build_system_prompt(&current_dir, agents_files, skill_catalog);
        let skill_locations = Self::skill_locations(skill_catalog);

        if self.continue_session {
            info!(target: "cake", "Continuing latest session for directory: {}", current_dir.display());
            let restored = data_dir
                .load_latest_session(&current_dir)?
                .ok_or_else(|| anyhow::anyhow!("No previous session found for this directory"))?;
            info!(target: "cake", "Continuing session: {}", restored.id);
            let resolved =
                self.resolve_model_for_session(models, default_model, restored.model.as_deref())?;
            Ok(Self::restored_client_and_session(
                restored,
                resolved,
                &system_prompt,
                &skill_locations,
            ))
        } else if let Some(ref arg) = self.resume {
            let restored = if looks_like_uuid(arg) {
                data_dir
                    .load_session(arg)?
                    .ok_or_else(|| anyhow::anyhow!("Session {arg} not found"))?
            } else {
                let path = PathBuf::from(arg);
                load_session_from_path(&path)?
            };
            info!(target: "cake", "Resumed session: {}", restored.id);

            if !looks_like_uuid(arg) {
                Self::ensure_session_directory_matches(&restored, &current_dir, "--resume")?;
            }

            let resolved =
                self.resolve_model_for_session(models, default_model, restored.model.as_deref())?;
            Ok(Self::restored_client_and_session(
                restored,
                resolved,
                &system_prompt,
                &skill_locations,
            ))
        } else if let Some(ref fork_id) = self.fork {
            info!(target: "cake", "Forking session");
            let restored = if fork_id.is_empty() {
                data_dir.load_latest_session(&current_dir)?.ok_or_else(|| {
                    anyhow::anyhow!("No previous session found for this directory")
                })?
            } else if looks_like_uuid(fork_id) {
                data_dir
                    .load_session(fork_id)?
                    .ok_or_else(|| anyhow::anyhow!("Session {fork_id} not found"))?
            } else {
                let path = PathBuf::from(fork_id);
                load_session_from_path(&path)?
            };

            if !fork_id.is_empty() && !looks_like_uuid(fork_id) {
                Self::ensure_session_directory_matches(&restored, &current_dir, "--fork")?;
            }

            info!(target: "cake", "Forking from session: {}", restored.id);
            let resolved =
                self.resolve_model_for_session(models, default_model, restored.model.as_deref())?;
            Ok(Self::forked_client_and_session(
                restored,
                resolved,
                current_dir,
                &system_prompt,
                skill_locations,
            ))
        } else {
            let resolved =
                ResolvedModelConfig::resolve(self.resolve_model_config(models, default_model)?)?;
            Ok(Self::new_client_and_session(
                resolved,
                current_dir,
                &system_prompt,
                skill_locations,
            ))
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
    #[allow(clippy::too_many_lines)]
    async fn run(&self, data_dir: &DataDir) -> anyhow::Result<()> {
        let original_dir = std::env::current_dir()?;
        let wt = self.setup_worktree(&original_dir)?;

        let stdin_content = Self::read_stdin_content();
        let content = Self::build_content(self.prompt.as_deref(), stdin_content)?;

        let current_dir = std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get current directory: {e}"))?;

        // Load settings from TOML files
        let loaded = SettingsLoader::load(Some(&current_dir))?;

        let agents_files = data_dir.read_agents_files(&current_dir);

        // Load skill settings and discover skills
        let skill_settings = SettingsLoader::load_skill_settings(Some(&current_dir));
        let skill_config = SettingsLoader::resolve_skill_config(
            self.no_skills,
            self.skills.as_deref(),
            &skill_settings,
        );

        let mut skill_catalog = discover_skills(&current_dir);
        skill_catalog = skill_config.apply(skill_catalog);

        // Set up skill directories for path validation
        let skill_base_dirs: Vec<PathBuf> = skill_catalog
            .skills
            .iter()
            .map(|s| s.base_directory.clone())
            .collect();
        set_skill_dirs(skill_base_dirs);

        // Set up settings directories (from settings.toml) for read-write access
        let settings_dirs: Vec<PathBuf> = loaded
            .directories
            .iter()
            .map(PathBuf::from)
            .filter(|p| {
                if p.exists() && p.is_dir() {
                    true
                } else {
                    tracing::warn!(
                        "settings.toml directory '{}' does not exist or is not a directory, ignoring",
                        p.display()
                    );
                    false
                }
            })
            .collect();
        set_settings_dirs(settings_dirs);

        // Log diagnostics for skills
        for diagnostic in &skill_catalog.diagnostics {
            match diagnostic.level {
                DiagnosticLevel::Warning => {
                    tracing::warn!(
                        "Skill diagnostic ({}): {}",
                        diagnostic.file.display(),
                        diagnostic.message
                    );
                },
                DiagnosticLevel::Error => {
                    tracing::error!(
                        "Skill diagnostic ({}): {}",
                        diagnostic.file.display(),
                        diagnostic.message
                    );
                },
            }
        }

        if !skill_catalog.skills.is_empty() {
            tracing::info!(
                "Discovered {} skill(s): {}",
                skill_catalog.skills.len(),
                skill_catalog
                    .skills
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        let (mut client, mut session) = self.build_client_and_session(
            data_dir,
            current_dir,
            &agents_files,
            &loaded.models,
            loaded.default_model.as_deref(),
            &skill_catalog,
        )?;

        if self.output_format == OutputFormat::StreamJson {
            client = client.with_streaming_json(|json| {
                println!("{json}");
            });
        }

        #[allow(clippy::useless_let_if_seq)]
        let mut normal_spinner: Option<ProgressBar> = None;

        if self.output_format == OutputFormat::Text {
            let spinner = ProgressBar::new_spinner();
            #[allow(clippy::literal_string_with_formatting_args)]
            let style = ProgressStyle::with_template("{spinner:.cyan} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner());
            spinner.set_style(style);
            spinner.enable_steady_tick(Duration::from_millis(80));
            spinner.set_message("Thinking...");

            let spinner_clone = spinner.clone();
            client = client.with_progress_callback(move |item| {
                let msg = format_spinner_message(item);
                if let Some(msg) = msg {
                    spinner_clone.set_message(msg);
                }
            });

            normal_spinner = Some(spinner);
        }

        let msg = Message {
            role: Role::User,
            content,
        };

        let start = Instant::now();

        client.emit_init_message();

        let result: anyhow::Result<Option<Message>> = client.send(msg).await;

        let duration_ms: u64 = start.elapsed().as_millis().try_into().unwrap_or(u64::MAX);

        // Emit and store the Result record for both text and stream-json modes.
        match &result {
            Ok(response_msg) => {
                let result_text = response_msg.as_ref().map(|m| m.content.clone());
                client.emit_result_message(true, duration_ms, result_text, None);
            },
            Err(e) => {
                client.emit_result_message(false, duration_ms, None, Some(e.to_string()));
            },
        }

        if let Some(spinner) = normal_spinner.take() {
            let summary = format_done_summary(duration_ms, &client);
            spinner.finish_with_message(format!("Done: {summary}"));
        }

        if !self.no_session {
            session.records = client.drain_stream();
            if session.model.is_none() {
                session.model = Some(client.model().to_string());
            }
            info!(target: "cake", "Saving session {} ({} records)", session.id, session.records.len());
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

/// Format a completion summary with elapsed time, turns, and token usage.
fn format_done_summary(duration_ms: u64, client: &Agent) -> String {
    // Precision loss acceptable: used only for display
    #[allow(clippy::cast_precision_loss)]
    let secs = duration_ms as f64 / 1000.0;
    let turns = client.turn_count;
    let input_tokens = client.total_usage.input_tokens;
    let output_tokens = client.total_usage.output_tokens;
    let cached_reads_tokens = client.total_usage.input_tokens_details.cached_tokens;
    format!(
        "session {}, {secs:.1}s, {turns} turns, {input_tokens} input tokens, {cached_reads_tokens} cached reads, {output_tokens} output tokens",
        client.session_id
    )
}

/// Format a conversation item as a short spinner message for normal mode.
///
/// Returns `Some(message)` for items worth showing, `None` otherwise.
fn format_spinner_message(item: &ConversationItem) -> Option<String> {
    match item {
        ConversationItem::FunctionCall {
            name, arguments, ..
        } => {
            let summary = clients::summarize_tool_args(name, arguments);
            Some(format!("{name}: {summary}"))
        },
        ConversationItem::Reasoning { .. } => Some("Thinking...".to_string()),
        ConversationItem::Message { role, .. } if *role == Role::Assistant => {
            Some("Responding...".to_string())
        },
        _ => None,
    }
}

fn main() -> std::process::ExitCode {
    let data_dir = match DataDir::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error: {e}");
            return exit_code::classify(&e);
        },
    };

    let _ = logger::configure(&data_dir.get_cache_dir());

    info!("data dir: {}", data_dir.get_cache_dir().display());

    let args = match CodingAssistant::try_parse() {
        Ok(a) => a,
        Err(e) => {
            // Print the clap error (includes --help and --version output).
            // For --help/--version, clap returns exit_code() == 0 and the
            // formatted output goes to stdout. For actual errors (bad flags,
            // missing required args), it goes to stderr with exit_code() != 0.
            e.print().ok();
            let exit = if e.exit_code() == 0 {
                std::process::ExitCode::from(exit_code::code::SUCCESS)
            } else {
                std::process::ExitCode::from(exit_code::code::INPUT_ERROR)
            };
            return exit;
        },
    };

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

    // Set up the Tokio runtime and run the async command
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            let err = anyhow::anyhow!("Failed to initialize Tokio runtime: {e}");
            eprintln!("Error: {err}");
            return exit_code::classify(&err);
        },
    };

    let result = rt.block_on(args.run(&data_dir));

    match result {
        Ok(()) => std::process::ExitCode::from(exit_code::code::SUCCESS),
        Err(e) => {
            eprintln!("Error: {e}");
            exit_code::classify(&e)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::ApiType;

    #[test]
    fn test_cli_parsing_positional_prompt() {
        let args = CodingAssistant::parse_from(["cake", "test prompt"]);
        assert_eq!(args.prompt, Some("test prompt".to_string()));
    }

    #[test]
    fn test_cli_parsing_dash_for_stdin() {
        let args = CodingAssistant::parse_from(["cake", "-"]);
        assert_eq!(args.prompt, Some("-".to_string()));
    }

    #[test]
    fn test_cli_parsing_no_prompt() {
        let args = CodingAssistant::parse_from(["cake"]);
        assert_eq!(args.prompt, None);
    }

    #[test]
    fn test_cli_parsing_model_flag() {
        let args = CodingAssistant::parse_from(["cake", "--model", "claude", "test prompt"]);
        assert_eq!(args.model, Some("claude".to_string()));
        assert_eq!(args.prompt, Some("test prompt".to_string()));
    }

    #[test]
    fn test_cli_parsing_model_flag_without_prompt() {
        let args = CodingAssistant::parse_from(["cake", "--model", "deepseek"]);
        assert_eq!(args.model, Some("deepseek".to_string()));
        assert_eq!(args.prompt, None);
    }

    #[test]
    fn test_cli_parsing_no_model_flag() {
        let args = CodingAssistant::parse_from(["cake", "test prompt"]);
        assert_eq!(args.model, None);
    }

    #[test]
    fn test_cli_parsing_no_session() {
        let args = CodingAssistant::parse_from(["cake", "--no-session", "test prompt"]);
        assert!(args.no_session);
    }

    #[test]
    fn test_cli_parsing_no_session_defaults_false() {
        let args = CodingAssistant::parse_from(["cake", "test prompt"]);
        assert!(!args.no_session);
    }

    #[test]
    fn test_cli_parsing_add_dir_single() {
        let args =
            CodingAssistant::parse_from(["cake", "--add-dir", "/path/to/dir", "test prompt"]);
        assert_eq!(args.add_dir, vec!["/path/to/dir"]);
        assert_eq!(args.prompt, Some("test prompt".to_string()));
    }

    #[test]
    fn test_cli_parsing_add_dir_multiple() {
        let args = CodingAssistant::parse_from([
            "cake",
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
        let args = CodingAssistant::parse_from(["cake", "test prompt"]);
        assert!(args.add_dir.is_empty());
    }

    #[test]
    fn test_cli_parsing_no_skills() {
        let args = CodingAssistant::parse_from(["cake", "--no-skills", "test prompt"]);
        assert!(args.no_skills);
        assert!(args.skills.is_none());
    }

    #[test]
    fn test_cli_parsing_skills_filter() {
        let args =
            CodingAssistant::parse_from(["cake", "--skills", "debugging,review", "test prompt"]);
        assert!(!args.no_skills);
        assert_eq!(args.skills, Some("debugging,review".to_string()));
    }

    #[test]
    fn test_cli_parsing_skills_defaults() {
        let args = CodingAssistant::parse_from(["cake", "test prompt"]);
        assert!(!args.no_skills);
        assert!(args.skills.is_none());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_resolve_model_config_no_model_configured() {
        let args = CodingAssistant::parse_from(["cake", "test prompt"]);
        let models = HashMap::new();
        let result = args.resolve_model_config(&models, None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No model specified"));
        assert!(err.contains("settings.toml"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_resolve_model_config_default_model() {
        let args = CodingAssistant::parse_from(["cake", "test prompt"]);
        let mut models = HashMap::new();
        models.insert(
            "zen".to_string(),
            ModelDefinition {
                name: "zen".to_string(),
                model: "glm-5.1".to_string(),
                base_url: "https://opencode.ai/zen/go/v1/".to_string(),
                api_key_env: "OPENCODE_ZEN_API_TOKEN".to_string(),
                api_type: ApiType::ChatCompletions,
                temperature: None,
                top_p: None,
                max_output_tokens: None,
                reasoning_effort: None,
                reasoning_summary: None,
                reasoning_max_tokens: None,
                providers: vec![],
            },
        );

        let config = args.resolve_model_config(&models, Some("zen")).unwrap();
        assert_eq!(config.model, "glm-5.1");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_resolve_model_config_unknown_model() {
        let mut args = CodingAssistant::parse_from(["cake", "test prompt"]);
        args.model = Some("nonexistent".to_string());

        let models = HashMap::new();
        let result = args.resolve_model_config(&models, None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown model 'nonexistent'"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_resolve_model_config_invalid_name_format() {
        let mut args = CodingAssistant::parse_from(["cake", "test prompt"]);
        args.model = Some("Invalid Name!".to_string());

        let models = HashMap::new();
        let result = args.resolve_model_config(&models, None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid model name 'Invalid Name!'"));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_resolve_model_config_from_settings() {
        let args = CodingAssistant::parse_from(["cake", "--model", "claude", "test"]);

        let mut models = HashMap::new();
        models.insert(
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

        let config = args.resolve_model_config(&models, None).unwrap();
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
        assert!(err_msg.contains("cake -"));
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

    // Tests for format_spinner_message
    #[test]
    fn test_format_spinner_message_function_call() {
        let item = ConversationItem::FunctionCall {
            id: "fc-1".to_string(),
            call_id: "call-1".to_string(),
            name: "Bash".to_string(),
            arguments: r#"{"command":"ls -la"}"#.to_string(),
            timestamp: None,
        };
        let msg = format_spinner_message(&item);
        assert!(msg.is_some());
        let msg = msg.unwrap_or_default();
        assert!(msg.contains("Bash:"));
        assert!(msg.contains("ls -la"));
    }

    #[test]
    fn test_format_spinner_message_reasoning() {
        let item = ConversationItem::Reasoning {
            id: "r-1".to_string(),
            summary: vec!["thinking...".to_string()],
            encrypted_content: None,
            content: None,
            timestamp: None,
        };
        let msg = format_spinner_message(&item);
        assert_eq!(msg, Some("Thinking...".to_string()));
    }

    #[test]
    fn test_format_spinner_message_assistant() {
        let item = ConversationItem::Message {
            role: Role::Assistant,
            content: "Here is the answer".to_string(),
            id: Some("msg-1".to_string()),
            status: Some("completed".to_string()),
            timestamp: None,
        };
        let msg = format_spinner_message(&item);
        assert_eq!(msg, Some("Responding...".to_string()));
    }

    #[test]
    fn test_format_spinner_message_user_returns_none() {
        let item = ConversationItem::Message {
            role: Role::User,
            content: "Hello".to_string(),
            id: None,
            status: None,
            timestamp: None,
        };
        assert!(format_spinner_message(&item).is_none());
    }

    #[test]
    fn test_format_spinner_message_function_output_returns_none() {
        let item = ConversationItem::FunctionCallOutput {
            call_id: "call-1".to_string(),
            output: "result".to_string(),
            timestamp: None,
        };
        assert!(format_spinner_message(&item).is_none());
    }

    #[test]
    fn test_format_done_summary() {
        temp_env::with_var("CAKE_TEST_VALID_KEY", Some("sk-test-123"), || {
            let config = ModelConfig {
                model: "test/model".to_string(),
                api_type: ApiType::ChatCompletions,
                base_url: "https://api.example.com".to_string(),
                api_key_env: "CAKE_TEST_VALID_KEY".to_string(),
                temperature: None,
                top_p: None,
                max_output_tokens: None,
                reasoning_effort: None,
                reasoning_summary: None,
                reasoning_max_tokens: None,
                providers: vec![],
            };
            let resolved = match ResolvedModelConfig::resolve(config) {
                Ok(resolved) => resolved,
                Err(err) => panic!("test config should resolve: {err}"),
            };
            let mut agent = Agent::new(resolved, "test system prompt");
            agent.session_id = "session-123".to_string();
            agent.turn_count = 3;
            agent.total_usage.input_tokens = 1000;
            agent.total_usage.input_tokens_details.cached_tokens = 250;
            agent.total_usage.output_tokens = 500;

            let summary = format_done_summary(1500, &agent);
            assert!(summary.contains("session session-123"));
            assert!(summary.contains("1.5s"));
            assert!(summary.contains("3 turns"));
            assert!(summary.contains("1000 input tokens"));
            assert!(summary.contains("250 cached reads"));
            assert!(summary.contains("500 output tokens"));
        });
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_resolve_model_for_session_by_model_field() {
        // Session stores the API model identifier, not the config name.
        // This test verifies that --continue works when the session model
        // matches a definition's `model` field even if the `name` differs.
        temp_env::with_var("CAKE_TEST_VALID_KEY", Some("sk-test-123"), || {
            let args = CodingAssistant::parse_from(["cake", "test prompt"]);

            let mut models = HashMap::new();
            models.insert(
                "my-alias".to_string(),
                ModelDefinition {
                    name: "my-alias".to_string(),
                    model: "deepseek-v4-pro".to_string(),
                    base_url: "https://api.example.com".to_string(),
                    api_key_env: "CAKE_TEST_VALID_KEY".to_string(),
                    api_type: ApiType::ChatCompletions,
                    temperature: None,
                    top_p: None,
                    max_output_tokens: None,
                    reasoning_effort: None,
                    reasoning_summary: None,
                    reasoning_max_tokens: None,
                    providers: vec![],
                },
            );

            let resolved = args
                .resolve_model_for_session(&models, None, Some("deepseek-v4-pro"))
                .unwrap();
            assert_eq!(resolved.config.model, "deepseek-v4-pro");
        });
    }
}
