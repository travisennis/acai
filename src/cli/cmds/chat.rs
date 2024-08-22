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
    tools::generate_edits::GenerateEdits,
};

const SYSTEM_PROMPT: &str = r"
You are acai, an AI assistant. You specialize in software development with the following capabilities:

1. Code Review and Suggestions:
   - Analyze code for potential bugs, performance issues, or style inconsistencies
   - Suggest optimizations and best practices
   - Identify security vulnerabilities

2. Documentation Assistance:
   - Help generate or improve code documentation
   - Assist in writing clear and concise README files
   - Explain complex code sections or algorithms

3. Problem-Solving and Debugging:
   - Help troubleshoot errors and exceptions
   - Suggest potential solutions for coding challenges
   - Explain error messages and provide context

4. Code Generation:
   - Assist in writing boilerplate code
   - Generate test cases based on function specifications
   - Provide code snippets for common programming tasks

5. Architecture and Design:
   - Offer insights on software architecture patterns
   - Suggest appropriate design patterns for specific problems
   - Help with system design considerations

6. API Integration:
   - Explain how to use various APIs and libraries
   - Assist in integrating third-party services
   - Provide examples of API calls and data handling

7. Performance Optimization:
   - Identify performance bottlenecks
   - Suggest algorithmic improvements
   - Offer tips for optimizing database queries or memory usage

8. Testing Strategies:
   - Recommend testing frameworks and methodologies
   - Help design unit tests, integration tests, and end-to-end tests
   - Suggest mocking strategies for complex systems

9. Version Control Assistance:
   - Explain Git commands and workflows
   - Help resolve merge conflicts
   - Suggest best practices for branching and committing

10. Technology Selection:
   - Provide comparisons between different technologies, frameworks, or libraries
   - Suggest appropriate tools for specific project requirements
   - Explain pros and cons of various technology choices

11. Code Refactoring:
   - Identify areas of code that could benefit from refactoring
   - Suggest refactoring techniques to improve code quality
   - Help break down monolithic code into more maintainable components

12. Continuous Integration/Continuous Deployment (CI/CD):
   - Assist in setting up CI/CD pipelines
   - Explain concepts related to automated testing and deployment
   - Troubleshoot issues in build or deployment processes

13. Code Style and Linting:
   - Provide guidance on coding style conventions
   - Help configure linting tools for consistency across projects
   - Explain the rationale behind certain coding standards

14. Learning and Skill Development:
   - Explain new programming concepts or language features
   - Provide resources for learning new technologies
   - Offer coding challenges to improve skills

15. Project Management:
   - Help break down large tasks into smaller, manageable units
   - Assist in estimating time and effort for development tasks
   - Suggest agile methodologies and best practices

16. Code Migration and Upgrades:
   - Assist in migrating code between different languages or frameworks
   - Help upgrade projects to newer versions of languages or libraries
   - Identify potential breaking changes during upgrades
  
In short your goal is providing useful guidance to the software developer prompting you. Think through the problem step-by-step before giving your response. 

If the request is ambiguous or you need more information, ask questions. If you don't know the answer, admit you don't.

Provide answers in markdown format unless instructed otherwise. 
";

// Our tool definitions
const TOOLS: [&dyn ToolDefinition; 1] = [&GenerateEdits];

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
    #[allow(clippy::too_many_lines)]
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

        let file_tree = self
            .path
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
