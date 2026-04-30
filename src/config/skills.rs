//! Skill discovery, parsing, and catalog management for cake.
//!
//! Skills provide specialized instructions for specific tasks. They are discovered
//! from `.agents/skills/` directories at both project and user levels.
//!
//! Each skill is defined by a `SKILL.md` file with YAML frontmatter containing
//! metadata (name, description) and markdown body content.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

// =============================================================================
// Core Types
// =============================================================================

/// A discovered skill with parsed metadata.
///
/// Skills are loaded from `SKILL.md` files found in `.agents/skills/` subdirectories.
/// The body content is lazy-loaded at activation time to minimize memory usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// Skill name (from YAML frontmatter)
    pub name: String,
    /// Skill description (from YAML frontmatter)
    pub description: String,
    /// Absolute path to the SKILL.md file
    pub location: PathBuf,
    /// Parent directory of SKILL.md (base for resolving relative paths)
    pub base_directory: PathBuf,
    /// Scope indicating whether this is a project-level or user-level skill
    pub scope: SkillScope,
}

/// Scope of a skill indicating its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillScope {
    /// Skill discovered from the project's `.agents/skills/` directory
    Project,
    /// Skill discovered from the user's `~/.agents/skills/` directory
    User,
}

/// Diagnostic level for skill parsing/loading issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    /// Non-fatal issue (e.g., name mismatch)
    Warning,
    /// Fatal issue that prevents the skill from being loaded
    Error,
}

/// A diagnostic message about a skill.
#[derive(Debug, Clone)]
pub struct SkillDiagnostic {
    /// Severity level
    pub level: DiagnosticLevel,
    /// Human-readable message
    pub message: String,
    /// Path to the file that caused the diagnostic
    pub file: PathBuf,
}

/// Collection of discovered skills with diagnostics.
#[derive(Debug, Clone)]
pub struct SkillCatalog {
    /// Discovered skills (filtered by configuration)
    pub skills: Vec<Skill>,
    /// Diagnostics from discovery/parsing
    pub diagnostics: Vec<SkillDiagnostic>,
}

// =============================================================================
// Skill Parsing
// =============================================================================

/// Parsed YAML frontmatter from a SKILL.md file.
#[derive(Debug, serde::Deserialize)]
struct SkillFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

impl Skill {
    /// Parse a SKILL.md file and extract metadata.
    ///
    /// Reads the file, extracts YAML frontmatter between `---` delimiters,
    /// and validates required fields.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if the file cannot be read, has invalid frontmatter,
    /// or is missing required fields.
    pub fn parse(path: &Path, scope: SkillScope) -> Result<Self, SkillDiagnostic> {
        let content = std::fs::read_to_string(path).map_err(|e| SkillDiagnostic {
            level: DiagnosticLevel::Error,
            message: format!("Failed to read SKILL.md: {e}"),
            file: path.to_path_buf(),
        })?;

        // Extract YAML frontmatter between --- delimiters
        let (name, description) = Self::parse_frontmatter(&content, path)?;

        let base_directory = path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);

        Ok(Self {
            name,
            description,
            location: path.to_path_buf(),
            base_directory,
            scope,
        })
    }

    /// Extract YAML frontmatter from skill content.
    fn parse_frontmatter(content: &str, path: &Path) -> Result<(String, String), SkillDiagnostic> {
        let trimmed = content.trim_start();

        // Check for opening ---
        if !trimmed.starts_with("---") {
            return Err(SkillDiagnostic {
                level: DiagnosticLevel::Error,
                message: "SKILL.md missing YAML frontmatter (expected '---' at start)".to_string(),
                file: path.to_path_buf(),
            });
        }

        // Find closing ---
        let after_open = &trimmed[3..]; // skip "---"
        let close_idx = after_open.find("\n---");

        let yaml_text = match close_idx {
            Some(idx) => &after_open[..idx],
            None => {
                return Err(SkillDiagnostic {
                    level: DiagnosticLevel::Error,
                    message: "SKILL.md frontmatter not closed (expected closing '---')".to_string(),
                    file: path.to_path_buf(),
                });
            },
        };

        // Try parsing with serde_yaml first
        let frontmatter: SkillFrontmatter = match serde_yaml::from_str(yaml_text) {
            Ok(fm) => fm,
            Err(_e) => {
                // Try fallback: the YAML might have issues like unquoted colons in values
                // Retry with a more lenient approach
                return Self::parse_frontmatter_fallback(yaml_text, path);
            },
        };

        let name = frontmatter.name.ok_or_else(|| SkillDiagnostic {
            level: DiagnosticLevel::Error,
            message: "SKILL.md missing required field 'name' in frontmatter".to_string(),
            file: path.to_path_buf(),
        })?;

        let description = frontmatter.description.ok_or_else(|| SkillDiagnostic {
            level: DiagnosticLevel::Error,
            message: "SKILL.md missing required field 'description' in frontmatter".to_string(),
            file: path.to_path_buf(),
        })?;

        Ok((name, description))
    }

    /// Fallback parser for malformed YAML frontmatter.
    ///
    /// Handles common issues like unquoted colons by extracting key-value pairs manually.
    fn parse_frontmatter_fallback(
        yaml_text: &str,
        path: &Path,
    ) -> Result<(String, String), SkillDiagnostic> {
        let mut name = None;
        let mut description = None;
        let mut current_key: Option<String> = None;
        let mut current_value = String::new();

        for line in yaml_text.lines() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }

            // Check if this is a new key (starts with non-whitespace and contains :)
            if !trimmed.starts_with(' ') && !trimmed.starts_with('\t') {
                // Save previous key-value pair
                if let Some(key) = current_key.take() {
                    let value = current_value.trim().to_string();
                    if key == "name" {
                        name = Some(value);
                    } else if key == "description" {
                        description = Some(value);
                    }
                }

                // Parse new key
                if let Some(colon_idx) = trimmed.find(':') {
                    let key = trimmed[..colon_idx].trim().to_string();
                    let value_after = trimmed[colon_idx + 1..].trim().to_string();
                    current_key = Some(key);
                    current_value = value_after;
                }
            } else {
                // Continuation of previous value (multiline)
                current_value.push('\n');
                current_value.push_str(trimmed);
            }
        }

        // Save the last key-value pair
        if let Some(key) = current_key {
            let value = current_value.trim().to_string();
            if key == "name" {
                name = Some(value);
            } else if key == "description" {
                description = Some(value);
            }
        }

        let name = name.ok_or_else(|| SkillDiagnostic {
            level: DiagnosticLevel::Error,
            message: "SKILL.md missing required field 'name' in frontmatter".to_string(),
            file: path.to_path_buf(),
        })?;

        let description = description.ok_or_else(|| SkillDiagnostic {
            level: DiagnosticLevel::Error,
            message: "SKILL.md missing required field 'description' in frontmatter".to_string(),
            file: path.to_path_buf(),
        })?;

        Ok((name, description))
    }

    /// Load the full body content of the skill (markdown after frontmatter).
    ///
    /// This is lazy-loaded at activation time, not during discovery.
    #[allow(dead_code)]
    pub fn load_body(&self) -> Result<String, std::io::Error> {
        let content = std::fs::read_to_string(&self.location)?;

        // Strip frontmatter
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return Ok(content);
        }

        let after_open = &trimmed[3..];
        if let Some(close_idx) = after_open.find("\n---") {
            let body_start = close_idx + 4; // skip past "\n---"
            let body = &after_open[body_start..];
            return Ok(body.trim_start().to_string());
        }

        Ok(content)
    }
}

// =============================================================================
// Skill Catalog
// =============================================================================

impl SkillCatalog {
    /// Create an empty skill catalog.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            skills: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Check if a path corresponds to a known skill location.
    #[allow(dead_code)]
    pub fn get_skill_by_location(&self, path: &Path) -> Option<&Skill> {
        self.skills.iter().find(|s| s.location == path)
    }

    /// Filter catalog to only include specified skills.
    pub fn filter_to(&mut self, skill_names: &[String]) {
        self.skills.retain(|s| skill_names.contains(&s.name));
    }

    /// Create a disabled catalog (no skills).
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            skills: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Generate XML catalog for system prompt.
    ///
    /// Returns an empty string if no valid skills are in the catalog.
    pub fn to_prompt_xml(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }

        let mut xml = String::from("<available_skills>\n");
        for skill in &self.skills {
            use std::fmt::Write;
            xml.push_str("  <skill>\n");
            let _ = writeln!(xml, "    <name>{}</name>", xml_escape(&skill.name));
            let _ = writeln!(
                xml,
                "    <description>{}</description>",
                xml_escape(&skill.description)
            );
            let _ = writeln!(xml, "    <location>{}</location>", skill.location.display());
            xml.push_str("  </skill>\n");
        }
        xml.push_str("</available_skills>");
        xml
    }
}

/// Escape special XML characters in a string.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// =============================================================================
// Skill Discovery
// =============================================================================

/// Directories to skip during skill discovery.
const EXCLUDED_DIRS: &[&str] = &[".git", "node_modules", "target"];

/// Maximum directory depth for skill discovery.
const MAX_DEPTH: usize = 4;

/// Maximum number of directories to scan.
const MAX_DIRS: usize = 2000;

/// Discover skills from the filesystem.
///
/// Scans project-level and user-level skill directories for SKILL.md files.
/// Project skills take precedence over user skills with the same name.
///
/// # Arguments
///
/// * `working_dir` - The current working directory (for finding project-level skills)
///
/// Returns a `SkillCatalog` with discovered skills and any diagnostics.
pub fn discover_skills(working_dir: &Path) -> SkillCatalog {
    let mut catalog = SkillCatalog::empty();
    let mut scanned_dirs = 0;

    // Scan paths: project first, then user
    let project_skills_dir = working_dir.join(".agents").join("skills");
    let user_skills_dir = dirs::home_dir().map(|h| h.join(".agents").join("skills"));

    // Collect project skills first
    let mut project_skill_names = HashSet::new();
    if project_skills_dir.exists() && project_skills_dir.is_dir() {
        scan_directory(
            &project_skills_dir,
            SkillScope::Project,
            &mut catalog,
            &mut project_skill_names,
            &mut scanned_dirs,
            0,
        );
    }

    // Then collect user skills (skip if name collision with project)
    if let Some(ref user_dir) = user_skills_dir
        && user_dir.exists()
        && user_dir.is_dir()
    {
        let mut user_skill_names = HashSet::new();
        scan_directory(
            user_dir,
            SkillScope::User,
            &mut catalog,
            &mut user_skill_names,
            &mut scanned_dirs,
            0,
        );

        // Filter out user skills that collide with project skills
        catalog.skills.retain(|s| {
            if s.scope == SkillScope::User && project_skill_names.contains(&s.name) {
                catalog.diagnostics.push(SkillDiagnostic {
                    level: DiagnosticLevel::Warning,
                    message: format!(
                        "User skill '{}' shadowed by project skill with same name",
                        s.name
                    ),
                    file: s.location.clone(),
                });
                false
            } else {
                true
            }
        });
    }

    catalog
}

/// Recursively scan a directory for SKILL.md files.
fn scan_directory(
    dir: &Path,
    scope: SkillScope,
    catalog: &mut SkillCatalog,
    names_seen: &mut HashSet<String>,
    scanned_dirs: &mut usize,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    if *scanned_dirs >= MAX_DIRS {
        catalog.diagnostics.push(SkillDiagnostic {
            level: DiagnosticLevel::Warning,
            message: format!("Skill discovery stopped after scanning {MAX_DIRS} directories"),
            file: dir.to_path_buf(),
        });
        return;
    }
    *scanned_dirs += 1;

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            catalog.diagnostics.push(SkillDiagnostic {
                level: DiagnosticLevel::Warning,
                message: format!("Failed to read directory: {e}"),
                file: dir.to_path_buf(),
            });
            return;
        },
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip excluded directories
        if EXCLUDED_DIRS.contains(&name) {
            continue;
        }

        // Check if this directory contains a SKILL.md
        let skill_md = path.join("SKILL.md");
        if skill_md.exists() && skill_md.is_file() {
            match Skill::parse(&skill_md, scope) {
                Ok(skill) => {
                    if names_seen.contains(&skill.name) {
                        catalog.diagnostics.push(SkillDiagnostic {
                            level: DiagnosticLevel::Warning,
                            message: format!(
                                "Duplicate skill name '{}' within same scope, using first found",
                                skill.name
                            ),
                            file: skill.location.clone(),
                        });
                    } else {
                        names_seen.insert(skill.name.clone());
                        catalog.skills.push(skill);
                    }
                },
                Err(diagnostic) => {
                    catalog.diagnostics.push(diagnostic);
                },
            }
        }

        // Recurse into subdirectories
        scan_directory(&path, scope, catalog, names_seen, scanned_dirs, depth + 1);
    }
}

// =============================================================================
// Skill Configuration
// =============================================================================

/// Resolved skill configuration from CLI and settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillConfig {
    /// Load all discovered skills
    All,
    /// Don't load any skills
    Disabled,
    /// Load only these named skills
    Only(Vec<String>),
}

impl SkillConfig {
    /// Apply this configuration to a skill catalog.
    ///
    /// Returns a new catalog with skills filtered according to the configuration.
    pub fn apply(&self, mut catalog: SkillCatalog) -> SkillCatalog {
        match *self {
            Self::All => catalog,
            Self::Disabled => SkillCatalog::disabled(),
            Self::Only(ref names) => {
                catalog.filter_to(names);
                catalog
            },
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_skill_file(dir: &Path, name: &str, description: &str) -> PathBuf {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let content = format!(
            "---\nname: {name}\ndescription: |\n  {description}\n---\n\n# {name}\n\nSkill content here.\n"
        );
        let path = skill_dir.join("SKILL.md");
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn skill_parse_valid() {
        let tmp = TempDir::new().unwrap();
        let path = create_skill_file(tmp.path(), "test-skill", "A test skill description");

        let skill = Skill::parse(&path, SkillScope::Project).unwrap();
        assert_eq!(skill.name, "test-skill");
        assert_eq!(skill.description, "A test skill description");
        assert_eq!(skill.scope, SkillScope::Project);
        assert_eq!(skill.location, path);
        assert_eq!(skill.base_directory, path.parent().unwrap());
    }

    #[test]
    fn skill_parse_multiline_description() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("my-skill");
        std::fs::create_dir_all(&dir).unwrap();
        let content = r"---
name: my-skill
description: |
  First line of description
  Second line of description
---

# My Skill
";
        let path = dir.join("SKILL.md");
        std::fs::write(&path, content).unwrap();

        let skill = Skill::parse(&path, SkillScope::Project).unwrap();
        assert_eq!(skill.name, "my-skill");
        assert!(skill.description.contains("First line"));
        assert!(skill.description.contains("Second line"));
    }

    #[test]
    fn skill_parse_missing_name() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("bad-skill");
        std::fs::create_dir_all(&dir).unwrap();
        let content = r"---
description: Something
---
";
        let path = dir.join("SKILL.md");
        std::fs::write(&path, content).unwrap();

        let result = Skill::parse(&path, SkillScope::Project);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.level, DiagnosticLevel::Error);
        assert!(err.message.contains("name"));
    }

    #[test]
    fn skill_parse_missing_description() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("bad-skill");
        std::fs::create_dir_all(&dir).unwrap();
        let content = r"---
name: something
---
";
        let path = dir.join("SKILL.md");
        std::fs::write(&path, content).unwrap();

        let result = Skill::parse(&path, SkillScope::Project);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.level, DiagnosticLevel::Error);
        assert!(err.message.contains("description"));
    }

    #[test]
    fn skill_parse_missing_frontmatter() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("bad-skill");
        std::fs::create_dir_all(&dir).unwrap();
        let content = "# Just markdown\n";
        let path = dir.join("SKILL.md");
        std::fs::write(&path, content).unwrap();

        let result = Skill::parse(&path, SkillScope::Project);
        assert!(result.is_err());
    }

    #[test]
    fn skill_load_body() {
        let tmp = TempDir::new().unwrap();
        let path = create_skill_file(tmp.path(), "body-test", "A test");

        let skill = Skill::parse(&path, SkillScope::Project).unwrap();
        let body = skill.load_body().unwrap();
        assert!(body.contains("# body-test"));
        assert!(body.contains("Skill content here"));
        assert!(!body.contains("---"));
    }

    #[test]
    fn catalog_get_skill_by_location() {
        let tmp = TempDir::new().unwrap();
        let path = create_skill_file(tmp.path(), "loc-test", "A test");

        let mut catalog = SkillCatalog::empty();
        let skill = Skill::parse(&path, SkillScope::Project).unwrap();
        catalog.skills.push(skill);

        assert!(catalog.get_skill_by_location(&path).is_some());
        assert!(
            catalog
                .get_skill_by_location(Path::new("/nonexistent"))
                .is_none()
        );
    }

    #[test]
    fn catalog_filter_to() {
        let mut catalog = SkillCatalog::empty();
        catalog.skills.push(Skill {
            name: "skill-a".to_string(),
            description: "A".to_string(),
            location: PathBuf::from("/a"),
            base_directory: PathBuf::from("/"),
            scope: SkillScope::Project,
        });
        catalog.skills.push(Skill {
            name: "skill-b".to_string(),
            description: "B".to_string(),
            location: PathBuf::from("/b"),
            base_directory: PathBuf::from("/"),
            scope: SkillScope::Project,
        });
        catalog.skills.push(Skill {
            name: "skill-c".to_string(),
            description: "C".to_string(),
            location: PathBuf::from("/c"),
            base_directory: PathBuf::from("/"),
            scope: SkillScope::Project,
        });

        catalog.filter_to(&["skill-a".to_string(), "skill-c".to_string()]);
        assert_eq!(catalog.skills.len(), 2);
        assert_eq!(catalog.skills[0].name, "skill-a");
        assert_eq!(catalog.skills[1].name, "skill-c");
    }

    #[test]
    fn catalog_to_prompt_xml() {
        let mut catalog = SkillCatalog::empty();
        catalog.skills.push(Skill {
            name: "debugging".to_string(),
            description: "How to debug things".to_string(),
            location: PathBuf::from("/path/to/debugging/SKILL.md"),
            base_directory: PathBuf::from("/path/to/debugging"),
            scope: SkillScope::Project,
        });

        let xml = catalog.to_prompt_xml();
        assert!(xml.contains("<available_skills>"));
        assert!(xml.contains("<name>debugging</name>"));
        assert!(xml.contains("<description>How to debug things</description>"));
        assert!(xml.contains("<location>/path/to/debugging/SKILL.md</location>"));
        assert!(xml.contains("</available_skills>"));
    }

    #[test]
    fn snapshot_skill_catalog_single_skill_xml() {
        let mut catalog = SkillCatalog::empty();
        catalog.skills.push(Skill {
            name: "debugging".to_string(),
            description: "How to debug Rust programs".to_string(),
            location: PathBuf::from("/project/.agents/skills/debugging/SKILL.md"),
            base_directory: PathBuf::from("/project/.agents/skills/debugging"),
            scope: SkillScope::Project,
        });

        insta::assert_snapshot!("skill_catalog_single_skill_xml", catalog.to_prompt_xml());
    }

    #[test]
    fn catalog_to_prompt_xml_empty() {
        let catalog = SkillCatalog::empty();
        assert_eq!(catalog.to_prompt_xml(), "");
    }

    #[test]
    fn catalog_to_prompt_xml_escapes() {
        let mut catalog = SkillCatalog::empty();
        catalog.skills.push(Skill {
            name: "test <&>".to_string(),
            description: "Desc with <tag> & \"quotes\"".to_string(),
            location: PathBuf::from("/a"),
            base_directory: PathBuf::from("/"),
            scope: SkillScope::Project,
        });

        let xml = catalog.to_prompt_xml();
        assert!(xml.contains("&lt;"));
        assert!(xml.contains("&gt;"));
        assert!(xml.contains("&amp;"));
        assert!(xml.contains("&quot;"));
    }

    #[test]
    fn discover_skills_finds_project_skills() {
        let tmp = TempDir::new().unwrap();
        let agents_dir = tmp.path().join(".agents").join("skills");
        std::fs::create_dir_all(&agents_dir).unwrap();
        create_skill_file(&agents_dir, "skill-a", "First skill");
        create_skill_file(&agents_dir, "skill-b", "Second skill");

        let catalog = discover_skills(tmp.path());
        let project_skills: Vec<_> = catalog
            .skills
            .iter()
            .filter(|s| s.scope == SkillScope::Project)
            .collect();
        assert_eq!(project_skills.len(), 2);
        let names: Vec<_> = project_skills.iter().map(|s| s.name.clone()).collect();
        assert!(names.contains(&"skill-a".to_string()));
        assert!(names.contains(&"skill-b".to_string()));
    }

    #[test]
    fn discover_skills_skips_excluded_dirs() {
        let tmp = TempDir::new().unwrap();
        let agents_dir = tmp.path().join(".agents").join("skills");
        std::fs::create_dir_all(&agents_dir).unwrap();

        // Create a skill inside .git (should be skipped)
        let git_dir = agents_dir.join(".git").join("nested");
        std::fs::create_dir_all(&git_dir).unwrap();
        create_skill_file(&git_dir, "git-skill", "Should not appear");

        // Create a normal skill
        create_skill_file(&agents_dir, "normal-skill", "Should appear");

        let catalog = discover_skills(tmp.path());
        let project_skills: Vec<_> = catalog
            .skills
            .iter()
            .filter(|s| s.scope == SkillScope::Project)
            .collect();
        assert_eq!(project_skills.len(), 1);
        assert_eq!(project_skills[0].name, "normal-skill");
    }

    #[test]
    fn skill_config_apply_disabled() {
        let mut catalog = SkillCatalog::empty();
        catalog.skills.push(Skill {
            name: "a".to_string(),
            description: "A".to_string(),
            location: PathBuf::from("/a"),
            base_directory: PathBuf::from("/"),
            scope: SkillScope::Project,
        });

        let result = SkillConfig::Disabled.apply(catalog);
        assert!(result.skills.is_empty());
    }

    #[test]
    fn skill_config_apply_only() {
        let mut catalog = SkillCatalog::empty();
        catalog.skills.push(Skill {
            name: "a".to_string(),
            description: "A".to_string(),
            location: PathBuf::from("/a"),
            base_directory: PathBuf::from("/"),
            scope: SkillScope::Project,
        });
        catalog.skills.push(Skill {
            name: "b".to_string(),
            description: "B".to_string(),
            location: PathBuf::from("/b"),
            base_directory: PathBuf::from("/"),
            scope: SkillScope::Project,
        });

        let result = SkillConfig::Only(vec!["a".to_string()]).apply(catalog);
        assert_eq!(result.skills.len(), 1);
        assert_eq!(result.skills[0].name, "a");
    }

    #[test]
    fn discover_skills_respects_max_depth() {
        let tmp = TempDir::new().unwrap();
        let agents_dir = tmp.path().join(".agents").join("skills");
        std::fs::create_dir_all(&agents_dir).unwrap();

        // Create nested structure deeper than MAX_DEPTH
        let mut deep_dir = agents_dir.clone();
        for i in 0..=MAX_DEPTH + 2 {
            deep_dir = deep_dir.join(format!("level{i}"));
            std::fs::create_dir_all(&deep_dir).unwrap();
        }
        create_skill_file(&deep_dir, "deep-skill", "Too deep");

        // Create a shallow skill
        create_skill_file(&agents_dir, "shallow-skill", "Should appear");

        let catalog = discover_skills(tmp.path());
        let project_skills: Vec<_> = catalog
            .skills
            .iter()
            .filter(|s| s.scope == SkillScope::Project)
            .collect();
        assert_eq!(project_skills.len(), 1);
        assert_eq!(project_skills[0].name, "shallow-skill");
    }
}
