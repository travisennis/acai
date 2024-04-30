mod clients;
mod macros;
mod messages;

use std::error::Error;
use std::fs;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use clap::{Parser, ValueEnum};
use clients::LLMClient;
use clients::Model;
use clients::Provider;
use messages::Message;
use messages::Role;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use termimad::MadSkin;

#[derive(Debug, Copy, Clone)]
enum CAError {
    Input,
}

#[derive(Debug, ValueEnum, Clone, PartialEq)]
enum Mode {
    Chat,
    Pipe,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Sets the model to use
    #[arg(long, default_value_t = String::from("gpt-4-turbo"))]
    model: String,

    /// Sets the prompt
    #[arg(short, long)]
    prompt: Option<String>,

    /// Sets the mode: chat or search
    #[arg(short, long, value_enum, default_value_t = Mode::Chat)]
    mode: Mode,

    /// Sets the temperature value
    #[arg(short, long, default_value_t = 0.2)]
    temperature: f32,

    /// Sets the stdin prompt
    std_prompt: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = Args::parse();

    // println!("Model: {}", args.model);
    // println!("Prompt: {:?}", args.prompt);
    // println!("Mode: {:?}", args.mode);
    // println!("Temperature: {}", args.temperature);

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

    let provider_model = match args.model.as_str() {
        "gpt-4-turbo" => (Provider::OpenAI, Model::GPT4Turbo),
        "gpt-3-turbo" => (Provider::OpenAI, Model::GPT3Turbo),
        "opus" => (Provider::Anthropic, Model::ClaudeOpus),
        "sonnet" => (Provider::Anthropic, Model::ClaudeSonnet),
        "haiku" => (Provider::Anthropic, Model::ClaudeHaiku),
        _ => (Provider::OpenAI, Model::GPT4Turbo),
    };

    if args.mode == Mode::Chat {
        let mut client = LLMClient::new(provider_model.0, provider_model.1, "You are a helpful coding assistant. Provide answers in markdown format unless instructed otherwise.");

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
    } else {
        let mut client = LLMClient::new(provider_model.0, provider_model.1, "You are a helpful coding assistant. Provide the answer and only the answer. The answer should be in plain text without Markdown formatting.");

        let mut messages: Vec<Message> = vec![];

        if let Ok(context) = context {
            messages.push(Message {
                role: Role::User,
                content: context,
            });
        };

        let prompt: Result<String, CAError> = {
            if args.std_prompt.is_empty() {
                Err(CAError::Input)
            } else {
                Ok(args.std_prompt.join(" "))
            }
        };

        if let Ok(prompt) = prompt {
            messages.push(Message {
                role: Role::User,
                content: prompt,
            });
        };

        let response = client.send_message(&mut messages).await?;

        if let Some(msg) = response {
            println!("{}", msg.content);
        }
    }
    Ok(())
}
