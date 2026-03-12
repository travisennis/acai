use std::time::Instant;

use clap::{Args, ValueEnum};

use crate::{
    cli::CmdRunner,
    clients::Responses,
    config::{AgentsFile, DEFAULT_MODEL, DataDir, Session, worktree},
    models::{Message, Role},
    prompts::build_system_prompt,
};

/// Output format for the response
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Plain text output
    #[default]
    Text,
    /// Stream each message as JSON as it's received
    StreamJson,
}

#[derive(Clone, Args)]
pub struct Cmd {
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

    /// Sets the prompt
    #[arg(short, long, conflicts_with = "prompt_file")]
    prompt: Option<String>,

    /// Read the prompt from a file instead of the command line
    #[arg(long, value_name = "FILE", conflicts_with = "prompt")]
    prompt_file: Option<String>,

    /// Output format for the response (text or stream-json)
    #[arg(long, value_enum, default_value = "text")]
    pub output_format: OutputFormat,

    /// Continue the most recent session for this directory
    #[arg(long = "continue")]
    pub continue_session: bool,

    /// Resume a specific session by UUID
    #[arg(long, value_name = "UUID")]
    pub resume: Option<String>,

    /// Do not save the session to disk
    #[arg(long)]
    pub no_session: bool,

    /// Run in an isolated git worktree (optionally provide a name)
    #[arg(short, long, num_args = 0..=1, default_missing_value = "")]
    pub worktree: Option<String>,

    /// Restrict which providers can serve requests (comma-separated or multiple flags).
    /// Use "all" to allow any provider. Defaults to "Fireworks,Moonshot AI".
    #[arg(long, num_args = 0.., value_delimiter = ',')]
    pub providers: Vec<String>,
}

impl Cmd {
    fn build_client_and_session(
        &self,
        data_dir: &crate::config::DataDir,
        current_dir: std::path::PathBuf,
        agents_files: &[AgentsFile],
    ) -> anyhow::Result<(Responses, crate::config::Session)> {
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
        std::env::set_current_dir(original_dir).ok();

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

impl CmdRunner for Cmd {
    async fn run(&self, data_dir: &DataDir) -> anyhow::Result<()> {
        // Validate mutually exclusive flags
        if self.continue_session && self.resume.is_some() {
            return Err(anyhow::anyhow!(
                "Cannot use both --continue and --resume at the same time"
            ));
        }

        // Read prompt from file if --prompt-file was provided
        let file_prompt: Option<String> = if let Some(ref path) = self.prompt_file {
            Some(std::fs::read_to_string(path)?)
        } else {
            None
        };

        // Only read from stdin if a prompt is not provided
        // Note: We always attempt to read stdin unless --prompt is explicitly provided.
        // If stdin is a TTY (interactive terminal), it will be empty anyway.
        let input_context: Option<String> = if self.prompt.is_some() || self.prompt_file.is_some() {
            None
        } else {
            std::io::read_to_string(std::io::stdin()).ok()
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

        // Build content from prompt and optional stdin context
        // Error if neither prompt nor stdin input is provided
        let content = match (&self.prompt, &file_prompt, &input_context) {
            (Some(prompt), _, Some(ctx)) => format!("{prompt}\n\n{ctx}"),
            (Some(prompt), _, None) => prompt.clone(),
            (_, Some(file_prompt), Some(ctx)) => format!("{file_prompt}\n\n{ctx}"),
            (_, Some(file_prompt), None) => file_prompt.clone(),
            (None, None, Some(ctx)) => ctx.clone(),
            (None, None, None) => {
                return Err(anyhow::anyhow!(
                    "No input provided. Use --prompt, --prompt-file, or pipe input to stdin."
                ));
            },
        };

        let msg = Message {
            role: Role::User,
            content,
        };

        // Start timing
        let start = Instant::now();

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
            session.touch();
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
