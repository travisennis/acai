use serde::Deserialize;
use std::path::Path;

use super::validate_path_in_cwd;

// =============================================================================
// Edit Tool Definition
// =============================================================================

/// Returns the Edit tool definition
pub(super) fn edit_tool() -> super::Tool {
    super::Tool {
        type_: "function".to_string(),
        name: "Edit".to_string(),
        description: "Make a targeted edit to an existing file by replacing an exact text match. \
            The file must exist. Use for modifying existing code — for new files, use Write instead. \
            The old_text must appear exactly once in the file unless replace_all is true."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to edit. File must exist."
                },
                "old_text": {
                    "type": "string",
                    "description": "The exact text to find in the file. Must match exactly, including whitespace and indentation."
                },
                "new_text": {
                    "type": "string",
                    "description": "The replacement text. Use an empty string to delete the matched text."
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "If true, replace all occurrences of old_text. If false (default), old_text must appear exactly once."
                }
            },
            "required": ["file_path", "old_text", "new_text"]
        }),
    }
}

// =============================================================================
// Edit Execution
// =============================================================================

/// Execute an edit command
#[allow(clippy::unused_async)]
pub(super) async fn execute_edit(arguments: &str) -> Result<super::ToolResult, String> {
    #[derive(Deserialize)]
    struct EditArgs {
        file_path: String,
        old_text: String,
        new_text: String,
        replace_all: Option<bool>,
    }

    let args: EditArgs =
        serde_json::from_str(arguments).map_err(|e| format!("Invalid edit arguments: {e}"))?;

    // Validate old_text != new_text (no-op check)
    if args.old_text == args.new_text {
        return Err("old_text and new_text are identical — no changes needed".to_string());
    }

    // Validate and canonicalize the path
    let path = validate_path_in_cwd(&args.file_path)?;

    // Check if file exists and is a file
    let metadata = std::fs::metadata(&path)
        .map_err(|e| format!("Failed to access file '{}': {e}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("Path is not a file: {}", path.display()));
    }

    // Refuse binary files (check for null bytes in first 8KB)
    let file_bytes = std::fs::read(&path)
        .map_err(|e| format!("Failed to read file '{}': {e}", path.display()))?;
    let check_len = file_bytes.len().min(8192);
    if file_bytes[..check_len].contains(&0) {
        return Err(format!(
            "Cannot edit binary file: {} (detected null bytes)",
            path.display()
        ));
    }

    // Read file content as string
    let content = String::from_utf8(file_bytes)
        .map_err(|_e| format!("File contains invalid UTF-8: {}", path.display()))?;

    // Count occurrences of old_text
    let occurrences = content.matches(&args.old_text).count();

    if occurrences == 0 {
        return Err(format!("old_text not found in file: {}", path.display()));
    }

    let replace_all = args.replace_all.unwrap_or(false);

    if !replace_all && occurrences > 1 {
        return Err(format!(
            "old_text matches {occurrences} locations in file: {} — add more context to make it unique, or set replace_all to true",
            path.display()
        ));
    }

    // Perform the replacement
    let new_content = if replace_all {
        content.replace(&args.old_text, &args.new_text)
    } else {
        content.replacen(&args.old_text, &args.new_text, 1)
    };

    // Write the modified content back
    std::fs::write(&path, &new_content)
        .map_err(|e| format!("Failed to write file '{}': {e}", path.display()))?;

    // Generate diff output
    let diff = generate_diff(
        &content,
        &new_content,
        &path,
        &args.old_text,
        &args.new_text,
    );

    let result = format!(
        "Edited: {}\nReplacements: {}\n{}",
        path.display(),
        if replace_all { occurrences } else { 1 },
        diff
    );

    Ok(super::ToolResult { output: result })
}

/// Generate a simple context diff showing the changes
fn generate_diff(
    old_content: &str,
    new_content: &str,
    path: &Path,
    old_text: &str,
    new_text: &str,
) -> String {
    use std::fmt::Write;

    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();

    // Find where the change occurred
    let mut change_line = 0usize;
    for (i, (old, new)) in old_lines.iter().zip(new_lines.iter()).enumerate() {
        if old != new {
            change_line = i;
            break;
        }
    }

    // Determine context range (3 lines before and after)
    let context_start = change_line.saturating_sub(3);
    let context_end = (change_line + 7).min(old_lines.len());

    let mut diff_output = String::new();
    let _ = writeln!(diff_output, "--- {}", path.display());
    let _ = writeln!(diff_output, "+++ {}", path.display());
    let _ = writeln!(
        diff_output,
        "@@ -{},{} +{},{} @@",
        context_start + 1,
        context_end - context_start,
        context_start + 1,
        context_end - context_start
    );

    for i in context_start..context_end {
        if i < old_lines.len() {
            if old_lines[i].contains(old_text) && new_text != old_text {
                let _ = writeln!(diff_output, "-{old_line}", old_line = old_lines[i]);
                // Show the new line with replacement
                let new_line = old_lines[i].replace(old_text, new_text);
                let _ = writeln!(diff_output, "+{new_line}");
            } else {
                let _ = writeln!(diff_output, " {line}", line = old_lines[i]);
            }
        }
    }

    diff_output
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[tokio::test]
    async fn edit_single_occurrence() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world\nGoodbye earth").unwrap();

        let args = serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "old_text": "world",
            "new_text": "universe"
        })
        .to_string();

        let result = execute_edit(&args).await.unwrap();
        assert!(result.output.contains("Replacements: 1"));
        assert!(result.output.contains("-Hello world"));
        assert!(result.output.contains("+Hello universe"));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello universe\nGoodbye earth");
    }

    #[tokio::test]
    async fn error_when_old_text_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world").unwrap();

        let args = serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "old_text": "notfound",
            "new_text": "replacement"
        })
        .to_string();

        let result = execute_edit(&args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn error_when_multiple_occurrences_without_replace_all() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world\nGoodbye world").unwrap();

        let args = serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "old_text": "world",
            "new_text": "universe"
        })
        .to_string();

        let result = execute_edit(&args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("matches 2 locations"));
    }

    #[tokio::test]
    async fn replace_all_occurrences() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world\nGoodbye world").unwrap();

        let args = serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "old_text": "world",
            "new_text": "universe",
            "replace_all": true
        })
        .to_string();

        let result = execute_edit(&args).await.unwrap();
        assert!(result.output.contains("Replacements: 2"));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello universe\nGoodbye universe");
    }

    #[tokio::test]
    async fn delete_text_with_empty_new_text() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world").unwrap();

        let args = serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "old_text": " world",
            "new_text": ""
        })
        .to_string();

        let result = execute_edit(&args).await.unwrap();
        assert!(result.output.contains("Replacements: 1"));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello");
    }

    #[tokio::test]
    async fn error_on_binary_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.bin");
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(&[0u8, 1, 2, 3, 0, 4, 5]).unwrap(); // Contains null bytes

        let args = serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "old_text": "test",
            "new_text": "replacement"
        })
        .to_string();

        let result = execute_edit(&args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("binary file"));
    }

    #[tokio::test]
    async fn error_on_nonexistent_file() {
        let args = serde_json::json!({
            "file_path": "/etc/nonexistent_file_12345.txt",
            "old_text": "test",
            "new_text": "replacement"
        })
        .to_string();

        let result = execute_edit(&args).await;
        assert!(result.is_err());
        // Path doesn't exist, so should fail with "not found" error
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn error_on_no_op_edit() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world").unwrap();

        let args = serde_json::json!({
            "file_path": file_path.to_str().unwrap(),
            "old_text": "world",
            "new_text": "world"
        })
        .to_string();

        let result = execute_edit(&args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("identical"));
    }

    #[tokio::test]
    async fn error_on_directory() {
        let temp_dir = TempDir::new().unwrap();

        let args = serde_json::json!({
            "file_path": temp_dir.path().to_str().unwrap(),
            "old_text": "test",
            "new_text": "replacement"
        })
        .to_string();

        let result = execute_edit(&args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a file"));
    }
}
