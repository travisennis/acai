use log::debug;
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, timeout};

/// Maximum number of bytes the Bash tool will return inline.
/// Output exceeding this limit is written to a temporary file and the agent
/// receives a truncated message with a path to the full output.
const BASH_OUTPUT_MAX_BYTES: usize = 50_000;

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

// =============================================================================
// Bash Tool Definition
// =============================================================================

/// Returns the Bash tool definition
pub(super) fn bash_tool() -> Tool {
    Tool {
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
// Tool Execution
// =============================================================================

/// Result of executing a tool
#[derive(Debug)]
pub struct ToolResult {
    pub output: String,
}

/// Execute a tool call
pub(super) async fn execute_tool(name: &str, arguments: &str) -> Result<ToolResult, String> {
    match name {
        "Bash" => execute_bash(arguments).await,
        _ => Err(format!("Unknown tool: {name}")),
    }
}

async fn execute_bash(arguments: &str) -> Result<ToolResult, String> {
    #[derive(Deserialize)]
    struct BashArgs {
        command: String,
        timeout: Option<u64>,
    }

    let args: BashArgs =
        serde_json::from_str(arguments).map_err(|e| format!("Invalid bash arguments: {e}"))?;

    // Use default timeout of 60 seconds if not specified
    let timeout_secs = args.timeout.unwrap_or(60);

    // Run the shell command with timeout using tokio
    // timeout() returns Result<Result<Output, io::Error>, Elapsed>
    let output = match timeout(
        Duration::from_secs(timeout_secs),
        tokio::process::Command::new("bash")
            .arg("-c")
            .arg(&args.command)
            .kill_on_drop(true)
            .output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => return Err(format!("Failed to execute command: {e}")),
        Err(_) => return Err(format!("Command timed out after {timeout_secs} seconds")),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Always include both stdout and stderr in output, since many commands
    // (like cargo clippy) output to stderr even on success
    let result = if stdout.is_empty() && stderr.is_empty() {
        String::new()
    } else if output.status.success() {
        format!("{stdout}{stderr}")
    } else {
        format!(
            "Exit code {}:\n{}{}",
            output.status.code().unwrap_or(-1),
            stdout,
            stderr
        )
    };

    let result = truncate_output(&result);

    Ok(ToolResult { output: result })
}

/// If `output` exceeds [`BASH_OUTPUT_MAX_BYTES`], write the full text to a
/// temporary file and return a summary pointing to that file. Otherwise return
/// the output unchanged.
fn truncate_output(output: &str) -> String {
    if output.len() <= BASH_OUTPUT_MAX_BYTES {
        return output.to_string();
    }

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
             --- last ~{half} bytes ---\n{tail}",
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
         --- last ~{preview} bytes ---\n{tail}",
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
        assert_eq!(truncate_output(small), small);
    }

    #[test]
    fn truncate_output_passes_through_at_limit() {
        let exact = "a".repeat(BASH_OUTPUT_MAX_BYTES);
        assert_eq!(truncate_output(&exact), exact);
    }

    #[test]
    fn truncate_output_truncates_large_output() {
        let large = "x".repeat(BASH_OUTPUT_MAX_BYTES + 1000);
        let result = truncate_output(&large);
        assert!(result.len() < large.len());
        assert!(result.contains("[Output too long"));
        assert!(result.contains("Full output saved to:"));
    }

    #[test]
    fn truncate_output_handles_multibyte_chars() {
        // Create output with multi-byte UTF-8 characters that exceeds the limit
        let large = "é".repeat(BASH_OUTPUT_MAX_BYTES); // each 'é' is 2 bytes
        let result = truncate_output(&large);
        assert!(result.contains("[Output too long"));
        // Verify the result is valid UTF-8 (would panic if not)
        let _ = result.as_bytes();
    }
}
