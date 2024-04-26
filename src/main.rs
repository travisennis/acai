mod clients;
mod macros;
mod messages;
mod open_ai;

use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use clap::{Parser, ValueEnum};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use termimad::MadSkin;

use crate::clients::LLMClient;
use crate::messages::Message;
use crate::messages::Role;
use crate::open_ai::Model;
use crate::open_ai::OpenAIApi;

#[derive(Debug, Copy, Clone)]
enum Error {
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
    #[arg(short, long, default_value_t = 0.0)]
    temperature: f32,

    /// Sets the stdin prompt
    std_prompt: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    let args = Args::parse();

    // println!("Model: {}", args.model);
    // println!("Prompt: {:?}", args.prompt);
    // println!("Mode: {:?}", args.mode);
    // println!("Temperature: {}", args.temperature);

    let context: Result<String, Error> = {
        if atty::is(atty::Stream::Stdin) {
            Err(Error::Input)
        } else {
            Ok(std::io::read_to_string(std::io::stdin()).unwrap())
        }
    };

    let model = match args.model.as_str() {
        "gpt-4-turbo" => Model::GPT4Turbo,
        "gpt-3-turbo" => Model::GPT3Turbo,
        _ => Model::GPT4Turbo,
    };

    let open_ai_client = OpenAIApi {
        model,
        temperature: args.temperature,
    };

    if args.mode == Mode::Chat {
        let mut history: Vec<Message> = vec![Message {
            role: Role::System,
            content: String::from("You are a helpful coding assistant. Provide answers in markdown format unless instructed otherwise."),
        }];

        if let Ok(context) = context {
            history.push(Message {
                role: Role::User,
                content: context,
            })
        }

        let mut rl = DefaultEditor::new().expect("Editor not initialized.");
        // #[cfg(feature = "with-file-history")]
        if rl.load_history("data/history.txt").is_err() {
            println!("No previous history.");
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

                    history.push(user_msg);

                    let response = open_ai_client.send_message(&history).await?;

                    // println!("Response> {:?}", response);

                    if let Some(choice) = response.choices.first() {
                        let msg = choice.get_message();
                        // println!("> {:?}", msg.content);
                        println!("\n");
                        skin.print_text(&msg.content);
                        println!("\n");
                        history.push(msg);
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    break;
                }
                Err(ReadlineError::Eof) => {
                    break;
                }
                Err(err) => {
                    println!("Error: {:?}", err);
                    break;
                }
            }
        }

        // #[cfg(feature = "with-file-history")]
        let _ = rl.save_history("data/history.txt");

        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");

        let in_ms =
            since_the_epoch.as_secs() * 1000 + since_the_epoch.subsec_nanos() as u64 / 1_000_000;

        let output_path = format!("data/{}.json", in_ms);

        // Save the JSON structure into the other file.
        std::fs::write(output_path, serde_json::to_string_pretty(&history).unwrap()).unwrap();
    } else {
        let mut messages: Vec<Message> = vec![Message {
            role: Role::System,
            content: String::from(
                "You are a helpful coding assistant. Provide the answer and only the answer. The answer should be in plain text without Markdown formatting.",
            ),
        }];

        if let Ok(context) = context {
            messages.push(Message {
                role: Role::User,
                content: context,
            })
        }

        let prompt: Result<String, Error> = {
            if args.std_prompt.is_empty() {
                Err(Error::Input)
            } else {
                Ok(args.std_prompt.join(" "))
            }
        };

        if let Ok(prompt) = prompt {
            messages.push(Message {
                role: Role::User,
                content: prompt,
            })
        }

        let response = open_ai_client.send_message(&messages).await?;

        println!("{}", response.choices[0].message.content);
    }
    Ok(())
}
