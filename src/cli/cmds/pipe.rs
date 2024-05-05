use std::error::Error;

use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::{
    cli::get_provider_model,
    clients::LLMClient,
    errors::CAError,
    messages::{Message, Role},
};

const OPTIMIZE_PROMPT: &str = "Your task is to analyze the provided code snippet and suggest improvements to optimize its performance. Identify areas where the code can be made more efficient, faster, or less resource-intensive. The optimized code should maintain the same functionality as the original code while demonstrating improved efficiency. Only return the code.";
const FIX_PROMPT: &str = "Your task is to analyze the provided code snippet, identify any bugs or errors present, and provide a corrected version of the code that resolves these issues. The corrected code should be functional, efficient, and adhere to best practices in programming. Only return the code.";
const COMPLETE_PROMPT: &str = "Your task is to complete the provided code snippet. You should complete the function implementation. The completed code should be functional, efficient, and adhere to best practices in programming. Only return the code.";
const DOCUMENT_PROMPT: &str =
    "Your task is to document the provided code using the best practices for documenting code for this language.";
const TODO_PROMPT: &str = "Your task is to add todo comments to the provided code snippet. The todo comments are to be added to parts of the code that can be improved or fixed. The todo comment should explain what needs to be done and give a short explanation of why.";
const DEFAULT_PROMPT: &str = "You are a helpful coding assistant and senior software engineer. Provide the answer and only the answer to the user's request. The answer should be in plain text without Markdown formatting. Only return the code.";

#[derive(Debug, ValueEnum, Clone, PartialEq)]
enum Task {
    Optimize,
    Fix,
    Complete,
    Todo,
    Document,
}

#[derive(Clone, Args)]
pub struct Cmd {
    /// Sets the model to use
    #[arg(long, default_value_t = String::from("gpt-4-turbo"))]
    model: String,

    /// Sets the task
    #[arg(long, value_enum)]
    task: Option<Task>,

    /// Sets the temperature value
    #[arg(short, long, default_value_t = 0.2)]
    temperature: f32,

    /// Sets the stdin prompt
    std_prompt: Vec<String>,
}

impl Cmd {
    pub async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let system_prompt = match self.task {
            Some(Task::Optimize) => OPTIMIZE_PROMPT,
            Some(Task::Fix) => FIX_PROMPT,
            Some(Task::Complete) => COMPLETE_PROMPT,
            Some(Task::Todo) => TODO_PROMPT,
            Some(Task::Document) => DOCUMENT_PROMPT,
            _ => DEFAULT_PROMPT,
        };

        let provider_model = get_provider_model(&self.model);

        let mut client = LLMClient::new(provider_model.0, provider_model.1, system_prompt);

        let mut messages: Vec<Message> = vec![];

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

        let prompt: Result<String, CAError> = {
            if self.std_prompt.is_empty() {
                Err(CAError::Input)
            } else {
                Ok(self.std_prompt.join(" "))
            }
        };

        let user_prompt: Result<String, CAError> = {
            if let Ok(prompt) = prompt {
                if let Ok(context) = context {
                    Ok(format!(
                        "{prompt}\n\
                        ```\n\
                        {context}\n\
                        ```"
                    ))
                } else {
                    Ok(prompt)
                }
            } else if let Ok(context) = context {
                Ok(context)
            } else {
                Err(CAError::Input)
            }
        };

        if let Ok(prompt) = user_prompt {
            messages.push(Message {
                role: Role::User,
                content: prompt,
            });
        };

        let response = client.send_message(&mut messages).await?;

        if let Some(msg) = response {
            println!("{}", msg.content);
        } else {
            eprintln!("{response:?}");
        }

        Ok(())
    }
}
