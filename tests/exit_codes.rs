//! Integration tests for exit codes.
//!
//! These tests verify that cake returns the correct exit code for each
//! failure mode:
//!
//! - 0 — success
//! - 1 — agent/tool error
//! - 2 — API error (rate limit, auth failure, network error)
//! - 3 — input error (no prompt, invalid flags, missing API key)

#![allow(clippy::expect_used)]

mod support;

use std::process::Stdio;

use support::TestEnv;

fn cake_env() -> TestEnv {
    TestEnv::new("cake-exit-test")
}

// --- Exit code 0: success ---

#[test]
fn test_help_exits_zero() {
    let env = cake_env();
    let output = env
        .command()
        .arg("--help")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "--help should exit 0");
}

#[test]
fn test_version_exits_zero() {
    let env = cake_env();
    let output = env
        .command()
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "--version should exit 0");
}

// --- Exit code 3: input error ---

#[test]
fn test_no_prompt_exits_three() {
    let env = cake_env();
    let output = env
        .command()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 3, "No prompt should exit 3, got {code}");
}

#[test]
fn test_invalid_flag_exits_three() {
    let env = cake_env();
    let output = env
        .command()
        .arg("--bogus-flag")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 3, "Invalid flag should exit 3, got {code}");
}

#[test]
fn test_no_model_configured_exits_three() {
    let env = cake_env();
    let output = env
        .command()
        .arg("test prompt")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 3, "Missing model config should exit 3, got {code}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No model specified"),
        "Error message should mention missing model config. Stderr: {stderr}"
    );
}

#[test]
fn test_missing_api_key_exits_three() {
    let env = cake_env();
    env.write_project_settings(
        r#"
default_model = "test"

[[models]]
name = "test"
model = "glm-5.1"
base_url = "https://example.com"
api_key_env = "TEST_API_KEY"
"#,
    );

    let output = env
        .command()
        .arg("test prompt")
        .env_remove("TEST_API_KEY")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 3, "Missing API key should exit 3, got {code}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("TEST_API_KEY"),
        "Error message should mention the configured env var. Stderr: {stderr}"
    );
}

#[test]
fn test_unknown_model_exits_three() {
    let env = cake_env();
    let output = env
        .command()
        .arg("--model")
        .arg("nonexistent-model")
        .arg("test prompt")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 3, "Unknown model should exit 3, got {code}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Unknown model 'nonexistent-model'"),
        "Error message should mention the unknown model. Stderr: {stderr}"
    );
}

#[test]
fn test_invalid_session_uuid_exits_three() {
    let env = cake_env();
    let output = env
        .command()
        .arg("--resume")
        .arg("not-a-uuid")
        .arg("test prompt")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 3, "Invalid session UUID should exit 3, got {code}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("resume by file path is no longer supported"),
        "Error should reject non-UUID resume values. Stderr: {stderr}"
    );
}
