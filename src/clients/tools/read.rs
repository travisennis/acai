use serde::Deserialize;
use std::path::Path;

use crate::clients::tools::validate_path_in_cwd;

const DEFAULT_END_LINE: usize = 500;
const MAX_OUTPUT_BYTES: usize = 100_000;

// =============================================================================
// Read Tool Definition
// =============================================================================

/// Returns the Read tool definition
pub(super) fn read_tool() -> super::Tool {
    super::Tool {
        type_: "function".to_string(),
        name: "Read".to_string(),
        description: "Read a file's contents or list a directory's entries. \
            Returns line-numbered content for files, or a list of entries for directories. \
            Supports reading specific line ranges to avoid loading entire large files. \
            Use this instead of cat/head/tail/ls via Bash."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file or directory to read."
                },
                "start_line": {
                    "type": "integer",
                    "description": "First line to read (1-indexed, inclusive). Defaults to 1."
                },
                "end_line": {
                    "type": "integer",
                    "description": "Last line to read (1-indexed, inclusive). Defaults to 500. Use to limit output for large files."
                }
            },
            "required": ["path"]
        }),
    }
}

// =============================================================================
// Read Execution
// =============================================================================

/// Arguments for the Read tool
#[derive(Deserialize)]
struct ReadArgs {
    path: String,
    #[allow(dead_code)]
    start_line: Option<usize>,
    #[allow(dead_code)]
    end_line: Option<usize>,
}

/// Summarize read arguments for display
pub fn summarize_args(arguments: &str) -> String {
    serde_json::from_str::<ReadArgs>(arguments)
        .map(|args| args.path)
        .unwrap_or_default()
}

/// Execute a read command
pub(super) fn execute_read(arguments: &str) -> Result<super::ToolResult, String> {
    let args: ReadArgs =
        serde_json::from_str(arguments).map_err(|e| format!("Invalid read arguments: {e}"))?;

    // Validate and canonicalize the path
    let path = validate_path_in_cwd(&args.path)?;

    // Check if path exists
    if !path.exists() {
        return Err(format!("Path not found: {}", path.display()));
    }

    // Handle directory
    if path.is_dir() {
        return read_directory(&path);
    }

    // Handle file
    read_file(&path, args.start_line, args.end_line)
}

/// Read and format a directory listing
fn read_directory(path: &Path) -> Result<super::ToolResult, String> {
    let entries: Vec<_> = std::fs::read_dir(path)
        .map_err(|e| format!("Failed to read directory '{}': {e}", path.display()))?
        .filter_map(std::result::Result::ok)
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            if is_dir { format!("{name}/") } else { name }
        })
        .collect();

    if entries.is_empty() {
        return Ok(super::ToolResult {
            output: format!("Directory: {}\n(empty)", path.display()),
        });
    }

    let output = format!("Directory: {}\n{}", path.display(), entries.join("\n"));

    Ok(super::ToolResult { output })
}

/// Read and format a file with line numbers
fn read_file(
    path: &Path,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> Result<super::ToolResult, String> {
    // Read file content
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file '{}': {e}", path.display()))?;

    // Check for binary files (null bytes in first 8KB)
    let check_len = content.len().min(8192);
    if content.as_bytes()[..check_len].contains(&0) {
        return Err(format!(
            "Cannot read binary file: {} (detected null bytes)",
            path.display()
        ));
    }

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Default line range
    let start = start_line.unwrap_or(1).saturating_sub(1); // Convert to 0-indexed
    let end = end_line.unwrap_or(DEFAULT_END_LINE).saturating_sub(1); // Convert to 0-indexed

    // Clamp to valid range
    let start = start.min(total_lines);
    let end = end.min(total_lines.saturating_sub(1));

    if start > end {
        return Ok(super::ToolResult {
            output: format!(
                "File: {}\n{total_lines} lines total\n(start_line > end_line, no content to show)",
                path.display()
            ),
        });
    }

    // Build numbered output
    let mut numbered_lines: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate().take(end + 1).skip(start) {
        numbered_lines.push(format!("{:>6}: {line}", i + 1));
    }

    let mut output = format!(
        "File: {}\nLines {}-{}/{}\n{}",
        path.display(),
        start + 1,
        end + 1,
        total_lines,
        numbered_lines.join("\n")
    );

    // Truncate if too large
    if output.len() > MAX_OUTPUT_BYTES {
        use std::fmt::Write;
        let truncate_at = MAX_OUTPUT_BYTES - 100; // Leave room for truncation message
        let mut truncated = output.chars().take(truncate_at).collect::<String>();
        let _ = write!(
            truncated,
            "\n[... output truncated at {MAX_OUTPUT_BYTES} bytes ...]"
        );
        output = truncated;
    }

    // Note remaining lines if applicable
    if end < total_lines.saturating_sub(1) {
        use std::fmt::Write;
        let remaining = total_lines - end - 1;
        let _ = write!(output, "\n[... {remaining} more lines ...]");
    }

    Ok(super::ToolResult { output })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn read_small_file_full_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Line 1\nLine 2\nLine 3").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap()
        })
        .to_string();

        let result = execute_read(&args).unwrap();
        assert!(result.output.contains("File:"));
        assert!(result.output.contains("     1: Line 1"));
        assert!(result.output.contains("     2: Line 2"));
        assert!(result.output.contains("     3: Line 3"));
    }

    #[test]
    fn read_with_line_range() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Line 1\nLine 2\nLine 3\nLine 4\nLine 5").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "start_line": 2,
            "end_line": 4
        })
        .to_string();

        let result = execute_read(&args).unwrap();
        assert!(result.output.contains("Lines 2-4/5"));
        assert!(result.output.contains("     2: Line 2"));
        assert!(result.output.contains("     3: Line 3"));
        assert!(result.output.contains("     4: Line 4"));
        assert!(!result.output.contains("Line 1"));
        assert!(!result.output.contains("Line 5"));
    }

    #[test]
    fn read_directory_listing() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        fs::write(temp_dir.path().join("file1.txt"), "content").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "content").unwrap();

        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap()
        })
        .to_string();

        let result = execute_read(&args).unwrap();
        assert!(result.output.contains("Directory:"));
        assert!(result.output.contains("file1.txt"));
        assert!(result.output.contains("file2.txt"));
        assert!(result.output.contains("subdir/"));
    }

    #[test]
    fn error_on_nonexistent_path() {
        let args = serde_json::json!({
            "path": "/nonexistent/path/xyz123"
        })
        .to_string();

        let result = execute_read(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Path not found"));
    }

    #[test]
    fn default_end_line_caps_at_500() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let lines: Vec<String> = (1..=600).map(|i| format!("Line {i}")).collect();
        fs::write(&file_path, lines.join("\n")).unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap()
        })
        .to_string();

        let result = execute_read(&args).unwrap();
        assert!(result.output.contains("Lines 1-500/600"));
        assert!(result.output.contains("[... 100 more lines ...]"));
    }

    #[test]
    fn truncation_note_when_exceeds_range() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Line 1\nLine 2\nLine 3").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "start_line": 1,
            "end_line": 2
        })
        .to_string();

        let result = execute_read(&args).unwrap();
        assert!(result.output.contains("Lines 1-2/3"));
        assert!(result.output.contains("[... 1 more lines ...]"));
    }

    #[test]
    fn read_empty_directory() {
        let temp_dir = TempDir::new().unwrap();

        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap()
        })
        .to_string();

        let result = execute_read(&args).unwrap();
        assert!(result.output.contains("(empty)"));
    }
}
