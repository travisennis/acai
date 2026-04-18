//! System prompt construction for the AI agent.
//!
//! This module builds the system prompt that is sent to the AI model at the
//! start of each conversation. The prompt includes base instructions, project
//! context from AGENTS.md files, and environmental information.

use std::path::Path;

use chrono::Local;

use crate::config::AgentsFile;

/// Builds the system prompt for the AI agent.
///
/// Constructs a prompt that includes base instructions, project context from
/// AGENTS.md files, the current working directory, and today's date.
///
/// # Examples
///
/// ```
/// use cake::prompts::build_system_prompt;
/// use std::path::Path;
///
/// let prompt = build_system_prompt(Path::new("/project"), &[]);
/// assert!(prompt.contains("You are cake"));
/// assert!(prompt.contains("Current working directory: /project"));
/// ```
pub fn build_system_prompt(working_dir: &Path, agents_files: &[AgentsFile]) -> String {
    let mut prompt = String::from(
        "You are cake. You are running as a coding agent in a CLI on the user's computer.",
    );

    let context = format_agents_context(agents_files);
    if !context.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(&context);
    }

    // Append current working directory and today's date
    let today = Local::now().format("%Y-%m-%d").to_string();
    let working_dir_str = working_dir.to_string_lossy();
    prompt.push_str("\n\nCurrent working directory: ");
    prompt.push_str(&working_dir_str);
    prompt.push_str("\nToday's date: ");
    prompt.push_str(&today);

    prompt
}

/// Format AGENTS.md files into a Project Context section.
/// Returns an empty string if no files have non-empty content.
fn format_agents_context(agents_files: &[AgentsFile]) -> String {
    // Filter to only files with non-empty content
    let non_empty_files: Vec<_> = agents_files
        .iter()
        .filter(|f| !f.content.trim().is_empty())
        .collect();

    if non_empty_files.is_empty() {
        return String::new();
    }

    let mut context = String::from("## Additional Context:\n\n");

    context.push_str("Project and user instructions are shown below. Be sure to adhere to these instructions. IMPORTANT: These instructions OVERRIDE any default behavior and you MUST follow them exactly as written.");

    for file in non_empty_files {
        let entry = format!(
            "### {}\n\n<instructions>\n{}\n</instructions>\n\n",
            file.path,
            file.content.trim()
        );
        context.push_str(&entry);
    }

    context
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn empty_agents_files() {
        let prompt = build_system_prompt(Path::new("/tmp"), &[]);
        assert!(prompt.starts_with(
            "You are cake. You are running as a coding agent in a CLI on the user's computer."
        ));
        assert!(prompt.contains("Current working directory: /tmp"));
        assert!(prompt.contains("Today's date:"));
    }

    #[test]
    fn with_agents_files() {
        let files = vec![
            AgentsFile {
                path: "~/.cake/AGENTS.md".to_string(),
                content: "User level instructions".to_string(),
            },
            AgentsFile {
                path: "./AGENTS.md".to_string(),
                content: "Project level instructions".to_string(),
            },
        ];
        let prompt = build_system_prompt(Path::new("/tmp"), &files);
        assert!(prompt.contains("## Additional Context:"));
        assert!(prompt.contains("~/.cake/AGENTS.md"));
        assert!(prompt.contains("./AGENTS.md"));
        assert!(prompt.contains("<instructions>"));
        assert!(prompt.contains("User level instructions"));
        assert!(prompt.contains("Project level instructions"));
        assert!(prompt.contains("Current working directory: /tmp"));
        assert!(prompt.contains("Today's date:"));
    }

    #[test]
    fn only_user_agents_file() {
        let files = vec![AgentsFile {
            path: "~/.cake/AGENTS.md".to_string(),
            content: "User instructions".to_string(),
        }];
        let prompt = build_system_prompt(Path::new("/tmp"), &files);
        assert!(prompt.contains("## Additional Context:"));
        assert!(prompt.contains("~/.cake/AGENTS.md"));
        assert!(!prompt.contains("./AGENTS.md"));
        assert!(prompt.contains("Current working directory: /tmp"));
        assert!(prompt.contains("Today's date:"));
    }

    #[test]
    fn empty_content_skipped() {
        let files = vec![
            AgentsFile {
                path: "~/.cake/AGENTS.md".to_string(),
                content: String::new(),
            },
            AgentsFile {
                path: "./AGENTS.md".to_string(),
                content: "   ".to_string(), // whitespace only
            },
        ];
        let prompt = build_system_prompt(Path::new("/tmp"), &files);
        // Should not include Project Context section since all files are empty
        assert!(!prompt.contains("## Additional Context:"));
        // But should still include working directory and date
        assert!(prompt.contains("Current working directory: /tmp"));
        assert!(prompt.contains("Today's date:"));
    }
}
