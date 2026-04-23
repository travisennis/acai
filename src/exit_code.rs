//! Exit codes for cake CLI.
//!
//! cake uses structured exit codes so that calling scripts, CI pipelines, and
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

/// Exit code constants for cake.
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

/// A structured API error that preserves the HTTP status code.
///
/// This allows `classify` to inspect the status code directly instead of
/// relying on fragile string matching against status code numbers that could
/// appear anywhere in an error message.
#[derive(Debug, thiserror::Error)]
#[error("{body}")]
pub struct ApiError {
    /// The HTTP status code from the API response.
    pub status: u16,
    /// The formatted error body (model name + response text).
    pub body: String,
}

/// Classify an `anyhow::Error` into a `u8` exit code value.
///
/// This is the primary classification function. Returns the raw `u8` code
/// which can be embedded in structured output (e.g. streaming JSON) or
/// converted to `std::process::ExitCode` via [`classify_to_exit_code`].
pub fn classify_to_u8(err: &anyhow::Error) -> u8 {
    // Check for structured ApiError first — this gives us reliable status codes.
    if let Some(api_err) = err.downcast_ref::<ApiError>() {
        return match api_err.status {
            401 | 403 | 429 => code::API_ERROR,
            _ => code::AGENT_ERROR,
        };
    }

    // Walk the error chain for reqwest::Error and string-based patterns.
    for cause in err.chain() {
        // Check for reqwest::Error at each level of the chain.
        if let Some(req_err) = cause.downcast_ref::<reqwest::Error>()
            && is_reqwest_api_error(req_err)
        {
            return code::API_ERROR;
        }

        let msg = cause.to_string();

        // --- Input errors (exit 3) ---
        if is_input_error(&msg) {
            return code::INPUT_ERROR;
        }

        // --- API errors (exit 2) via string patterns ---
        // These cover network/connection errors that appear as string messages
        // rather than typed reqwest errors (e.g. when re-wrapped by anyhow).
        if is_api_network_error(&msg) {
            return code::API_ERROR;
        }
    }

    // Default: agent/tool error
    code::AGENT_ERROR
}

/// Classify an `anyhow::Error` into an `ExitCode`.
///
/// Convenience wrapper around [`classify_to_u8`] for use in `main()`.
pub fn classify(err: &anyhow::Error) -> ExitCode {
    ExitCode::from(classify_to_u8(err))
}

/// Check if a `reqwest::Error` represents an API-level failure.
fn is_reqwest_api_error(req_err: &reqwest::Error) -> bool {
    // Auth failures and rate limiting (401/403/429)
    if let Some(status) = req_err.status()
        && matches!(status.as_u16(), 401 | 403 | 429)
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
    if msg.contains("Session") && msg.contains("not found") {
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

/// Determine if an error message indicates a network/connection error.
///
/// This only matches network-level patterns (connection refused, DNS, timeout).
/// HTTP status code classification is handled structurally via [`ApiError`].
fn is_api_network_error(msg: &str) -> bool {
    if msg.contains("error sending request")
        || msg.contains("connection refused")
        || msg.contains("connection timed out")
        || msg.contains("dns error")
        || msg.contains("resolve error")
    {
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
        assert_eq!(classify_to_u8(&err), code::INPUT_ERROR);
    }

    #[test]
    fn classify_empty_api_key() {
        let err = anyhow::anyhow!("Environment variable 'OPENCODE_ZEN_API_TOKEN' is set but empty");
        assert_eq!(classify_to_u8(&err), code::INPUT_ERROR);
    }

    #[test]
    fn classify_no_input() {
        let err = anyhow::anyhow!(
            "No input provided. Provide a prompt as an argument, use 'cake -' for stdin, or pipe input to cake."
        );
        assert_eq!(classify_to_u8(&err), code::INPUT_ERROR);
    }

    #[test]
    fn classify_no_stdin() {
        let err = anyhow::anyhow!("No input provided via stdin");
        assert_eq!(classify_to_u8(&err), code::INPUT_ERROR);
    }

    #[test]
    fn classify_invalid_model_name() {
        let err = anyhow::anyhow!(
            "Invalid model name 'Invalid Name!': names must contain only lowercase letters"
        );
        assert_eq!(classify_to_u8(&err), code::INPUT_ERROR);
    }

    #[test]
    fn classify_unknown_model() {
        let err = anyhow::anyhow!(
            "Unknown model 'nonexistent': claude, deepseek. Use a model name from settings.toml"
        );
        assert_eq!(classify_to_u8(&err), code::INPUT_ERROR);
    }

    #[test]
    fn classify_invalid_session_uuid() {
        let err = anyhow::anyhow!("Invalid session UUID 'not-a-uuid': invalid character");
        assert_eq!(classify_to_u8(&err), code::INPUT_ERROR);
    }

    #[test]
    fn classify_no_previous_session() {
        let err = anyhow::anyhow!("No previous session found for this directory");
        assert_eq!(classify_to_u8(&err), code::INPUT_ERROR);
    }

    #[test]
    fn classify_session_not_found() {
        let err = anyhow::anyhow!("Session abc123 not found");
        assert_eq!(classify_to_u8(&err), code::INPUT_ERROR);
    }

    // --- API error classification via structured ApiError ---

    #[test]
    fn classify_rate_limit_via_api_error() {
        let err = anyhow::Error::new(ApiError {
            status: 429,
            body: "glm-5\n\nRate limit exceeded".to_string(),
        });
        assert_eq!(classify_to_u8(&err), code::API_ERROR);
    }

    #[test]
    fn classify_auth_failure_via_api_error() {
        let err = anyhow::Error::new(ApiError {
            status: 401,
            body: "glm-5\n\nInvalid API key".to_string(),
        });
        assert_eq!(classify_to_u8(&err), code::API_ERROR);
    }

    #[test]
    fn classify_forbidden_via_api_error() {
        let err = anyhow::Error::new(ApiError {
            status: 403,
            body: "glm-5\n\nForbidden".to_string(),
        });
        assert_eq!(classify_to_u8(&err), code::API_ERROR);
    }

    #[test]
    fn classify_server_error_via_api_error() {
        let err = anyhow::Error::new(ApiError {
            status: 500,
            body: "glm-5\n\nInternal server error".to_string(),
        });
        assert_eq!(classify_to_u8(&err), code::AGENT_ERROR);
    }

    // --- API error classification via network patterns ---

    #[test]
    fn classify_connection_refused() {
        let err = anyhow::anyhow!("connection refused");
        assert_eq!(classify_to_u8(&err), code::API_ERROR);
    }

    #[test]
    fn classify_dns_error() {
        let err = anyhow::anyhow!("dns error: could not resolve host");
        assert_eq!(classify_to_u8(&err), code::API_ERROR);
    }

    // --- Agent error classification (default) ---

    #[test]
    fn classify_generic_error_as_agent_error() {
        let err = anyhow::anyhow!("Something unexpected went wrong");
        assert_eq!(classify_to_u8(&err), code::AGENT_ERROR);
    }

    #[test]
    fn classify_parse_error_as_agent_error() {
        let err = anyhow::anyhow!("Failed to deserialize API response");
        assert_eq!(classify_to_u8(&err), code::AGENT_ERROR);
    }

    // --- Verify that bare status code numbers don't cause false positives ---

    #[test]
    fn bare_429_in_message_is_not_api_error() {
        let err = anyhow::anyhow!("Found 429 results in the database");
        assert_eq!(classify_to_u8(&err), code::AGENT_ERROR);
    }

    #[test]
    fn bare_401_in_message_is_not_api_error() {
        let err = anyhow::anyhow!("File at /path/401/index.html not found");
        assert_eq!(classify_to_u8(&err), code::AGENT_ERROR);
    }

    // --- classify() returns correct ExitCode ---

    #[test]
    fn classify_returns_exit_code() {
        let err = anyhow::anyhow!("Something unexpected went wrong");
        assert_eq!(classify(&err), ExitCode::from(code::AGENT_ERROR));

        let err = anyhow::anyhow!("No input provided");
        assert_eq!(classify(&err), ExitCode::from(code::INPUT_ERROR));
    }
}
