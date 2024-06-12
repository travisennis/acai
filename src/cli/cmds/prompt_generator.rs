use regex::Regex;
use std::{collections::HashMap, env, error::Error};

use clap::Args;

use crate::{
    cli::CmdRunner,
    errors::CAError,
    models::{Message, Role},
    prompts::PromptBuilder,
};
use readability::extractor;

#[derive(Clone, Args)]
pub struct Cmd {
    /// Sets the stdin prompt
    prompt: Vec<String>,
}

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        match env::current_dir() {
            Ok(path) => println!("The current working directory is: {}", path.display()),
            Err(e) => println!("Error: {e}"),
        }

        let context: Result<String, CAError> = {
            if atty::is(atty::Stream::Stdin) {
                Err(CAError::Input)
            } else {
                match std::io::read_to_string(std::io::stdin()) {
                    Ok(result) => Ok(result),
                    Err(_error) => Err(CAError::Input),
                }
            }
        };

        let prompt_builder = PromptBuilder::new()?;

        let std_prompt: Result<String, CAError> = {
            if self.prompt.is_empty() {
                Err(CAError::Input)
            } else {
                Ok(self.prompt.join(" "))
            }
        };

        let mut data = HashMap::new();

        if let Ok(prompt) = std_prompt {
            data.insert("prompt".to_string(), prompt);
        }
        if let Ok(context) = context {
            println!("{context}");
            let t = process_todo_comment(&context);
            println!("Prompt: {}", t.0);
            for item in t.1 {
                println!("URL: {item}");

                match extractor::scrape(&item) {
                    Ok(product) => {
                        println!("------- html ------");
                        println!("{}", product.content);
                        println!("---- plain text ---");
                        println!("{}", product.text);
                    }
                    Err(e) => println!("error occured: {e}"),
                }
            }
            println!("Temp: {}", t.2);
            data.insert("context".to_string(), t.0.to_string());
        }

        if !data.is_empty() {
            let msg = Message {
                role: Role::User,
                content: prompt_builder.build(&data)?,
            };

            println!("Final: {}", msg.content);
        }

        Ok(())
    }
}

fn process_todo_comment(comment: &str) -> (String, Vec<String>, f32) {
    // Regular expressions to match the URLs and temperature
    let url_re = Regex::new(r"https?://[^\s]+").unwrap();
    let temp_re = Regex::new(r"Temperature=(\d+(\.\d+)?)").unwrap();

    // Extract URLs
    let urls: Vec<String> = url_re
        .find_iter(comment)
        .map(|m| m.as_str().to_string())
        .collect();

    // Extract temperature
    let temp_cap = temp_re
        .captures(comment)
        .expect("Temperature not found in comment");
    let temp: f32 = temp_cap
        .get(1)
        .unwrap()
        .as_str()
        .parse()
        .expect("Failed to parse temperature value");

    // Remove URL sentences
    let comment_without_urls = url_re.replace_all(comment, "").to_string();

    // Remove temperature sentence
    let comment_final = temp_re.replace(&comment_without_urls, "").to_string();

    // Clean up the remaining comment by removing empty lines and trimming whitespace
    let cleaned_comment = comment_final
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<&str>>()
        .join("\n");

    (cleaned_comment, urls, temp)
}
