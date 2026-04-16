//! Exit codes for acai CLI.
//!
//! acai uses structured exit codes so that calling scripts, CI pipelines, and
//! other automation can distinguish between failure modes without parsing
//! stderr.
//!
//! | Code | Meaning       | Description                                              |
//! |------|---------------|----------------------------------------------------------|
//! | `0`  | Success       | The agent completed and produced a response               |
//! | `1`  | Agent error   | The model or a tool encountered an error during execution|
//! | `2`  | API error     | Rate limit, auth failure, or network error               |
//! | `3`  | Input error   | No prompt provided, invalid flags, missing API key       |

use std::process::ExitCode;

/// Exit code constants for acai.
///
/// These values are returned from `main()` so that shell scripts and CI
/// pipelines can branch on the reason for failure.
pub mod code {
    /// Successful execution.
    pub const SUCCESS: u8 = 0;
    /// Agent or tool error during execution.
    pub const AGENT_ERROR: u8 = 1;
    /// API error (rate limit, auth failure, network error).
    pub const API_ERROR: u8 = 2;
    /// Input error (no prompt, invalid flags, missing API key).
    pub const INPUT_ERROR: u8 = 3;
}

/// Classify an `anyhow::Error` into a `u8` exit code value.
///
/// Convenience wrapper around [`classify`] that returns the raw `u8`
/// value instead of `std::process::ExitCode`. Useful when the code value
/// needs to be embedded in structured output (e.g. streaming JSON).
pub fn classify_to_u8(err: &anyhow::Error) -> u8 {
    let exit = classify(err);
    // ExitCode is guaranteed to be in 0..=255, so this conversion is safe.
    // We extract the value by matching the known codes.
    if exit == std::process::ExitCode::from(code::SUCCESS) {
        code::SUCCESS
    } else if exit == std::process::ExitCode::from(code::INPUT_ERROR) {
        code::INPUT_ERROR
    } else if exit == std::process::ExitCode::from(code::API_ERROR) {
        code::API_ERROR
    } else {
        code::AGENT_ERROR
    }
}

/// Classify an `anyhow::Error` into an exit code.
///
/// The classification inspects the error chain for known patterns:
///
/// - **Input errors** (exit 3): missing API key, missing prompt, invalid
///   model name, invalid session UUID, clap argument errors, and other
///   validation failures.
/// - **API errors** (exit 2): HTTP 401/403/429 responses, connection
///   failures, and timeouts.
/// - **Agent/tool errors** (exit 1): everything else (tool execution
///   failures, unexpected API responses, internal errors).
pub fn classify(err: &anyhow::Error) -> ExitCode {
    // Check the full error chain from outermost to innermost.
    let mut source: Option<&dyn std::error::Error> = Some(err.as_ref());
    while let Some(e) = source {
        let msg = e.to_string();

        // --- Input errors (exit 3) ---
        if is_input_error(&msg) {
            return ExitCode::from(code::INPUT_ERROR);
        }

        // --- API errors (exit 2) ---
        if is_api_error(&msg) {
            return ExitCode::from(code::API_ERROR);
        }

        // Check for reqwest::Error in the chain via anyhow's downcast
        if let Some(req_err) = err.downcast_ref::<reqwest::Error>()
            && is_reqwest_api_error(req_err)
        {
            return ExitCode::from(code::API_ERROR);
        }

        source = e.source();
    }

    // Default: agent/tool error
    ExitCode::from(code::AGENT_ERROR)
}

/// Check if a `reqwest::Error` represents an API-level failure.
fn is_reqwest_api_error(req_err: &reqwest::Error) -> bool {
    // Auth failures (401/403)
    if let Some(status) = req_err.status()
        && matches!(status.as_u16(), 401 | 403)
    {
        return true;
    }
    // Connection failures
    if req_err.is_connect() {
        return true;
    }
    // Timeouts
    if req_err.is_timeout() {
        return true;
    }
    // Request construction errors (bad URL, etc.)
    if req_err.is_request() {
        return true;
    }
    false
}

/// Determine if an error message indicates an input/validation error.
fn is_input_error(msg: &str) -> bool {
    // Missing API key
    if msg.contains("Environment variable") && msg.contains("is not set") && msg.contains("API key")
    {
        return true;
    }
    if msg.contains("Environment variable") && msg.contains("is set but empty") {
        return true;
    }

    // Missing prompt
    if msg.contains("No input provided") {
        return true;
    }
    if msg.contains("No input provided via stdin") {
        return true;
    }

    // Invalid model name
    if msg.contains("Invalid model name") {
        return true;
    }
    if msg.contains("Unknown model") {
        return true;
    }

    // Invalid session UUID
    if msg.contains("Invalid session UUID") {
        return true;
    }

    // Session not found
    if msg.contains("No previous session found") {
        return true;
    }
    if msg.contains("not found in this directory") {
        return true;
    }

    // clap argument errors (e.g. required arguments missing, bad flag values)
    if msg.contains("error:") && msg.contains("USAGE") {
        return true;
    }

    // Worktree errors that are input-related
    if msg.contains("Failed to cd into worktree") {
        return true;
    }

    // Failed to get current directory (unlikely but input-related)
    if msg.contains("Failed to get current directory") {
        return true;
    }

    false
}

/// Determine if an error is an API/network error based on the message.
fn is_api_error(msg: &str) -> bool {
    // Rate limiting
    if msg.contains("429") {
        return true;
    }

    // Auth failures in API response bodies
    if msg.contains("401") || msg.contains("403") {
        return true;
    }

    // Common API error patterns from the agent's complete_turn error formatting
    // The agent formats non-success responses as "model_name\n\n{error_body}"
    // Rate limit and auth errors will appear in these bodies.
    if msg.contains("rate_limit") || msg.contains("rate_limit_exceeded") {
        return true;
    }
    if msg.contains("authentication")
        || msg.contains("invalid_api_key")
        || msg.contains("invalid x-api-key")
    {
        return true;
    }

    // Connection/network errors
    if msg.contains("error sending request")
        || msg.contains("connection refused")
        || msg.contains("connection timed out")
        || msg.contains("dns error")
        || msg.contains("resolve error")
    {
        return true;
    }

    // reqwest timeout pattern
    if msg.contains("builder error") || msg.contains("request error") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_code_is_zero() {
        assert_eq!(code::SUCCESS, 0);
    }

    #[test]
    fn agent_error_code_is_one() {
        assert_eq!(code::AGENT_ERROR, 1);
    }

    #[test]
    fn api_error_code_is_two() {
        assert_eq!(code::API_ERROR, 2);
    }

    #[test]
    fn input_error_code_is_three() {
        assert_eq!(code::INPUT_ERROR, 3);
    }

    // --- Input error classification ---

    #[test]
    fn classify_missing_api_key() {
        let err = anyhow::anyhow!(
            "Environment variable 'OPENCODE_ZEN_API_TOKEN' is not set. \
             Please set it to your API key: environment variable not found"
        );
        assert_eq!(classify(&err), ExitCode::from(code::INPUT_ERROR));
    }

    #[test]
    fn classify_empty_api_key() {
        let err = anyhow::anyhow!("Environment variable 'OPENCODE_ZEN_API_TOKEN' is set but empty");
        assert_eq!(classify(&err), ExitCode::from(code::INPUT_ERROR));
    }

    #[test]
    fn classify_no_input() {
        let err = anyhow::anyhow!(
            "No input provided. Provide a prompt as an argument, use 'acai -' for stdin, or pipe input to acai."
        );
        assert_eq!(classify(&err), ExitCode::from(code::INPUT_ERROR));
    }

    #[test]
    fn classify_no_stdin() {
        let err = anyhow::anyhow!("No input provided via stdin");
        assert_eq!(classify(&err), ExitCode::from(code::INPUT_ERROR));
    }

    #[test]
    fn classify_invalid_model_name() {
        let err = anyhow::anyhow!(
            "Invalid model name 'Invalid Name!': names must contain only lowercase letters"
        );
        assert_eq!(classify(&err), ExitCode::from(code::INPUT_ERROR));
    }

    #[test]
    fn classify_unknown_model() {
        let err = anyhow::anyhow!(
            "Unknown model 'nonexistent': claude, deepseek. Use a model name from settings.toml"
        );
        assert_eq!(classify(&err), ExitCode::from(code::INPUT_ERROR));
    }

    #[test]
    fn classify_invalid_session_uuid() {
        let err = anyhow::anyhow!("Invalid session UUID 'not-a-uuid': invalid character");
        assert_eq!(classify(&err), ExitCode::from(code::INPUT_ERROR));
    }

    #[test]
    fn classify_no_previous_session() {
        let err = anyhow::anyhow!("No previous session found for this directory");
        assert_eq!(classify(&err), ExitCode::from(code::INPUT_ERROR));
    }

    #[test]
    fn classify_session_not_found() {
        let err = anyhow::anyhow!("Session abc123 not found in this directory");
        assert_eq!(classify(&err), ExitCode::from(code::INPUT_ERROR));
    }

    // --- API error classification ---

    #[test]
    fn classify_rate_limit_in_message() {
        let err = anyhow::anyhow!(
            "glm-5\n\n{{\"error\":{{\"code\":429,\"message\":\"Rate limit exceeded\"}}}}"
        );
        assert_eq!(classify(&err), ExitCode::from(code::API_ERROR));
    }

    #[test]
    fn classify_auth_failure_in_message() {
        let err = anyhow::anyhow!(
            "glm-5\n\n{{\"error\":{{\"code\":401,\"message\":\"Invalid API key\"}}}}"
        );
        assert_eq!(classify(&err), ExitCode::from(code::API_ERROR));
    }

    #[test]
    fn classify_forbidden_in_message() {
        let err =
            anyhow::anyhow!("glm-5\n\n{{\"error\":{{\"code\":403,\"message\":\"Forbidden\"}}}}");
        assert_eq!(classify(&err), ExitCode::from(code::API_ERROR));
    }

    #[test]
    fn classify_rate_limit_exceeded_pattern() {
        let err = anyhow::anyhow!("glm-5\n\n{{\"error\":{{\"type\":\"rate_limit_exceeded\"}}}}");
        assert_eq!(classify(&err), ExitCode::from(code::API_ERROR));
    }

    // --- Agent error classification (default) ---

    #[test]
    fn classify_generic_error_as_agent_error() {
        let err = anyhow::anyhow!("Something unexpected went wrong");
        assert_eq!(classify(&err), ExitCode::from(code::AGENT_ERROR));
    }

    #[test]
    fn classify_parse_error_as_agent_error() {
        let err = anyhow::anyhow!("Failed to deserialize API response");
        assert_eq!(classify(&err), ExitCode::from(code::AGENT_ERROR));
    }

    #[test]
    fn classify_server_error_as_agent_error() {
        // 500/503 are retryable and exhausted, but they are server errors,
        // not auth/rate-limit, so they fall into agent error.
        let err = anyhow::anyhow!(
            "glm-5\n\n{{\"error\":{{\"code\":500,\"message\":\"Internal server error\"}}}}"
        );
        assert_eq!(classify(&err), ExitCode::from(code::AGENT_ERROR));
    }
}
