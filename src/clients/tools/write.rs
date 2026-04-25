use serde::Deserialize;
use std::path::Path;

// =============================================================================
// Write Tool Definition
// =============================================================================

/// Returns the Write tool definition
pub(super) fn write_tool() -> super::Tool {
    super::Tool {
        type_: "function".to_string(),
        name: "Write".to_string(),
        description: "Create a new file or overwrite an existing file with the given content. \
            Use this for creating new files. For modifying existing files, prefer the Edit tool \
            instead — it makes targeted changes without rewriting the entire file."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to create or overwrite."
                },
                "content": {
                    "type": "string",
                    "description": "The full content to write to the file."
                }
            },
            "required": ["path", "content"]
        }),
    }
}

// =============================================================================
// Write Execution
// =============================================================================

/// Arguments for the Write tool
#[derive(Deserialize)]
struct WriteArgs {
    path: String,
    #[allow(dead_code)]
    content: String,
}

/// Summarize write arguments for display
pub fn summarize_args(arguments: &str) -> String {
    serde_json::from_str::<WriteArgs>(arguments)
        .map(|args| args.path)
        .unwrap_or_default()
}

/// Execute a write command
pub(super) fn execute_write(arguments: &str) -> Result<super::ToolResult, String> {
    let args: WriteArgs =
        serde_json::from_str(arguments).map_err(|e| format!("Invalid write arguments: {e}"))?;

    // Check if file exists to determine if it's a create or overwrite
    let file_existed = Path::new(&args.path).exists();

    // Validate path is within working directory
    // For new files, we need to handle the case where the file doesn't exist yet
    let path = validate_path_for_write(&args.path)?;

    // Create parent directories if they don't exist
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directories '{}': {e}", parent.display()))?;
    }

    // Write content to file
    std::fs::write(&path, &args.content)
        .map_err(|e| format!("Failed to write file '{}': {e}", path.display()))?;

    let bytes_written = args.content.len();

    let action = if file_existed {
        "Overwritten"
    } else {
        "Created"
    };

    let warning = if file_existed {
        " (Note: Consider using Edit tool for targeted changes to existing files)"
    } else {
        ""
    };

    let result = format!(
        "{action}: {}{}\nBytes written: {}",
        path.display(),
        warning,
        bytes_written
    );

    Ok(super::ToolResult { output: result })
}

/// Validate a path for writing - allows non-existent paths as long as parent is in cwd or temp directories.
/// Rejects paths in read-only additional directories (--add-dir).
fn validate_path_for_write(path_str: &str) -> Result<std::path::PathBuf, String> {
    let path = Path::new(path_str);

    // If the file exists, use the shared validation which checks read-only status
    if path.exists() {
        return super::validate_path_for_write(path_str);
    }

    // For new files, find the deepest existing parent directory
    let parent = path
        .parent()
        .ok_or_else(|| format!("Invalid file path (no parent): {path_str}"))?;

    // Walk up the tree to find an existing directory we can canonicalize
    let mut current_parent = parent;
    let mut components_to_create = Vec::new();

    while !current_parent.exists() {
        if let Some(file_name) = current_parent.file_name() {
            components_to_create.push(file_name.to_os_string());
        }
        current_parent = current_parent
            .parent()
            .ok_or_else(|| format!("Cannot find existing parent directory for path: {path_str}"))?;
    }

    // Canonicalize the deepest existing parent
    let canonical_parent = current_parent.canonicalize().map_err(|e| {
        format!(
            "Parent directory not found '{}': {e}",
            current_parent.display()
        )
    })?;

    // Check if the existing parent is within allowed directories
    let cwd = super::cached_cwd()?;
    let is_in_cwd = canonical_parent.starts_with(cwd);
    let is_in_temp = super::get_temp_directories()
        .iter()
        .any(|temp_dir| canonical_parent.starts_with(temp_dir));
    let is_in_settings = super::get_settings_dirs()
        .iter()
        .any(|settings_dir| canonical_parent.starts_with(settings_dir));

    // Check if parent is in a read-only additional directory
    let additional_dirs = super::get_additional_dirs();
    let is_in_read_only = additional_dirs
        .iter()
        .any(|add_dir| canonical_parent.starts_with(add_dir));

    if is_in_read_only {
        return Err(format!(
            "Path '{}' is in a read-only directory (added via --add-dir). Write operations are not allowed.",
            path.display()
        ));
    }

    if !is_in_cwd && !is_in_temp && !is_in_settings {
        return Err(format!(
            "Path '{}' is outside the working directory",
            path.display()
        ));
    }

    // Reconstruct the full path with the canonicalized parent
    let mut final_path = canonical_parent;
    for component in components_to_create.iter().rev() {
        final_path = final_path.join(component);
    }

    // Add the filename
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("Invalid file path (no file name): {path_str}"))?;
    Ok(final_path.join(file_name))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn create_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("new_file.txt");

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "Hello, world!"
        })
        .to_string();

        let result = execute_write(&args).unwrap();
        assert!(result.output.contains("Created:"));
        assert!(result.output.contains("Bytes written: 13"));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello, world!");
    }

    #[test]
    fn overwrite_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("existing.txt");
        fs::write(&file_path, "old content").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "new content"
        })
        .to_string();

        let result = execute_write(&args).unwrap();
        assert!(result.output.contains("Overwritten:"));
        assert!(result.output.contains("Note: Consider using Edit tool"));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "new content");
    }

    #[test]
    fn auto_create_parent_directories() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("a/b/c/deep_file.txt");

        let args = serde_json::json!({
            "path": nested_path.to_str().unwrap(),
            "content": "Deep content"
        })
        .to_string();

        let result = execute_write(&args).unwrap();
        assert!(result.output.contains("Created:"));

        let content = fs::read_to_string(&nested_path).unwrap();
        assert_eq!(content, "Deep content");
    }

    #[test]
    fn error_on_path_outside_working_directory() {
        let args = serde_json::json!({
            "path": "/etc/passwd",
            "content": "test"
        })
        .to_string();

        let result = execute_write(&args);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("outside the working directory")
        );
    }

    #[test]
    fn empty_content_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.txt");

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": ""
        })
        .to_string();

        let result = execute_write(&args).unwrap();
        assert!(result.output.contains("Created:"));
        assert!(result.output.contains("Bytes written: 0"));

        assert!(file_path.exists());
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.is_empty());
    }
}
