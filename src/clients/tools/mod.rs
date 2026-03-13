use serde::Serialize;
use std::path::Path;

mod sandbox;

// =============================================================================
// Module Declarations
// =============================================================================

mod bash;
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

/// Validate that a path exists and is within the current working directory or allowed temp directories
pub(super) fn validate_path_in_cwd(path_str: &str) -> Result<std::path::PathBuf, String> {
    let path = Path::new(path_str);

    // Get the current working directory
    let cwd =
        std::env::current_dir().map_err(|e| format!("Failed to get working directory: {e}"))?;

    // Canonicalize the path (resolve symlinks, relative paths, etc.)
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Path not found or not accessible '{}': {e}", path.display()))?;

    // Check if path is within working directory
    if canonical.starts_with(&cwd) {
        return Ok(canonical);
    }

    // Allow paths in standard temp directories
    let temp_dirs = get_temp_directories();
    for temp_dir in &temp_dirs {
        if canonical.starts_with(temp_dir) {
            return Ok(canonical);
        }
    }

    Err(format!(
        "Path '{}' is outside the working directory",
        canonical.display()
    ))
}

/// Get standard temporary directory paths
pub(super) fn get_temp_directories() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();

    // /tmp on Unix-like systems
    if let Ok(tmp) = std::fs::canonicalize("/tmp") {
        dirs.push(tmp);
    }

    // macOS temp directory (/var/folders/...)
    if let Ok(tmp) = std::fs::canonicalize("/var/folders") {
        dirs.push(tmp);
    }

    // TMPDIR environment variable
    if let Ok(tmpdir) = std::env::var("TMPDIR")
        && let Ok(canonical) = std::fs::canonicalize(&tmpdir)
    {
        dirs.push(canonical);
    }

    dirs
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
