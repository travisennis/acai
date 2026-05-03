//! Tool definitions and execution for the AI agent.
//!
//! This module provides the tool interface that allows the AI agent to interact
//! with the host system through controlled operations. All tools are sandboxed
//! to restrict file access to the working directory and allowed paths.
//!
//! # Available Tools
//!
//! - `Bash` - Execute shell commands with timeout and output capture
//! - `Read` - Read file contents with line range support
//! - `Edit` - Make targeted edits to files using literal search-replace
//! - `Write` - Create or overwrite files with content
//!
//! # Security
//!
//! All tools validate paths against the current working directory and
//! directories added via `--add-dir` flag. Write operations are only allowed
//! in the working directory and temp directories.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

mod sandbox;

// =============================================================================
// Cached Directory Lookups
// =============================================================================

static CWD: OnceLock<Result<PathBuf, String>> = OnceLock::new();
static TEMP_DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();

fn cached_cwd() -> Result<&'static PathBuf, String> {
    CWD.get_or_init(|| {
        std::env::current_dir().map_err(|e| format!("Failed to get working directory: {e}"))
    })
    .as_ref()
    .map_err(String::clone)
}

fn cached_temp_dirs() -> &'static [PathBuf] {
    TEMP_DIRS.get_or_init(compute_temp_directories)
}

fn compute_temp_directories() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // Include symlink path first, then canonical path.
    // On macOS, /tmp -> /private/tmp and /var/folders -> /private/var/folders.
    // Both forms are needed so that ancestor literals and subpath rules
    // cover the paths regardless of which form a process uses.
    dirs.push(PathBuf::from("/tmp"));
    if let Ok(canonical) = std::fs::canonicalize("/tmp")
        && canonical.as_path() != Path::new("/tmp")
    {
        dirs.push(canonical);
    }

    dirs.push(PathBuf::from("/var/folders"));
    if let Ok(canonical) = std::fs::canonicalize("/var/folders")
        && canonical.as_path() != Path::new("/var/folders")
    {
        dirs.push(canonical);
    }

    if let Ok(tmpdir) = std::env::var("TMPDIR") {
        let tmpdir_path = PathBuf::from(&tmpdir);
        dirs.push(tmpdir_path.clone());
        if let Ok(canonical) = std::fs::canonicalize(&tmpdir)
            && canonical != tmpdir_path
        {
            dirs.push(canonical);
        }
    }

    dirs
}

// =============================================================================
// Global Directory Storage
// =============================================================================

// Global storage for additional directories added via --add-dir flag.
// These directories are read-only for the agent.
// Uses OnceLock instead of thread_local! to work correctly with
// Tokio's multi-threaded runtime where tool execution may run on
// a different thread than the startup thread.
static ADDITIONAL_DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
// Skill base directories (parent dirs of SKILL.md files) - read-only access
static SKILL_DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
// Settings directories (from settings.toml) - read-write access
static SETTINGS_DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();

/// Set the additional directories globally.
/// This should be called once at startup from main.
pub fn set_additional_dirs(dirs: Vec<PathBuf>) {
    let _ = ADDITIONAL_DIRS.set(dirs);
}

/// Get the additional directories globally.
pub fn get_additional_dirs() -> Vec<PathBuf> {
    ADDITIONAL_DIRS.get().cloned().unwrap_or_default()
}

/// Set the skill directories globally.
/// These directories are granted read-only access for the Read tool.
pub fn set_skill_dirs(dirs: Vec<PathBuf>) {
    let _ = SKILL_DIRS.set(dirs);
}

/// Get the skill directories globally.
pub fn get_skill_dirs() -> Vec<PathBuf> {
    SKILL_DIRS.get().cloned().unwrap_or_default()
}

/// Set the settings directories globally.
/// These directories are granted read-write access and are loaded from
/// settings.toml (both global and project-level).
pub fn set_settings_dirs(dirs: Vec<PathBuf>) {
    let _ = SETTINGS_DIRS.set(dirs);
}

/// Get the settings directories globally.
pub fn get_settings_dirs() -> Vec<PathBuf> {
    SETTINGS_DIRS.get().cloned().unwrap_or_default()
}

// =============================================================================
// Module Declarations
// =============================================================================

mod bash;
mod bash_safety;
mod edit;
pub mod read;
mod write;

// =============================================================================
// Tool Types
// =============================================================================

/// Tool definition sent in API requests.
///
/// Represents a function tool that the AI model can call during conversation.
/// Each tool has a name, description, and JSON schema for its parameters.
///
/// # Example
///
/// ```
/// use cake::clients::tools::bash_tool;
/// let tool = bash_tool();
/// assert_eq!(tool.name, "Bash");
/// ```
#[derive(Serialize, Clone, Debug)]
pub struct Tool {
    #[serde(rename = "type")]
    pub(super) type_: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) parameters: serde_json::Value,
}

/// Result of executing a tool.
///
/// Contains the output string from tool execution, which may be stdout/stderr
/// for Bash or file contents for Read operations.
#[derive(Debug)]
pub struct ToolResult {
    pub output: String,
}

// =============================================================================
// Path Validation
// =============================================================================

/// Access level for a validated path
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PathAccess {
    /// Path is in a read-write location (cwd, temp dirs)
    ReadWrite,
    /// Path is in a read-only location (--add-dir directories)
    ReadOnly,
}

/// Result of path validation containing the canonical path and access level
#[derive(Debug)]
pub(super) struct ValidatedPath {
    pub canonical: std::path::PathBuf,
    pub access: PathAccess,
}

/// Validate that a path exists and is within the current working directory, allowed temp directories,
/// or directories added via --add-dir flag (read-only access).
///
/// Returns the canonical path along with its access level.
pub(super) fn validate_path(path_str: &str) -> Result<ValidatedPath, String> {
    let cwd = cached_cwd()?;
    validate_path_with_dirs(
        path_str,
        cwd,
        cached_temp_dirs(),
        &get_settings_dirs(),
        &get_additional_dirs(),
        &get_skill_dirs(),
    )
}

/// Core path validation logic, separated for testability.
fn validate_path_with_dirs(
    path_str: &str,
    cwd: &Path,
    temp_dirs: &[PathBuf],
    settings_dirs: &[PathBuf],
    additional_dirs: &[PathBuf],
    skill_dirs: &[PathBuf],
) -> Result<ValidatedPath, String> {
    let path = Path::new(path_str);

    // Canonicalize the path (resolve symlinks, relative paths, etc.)
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Path not found or not accessible '{}': {e}", path.display()))?;

    // Check if path is within working directory (read-write)
    if path_starts_with(&canonical, &[cwd.to_path_buf()]) {
        return Ok(ValidatedPath {
            canonical,
            access: PathAccess::ReadWrite,
        });
    }

    // Allow paths in standard temp directories (read-write)
    if path_starts_with(&canonical, temp_dirs) {
        return Ok(ValidatedPath {
            canonical,
            access: PathAccess::ReadWrite,
        });
    }

    // Allow paths in settings directories from settings.toml (read-write)
    if path_starts_with(&canonical, settings_dirs) {
        return Ok(ValidatedPath {
            canonical,
            access: PathAccess::ReadWrite,
        });
    }

    // Allow paths in directories added via --add-dir flag (read-only)
    if path_starts_with(&canonical, additional_dirs) {
        return Ok(ValidatedPath {
            canonical,
            access: PathAccess::ReadOnly,
        });
    }

    // Allow paths in skill directories (read-only)
    if path_starts_with(&canonical, skill_dirs) {
        return Ok(ValidatedPath {
            canonical,
            access: PathAccess::ReadOnly,
        });
    }

    Err(format!(
        "Path '{}' is outside the working directory",
        canonical.display()
    ))
}

/// Check if a canonical path starts with any of the given directories.
/// Each directory is canonicalized before comparison to handle symlinks
/// (e.g., /tmp → /private/tmp on macOS).
fn path_starts_with(canonical: &Path, dirs: &[PathBuf]) -> bool {
    dirs.iter().any(|dir| {
        // Try canonical form first (fast path when no symlinks involved)
        if canonical.starts_with(dir) {
            return true;
        }
        // Also try the canonicalized form of the dir
        dir.canonicalize()
            .is_ok_and(|canon_dir| canonical.starts_with(&canon_dir))
    })
}

/// Validate that a path exists and is within the current working directory, allowed temp directories,
/// or directories added via --add-dir flag (read-only access).
///
/// This is a convenience function for read operations that don't need to check access level.
pub(super) fn validate_path_in_cwd(path_str: &str) -> Result<std::path::PathBuf, String> {
    validate_path(path_str).map(|vp| vp.canonical)
}

/// Validate that a path is writable (not in a read-only additional directory).
/// Returns the canonical path if valid, or an error if the path is read-only.
pub(super) fn validate_path_for_write(path_str: &str) -> Result<std::path::PathBuf, String> {
    let validated = validate_path(path_str)?;
    if validated.access == PathAccess::ReadOnly {
        return Err(format!(
            "Path '{}' is read-only (added via --add-dir). Write operations are not allowed.",
            validated.canonical.display()
        ));
    }
    Ok(validated.canonical)
}

/// Get standard temporary directory paths (cached)
pub(super) fn get_temp_directories() -> &'static [PathBuf] {
    cached_temp_dirs()
}

// =============================================================================
// Tool Execution
// =============================================================================

/// Execute a tool call
pub(super) async fn execute_tool(name: &str, arguments: &str) -> Result<ToolResult, String> {
    match name {
        "Bash" => Box::pin(bash::execute_bash(arguments)).await,
        "Edit" => {
            let args = arguments.to_string();
            tokio::task::spawn_blocking(move || edit::execute_edit(&args))
                .await
                .map_err(|e| format!("Task join error: {e}"))?
        },
        "Read" => {
            let args = arguments.to_string();
            tokio::task::spawn_blocking(move || read::execute_read(&args))
                .await
                .map_err(|e| format!("Task join error: {e}"))?
        },
        "Write" => {
            let args = arguments.to_string();
            tokio::task::spawn_blocking(move || write::execute_write(&args))
                .await
                .map_err(|e| format!("Task join error: {e}"))?
        },
        _ => Err(format!("Unknown tool: {name}")),
    }
}

// =============================================================================
// Tool Argument Summarization
// =============================================================================

/// Summarize tool arguments for display.
/// This function uses the same typed argument structs as the tool execution,
/// ensuring that parameter names stay in sync.
pub fn summarize_tool_args(tool_name: &str, arguments: &str) -> String {
    let raw = match tool_name {
        "Bash" => bash::summarize_args(arguments),
        "Read" => read::summarize_args(arguments),
        "Edit" => edit::summarize_args(arguments),
        "Write" => write::summarize_args(arguments),
        _ => String::new(),
    };

    truncate_display(&raw, 120)
}

/// Truncate a string for display, appending "..." if needed.
fn truncate_display(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

// =============================================================================
// Tool Definitions (Re-exports)
// =============================================================================

/// Returns the Bash tool definition
pub fn bash_tool() -> Tool {
    bash::bash_tool()
}

/// Returns the Edit tool definition
pub fn edit_tool() -> Tool {
    edit::edit_tool()
}

/// Returns the Read tool definition
pub fn read_tool() -> Tool {
    read::read_tool()
}

/// Returns the Write tool definition
pub fn write_tool() -> Tool {
    write::write_tool()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    /// Verify that `validate_path_with_dirs` accepts paths within skill directories.
    #[test]
    fn skill_dir_path_accepted() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("fetching-x-content");
        fs::create_dir_all(&skill_dir).unwrap();

        // Create a SKILL.md and a script file in the skill directory
        let skill_file = skill_dir.join("SKILL.md");
        let script_file = skill_dir.join("scripts").join("x-fetch.js");
        fs::create_dir_all(script_file.parent().unwrap()).unwrap();
        fs::write(&skill_file, "# Skill content").unwrap();
        fs::write(&script_file, "// script content").unwrap();

        let cwd = tmp.path().join("project");
        fs::create_dir_all(&cwd).unwrap();

        let result = validate_path_with_dirs(
            skill_file.to_str().unwrap(),
            &cwd,
            &[],
            &[],
            &[],
            std::slice::from_ref(&skill_dir),
        );
        assert!(
            result.is_ok(),
            "Skill file should be readable: {:?}",
            result.err()
        );
        let validated = result.unwrap();
        assert_eq!(validated.access, PathAccess::ReadOnly);
    }

    /// Verify that files nested in skill subdirectories are also accepted.
    #[test]
    fn nested_path_in_skill_dir_accepted() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("fetching-x-content");
        let nested = skill_dir.join("scripts").join("x-fetch.js");
        fs::create_dir_all(nested.parent().unwrap()).unwrap();
        fs::write(&nested, "// script").unwrap();

        let cwd = tmp.path().join("project");
        fs::create_dir_all(&cwd).unwrap();

        let result = validate_path_with_dirs(
            nested.to_str().unwrap(),
            &cwd,
            &[],
            &[],
            &[],
            std::slice::from_ref(&skill_dir),
        );
        assert!(
            result.is_ok(),
            "Nested skill file should be readable: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap().access, PathAccess::ReadOnly);
    }

    /// Verify that paths outside skill directories are still rejected.
    #[test]
    fn path_outside_skill_dir_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("fetching-x-content");
        fs::create_dir_all(&skill_dir).unwrap();
        let outside_file = tmp.path().join("outside.md");
        fs::write(&outside_file, "nope").unwrap();

        let cwd = tmp.path().join("project");
        fs::create_dir_all(&cwd).unwrap();

        let result = validate_path_with_dirs(
            outside_file.to_str().unwrap(),
            &cwd,
            &[],
            &[],
            &[],
            std::slice::from_ref(&skill_dir),
        );
        assert!(result.is_err(), "File outside skill dir should be rejected");
    }

    /// Verify that multiple skill directories are all recognized.
    #[test]
    fn multiple_skill_dirs_accepted() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_a = tmp.path().join("skill-a");
        let skill_b = tmp.path().join("skill-b");
        fs::create_dir_all(&skill_a).unwrap();
        fs::create_dir_all(&skill_b).unwrap();
        let file_a = skill_a.join("SKILL.md");
        let file_b = skill_b.join("SKILL.md");
        fs::write(&file_a, "a").unwrap();
        fs::write(&file_b, "b").unwrap();

        let cwd = tmp.path().join("project");
        fs::create_dir_all(&cwd).unwrap();

        let skill_dirs = [skill_a, skill_b];
        let result_a =
            validate_path_with_dirs(file_a.to_str().unwrap(), &cwd, &[], &[], &[], &skill_dirs);
        assert!(result_a.is_ok());

        let result_b =
            validate_path_with_dirs(file_b.to_str().unwrap(), &cwd, &[], &[], &[], &skill_dirs);
        assert!(result_b.is_ok());
    }
}
