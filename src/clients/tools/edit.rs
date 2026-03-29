use serde::Deserialize;
use std::path::Path;

use crate::clients::tools::validate_path_in_cwd;

// =============================================================================
// Constants
// =============================================================================

/// Maximum number of edits allowed in a single call
const MAX_EDITS_PER_CALL: usize = 10;

// =============================================================================
// Edit Tool Definition
// =============================================================================

/// Returns the Edit tool definition
pub(super) fn edit_tool() -> super::Tool {
    super::Tool {
        type_: "function".to_string(),
        name: "Edit".to_string(),
        description: "Edit text in files using literal search-and-replace.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path of the file to edit."
                },
                "edits": {
                    "type": "array",
                    "description": "The edits to make to the file.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_text": {
                                "type": "string",
                                "description": "Text to search for - must match exactly. The old_text must uniquely identify the location - include enough surrounding context (e.g., 3+ lines or function/class names) to ensure only ONE match exists in the file. Special characters require JSON escaping: backticks (`\\``...\\``), quotes, backslashes. For multi-line content, include exact newlines and indentation."
                            },
                            "new_text": {
                                "type": "string",
                                "description": "Text to replace with"
                            }
                        },
                        "required": ["old_text", "new_text"]
                    }
                }
            },
            "required": ["path", "edits"]
        }),
    }
}

// =============================================================================
// Types
// =============================================================================

/// A single edit operation
#[derive(Debug, Clone, Deserialize)]
struct Edit {
    old_text: String,
    new_text: String,
}

/// Arguments for the Edit tool
#[derive(Debug, Deserialize)]
struct EditArgs {
    path: String,
    #[allow(dead_code)]
    edits: Vec<Edit>,
}

/// Summarize edit arguments for display
pub fn summarize_args(arguments: &str) -> String {
    serde_json::from_str::<EditArgs>(arguments)
        .map(|args| args.path)
        .unwrap_or_default()
}

/// A matched edit with position information
#[derive(Debug, Clone)]
struct MatchedEdit {
    /// The replacement text
    new_text: String,
    /// Position in content where the match starts (byte offset)
    index: usize,
    /// Length of the matched text
    match_length: usize,
    /// Original index in the edits array (1-based for error messages)
    edit_index: usize,
}

/// Line ending type for preservation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineEnding {
    Lf,   // \n (Unix)
    Crlf, // \r\n (Windows)
}

// =============================================================================
// Edit Execution
// =============================================================================

/// Execute an edit command
pub(super) fn execute_edit(arguments: &str) -> Result<super::ToolResult, String> {
    let args: EditArgs =
        serde_json::from_str(arguments).map_err(|e| format!("Invalid edit arguments: {e}"))?;

    // Validate number of edits
    if args.edits.is_empty() {
        return Err("No edits provided. At least one edit is required.".to_string());
    }

    if args.edits.len() > MAX_EDITS_PER_CALL {
        return Err(format!(
            "Too many edits ({}). Maximum {} edits per call. Please split your changes into multiple tool calls.",
            args.edits.len(),
            MAX_EDITS_PER_CALL
        ));
    }

    // Check for no-op edits
    for (i, edit) in args.edits.iter().enumerate() {
        if edit.old_text == edit.new_text {
            return Err(format!(
                "Edit {}: old_text and new_text are identical — no changes needed",
                i + 1
            ));
        }
    }

    // Validate and canonicalize the path
    let path = validate_path_in_cwd(&args.path)?;

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

    // Detect and strip BOM
    let (bom, content) = strip_bom(&content);

    // Detect line ending style
    let line_ending = detect_line_ending(content);

    // Normalize line endings to LF for consistent processing
    let normalized_content = normalize_line_endings(content);

    // Preflight validation: find all match positions
    let matched_edits = preflight_edits(&args.edits, &normalized_content, &path)?;

    // Apply edits in reverse order (highest position first)
    let new_content = apply_edits_reverse_order(&normalized_content, &matched_edits);

    // Restore line endings if needed
    let new_content = restore_line_endings(&new_content, line_ending);

    // Restore BOM if it was present
    let new_content = restore_bom(new_content, bom);

    // Write the modified content back
    std::fs::write(&path, &new_content)
        .map_err(|e| format!("Failed to write file '{}': {e}", path.display()))?;

    // Generate diff output
    let diff = generate_unified_diff(
        &restore_bom(restore_line_endings(content, line_ending), bom),
        &new_content,
        &path,
    );

    let result = format!(
        "Applied {} edit{} to: {}\n{}",
        matched_edits.len(),
        if matched_edits.len() == 1 { "" } else { "s" },
        path.display(),
        diff
    );

    Ok(super::ToolResult { output: result })
}

// =============================================================================
// Preflight Validation
// =============================================================================

/// Validate all edits and find their match positions
fn preflight_edits(edits: &[Edit], content: &str, path: &Path) -> Result<Vec<MatchedEdit>, String> {
    let mut matched_edits = Vec::with_capacity(edits.len());

    for (i, edit) in edits.iter().enumerate() {
        let edit_index = i + 1; // 1-based for error messages

        // Count occurrences
        let occurrences: Vec<usize> = content
            .match_indices(&edit.old_text)
            .map(|(idx, _)| idx)
            .collect();

        if occurrences.is_empty() {
            return Err(format!(
                "Edit {}: Could not find the exact text in {}. The old_text must match exactly including all whitespace and newlines.",
                edit_index,
                path.display()
            ));
        }

        if occurrences.len() > 1 {
            return Err(format!(
                "Edit {}: old_text matches {} locations but should match only 1. Please provide a more specific old_text that includes more surrounding context.",
                edit_index,
                occurrences.len()
            ));
        }

        matched_edits.push(MatchedEdit {
            new_text: edit.new_text.clone(),
            index: occurrences[0],
            match_length: edit.old_text.len(),
            edit_index,
        });
    }

    // Sort by position for overlap detection
    matched_edits.sort_by_key(|e| e.index);

    // Check for overlapping edits
    for window in matched_edits.windows(2) {
        let first = &window[0];
        let second = &window[1];
        let first_end = first.index + first.match_length;

        if first_end > second.index {
            return Err(format!(
                "Edits {} and {} overlap in the file. Each edit must target a distinct region. Please combine overlapping edits into a single edit.",
                first.edit_index, second.edit_index
            ));
        }
    }

    Ok(matched_edits)
}

// =============================================================================
// Edit Application
// =============================================================================

/// Apply edits in reverse order (highest position first) to prevent position shifting
fn apply_edits_reverse_order(content: &str, matched_edits: &[MatchedEdit]) -> String {
    let mut result = content.to_string();

    // Process highest index first
    for edit in matched_edits.iter().rev() {
        result = format!(
            "{}{}{}",
            &result[..edit.index],
            edit.new_text,
            &result[edit.index + edit.match_length..]
        );
    }

    result
}

// =============================================================================
// Line Ending Handling
// =============================================================================

/// Detect the line ending style used in the content
fn detect_line_ending(content: &str) -> LineEnding {
    if content.contains("\r\n") {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    }
}

/// Normalize line endings to LF for consistent processing
fn normalize_line_endings(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

/// Restore line endings to the content
fn restore_line_endings(content: &str, ending: LineEnding) -> String {
    match ending {
        LineEnding::Crlf => content.replace('\n', "\r\n"),
        LineEnding::Lf => content.to_string(),
    }
}

// =============================================================================
// BOM Handling
// =============================================================================

/// Strip BOM from content if present, returning (`optional_bom`, `content_without_bom`)
fn strip_bom(content: &str) -> (Option<&str>, &str) {
    // UTF-8 BOM is \u{FEFF} which is 3 bytes in UTF-8
    const BOM: &str = "\u{FEFF}";

    content
        .strip_prefix(BOM)
        .map_or((None, content), |stripped| (Some(BOM), stripped))
}

/// Restore BOM to content if it was present
fn restore_bom(content: String, bom: Option<&str>) -> String {
    match bom {
        Some(bom_str) => format!("{bom_str}{content}"),
        None => content,
    }
}

// =============================================================================
// Unified Diff Generation
// =============================================================================

/// Generate a unified diff showing the changes
fn generate_unified_diff(original: &str, modified: &str, path: &Path) -> String {
    use similar::TextDiff;
    use std::fmt::Write;

    let diff = TextDiff::from_lines(original, modified);

    let mut diff_output = String::new();
    let _ = writeln!(diff_output, "--- {}", path.display());
    let _ = writeln!(diff_output, "+++ {}", path.display());

    // Use the unified_diff method from similar crate
    let unified = diff
        .unified_diff()
        .context_radius(3)
        .header(
            &format!("--- {}", path.display()),
            &format!("+++ {}", path.display()),
        )
        .to_string();

    // The unified_diff includes its own header, so we need to remove the duplicate
    // or just use the unified output directly
    diff_output.clear();
    diff_output.push_str(&unified);

    diff_output
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // =========================================================================
    // Multiple Edits Tests
    // =========================================================================

    #[test]
    fn multiple_edits_in_single_call() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world\nGoodbye earth\n").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "Hello", "new_text": "Hi" },
                { "old_text": "Goodbye", "new_text": "Bye" }
            ]
        })
        .to_string();

        let result = execute_edit(&args).unwrap();
        assert!(result.output.contains("Applied 2 edits"));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hi world\nBye earth\n");
    }

    #[test]
    fn error_on_overlapping_edits() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "Hello", "new_text": "Hi" },
                { "old_text": "lo wor", "new_text": "xx" }
            ]
        })
        .to_string();

        let result = execute_edit(&args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("overlap"),
            "Error should mention overlap: {err}"
        );
    }

    #[test]
    fn error_on_too_many_edits() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world").unwrap();

        let edits: Vec<_> = (0..15)
            .map(|i| serde_json::json!({ "old_text": format!("text{i}"), "new_text": format!("new{i}") }))
            .collect();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": edits
        })
        .to_string();

        let result = execute_edit(&args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Too many edits"),
            "Error should mention too many edits: {err}"
        );
        assert!(
            err.contains("Maximum 10"),
            "Error should mention max: {err}"
        );
    }

    #[test]
    fn error_with_specific_edit_number() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "Hello", "new_text": "Hi" },
                { "old_text": "notfound", "new_text": "replacement" }
            ]
        })
        .to_string();

        let result = execute_edit(&args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Edit 2:"),
            "Error should mention edit number: {err}"
        );
        assert!(
            err.contains("Could not find"),
            "Error should mention not found: {err}"
        );
    }

    #[test]
    fn error_on_multiple_matches_in_single_edit() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world\nGoodbye world").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "world", "new_text": "universe" }
            ]
        })
        .to_string();

        let result = execute_edit(&args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("Edit 1:"),
            "Error should mention edit number: {err}"
        );
        assert!(
            err.contains("matches 2 locations"),
            "Error should mention multiple matches: {err}"
        );
    }

    #[test]
    fn error_on_no_edits_provided() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": []
        })
        .to_string();

        let result = execute_edit(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No edits provided"));
    }

    #[test]
    fn error_on_identical_old_and_new_text() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "world", "new_text": "world" }
            ]
        })
        .to_string();

        let result = execute_edit(&args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("identical"),
            "Error should mention identical: {err}"
        );
    }

    // =========================================================================
    // Line Ending Tests
    // =========================================================================

    #[test]
    fn preserves_lf_line_endings() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world\nGoodbye earth\n").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "world", "new_text": "universe" }
            ]
        })
        .to_string();

        let _ = execute_edit(&args).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("Hello universe\n"));
        assert!(!content.contains("\r\n"));
    }

    #[test]
    fn preserves_crlf_line_endings() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world\r\nGoodbye earth\r\n").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "world", "new_text": "universe" }
            ]
        })
        .to_string();

        let _ = execute_edit(&args).unwrap();

        let content = String::from_utf8(fs::read(&file_path).unwrap()).unwrap();
        assert!(content.contains("Hello universe\r\n"));
        assert!(content.contains("\r\n"));
    }

    // =========================================================================
    // BOM Tests
    // =========================================================================

    #[test]
    fn handles_utf8_bom() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let bom: &[u8] = b"\xEF\xBB\xBF";
        let content_with_bom: Vec<u8> = [bom, b"Hello world"].concat();
        fs::write(&file_path, &content_with_bom).unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "world", "new_text": "universe" }
            ]
        })
        .to_string();

        let _ = execute_edit(&args).unwrap();

        let content = fs::read(&file_path).unwrap();
        assert!(
            content.starts_with(b"\xEF\xBB\xBF"),
            "BOM should be preserved"
        );
        let content_str = String::from_utf8(content).unwrap();
        assert!(content_str.contains("Hello universe"));
    }

    #[test]
    fn no_bom_stays_no_bom() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "world", "new_text": "universe" }
            ]
        })
        .to_string();

        let _ = execute_edit(&args).unwrap();

        let content = fs::read(&file_path).unwrap();
        assert!(
            !content.starts_with(b"\xEF\xBB\xBF"),
            "No BOM should be added"
        );
    }

    // =========================================================================
    // Legacy Tests (from original implementation)
    // =========================================================================

    #[test]
    fn edit_single_occurrence() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world\nGoodbye earth").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "world", "new_text": "universe" }
            ]
        })
        .to_string();

        let result = execute_edit(&args).unwrap();
        assert!(result.output.contains("Applied 1 edit"));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello universe\nGoodbye earth");
    }

    #[test]
    fn error_when_old_text_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "notfound", "new_text": "replacement" }
            ]
        })
        .to_string();

        let result = execute_edit(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Could not find"));
    }

    #[test]
    fn delete_text_with_empty_new_text() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Hello world").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": " world", "new_text": "" }
            ]
        })
        .to_string();

        let result = execute_edit(&args).unwrap();
        assert!(result.output.contains("Applied 1 edit"));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello");
    }

    #[test]
    fn error_on_binary_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.bin");
        fs::write(&file_path, [0u8, 1, 2, 3, 0, 4, 5]).unwrap(); // Contains null bytes

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "test", "new_text": "replacement" }
            ]
        })
        .to_string();

        let result = execute_edit(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("binary file"));
    }

    #[test]
    fn error_on_nonexistent_file() {
        let args = serde_json::json!({
            "path": "/etc/nonexistent_file_12345.txt",
            "edits": [
                { "old_text": "test", "new_text": "replacement" }
            ]
        })
        .to_string();

        let result = execute_edit(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn error_on_directory() {
        let temp_dir = TempDir::new().unwrap();

        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap(),
            "edits": [
                { "old_text": "test", "new_text": "replacement" }
            ]
        })
        .to_string();

        let result = execute_edit(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a file"));
    }

    // =========================================================================
    // Path Validation Tests
    // =========================================================================

    #[test]
    fn error_on_path_outside_working_directory() {
        let args = serde_json::json!({
            "path": "/etc/passwd",
            "edits": [
                { "old_text": "test", "new_text": "replacement" }
            ]
        })
        .to_string();

        let result = execute_edit(&args);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("outside the working directory")
        );
    }

    // =========================================================================
    // Reverse Order Application Tests
    // =========================================================================

    #[test]
    fn edits_applied_in_correct_positions() {
        // This test verifies that edits are applied in reverse order
        // so that position shifts don't affect later edits
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "AAA BBB CCC").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "AAA", "new_text": "XXX" },
                { "old_text": "CCC", "new_text": "ZZZ" }
            ]
        })
        .to_string();

        let _ = execute_edit(&args).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "XXX BBB ZZZ");
    }

    #[test]
    fn multiple_edits_with_different_lengths() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "short medium verylong").unwrap();

        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "edits": [
                { "old_text": "short", "new_text": "s" },
                { "old_text": "medium", "new_text": "MEDIUM" },
                { "old_text": "verylong", "new_text": "vl" }
            ]
        })
        .to_string();

        let _ = execute_edit(&args).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "s MEDIUM vl");
    }
}
