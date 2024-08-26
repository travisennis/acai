use std::{collections::HashMap, fs, io, path::Path, str::FromStr};

use rustyline::DefaultEditor;
use serde_json::Value;
use similar::{ChangeTag, TextDiff};
use termimad::MadSkin;

use crate::llm_api::{
    open_ai::Message, ChatCompletionRequest, JsonSchema, Provider, ToolDefinition,
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Readline error")]
    Readline,
    #[error("Prompt builder failure")]
    PromptBuilder,
    #[error("Invalid provider")]
    InvalidProvider,
    #[error("Missing \"instructions\" argument")]
    MissingInstructions,
    #[error("Failed to construct prompt")]
    PromptConstruction,
    #[error("Failed to complete tool request")]
    ToolRequest,
    #[error("Unsupported message type: {0}")]
    UnsupportedMessageType(String),
    #[error("User error")]
    User,
    #[error("Error getting feedback from user")]
    UserFeedback,
    #[error("No changes applied")]
    NoChangesApplied,
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
}

pub struct GenerateEdits;

impl ToolDefinition for GenerateEdits {
    fn name(&self) -> &'static str {
        "generate_edits"
    }
    fn description(&self) -> &'static str {
        "This function generates a set of edits that can applied to the current code base based on the specific instructions provided. This function will return the edits and give the user the ability to accept or reject the suggested edits before applying them to the code base."
    }
    fn get_parameters(&self) -> JsonSchema {
        let mut properties = HashMap::new();

        properties.insert(
            "instructions".to_owned(),
            JsonSchema::String {
                description: "After the reviewing the provided code, construct a plan for the necessary changes. These instructions will be used to determine what edits need to made to the code base."
                    .to_string(),
            },
        );

        JsonSchema::Object {
            required: properties.keys().map(|s| (*s).clone()).collect(),
            properties,
        }
    }
}

const SYSTEM_PROMPT: &str = "You are acai, an AI coding assistant. You specialize in helping software developers with the tasks that help them write better software. Pay close attention to the instructions given to you by the user and always follow those instructions. Return your reponse as markdown unless the user indicates a different return format. It is very important that you format your response according to the user instructions as that formatting will be used to accomplish specific tasks.";

const PROMPT_TEMPLATE: &str = r"
Your tasks it to generate edit instructions for code files by analyzing the provided code and generating SEARCH/REPLACE blocks for necessary changes. Follow these steps:

1. Carefully analyze the specific instructions:

{{prompt}}

2. Consider the full context of all files in the project:

{{#if files}}
File Contents:

{{/if}}
{{#each files}}
{{#if path}}
File: {{path}}

{{/if}}
{{#if content}}
{{content}}
{{/if}}

---	

{{/each}}

3. Generate SEARCH/REPLACE blocks for each necessary change. Each block should:
   - Indicate the path of the file where the code needs to be changed. If the code should be in a new file, indicate the path where that file should live in the project structure
   - Include enough context to uniquely identify the code to be changed
   - Provide the exact replacement code, maintaining correct indentation and formatting
   - Focus on specific, targeted changes rather than large, sweeping modifications

4. Ensure that your SEARCH/REPLACE blocks:
   - Address all relevant aspects of the instructions
   - Maintain or enhance code readability and efficiency
   - Consider the overall structure and purpose of the code
   - Follow best practices and coding standards for the language
   - Maintain consistency with the project context and previous edits
   - Take into account the full context of all files in the project

5. Make sure that each SEARCH/REPLACE block can be applied to the code that would exist after the block prior to it is applied. Remember that each block will update the code in place and each subsequent block can only be applied to the updated code. 

IMPORTANT: RETURN ONLY THE SEARCH/REPLACE BLOCKS. NO EXPLANATIONS OR COMMENTS.
USE THE FOLLOWING FORMAT FOR EACH BLOCK:

<BLOCK>
<PATH>The file path of the file to be edited</PATH>
<SEARCH>
Code to be replaced
</SEARCH>
<REPLACE>
New code to insert
</REPLACE>
</BLOCK>

If no changes are needed, return an empty list.
";

pub async fn callable_func(
    arguments: &Value,
    file_tree: &Option<String>,
    content_blocks: &Vec<Value>,
    skin: &MadSkin,
) -> Result<Value, Error> {
    if content_blocks.is_empty() {
        return Err(Error::PromptConstruction);
    }

    let mut rl = DefaultEditor::new().map_err(|_| Error::Readline)?;
    let mut prompt_builder = crate::prompts::Builder::new_from_string(PROMPT_TEMPLATE.to_owned())
        .map_err(|_| Error::PromptBuilder)?;

    if let Some(file_tree) = file_tree {
        prompt_builder.add_variable("file_tree".to_string(), file_tree.to_string());
    }

    prompt_builder.add_vec_variable("files".to_string(), content_blocks);
    if let Value::String(text) = &arguments["instructions"] {
        prompt_builder.add_variable("prompt".to_string(), text.to_string());
        let provider =
            Provider::from_str("anthropic/sonnet").map_err(|_| Error::InvalidProvider)?;

        let edits = get_edits(provider, prompt_builder).await?;

        let edit_blocks = process_blocks(&edits);

        println!("Proposed edits:\n\n");

        for edit in &edit_blocks {
            println!("Path: {}\n", edit.path);
            let diff = TextDiff::from_lines(edit.search, edit.replace);
            let mut diff_str = String::new();
            diff_str.push_str("```\n");
            for change in diff.iter_all_changes() {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                diff_str.push_str(&format!("{sign}{change}"));
            }
            diff_str.push_str("```\n");
            skin.print_text(&diff_str);
        }

        let input = rl
            .readline("Accept these edits? y or n: ")
            .map_err(|_| Error::UserFeedback)?;

        match input.trim() {
            "y" => {
                let results: Result<Vec<()>, Error> = edit_blocks
                    .iter()
                    .map(|item| apply_edit_block(item))
                    .collect();

                results.map_err(|_| Error::NoChangesApplied)?;

                Ok("Changes applied.".into())
            }
            "n" => Ok("Changes rejected by user.".into()),
            _ => Err(Error::User),
        }
    } else {
        Err(Error::MissingInstructions)
    }
}

async fn get_edits(
    provider: Provider,
    prompt_builder: crate::prompts::Builder<'_>,
) -> Result<String, Error> {
    let mut messages = provider.init_messages(SYSTEM_PROMPT);

    let prompt = prompt_builder
        .build()
        .map_err(|_| Error::PromptConstruction)?;

    messages.push(Message::User {
        content: prompt,
        name: None,
    });

    let client = crate::llm_api::create(provider);

    let result = client
        .chat(
            ChatCompletionRequest {
                system_prompt: SYSTEM_PROMPT.to_owned(),
                messages,
                ..ChatCompletionRequest::default()
            },
            &[],
        )
        .await;

    let edits = result.map_or(Err(Error::ToolRequest), |result| match result {
        crate::llm_api::open_ai::Message::System {
            content: _,
            name: _,
        } => Err(Error::UnsupportedMessageType("system".to_string())),
        crate::llm_api::open_ai::Message::User {
            content: _,
            name: _,
        } => Err(Error::UnsupportedMessageType("user".to_string())),
        crate::llm_api::open_ai::Message::Assistant {
            content,
            name: _,
            tool_calls: _,
        } => Ok(content.unwrap_or_default()),
        crate::llm_api::open_ai::Message::Tool {
            content: _,
            tool_call_id: _,
        } => Err(Error::UnsupportedMessageType("tool".to_string())),
    })?;
    Ok(edits)
}

struct EditBlock<'a> {
    path: &'a str,
    search: &'a str,
    replace: &'a str,
}

fn process_blocks(input: &str) -> Vec<EditBlock> {
    let blocks: Vec<&str> = input.split("<BLOCK>").collect();

    let mut edits: Vec<EditBlock> = Vec::new();
    for block in blocks.iter().skip(1) {
        // Skip the first empty split
        let parts: Vec<&str> = block.split("</BLOCK>").collect();
        if let Some(content) = parts.first() {
            edits.push(process_single_block(content));
        }
    }
    edits
}

fn process_single_block(block: &str) -> EditBlock {
    let path = extract_content(block, "PATH");
    let search = extract_content(block, "SEARCH");
    let replace = extract_content(block, "REPLACE");

    EditBlock {
        path,
        search,
        replace,
    }
}

fn extract_content<'a>(block: &'a str, tag: &str) -> &'a str {
    let start_tag = format!("<{tag}>");
    let end_tag = format!("</{tag}>");

    if let Some(start) = block.find(&start_tag) {
        if let Some(end) = block.find(&end_tag) {
            return &block[start + start_tag.len()..end];
        }
    }
    ""
}

fn apply_edit_block(block: &EditBlock) -> Result<(), Error> {
    let path = block.path;
    let search = block.search;
    let replace = block.replace;

    let path = Path::new(path.trim());

    if path.exists() {
        let content = fs::read_to_string(path)?;

        let content = if search.is_empty() {
            replace.trim().to_string()
        } else {
            content.replace(search.trim(), replace.trim())
        };

        fs::write(path, content)?;
    } else if search.is_empty() {
        fs::write(path, replace.trim())?;
    }

    Ok(())
}
