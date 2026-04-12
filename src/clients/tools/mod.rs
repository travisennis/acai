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
use std::cell::RefCell;
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
// Thread-Local Additional Directories
// =============================================================================

// Thread-local storage for additional directories added via --add-dir flag.
// These directories are read-only for the agent.
thread_local! {
    static ADDITIONAL_DIRS: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

/// Set the additional directories for the current thread.
/// This should be called once at startup from main.
pub fn set_additional_dirs(dirs: Vec<PathBuf>) {
    ADDITIONAL_DIRS.with(|cell| {
        *cell.borrow_mut() = dirs;
    });
}

/// Get the additional directories for the current thread.
pub fn get_additional_dirs() -> Vec<PathBuf> {
    ADDITIONAL_DIRS.with(|cell| cell.borrow().clone())
}

// =============================================================================
// Module Declarations
// =============================================================================

mod bash;
mod bash_safety;
mod edit;
mod read;
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
/// use acai::clients::tools::bash_tool;
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
    let path = Path::new(path_str);

    let cwd = cached_cwd()?;

    // Canonicalize the path (resolve symlinks, relative paths, etc.)
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Path not found or not accessible '{}': {e}", path.display()))?;

    // Check if path is within working directory (read-write)
    if canonical.starts_with(cwd) {
        return Ok(ValidatedPath {
            canonical,
            access: PathAccess::ReadWrite,
        });
    }

    // Allow paths in standard temp directories (read-write)
    for temp_dir in cached_temp_dirs() {
        if canonical.starts_with(temp_dir) {
            return Ok(ValidatedPath {
                canonical,
                access: PathAccess::ReadWrite,
            });
        }
    }

    // Allow paths in directories added via --add-dir flag (read-only)
    let additional_dirs = get_additional_dirs();
    for add_dir in &additional_dirs {
        if canonical.starts_with(add_dir) {
            return Ok(ValidatedPath {
                canonical,
                access: PathAccess::ReadOnly,
            });
        }
    }

    Err(format!(
        "Path '{}' is outside the working directory",
        canonical.display()
    ))
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
