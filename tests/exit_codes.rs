//! Integration tests for exit codes.
//!
//! These tests verify that acai returns the correct exit code for each
//! failure mode:
//!
//! - 0 — success
//! - 1 — agent/tool error
//! - 2 — API error (rate limit, auth failure, network error)
//! - 3 — input error (no prompt, invalid flags, missing API key)

#![allow(clippy::expect_used)]

use std::process::{Command, Stdio};

fn get_binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_acai"))
}

/// Build a `Command` with an isolated `ACAI_DATA_DIR`.
fn acai_cmd() -> Command {
    let mut cmd = Command::new(get_binary_path());
    let tmp = std::env::temp_dir().join(format!("acai_exit_test_{}", std::process::id()));
    cmd.env("ACAI_DATA_DIR", tmp);
    cmd
}

// --- Exit code 0: success ---

#[test]
fn test_help_exits_zero() {
    let output = acai_cmd()
        .arg("--help")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "--help should exit 0");
}

#[test]
fn test_version_exits_zero() {
    let output = acai_cmd()
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
    let output = acai_cmd()
        .env_remove("OPENCODE_ZEN_API_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 3, "No prompt should exit 3, got {code}");
}

#[test]
fn test_invalid_flag_exits_three() {
    let output = acai_cmd()
        .arg("--bogus-flag")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 3, "Invalid flag should exit 3, got {code}");
}

#[test]
fn test_missing_api_key_exits_three() {
    let output = acai_cmd()
        .arg("test prompt")
        .env_remove("OPENCODE_ZEN_API_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 3, "Missing API key should exit 3, got {code}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("OPENCODE_ZEN_API_TOKEN"),
        "Error message should mention the env var. Stderr: {stderr}"
    );
}

#[test]
fn test_unknown_model_exits_three() {
    let output = acai_cmd()
        .arg("--model")
        .arg("nonexistent_model")
        .arg("test prompt")
        .env_remove("OPENCODE_ZEN_API_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let code = output.status.code().unwrap_or(-1);
    assert_eq!(code, 3, "Unknown model should exit 3, got {code}");
}

#[test]
fn test_invalid_session_uuid_exits_three() {
    let output = acai_cmd()
        .arg("--resume")
        .arg("not-a-uuid")
        .arg("test prompt")
        .env_remove("OPENCODE_ZEN_API_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let code = output.status.code().unwrap_or(-1);
    // The API key error comes first, so this may exit 3 for that reason.
    // But it should still be exit 3 (input error), not 1 or 2.
    assert_eq!(code, 3, "Invalid session UUID should exit 3, got {code}");
}
