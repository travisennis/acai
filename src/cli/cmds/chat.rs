use std::{
    error::Error,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use clap::Args;
use rustyline::{error::ReadlineError, DefaultEditor};
use termimad::MadSkin;

use crate::{
    cli::get_provider_model,
    clients::LLMClient,
    errors::CAError,
    messages::{Message, Role},
};

#[derive(Clone, Args)]
pub struct Cmd {
    /// Sets the model to use
    #[arg(long, default_value_t = String::from("gpt-4-turbo"))]
    model: String,

    /// Sets the temperature value
    #[arg(short, long, default_value_t = 0.2)]
    temperature: f32,
}

impl Cmd {
    pub async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let home_dir = dirs::home_dir().expect("Home dir not found.");
        let coding_assistant_data_dir = home_dir.join(".config/coding-assistant");

        if let Some(p) = coding_assistant_data_dir.parent() {
            fs::create_dir_all(p).expect("Directory not created.");
        };

        let context: Result<String, CAError> = {
            if atty::is(atty::Stream::Stdin) {
                Err(CAError::Input)
            } else {
                Ok(std::io::read_to_string(std::io::stdin()).unwrap())
            }
        };

        let provider_model = get_provider_model(&self.model);

        let system_prompt = "You are a helpful coding assistant. Provide answers in markdown format unless instructed otherwise. If the request is ambiguous, ask questions. If you don't know the answer, admit you don't.";

        let mut client = LLMClient::new(provider_model.0, provider_model.1, system_prompt);

        let mut messages: Vec<Message> = vec![];

        if let Ok(context) = context {
            messages.push(Message {
                role: Role::User,
                content: context,
            });
        }

        let mut rl = DefaultEditor::new().expect("Editor not initialized.");
        // #[cfg(feature = "with-file-history")]
        if rl
            .load_history(coding_assistant_data_dir.join("history.txt").as_path())
            .is_err()
        {
            eprintln!("No previous history.");
        }

        let skin = MadSkin::default();

        loop {
            let readline = rl.readline("> ");
            match readline {
                Ok(line) if line.trim() == "bye" => {
                    break;
                }
                Ok(line) => {
                    let user_msg = Message {
                        role: Role::User,
                        content: line,
                    };

                    messages.push(user_msg);

                    let response = client.send_message(&mut messages).await?;

                    // println!("Response> {:?}", response);

                    if let Some(msg) = response {
                        println!("\n");
                        skin.print_text(&msg.content);
                        println!("\n");
                        messages.push(msg);
                    }
                }
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => {
                    break;
                }
                Err(err) => {
                    println!("Error: {err:?}");
                    break;
                }
            }
        }

        // #[cfg(feature = "with-file-history")]
        let _ = rl.save_history(coding_assistant_data_dir.join("history.txt").as_path());

        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");

        let in_ms = since_the_epoch.as_secs() * 1000
            + u64::from(since_the_epoch.subsec_nanos()) / 1_000_000;

        let output_file = format!("{in_ms}.json");
        let output_path = coding_assistant_data_dir.join("history").join(output_file);

        if let Some(p) = output_path.parent() {
            fs::create_dir_all(p).expect("Directory not created.");
        };

        // Save the JSON structure into the other file.
        std::fs::write(
            output_path,
            serde_json::to_string_pretty(&messages).unwrap(),
        )
        .unwrap();
        Ok(())
    }
}
