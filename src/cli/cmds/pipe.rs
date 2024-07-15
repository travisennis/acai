use std::error::Error;

use anyhow::Result;
use clap::{Args, ValueEnum};

use crate::{
    cli::CmdRunner,
    clients::{
        providers::{ModelConfig, Provider},
        ChatCompletionClient,
    },
    config::DataDir,
    errors::CAError,
    models::{Message, Role},
};

const OPTIMIZE_PROMPT: &str = "Review the code snippet below and suggest optimizations to improve performance. Focus on efficiency, speed, and resource usage while maintaining the original functionality. Provide only the optimized code.";

const FIX_PROMPT: &str = "Your task is to analyze the provided code snippet, identify any bugs or errors present, and provide a corrected version of the code that resolves these issues while retaining the same functionality. The corrected code should be functional, efficient, and adhere to best practices in programming. Only return the revised code.";

const COMPLETE_PROMPT: &str = "Your task is to complete the provided code snippet. You should complete the function implementation. The completed code should be functional, efficient, and adhere to best practices in programming. Only return the revised code.";

const DOCUMENT_PROMPT: &str = "Your task is to document the provided code using the best practices for documenting code for this language.";

const TODO_PROMPT: &str = "Your task is to add todo comments to the provided code snippet. The todo comments are to be added to parts of the code that can be improved or fixed. The todo comment should explain what needs to be done and give a short explanation of why.";

const SUGGESTION_PROMPT: &str = "Your task is to provide suggestions to improve the provided code snippet.Ths suggestions should focus on what can be improved or fixed about this code. The suggestions should explain what needs to be done and give a short explanation of why. Provide line numbers to indicate which part of the code the suggestion applies.";

const DEFAULT_PROMPT: &str = "You are a helpful coding assistant and senior software engineer. Provide the answer and only the answer to the user's request. The user's request will be in a TODO comment within the code snippet.  The answer should be in plain text without Markdown formatting. Only return the revised code and remove the TODO comment.";

#[derive(Debug, ValueEnum, Clone, PartialEq)]
enum Task {
    Optimize,
    Fix,
    Complete,
    Todo,
    Document,
    Suggestion,
}

#[derive(Clone, Args)]
pub struct Cmd {
    /// Sets the model to use
    #[arg(long)]
    pub model: Option<String>,

    /// Sets the temperature value
    #[arg(long)]
    pub temperature: Option<f32>,

    /// Sets the max tokens value
    #[arg(long)]
    pub max_tokens: Option<u32>,

    /// Sets the top-p value
    #[arg(long)]
    pub top_p: Option<f32>,

    /// Sets the task
    #[arg(long, value_enum)]
    task: Option<Task>,

    /// Sets the stdin prompt
    prompt: Vec<String>,
}

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let system_prompt = match self.task {
            Some(Task::Optimize) => OPTIMIZE_PROMPT,
            Some(Task::Fix) => FIX_PROMPT,
            Some(Task::Complete) => COMPLETE_PROMPT,
            Some(Task::Todo) => TODO_PROMPT,
            Some(Task::Document) => DOCUMENT_PROMPT,
            Some(Task::Suggestion) => SUGGESTION_PROMPT,
            _ => DEFAULT_PROMPT,
        };

        let model = self.model.clone().unwrap_or_default();
        let config = ModelConfig::get_or_default(&model, (Provider::Anthropic, "sonnet"));

        let mut client = ChatCompletionClient::new(config.provider, config.model, system_prompt)
            .temperature(self.temperature)
            .top_p(self.top_p)
            .max_tokens(self.max_tokens);

        let mut prompt_builder = crate::prompts::Builder::new()?;

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

        let std_prompt: Result<String, CAError> = {
            if self.prompt.is_empty() {
                Err(CAError::Input)
            } else {
                Ok(self.prompt.join(" "))
            }
        };

        if let Ok(prompt) = std_prompt {
            prompt_builder.add_variable("prompt".to_string(), prompt);
        }
        if let Ok(context) = context {
            prompt_builder.add_variable("context".to_string(), context);
        }

        if prompt_builder.contains_variables() {
            let msg = Message {
                role: Role::User,
                content: prompt_builder.build()?,
            };

            let response = client.send_message(msg).await?;

            if let Some(response_msg) = response {
                println!("{}", response_msg.content);
            } else {
                eprintln!("{response:?}");
            }

            DataDir::global().save_messages(&client.get_message_history());
        }

        Ok(())
    }
}
