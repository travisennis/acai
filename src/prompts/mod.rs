use std::fmt::Write;
use std::path::Path;

use crate::config::AgentsFile;

/// Build the system prompt including AGENTS.md content from user and project levels.
pub fn build_system_prompt(_working_dir: &Path, agents_files: &[AgentsFile]) -> String {
    let mut prompt = String::from(
        "You are acai. You are running as a coding agent in a CLI on the user's computer.",
    );

    let context = format_agents_context(agents_files);
    if !context.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(&context);
    }

    prompt
}

/// Format AGENTS.md files into a Project Context section.
/// Returns an empty string if no files have non-empty content.
#[allow(clippy::expect_used)]
fn format_agents_context(agents_files: &[AgentsFile]) -> String {
    // Filter to only files with non-empty content
    let non_empty_files: Vec<_> = agents_files
        .iter()
        .filter(|f| !f.content.trim().is_empty())
        .collect();

    if non_empty_files.is_empty() {
        return String::new();
    }

    let mut context = String::from("## Project Context:\n\n");

    for file in non_empty_files {
        write!(
            context,
            "### {}\n\n<instructions>\n{}\n</instructions>\n\n",
            file.path,
            file.content.trim()
        )
        .expect("write to String cannot fail");
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
        assert_eq!(
            prompt,
            "You are acai. You are running as a coding agent in a CLI on the user's computer."
        );
    }

    #[test]
    fn with_agents_files() {
        let files = vec![
            AgentsFile {
                path: "~/.acai/AGENTS.md".to_string(),
                content: "User level instructions".to_string(),
            },
            AgentsFile {
                path: "./AGENTS.md".to_string(),
                content: "Project level instructions".to_string(),
            },
        ];
        let prompt = build_system_prompt(Path::new("/tmp"), &files);
        assert!(prompt.contains("## Project Context:"));
        assert!(prompt.contains("~/.acai/AGENTS.md"));
        assert!(prompt.contains("./AGENTS.md"));
        assert!(prompt.contains("<instructions>"));
        assert!(prompt.contains("User level instructions"));
        assert!(prompt.contains("Project level instructions"));
    }

    #[test]
    fn only_user_agents_file() {
        let files = vec![AgentsFile {
            path: "~/.acai/AGENTS.md".to_string(),
            content: "User instructions".to_string(),
        }];
        let prompt = build_system_prompt(Path::new("/tmp"), &files);
        assert!(prompt.contains("## Project Context:"));
        assert!(prompt.contains("~/.acai/AGENTS.md"));
        assert!(!prompt.contains("./AGENTS.md"));
    }

    #[test]
    fn empty_content_skipped() {
        let files = vec![
            AgentsFile {
                path: "~/.acai/AGENTS.md".to_string(),
                content: String::new(),
            },
            AgentsFile {
                path: "./AGENTS.md".to_string(),
                content: "   ".to_string(), // whitespace only
            },
        ];
        let prompt = build_system_prompt(Path::new("/tmp"), &files);
        // Should not include Project Context section since all files are empty
        assert!(!prompt.contains("## Project Context:"));
    }
}
