use std::error::Error;
use std::time::Instant;

use anyhow::Result;
use clap::Args;

use crate::{
    cli::CmdRunner,
    clients::Responses,
    config::DataDir,
    models::{Message, Role},
};

#[derive(Clone, Args)]
pub struct Cmd {
    /// Sets the model to use (e.g., "minimax/minimax-m2.5")
    #[arg(long, default_value = "minimax/minimax-m2.5")]
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
    #[arg(short, long)]
    prompt: Option<String>,

    /// Stream each message as JSON as it's received
    #[arg(long)]
    pub streaming_json: bool,
}

const SYSTEM_PROMPT: &str = "You are a helpful AI CLI assistant that runs on the user's computer and follows their instructions.";

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Only read from stdin if a prompt is not provided
        let input_context: Option<String> =
            if self.prompt.is_some() || atty::is(atty::Stream::Stdin) {
                None
            } else {
                std::io::read_to_string(std::io::stdin()).ok()
            };

        let mut client = Responses::new(self.model.clone(), SYSTEM_PROMPT)
            .temperature(self.temperature)
            .top_p(self.top_p)
            .max_output_tokens(self.max_tokens);

        // Enable streaming JSON output if flag is set
        if self.streaming_json {
            client = client.with_streaming_json(|json| {
                println!("{json}");
            });
        }

        // Build content from prompt and optional stdin context
        let content = match (&self.prompt, &input_context) {
            (Some(prompt), Some(ctx)) => format!("{prompt}\n\n{ctx}"),
            (Some(prompt), None) => prompt.clone(),
            (None, Some(ctx)) => ctx.clone(),
            (None, None) => String::new(),
        };

        let msg = Message {
            role: Role::User,
            content,
        };

        // Start timing
        let start = Instant::now();

        // Send message and handle result
        let result: Result<Option<Message>, Box<dyn Error + Send + Sync>> = client.send(msg).await;

        // Calculate duration
        #[allow(clippy::cast_possible_truncation)]
        let duration_ms = start.elapsed().as_millis() as u64;

        // Emit result message
        if self.streaming_json {
            match &result {
                Ok(_) => {
                    client.emit_result_message(true, duration_ms, None);
                }
                Err(e) => {
                    client.emit_result_message(false, duration_ms, Some(e.to_string().as_ref()));
                }
            }
        }

        // Propagate error or continue with response handling
        let response = result?;

        DataDir::global().save_messages(&client.get_message_history());

        // Only print final response if NOT using streaming-json mode
        // (streaming mode already prints each message as JSON)
        if !self.streaming_json {
            if let Some(response_msg) = response {
                println!("{}", response_msg.content);
            } else {
                eprintln!("{response:?}");
            }
        }

        Ok(())
    }
}
