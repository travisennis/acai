//! Initial prompt construction for the AI agent.
//!
//! This module builds the stable system prompt plus mutable context messages
//! that are sent to the AI model at the start of each conversation.

use std::path::Path;

use chrono::Local;

use crate::config::{AgentsFile, SkillCatalog};
use crate::models::Role;

const SKILL_USAGE_INSTRUCTIONS: &str = r"<skill_instructions>
The following skills provide specialized instructions for specific tasks.
When a task matches a skill's description, use your file-read tool to load
the SKILL.md at the listed location before proceeding.
When a skill references relative paths, resolve them against the skill's
directory (the parent of SKILL.md) and use absolute paths in tool calls.
</skill_instructions>";

/// Builds the stable system prompt for the AI agent.
pub fn build_system_prompt() -> String {
    String::from("You are cake. You are running as a coding agent in a CLI on the user's computer.")
}

/// Builds all initial prompt messages for the AI agent.
///
/// The first message is the stable system prompt. Mutable context such as
/// AGENTS.md contents, available skills, and environment context is emitted as
/// separate developer messages so it is not tied to the system prompt.
pub fn build_initial_prompt_messages(
    working_dir: &Path,
    agents_files: &[AgentsFile],
    skill_catalog: &SkillCatalog,
) -> Vec<(Role, String)> {
    let mut messages = vec![(Role::System, build_system_prompt())];
    let context = format_agents_context(agents_files);
    if !context.is_empty() {
        messages.push((Role::Developer, context));
    }

    if !skill_catalog.skills.is_empty() {
        let catalog_xml = skill_catalog.to_prompt_xml();
        if !catalog_xml.is_empty() {
            messages.push((
                Role::Developer,
                format!("## Skills\n\n{SKILL_USAGE_INSTRUCTIONS}\n\n{catalog_xml}"),
            ));
        }
    }

    let today = Local::now().format("%Y-%m-%d").to_string();
    let working_dir_str = working_dir.to_string_lossy();
    messages.push((
        Role::Developer,
        format!("Current working directory: {working_dir_str}\nToday's date: {today}"),
    ));

    messages
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

    let mut context = String::from("## Additional Context\n\n");

    context.push_str("Project and user instructions are shown below. Be sure to adhere to these instructions. IMPORTANT: These instructions OVERRIDE any default behavior and you MUST follow them exactly as written.");
    context.push_str("\n\n");

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
    use crate::config::skills::{Skill, SkillScope};
    use std::path::PathBuf;

    fn render_messages(messages: &[(Role, String)]) -> String {
        messages
            .iter()
            .map(|(role, content)| format!("{}:\n{}", role.as_str(), content))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n")
    }

    fn assert_prompt_snapshot(name: &str, messages: &[(Role, String)]) {
        let prompt = render_messages(messages);
        insta::with_settings!({
            filters => vec![(r"Today's date: \d{4}-\d{2}-\d{2}", "Today's date: [DATE]")]
        }, {
            insta::assert_snapshot!(name, prompt);
        });
    }

    #[test]
    fn empty_agents_files() {
        let messages =
            build_initial_prompt_messages(Path::new("/tmp"), &[], &SkillCatalog::empty());
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].0, Role::System);
        assert!(messages[0].1.starts_with(
            "You are cake. You are running as a coding agent in a CLI on the user's computer."
        ));
        assert_eq!(messages[1].0, Role::Developer);
        assert!(messages[1].1.contains("Current working directory: /tmp"));
        assert!(messages[1].1.contains("Today's date:"));
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
        let messages =
            build_initial_prompt_messages(Path::new("/tmp"), &files, &SkillCatalog::empty());
        let prompt = render_messages(&messages);
        assert!(prompt.contains("## Additional Context"));
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
        let messages =
            build_initial_prompt_messages(Path::new("/tmp"), &files, &SkillCatalog::empty());
        let prompt = render_messages(&messages);
        assert!(prompt.contains("## Additional Context"));
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
        let messages =
            build_initial_prompt_messages(Path::new("/tmp"), &files, &SkillCatalog::empty());
        let prompt = render_messages(&messages);
        // Should not include Project Context section since all files are empty
        assert!(!prompt.contains("## Additional Context"));
        // But should still include working directory and date
        assert!(prompt.contains("Current working directory: /tmp"));
        assert!(prompt.contains("Today's date:"));
    }

    #[test]
    fn with_skill_catalog() {
        let mut catalog = SkillCatalog::empty();
        catalog.skills.push(Skill {
            name: "debugging".to_string(),
            description: "How to debug things".to_string(),
            location: PathBuf::from("/path/SKILL.md"),
            base_directory: PathBuf::from("/path"),
            scope: SkillScope::Project,
        });

        let messages = build_initial_prompt_messages(Path::new("/tmp"), &[], &catalog);
        let prompt = render_messages(&messages);
        assert!(prompt.contains("## Skills"));
        assert!(prompt.contains("<skill_instructions>"));
        assert!(prompt.contains("<available_skills>"));
        assert!(prompt.contains("<name>debugging</name>"));
        assert!(prompt.contains("<description>How to debug things</description>"));
        assert!(prompt.contains("Current working directory: /tmp"));
    }

    #[test]
    fn with_agents_and_skills() {
        let files = vec![AgentsFile {
            path: "./AGENTS.md".to_string(),
            content: "Project instructions".to_string(),
        }];
        let mut catalog = SkillCatalog::empty();
        catalog.skills.push(Skill {
            name: "test-skill".to_string(),
            description: "A test".to_string(),
            location: PathBuf::from("/a/SKILL.md"),
            base_directory: PathBuf::from("/a"),
            scope: SkillScope::Project,
        });

        let messages = build_initial_prompt_messages(Path::new("/tmp"), &files, &catalog);
        let prompt = render_messages(&messages);
        // AGENTS.md comes before Skills
        let agents_pos = prompt.find("## Additional Context").unwrap();
        let skills_pos = prompt.find("## Skills").unwrap();
        assert!(agents_pos < skills_pos);
    }

    #[test]
    fn snapshot_empty_prompt() {
        let messages =
            build_initial_prompt_messages(Path::new("/tmp"), &[], &SkillCatalog::empty());
        assert_prompt_snapshot("prompt_empty", &messages);
    }

    #[test]
    fn snapshot_with_project_agents() {
        let files = vec![AgentsFile {
            path: "./AGENTS.md".to_string(),
            content: "You are a Rust expert. Follow all project conventions.".to_string(),
        }];
        let messages =
            build_initial_prompt_messages(Path::new("/project"), &files, &SkillCatalog::empty());
        assert_prompt_snapshot("prompt_with_project_agents", &messages);
    }

    #[test]
    fn snapshot_with_user_and_project_agents() {
        let files = vec![
            AgentsFile {
                path: "~/.cake/AGENTS.md".to_string(),
                content: "User-level global instructions.".to_string(),
            },
            AgentsFile {
                path: "./AGENTS.md".to_string(),
                content: "Project-level overrides.".to_string(),
            },
        ];
        let messages =
            build_initial_prompt_messages(Path::new("/project"), &files, &SkillCatalog::empty());
        assert_prompt_snapshot("prompt_with_user_and_project_agents", &messages);
    }

    #[test]
    fn snapshot_with_skill_catalog() {
        let mut catalog = SkillCatalog::empty();
        catalog.skills.push(Skill {
            name: "debugging".to_string(),
            description: "How to debug Rust programs".to_string(),
            location: PathBuf::from("/project/.agents/skills/debugging/SKILL.md"),
            base_directory: PathBuf::from("/project/.agents/skills/debugging"),
            scope: SkillScope::Project,
        });
        let messages = build_initial_prompt_messages(Path::new("/project"), &[], &catalog);
        assert_prompt_snapshot("prompt_with_skill_catalog", &messages);
    }

    #[test]
    fn snapshot_with_agents_and_skills() {
        let files = vec![AgentsFile {
            path: "./AGENTS.md".to_string(),
            content: "Project instructions for all contributors.".to_string(),
        }];
        let mut catalog = SkillCatalog::empty();
        catalog.skills.push(Skill {
            name: "debugging".to_string(),
            description: "How to debug Rust programs".to_string(),
            location: PathBuf::from("/project/.agents/skills/debugging/SKILL.md"),
            base_directory: PathBuf::from("/project/.agents/skills/debugging"),
            scope: SkillScope::Project,
        });
        let messages = build_initial_prompt_messages(Path::new("/project"), &files, &catalog);
        assert_prompt_snapshot("prompt_with_agents_and_skills", &messages);
    }
}
