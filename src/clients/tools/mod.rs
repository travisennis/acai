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

    if let Ok(tmp) = std::fs::canonicalize("/tmp") {
        dirs.push(tmp);
    }

    if let Ok(tmp) = std::fs::canonicalize("/var/folders") {
        dirs.push(tmp);
    }

    if let Ok(tmpdir) = std::env::var("TMPDIR")
        && let Ok(canonical) = std::fs::canonicalize(&tmpdir)
    {
        dirs.push(canonical);
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

/// Tool definition sent in API requests
#[derive(Serialize, Clone, Debug)]
pub struct Tool {
    #[serde(rename = "type")]
    pub(super) type_: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) parameters: serde_json::Value,
}

/// Result of executing a tool
#[derive(Debug)]
pub struct ToolResult {
    pub output: String,
}

// =============================================================================
// Path Validation
// =============================================================================

/// Validate that a path exists and is within the current working directory, allowed temp directories,
/// or directories added via --add-dir flag (read-only access).
pub(super) fn validate_path_in_cwd(path_str: &str) -> Result<std::path::PathBuf, String> {
    let path = Path::new(path_str);

    let cwd = cached_cwd()?;

    // Canonicalize the path (resolve symlinks, relative paths, etc.)
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Path not found or not accessible '{}': {e}", path.display()))?;

    // Check if path is within working directory
    if canonical.starts_with(cwd) {
        return Ok(canonical);
    }

    // Allow paths in standard temp directories
    for temp_dir in cached_temp_dirs() {
        if canonical.starts_with(temp_dir) {
            return Ok(canonical);
        }
    }

    // Allow paths in directories added via --add-dir flag (read-only)
    let additional_dirs = get_additional_dirs();
    for add_dir in &additional_dirs {
        if canonical.starts_with(add_dir) {
            return Ok(canonical);
        }
    }

    Err(format!(
        "Path '{}' is outside the working directory",
        canonical.display()
    ))
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
