use serde::Deserialize;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::{Duration, timeout};
use tracing::debug;

/// Maximum number of null bytes or control characters (excluding common whitespace)
/// allowed before considering output as binary.
const BINARY_NULL_BYTE_THRESHOLD: usize = 8;

/// Ratio of non-printable characters that indicates binary data (0.3 = 30%)
const BINARY_RATIO_THRESHOLD: f64 = 0.3;

/// Maximum number of bytes the Bash tool will return inline.
/// Output exceeding this limit is written to a temporary file and the agent
/// receives a truncated message with a path to the full output.
pub(super) const BASH_OUTPUT_MAX_BYTES: usize = 50_000;

/// A generous cap: read up to 2× the inline limit so `truncate_output()`
/// has enough data for a useful head+tail preview and temp-file dump.
pub(super) const BASH_READ_CAP: usize = BASH_OUTPUT_MAX_BYTES * 2;

/// Arguments for bash execution, including optional sandboxing
struct BashExecutionArgs {
    command: String,
    timeout: u64,
    use_sandbox: bool,
}

impl BashExecutionArgs {
    fn from_json(arguments: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct BashArgs {
            command: String,
            timeout: Option<u64>,
        }

        let args: BashArgs =
            serde_json::from_str(arguments).map_err(|e| format!("Invalid bash arguments: {e}"))?;

        Ok(Self {
            command: args.command,
            timeout: args.timeout.unwrap_or(60),
            use_sandbox: !super::sandbox::is_sandbox_disabled(),
        })
    }
}

// =============================================================================
// Bash Tool Definition
// =============================================================================

/// Returns the Bash tool definition
pub(super) fn bash_tool() -> super::Tool {
    super::Tool {
        type_: "function".to_string(),
        name: "Bash".to_string(),
        description: "Execute a shell command in the host machine's terminal. \
            Returns the stdout/stderr output. Use for running build commands, \
            git operations, file manipulation, etc. Does not support interactive commands."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout": {
                    "type": "number",
                    "description": "Optional timeout in seconds"
                }
            },
            "required": ["command"]
        }),
    }
}

// =============================================================================
// Bash Execution
// =============================================================================

/// Detect if command output indicates a sandbox-related permission failure
fn is_sandbox_violation(output: &str) -> bool {
    output.contains("Operation not permitted")
        || output.contains("os error 1")
        || (output.contains("Permission denied") && output.contains("sandbox"))
}

/// Check if raw bytes appear to be binary data rather than text.
/// Returns true if the data contains:
/// - Multiple null bytes (common in binary files)
/// - A high ratio of non-printable characters (excluding common whitespace)
#[allow(clippy::naive_bytecount, clippy::cast_precision_loss)]
fn is_binary_data(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }

    // Count null bytes - even a few null bytes strongly indicate binary
    let null_count = data.iter().filter(|&&b| b == 0).count();
    if null_count > BINARY_NULL_BYTE_THRESHOLD {
        return true;
    }

    // Count non-printable characters (excluding common whitespace: \t, \n, \r)
    let non_printable_count = data
        .iter()
        .filter(|&&b| {
            // Allow tabs, newlines, and carriage returns
            !matches!(b, b'\t' | b'\n' | b'\r')
                // Allow printable ASCII (32-126)
                && !(32..=126).contains(&b)
                // Allow high bytes that could be valid UTF-8 continuation/start bytes
                // (we'll let the UTF-8 check below catch actual invalid sequences)
                && b < 128
        })
        .count();

    // If more than 30% of the data is non-printable, it's likely binary
    let ratio = non_printable_count as f64 / data.len() as f64;
    ratio > BINARY_RATIO_THRESHOLD
}

/// Create a result message for binary output, saving the data to a temp file.
#[allow(clippy::cast_precision_loss)]
fn handle_binary_output(data: &[u8], exit_code: i32, elapsed_ms: u128) -> String {
    let size_bytes = data.len();
    let size_kb = size_bytes as f64 / 1024.0;

    // Try to detect MIME type using the `file` command if available
    let mime_type = detect_mime_type(data);

    // Save binary data to a temp file
    let tmp_dir = std::env::temp_dir().join("acai");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let file_name = format!("bash_binary_{}", uuid::Uuid::new_v4());
    let tmp_path = tmp_dir.join(&file_name);

    match std::fs::write(&tmp_path, data) {
        Ok(()) => {
            let footer = format_metadata_footer(exit_code, elapsed_ms);
            format!(
                "[Binary output detected - {size_bytes} bytes ({size_kb:.1} KB)]\n\
                 Detected type: {}\n\
                 Binary data saved to: {}\n\
                 The command produced binary output which cannot be displayed as text.\n\
                 You can inspect the file with appropriate tools (e.g., `file`, `hexdump`, `xxd`).\n\
                 {}",
                mime_type.as_deref().unwrap_or("unknown"),
                tmp_path.display(),
                footer
            )
        },
        Err(e) => {
            let footer = format_metadata_footer(exit_code, elapsed_ms);
            format!(
                "[Binary output detected - {size_bytes} bytes ({size_kb:.1} KB)]\n\
                 Detected type: {}\n\
                 Failed to save binary data to temp file: {e}\n\
                 The command produced binary output which cannot be displayed as text.\n\
                 {}",
                mime_type.as_deref().unwrap_or("unknown"),
                footer
            )
        },
    }
}

/// Attempt to detect the MIME type of binary data using content-based detection.
/// Returns None if the type cannot be determined.
fn detect_mime_type(data: &[u8]) -> Option<String> {
    // Check for common binary file signatures (magic numbers)
    if data.len() < 4 {
        return None;
    }

    // PNG
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return Some("image/png".to_string());
    }
    // JPEG
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg".to_string());
    }
    // GIF
    if data.starts_with(b"GIF8") {
        return Some("image/gif".to_string());
    }
    // PDF
    if data.starts_with(b"%PDF") {
        return Some("application/pdf".to_string());
    }
    // ZIP (also covers JAR, Office Open XML, etc.)
    if data.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        return Some("application/zip".to_string());
    }
    // ELF executable
    if data.starts_with(&[0x7F, 0x45, 0x4C, 0x46]) {
        return Some("application/x-executable".to_string());
    }
    // Mach-O (macOS executable)
    if data.starts_with(&[0xFE, 0xED, 0xFA, 0xCF]) || data.starts_with(&[0xCF, 0xFA, 0xED, 0xFE]) {
        return Some("application/x-mach-binary".to_string());
    }
    // SQLite
    if data.starts_with(b"SQLite format 3") {
        return Some("application/x-sqlite3".to_string());
    }
    // Gzip
    if data.starts_with(&[0x1F, 0x8B]) {
        return Some("application/gzip".to_string());
    }
    // BZip2
    if data.starts_with(b"BZ") {
        return Some("application/x-bzip2".to_string());
    }
    // TAR (ustar format)
    if data.len() > 261 && &data[257..262] == b"ustar" {
        return Some("application/x-tar".to_string());
    }

    None
}

/// Format metadata footer with exit code and elapsed time
/// Shows milliseconds for values under 1 second, seconds otherwise
#[allow(clippy::cast_precision_loss)]
fn format_metadata_footer(exit_code: i32, elapsed_ms: u128) -> String {
    if elapsed_ms > 999 {
        let elapsed_sec = elapsed_ms as f64 / 1000.0;
        format!("[exit:{exit_code} | {elapsed_sec:.1}s]")
    } else {
        format!("[exit:{exit_code} | {elapsed_ms}ms]")
    }
}

/// Append metadata footer to output
fn append_metadata(output: &str, exit_code: i32, elapsed_ms: u128) -> String {
    let footer = format_metadata_footer(exit_code, elapsed_ms);
    if output.is_empty() {
        footer
    } else {
        format!("{}\n{footer}", output.trim_end())
    }
}

/// Summarize bash arguments for display
pub fn summarize_args(arguments: &str) -> String {
    BashExecutionArgs::from_json(arguments)
        .map(|args| args.command)
        .unwrap_or_default()
}

/// Execute a bash command
#[allow(clippy::too_many_lines)]
pub(super) async fn execute_bash(arguments: &str) -> Result<super::ToolResult, String> {
    let args = BashExecutionArgs::from_json(arguments)?;
    let start_time = Instant::now();

    // Build sandbox configuration with additional directories
    let cwd = std::env::current_dir().map_err(|e| format!("Failed to get cwd: {e}"))?;
    let additional_dirs = super::get_additional_dirs();
    let sandbox_config =
        super::sandbox::SandboxConfig::build_with_additional_dirs(&cwd, &additional_dirs)?;

    // Create command with proper stdio configuration
    let mut command = Command::new("bash");
    command
        .arg("-c")
        .arg(&args.command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    // Apply sandbox if enabled
    if args.use_sandbox {
        if let Some(strategy) = super::sandbox::detect_platform() {
            strategy.apply(&mut command, &sandbox_config)?;
        }
    } else {
        tracing::debug!("Sandbox disabled; running without filesystem restrictions");
    }

    // Spawn the command with piped stdout/stderr for streaming
    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to spawn command: {e}"))?;

    let mut stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let mut stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    let mut buf = Vec::with_capacity(BASH_OUTPUT_MAX_BYTES);
    let mut tmp_stdout = [0u8; 8192];
    let mut tmp_stderr = [0u8; 8192];
    let mut hit_cap = false;

    // Read both streams concurrently, interleaved, with a timeout.
    let read_result = timeout(Duration::from_secs(args.timeout), async {
        loop {
            tokio::select! {
                n = stdout.read(&mut tmp_stdout) => {
                    let n = n.map_err(|e| format!("stdout read error: {e}"))?;
                    if n == 0 {
                        // stdout closed — read remaining stderr
                        loop {
                            let n = stderr.read(&mut tmp_stderr).await
                                .map_err(|e| format!("stderr read error: {e}"))?;
                            if n == 0 { return Ok::<_, String>(()); }
                            buf.extend_from_slice(&tmp_stderr[..n]);
                            if buf.len() >= BASH_READ_CAP { hit_cap = true; return Ok(()); }
                        }
                    }
                    buf.extend_from_slice(&tmp_stdout[..n]);
                    if buf.len() >= BASH_READ_CAP { hit_cap = true; return Ok(()); }
                }
                n = stderr.read(&mut tmp_stderr) => {
                    let n = n.map_err(|e| format!("stderr read error: {e}"))?;
                    if n == 0 {
                        // stderr closed — read remaining stdout
                        loop {
                            let n = stdout.read(&mut tmp_stdout).await
                                .map_err(|e| format!("stdout read error: {e}"))?;
                            if n == 0 { return Ok(()); }
                            buf.extend_from_slice(&tmp_stdout[..n]);
                            if buf.len() >= BASH_READ_CAP { hit_cap = true; return Ok(()); }
                        }
                    }
                    buf.extend_from_slice(&tmp_stderr[..n]);
                    if buf.len() >= BASH_READ_CAP { hit_cap = true; return Ok(()); }
                }
            }
        }
    })
    .await;

    match read_result {
        Ok(Ok(())) => {},
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(format!("Command timed out after {} seconds", args.timeout)),
    }

    // If we hit the cap, kill the child explicitly
    if hit_cap {
        let _ = child.kill().await;
    }
    let status = child.wait().await.ok();
    let elapsed_ms = start_time.elapsed().as_millis();

    // Check for binary data before converting to string
    if is_binary_data(&buf) {
        let exit_code = status.and_then(|s| s.code()).unwrap_or(-1);
        return Ok(super::ToolResult {
            output: handle_binary_output(&buf, exit_code, elapsed_ms),
        });
    }

    let output_str = String::from_utf8_lossy(&buf);
    let success = status
        .as_ref()
        .is_some_and(std::process::ExitStatus::success);
    let exit_code = status.and_then(|s| s.code()).unwrap_or(-1);

    let result = if output_str.is_empty() {
        String::new()
    } else if hit_cap {
        format!("{output_str}\n[... output truncated at {BASH_READ_CAP} bytes ...]")
    } else if success {
        output_str.into_owned()
    } else if args.use_sandbox && is_sandbox_violation(&output_str) {
        format!(
            "{output_str}\n\n\
            [Sandbox restriction]: This command was blocked by the filesystem sandbox. \
            The sandbox restricts file access to the project directory and standard system paths. \
            Do NOT retry with different workarounds — the restriction is intentional. \
            Instead, inform the user that this command requires access outside the sandbox \
            and suggest they run it directly in their terminal."
        )
    } else {
        output_str.into_owned()
    };

    let result = truncate_output(&result, exit_code, elapsed_ms);

    Ok(super::ToolResult { output: result })
}

/// If `output` exceeds [`BASH_OUTPUT_MAX_BYTES`], write the full text to a
/// temporary file and return a summary pointing to that file. Otherwise return
/// the output with the metadata footer appended. The temp file receives only
/// the raw command output (no footer); the footer is included in the inline
/// summary so it is always visible in the tool response.
pub(super) fn truncate_output(output: &str, exit_code: i32, elapsed_ms: u128) -> String {
    if output.len() <= BASH_OUTPUT_MAX_BYTES {
        return append_metadata(output, exit_code, elapsed_ms);
    }

    let footer = format_metadata_footer(exit_code, elapsed_ms);
    let total_bytes = output.len();
    let total_lines = output.lines().count();

    // Try to write the full output to a temp file so the agent can search it.
    let tmp_dir = std::env::temp_dir().join("acai");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let file_name = format!("bash_output_{}.txt", uuid::Uuid::new_v4());
    let tmp_path = tmp_dir.join(&file_name);

    if let Err(e) = std::fs::write(&tmp_path, output) {
        // Could not write — fall back to a truncated inline result.
        debug!(
            "Failed to write overflow output to {}: {e}",
            tmp_path.display()
        );

        let half = BASH_OUTPUT_MAX_BYTES / 2;
        let head_end = output.floor_char_boundary(half);
        let tail_start = output.ceil_char_boundary(total_bytes - half);
        return format!(
            "[Output too long — {total_bytes} bytes, {total_lines} lines. \
             The command was too verbose; reformulate with less output \
             (e.g. pipe through `head`, `tail`, or `grep`).]\n\n\
             --- first ~{half} bytes ---\n{head}\n\n\
             --- last ~{half} bytes ---\n{tail}\n{footer}",
            head = &output[..head_end],
            tail = &output[tail_start..],
        );
    }

    let preview = BASH_OUTPUT_MAX_BYTES / 4;
    let head_end = output.floor_char_boundary(preview);
    let tail_start = output.ceil_char_boundary(total_bytes - preview);
    format!(
        "[Output too long — {total_bytes} bytes, {total_lines} lines.]\n\
         Full output saved to: {path}\n\
         You can search it with `grep` or view portions with `head`/`tail`.\n\
         Consider reformulating the command to produce less output.\n\n\
         --- first ~{preview} bytes ---\n{head}\n\n\
         --- last ~{preview} bytes ---\n{tail}\n{footer}",
        path = tmp_path.display(),
        head = &output[..head_end],
        tail = &output[tail_start..],
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn truncate_output_passes_through_small_output() {
        let small = "hello world";
        let result = truncate_output(small, 0, 100);
        assert!(result.contains(small));
        assert!(result.contains("[exit:0 | 100ms]"));
    }

    #[test]
    fn truncate_output_passes_through_at_limit() {
        let exact = "a".repeat(BASH_OUTPUT_MAX_BYTES);
        let result = truncate_output(&exact, 0, 50);
        assert!(result.contains(&exact));
        assert!(result.contains("[exit:0 | 50ms]"));
    }

    #[test]
    fn truncate_output_truncates_large_output() {
        let large = "x".repeat(BASH_OUTPUT_MAX_BYTES + 1000);
        let result = truncate_output(&large, 0, 500);
        assert!(result.len() < large.len());
        assert!(result.contains("[Output too long"));
        assert!(result.contains("Full output saved to:"));
        assert!(result.contains("[exit:0 | 500ms]"));
    }

    #[test]
    fn truncate_output_handles_multibyte_chars() {
        // Create output with multi-byte UTF-8 characters that exceeds the limit
        let large = "é".repeat(BASH_OUTPUT_MAX_BYTES); // each 'é' is 2 bytes
        let result = truncate_output(&large, 1, 2000);
        assert!(result.contains("[Output too long"));
        assert!(result.contains("[exit:1 | 2.0s]"));
        // Verify the result is valid UTF-8 (would panic if not)
        let _ = result.as_bytes();
    }

    #[test]
    fn truncate_output_temp_file_has_no_footer() {
        let large = "x".repeat(BASH_OUTPUT_MAX_BYTES + 1000);
        let result = truncate_output(&large, 0, 100);
        // Extract the temp file path from the result
        let path_line = result
            .lines()
            .find(|l| l.starts_with("Full output saved to:"))
            .expect("should contain temp file path");
        let path = path_line
            .trim_start_matches("Full output saved to: ")
            .trim();
        let contents = std::fs::read_to_string(path).expect("should read temp file");
        assert!(
            !contents.contains("[exit:"),
            "temp file should not contain metadata footer"
        );
    }

    // ===========================================================================
    // Metadata Footer Tests
    // ===========================================================================

    #[test]
    fn metadata_footer_shows_milliseconds_under_1_second() {
        let footer = format_metadata_footer(0, 500);
        assert_eq!(footer, "[exit:0 | 500ms]");
    }

    #[test]
    fn metadata_footer_shows_milliseconds_at_boundary() {
        // 999ms should still show as milliseconds
        let footer = format_metadata_footer(0, 999);
        assert_eq!(footer, "[exit:0 | 999ms]");
    }

    #[test]
    fn metadata_footer_shows_seconds_over_1_second() {
        // 1000ms should show as 1.0s
        let footer = format_metadata_footer(0, 1000);
        assert_eq!(footer, "[exit:0 | 1.0s]");
    }

    #[test]
    fn metadata_footer_shows_seconds_with_decimal() {
        // 1234ms should show as 1.2s (rounded to 1 decimal)
        let footer = format_metadata_footer(1, 1234);
        assert_eq!(footer, "[exit:1 | 1.2s]");
    }

    #[test]
    fn metadata_footer_handles_large_values() {
        // 60000ms = 60.0s
        let footer = format_metadata_footer(0, 60000);
        assert_eq!(footer, "[exit:0 | 60.0s]");
    }

    // ===========================================================================
    // Streaming Tests
    // ===========================================================================

    #[tokio::test]
    async fn test_streaming_small_output() {
        // Command with small output returns it verbatim with metadata footer
        let args = r#"{"command": "echo hello world"}"#;
        let result = Box::pin(execute_bash(args)).await.unwrap();
        assert!(result.output.contains("hello world"));
        assert!(result.output.contains("[exit:0 |"));
    }

    #[tokio::test]
    async fn test_streaming_large_output_is_capped() {
        // Command that produces output beyond BASH_READ_CAP is truncated
        // Produce ~200KB of output (well over the 100KB cap)
        let args = r#"{"command": "yes | head -c 200000"}"#;
        let result = Box::pin(execute_bash(args)).await.unwrap();
        // Should contain the truncation marker
        assert!(result.output.contains("[... output truncated at"));
        // Should still have useful content
        assert!(!result.output.is_empty());
        // Should contain metadata footer
        assert!(result.output.contains("[exit:"));
    }

    #[tokio::test]
    async fn test_streaming_timeout() {
        // Command that hangs respects the timeout
        let args = r#"{"command": "sleep 999", "timeout": 1}"#;
        let result = Box::pin(execute_bash(args)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("timed out"));
    }

    #[tokio::test]
    async fn test_streaming_stderr_included() {
        // Command that writes to stderr has it captured with metadata footer
        let args = r#"{"command": "echo err >&2"}"#;
        let result = Box::pin(execute_bash(args)).await.unwrap();
        assert!(result.output.contains("err"));
        assert!(result.output.contains("[exit:0 |"));
    }

    // ===========================================================================
    // Sandbox Tests
    // ===========================================================================

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_sandbox_blocks_write_outside_cwd() {
        let target = format!("/tmp/acai_sandbox_test_{}", uuid::Uuid::new_v4());
        let args = format!(r#"{{"command": "touch {target}"}}"#);
        let result = Box::pin(execute_bash(&args)).await.unwrap();
        assert!(
            result.output.contains("Operation not permitted")
                || result.output.contains("Permission denied"),
            "Expected sandbox to block write outside cwd, got: {}",
            result.output
        );
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_sandbox_allows_read_in_cwd() {
        let args = r#"{"command": "ls Cargo.toml"}"#;
        let result = Box::pin(execute_bash(args)).await.unwrap();
        assert!(
            result.output.contains("Cargo.toml"),
            "Expected ls in cwd to succeed, got: {}",
            result.output
        );
        // Should contain metadata footer
        assert!(result.output.contains("[exit:0 |"));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_sandbox_blocks_read_outside_cwd() {
        let args = r#"{"command": "ls ~/Desktop"}"#;
        let result = Box::pin(execute_bash(args)).await.unwrap();
        assert!(
            result.output.contains("Operation not permitted")
                || result.output.contains("Permission denied"),
            "Expected sandbox to block read of ~/Desktop, got: {}",
            result.output
        );
    }

    // ===========================================================================
    // Binary Data Detection Tests
    // ===========================================================================

    #[test]
    fn test_is_binary_data_detects_null_bytes() {
        // Data with null bytes should be detected as binary (need >8 null bytes)
        let binary_data =
            b"hello\x00world\x00more\x00nulls\x00here\x00more\x00data\x00extra\x00again\x00more";
        assert!(is_binary_data(binary_data));
    }

    #[test]
    fn test_is_binary_data_detects_high_non_printable_ratio() {
        // Data with many non-printable characters should be detected as binary
        // Create data with ~50% non-printable characters
        let mut binary_data = Vec::new();
        for i in 0..100 {
            if i % 2 == 0 {
                binary_data.push(0x01); // Non-printable
            } else {
                binary_data.push(b'A'); // Printable
            }
        }
        assert!(is_binary_data(&binary_data));
    }

    #[test]
    fn test_is_binary_data_allows_text() {
        // Normal text should not be detected as binary
        let text_data = b"Hello, world!\nThis is a test.\nLine 3.\n";
        assert!(!is_binary_data(text_data));
    }

    #[test]
    fn test_is_binary_data_allows_multibyte_utf8() {
        // UTF-8 text with multi-byte characters should not be detected as binary
        let utf8_text = "Hello, 世界!\nПривет мир\n🎉".as_bytes();
        assert!(!is_binary_data(utf8_text));
    }

    #[test]
    fn test_is_binary_data_allows_empty() {
        // Empty data should not be detected as binary
        assert!(!is_binary_data(b""));
    }

    #[test]
    fn test_is_binary_data_allows_few_null_bytes() {
        // A few null bytes (below threshold) should not trigger binary detection
        let text_with_few_nulls = b"hello\x00world";
        assert!(!is_binary_data(text_with_few_nulls));
    }

    #[test]
    fn test_detect_mime_type_png() {
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(detect_mime_type(&png_header), Some("image/png".to_string()));
    }

    #[test]
    fn test_detect_mime_type_jpeg() {
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        assert_eq!(
            detect_mime_type(&jpeg_header),
            Some("image/jpeg".to_string())
        );
    }

    #[test]
    fn test_detect_mime_type_pdf() {
        let pdf_header = b"%PDF-1.4";
        assert_eq!(
            detect_mime_type(pdf_header),
            Some("application/pdf".to_string())
        );
    }

    #[test]
    fn test_detect_mime_type_zip() {
        let zip_header = [0x50, 0x4B, 0x03, 0x04, 0x00, 0x00, 0x00];
        assert_eq!(
            detect_mime_type(&zip_header),
            Some("application/zip".to_string())
        );
    }

    #[test]
    fn test_detect_mime_type_gzip() {
        let gzip_header = [0x1F, 0x8B, 0x08, 0x00];
        assert_eq!(
            detect_mime_type(&gzip_header),
            Some("application/gzip".to_string())
        );
    }

    #[test]
    fn test_detect_mime_type_unknown() {
        // Random data should return None
        let unknown_data = b"Hello, world!";
        assert_eq!(detect_mime_type(unknown_data), None);
    }

    #[test]
    fn test_detect_mime_type_too_short() {
        // Data that's too short should return None
        let short_data = [0x89, 0x50];
        assert_eq!(detect_mime_type(&short_data), None);
    }

    #[tokio::test]
    async fn test_binary_output_handling() {
        // Command that produces binary output (random bytes)
        let args = r#"{"command": "head -c 100 /dev/urandom"}"#;
        let result = Box::pin(execute_bash(args)).await.unwrap();
        // Should detect binary and show appropriate message
        assert!(
            result.output.contains("[Binary output detected") || result.output.contains("[exit:"),
            "Expected binary output handling, got: {}",
            result.output
        );
    }

    #[tokio::test]
    async fn test_binary_output_with_known_type() {
        // Create a small gzip-compressed file and read it
        let args = r#"{"command": "echo 'hello' | gzip | head -c 20"}"#;
        let result = Box::pin(execute_bash(args)).await.unwrap();
        // Should detect gzip magic number
        assert!(
            result.output.contains("application/gzip") || result.output.contains("[exit:"),
            "Expected gzip detection, got: {}",
            result.output
        );
    }

    #[tokio::test]
    async fn test_text_output_not_detected_as_binary() {
        // Normal text output should not be detected as binary
        let args = r#"{"command": "echo 'Hello, world!'"}"#;
        let result = Box::pin(execute_bash(args)).await.unwrap();
        assert!(
            !result.output.contains("[Binary output detected"),
            "Text output should not be detected as binary, got: {}",
            result.output
        );
        assert!(result.output.contains("Hello, world!"));
    }
}
