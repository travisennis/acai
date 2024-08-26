use std::{error::Error, path::PathBuf, str::FromStr};

use anyhow::Result;
use clap::Args;
use log::debug;
use rustyline::{error::ReadlineError, DefaultEditor};
use termimad::{ansi, Alignment, MadSkin, StyledChar};

use crate::{
    cli::CmdRunner,
    config::DataDir,
    files::{get_content_blocks, get_file_info, get_file_tree, parse_patterns},
    llm_api::{open_ai::Message, ChatCompletionRequest, Provider, ToolDefinition},
    tools::{generate_edits::GenerateEdits, lint_code::LintCode},
};

const SYSTEM_PROMPT: &str = r"
You are acai, an AI assistant. You specialize in software development. Assume the software engineer you are working with is experienced and talented. Don't dumb things down. The goal of offering assistance is to make the best software possible. Offer useful guideance in the following areas:

1. Code Review and Suggestions
2. Documentation Assistance
3. Problem-Solving and Debugging
4. Code Generation
5. Performance Optimization
6. Testing Strategies
7. Code Refactoring
8. Code Style and Linting
  
Think through the problem step-by-step before giving your response. 

If the request is ambiguous or you need more information, ask questions. If you don't know the answer, admit you don't.

Provide answers in markdown format unless instructed otherwise. 
";

// Our tool definitions
const TOOLS: [&dyn ToolDefinition; 2] = [&GenerateEdits, &LintCode];

const ADD_COMMAND: &str = "/add";
const EXIT_COMMAND: &str = "/exit";
const SAVE_COMMAND: &str = "/save";
const RESET_COMMAND: &str = "/reset";

struct ProjectContext {
    project_path: Option<PathBuf>,
    file_tree: Option<String>,
    file_objects: Vec<crate::files::FileInfo>,
    content_blocks: Vec<serde_json::Value>,
}

struct ChatState {
    provider: Provider,
    messages: Vec<Message>,
    project_context: ProjectContext,
    prompt_builder: crate::prompts::Builder<'static>,
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

    // Path to the codebase directory
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Patterns to include
    #[clap(long)]
    pub include: Option<String>,

    /// Patterns to exclude
    #[clap(long)]
    pub exclude: Option<String>,

    // Path to a handlebars template
    #[clap(long)]
    pub template: Option<PathBuf>,
}

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let model = self
            .model
            .clone()
            .unwrap_or_else(|| "anthropic/sonnet".to_owned());

        let provider = Provider::from_str(&model)?;

        let messages = provider.init_messages(SYSTEM_PROMPT);

        let project_context = self.get_project_context();

        let mut rl = DefaultEditor::new()?;

        let skin = make_skin();

        let prompt_builder = crate::prompts::Builder::new(&self.template)?;

        let mut chat_state = ChatState {
            provider,
            messages,
            project_context,
            prompt_builder,
        };

        chat_state.print_usage(&skin);

        chat_state.chat_loop(&mut rl, &skin).await?;

        DataDir::global().save_messages(&chat_state.messages);

        Ok(())
    }
}

impl ChatState {
    fn print_usage(&self, skin: &MadSkin) {
        skin.print_text("**Greetings. I am acai.**");
        skin.print_text(&format!(
            "**Current working directory**: {:#?}",
            self.project_context
                .project_path
                .clone()
                .unwrap_or_else(|| ".".into())
        ));

        let usage_table = r"
|:-:|:-:|
|**command**|**description**|
|:-|:-|
| /add | Add files to the chat. |
| /reset | Saves the chat history and then resets it.|
| /save | Saves the chat history.|
| /exit | Exits and saves the chat history.|
|-
";

        skin.print_text(usage_table);

        if !self.project_context.file_objects.is_empty() {
            println!(
                "Files have been added to the context: {}",
                self.project_context.file_objects.len()
            );
        }
    }

    fn reset_chat(&mut self, skin: &MadSkin) {
        self.project_context.file_objects.clear();
        self.project_context.content_blocks.clear();

        let save_file = DataDir::global().save_messages(&self.messages);
        if let Some(sf) = save_file {
            self.messages.clear();
            println!("\n");
            skin.print_text(&format!(
                "messages saved to {} and history reset",
                sf.display()
            ));
            println!("\n");
        }
    }

    fn save_chat(&self, skin: &MadSkin) {
        let save_file = DataDir::global().save_messages(&self.messages);
        if let Some(sf) = save_file {
            println!("\n");
            skin.print_text(&format!("messages saved to {}", sf.display()));
            println!("\n");
        }
    }

    fn add_files(&mut self, include: &[String]) {
        let file_objects = self
            .project_context
            .project_path
            .clone()
            .map_or(Vec::new(), |path| {
                get_file_info(path.as_path(), include, &[]).map_or(Vec::new(), |files| files)
            });

        let new_content_blocks = get_content_blocks(&file_objects);

        self.project_context
            .content_blocks
            .extend(new_content_blocks);

        self.project_context.file_objects.extend(file_objects);

        self.prompt_builder
            .add_vec_variable("files".to_string(), &self.project_context.content_blocks);
    }

    async fn chat_loop(
        &mut self,
        rl: &mut DefaultEditor,
        skin: &MadSkin,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut is_first_iteration = true;

        loop {
            let readline = rl.readline("> ");
            match readline {
                Ok(line) if line.trim() == EXIT_COMMAND => break,
                Ok(line) if line.trim() == SAVE_COMMAND => {
                    self.save_chat(skin);
                    continue;
                }
                Ok(line) if line.trim() == RESET_COMMAND => {
                    self.reset_chat(skin);
                    continue;
                }
                Ok(line) if line.starts_with(ADD_COMMAND) => {
                    let include = parse_patterns(&Some(
                        line.trim_start_matches(ADD_COMMAND).trim().to_string(),
                    ));

                    self.add_files(&include);

                    continue;
                }
                Ok(line) => {
                    if is_first_iteration {
                        if let Some(file_tree) = &self.project_context.file_tree {
                            self.prompt_builder
                                .add_variable("file_tree".to_owned(), file_tree.to_string());
                        }
                        self.prompt_builder.add_vec_variable(
                            "files".to_string(),
                            &self.project_context.content_blocks,
                        );
                    }
                    self.process_user_input(line, skin).await?;
                    is_first_iteration = false;
                }
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
                Err(err) => {
                    println!("Error: {err:?}");
                    break;
                }
            }
        }

        Ok(())
    }
    async fn process_user_input(
        &mut self,
        line: String,
        skin: &MadSkin,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.prompt_builder.add_variable("prompt".to_string(), line);

        let user_msg = Message::User {
            content: self.prompt_builder.build()?,
            name: None,
        };

        self.messages.push(user_msg);

        let client = crate::llm_api::create(self.provider.clone());
        let result = client
            .chat(
                ChatCompletionRequest {
                    system_prompt: SYSTEM_PROMPT.to_owned(),
                    messages: self.messages.clone(),
                    ..ChatCompletionRequest::default()
                },
                &TOOLS,
            )
            .await?;

        match result {
            Message::Assistant {
                ref content,
                name: _,
                ref tool_calls,
            } => {
                self.messages.push(result.clone());

                if let Some(tool_calls) = tool_calls {
                    self.process_tool_calls(tool_calls, skin).await?;
                } else {
                    skin.print_text(&content.clone().unwrap_or_default());
                }
            }
            _ => skin.print_text("unexpected message"),
        }

        println!("\n");

        self.prompt_builder.clear_variables();

        Ok(())
    }

    async fn process_tool_calls(
        &mut self,
        tool_calls: &[crate::llm_api::open_ai::ToolCall],
        skin: &MadSkin,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        for tool_call in tool_calls {
            let tool = TOOLS.iter().find(|t| t.name() == tool_call.function.name);

            let Some(_tool) = tool else {
                println!(
                    "WARNING: Tried to call non-existent {} tool",
                    tool_call.function.name
                );
                continue;
            };

            debug!(target: "acai", "Function name {:#?}", tool_call.function.name);

            let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)?;

            debug!(target: "acai", "Function args {:#?}", args.clone());

            let fn_result = match tool_call.function.name.as_str() {
                "generate_edits" => crate::tools::generate_edits::callable_func(
                    &args,
                    &self.project_context.file_tree,
                    &self.project_context.content_blocks,
                    skin,
                )
                .await
                .unwrap_or_default(),
                "lint_code" => crate::tools::lint_code::callable_func().unwrap_or_default(),
                _ => serde_json::Value::String(String::new()),
            };

            debug!(target: "acai", "Function result {}", fn_result);

            self.messages.push(Message::Tool {
                content: fn_result.to_string(),
                tool_call_id: tool_call.id.clone(),
            });

            let client = crate::llm_api::create(self.provider.clone());
            let result = client
                .chat(
                    ChatCompletionRequest {
                        system_prompt: SYSTEM_PROMPT.to_owned(),
                        messages: self.messages.clone(),
                        ..ChatCompletionRequest::default()
                    },
                    &TOOLS,
                )
                .await?;

            if let Message::Assistant {
                ref content,
                name: _,
                tool_calls: _,
            } = result
            {
                self.messages.push(result.clone());
                skin.print_text(&content.clone().unwrap_or_default());
            } else {
                skin.print_text("missing message");
            }
        }

        Ok(())
    }
}

impl Cmd {
    fn get_project_context(&self) -> ProjectContext {
        // Parse Patterns
        let include_patterns = parse_patterns(&self.include);
        let exclude_patterns = parse_patterns(&self.exclude);

        let current_path = self
            .path
            .clone()
            .map_or_else(|| std::env::current_dir().ok(), Some);

        let file_tree = current_path
            .clone()
            .and_then(|path| get_file_tree(&path, &include_patterns, &exclude_patterns).ok());

        let file_objects = self.path.clone().map_or(Vec::new(), |path| {
            get_file_info(path.as_path(), &include_patterns, &exclude_patterns)
                .map_or(Vec::new(), |files| files)
        });

        let content_blocks = get_content_blocks(&file_objects);

        ProjectContext {
            project_path: current_path,
            file_tree,
            file_objects,
            content_blocks,
        }
    }
}

fn make_skin() -> MadSkin {
    let mut skin = match terminal_light::luma() {
        Ok(luma) if luma > 0.6 => MadSkin::default_light(),
        Ok(_) => MadSkin::default_dark(),
        Err(_) => MadSkin::default(),
    };

    skin.bullet = StyledChar::from_fg_char(ansi(178), '•');

    skin.code_block.align = Alignment::Left;

    skin
}
