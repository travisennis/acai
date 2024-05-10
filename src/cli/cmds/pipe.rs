use std::error::Error;

use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::{
    cli::{CmdConfig, CmdRunner},
    clients::LLMClient,
    errors::CAError,
    models::{Message, Role},
};

const OPTIMIZE_PROMPT: &str = "Please review the following code snippet and propose optimizations to enhance its performance. Focus on identifying opportunities to increase efficiency, speed, and resource conservation. Ensure that any optimized code retains the same functionality as the original but demonstrates measurable performance improvements. Provide only the revised code in your response.";

const FIX_PROMPT: &str = "Your task is to analyze the provided code snippet, identify any bugs or errors present, and provide a corrected version of the code that resolves these issues. The corrected code should be functional, efficient, and adhere to best practices in programming. Only return the code.";

const COMPLETE_PROMPT: &str = "Your task is to complete the provided code snippet. You should complete the function implementation. The completed code should be functional, efficient, and adhere to best practices in programming. Only return the code.";

const DOCUMENT_PROMPT: &str = "Your task is to document the provided code using the best practices for documenting code for this language.";

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
    /// Sets the task
    #[arg(long, value_enum)]
    task: Option<Task>,

    /// Sets the stdin prompt
    prompt: Vec<String>,
}

impl CmdRunner for Cmd {
    async fn run(&self, cfg: CmdConfig) -> Result<(), Box<dyn Error + Send + Sync>> {
        let system_prompt = match self.task {
            Some(Task::Optimize) => OPTIMIZE_PROMPT,
            Some(Task::Fix) => FIX_PROMPT,
            Some(Task::Complete) => COMPLETE_PROMPT,
            Some(Task::Todo) => TODO_PROMPT,
            Some(Task::Document) => DOCUMENT_PROMPT,
            _ => DEFAULT_PROMPT,
        };

        let mut client = LLMClient::new(
            cfg.provider,
            cfg.model,
            cfg.temperature,
            cfg.top_p,
            cfg.max_tokens,
            system_prompt,
        );

        let mut messages: Vec<Message> = vec![];

        let std_prompt: Result<String, CAError> = {
            if self.prompt.is_empty() {
                Err(CAError::Input)
            } else {
                Ok(self.prompt.join(" "))
            }
        };

        let user_prompt: Result<String, CAError> = {
            if let Ok(prompt) = std_prompt {
                if let Some(context) = cfg.context {
                    Ok(format!(
                        r###"
                        {prompt}

                        ```
                        {context}
                        ```
                        "###
                    ))
                } else {
                    Ok(prompt)
                }
            } else if let Some(context) = cfg.context {
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
