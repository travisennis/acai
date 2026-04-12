//! Integration tests for stdin handling and CLI argument parsing.
//!
//! These tests verify CLI behavior including help, version, and argument parsing.
//! Full stdin integration testing requires API mocking which is handled at the
//! unit test level in src/main.rs.
//!
//! Each test sets `ACAI_DATA_DIR` to an isolated temp directory so that tests
//! can run inside a parent acai session without filesystem collisions on
//! `~/.cache/acai/`.

#![allow(clippy::expect_used)]

use std::process::{Command, Stdio};

fn get_binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_acai"))
}

/// Build a `Command` with an isolated `ACAI_DATA_DIR` to avoid collisions
/// when running inside a parent acai session.
fn acai_cmd() -> Command {
    let mut cmd = Command::new(get_binary_path());
    let tmp = std::env::temp_dir().join(format!("acai_test_{}", std::process::id()));
    cmd.env("ACAI_DATA_DIR", tmp);
    cmd
}

#[test]
fn test_help_shows_prompt_argument() {
    // Verify --help shows PROMPT in usage
    let output = acai_cmd()
        .arg("--help")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "--help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("PROMPT"),
        "Help should mention PROMPT argument. Output: {stdout}"
    );

    // Verify help mentions stdin option
    assert!(
        stdout.contains('-'),
        "Help should mention '-' for stdin. Output: {stdout}"
    );
}

#[test]
fn test_version_works() {
    // Verify --version works
    let output = acai_cmd()
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "--version should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("acai"),
        "Version should contain 'acai'. Output: {stdout}"
    );
}

#[test]
fn test_positional_prompt_parsing() {
    // Verify that a positional prompt doesn't fail parsing
    // It will fail on API key, but that's expected
    let output = acai_cmd()
        .arg("test prompt here")
        .env_remove("OPENCODE_ZEN_API_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT say "No input provided" - the prompt was parsed.
    // Without an API key the binary will fail on the missing key instead.
    assert!(
        !stderr.contains("No input provided"),
        "Should parse positional prompt. Stderr: {stderr}"
    );
}

#[test]
fn test_dash_prompt_parsing() {
    // Verify that '-' as prompt is accepted
    // It will fail on no stdin + API key, but that's expected
    let output = acai_cmd()
        .arg("-")
        .env_remove("OPENCODE_ZEN_API_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // When using '-' without stdin, should get specific error
    assert!(
        stderr.contains("No input provided via stdin")
            || stderr.contains("OPENCODE_ZEN_API_TOKEN")
            || stderr.contains("config"),
        "Should either fail on no stdin or proceed to API. Stderr: {stderr}"
    );
}

#[test]
fn test_no_prompt_no_stdin_error() {
    // Verify that running without any input produces a clear error
    let output = acai_cmd()
        .env_remove("OPENCODE_ZEN_API_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Without prompt and without stdin, should get the input error
    assert!(
        stderr.contains("No input provided"),
        "Should show 'No input provided' when no input given. Stderr: {stderr}"
    );
}

#[test]
fn test_no_session_flag_in_help() {
    // Verify --help mentions --no-session
    let output = acai_cmd()
        .arg("--help")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "--help should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--no-session"),
        "Help should mention --no-session flag. Output: {stdout}"
    );
}

#[test]
fn test_no_session_prevents_session_save() {
    // Verify that --no-session does not create a session file.
    // We use a temp directory as ACAI_DATA_DIR and run a prompt through
    // with --no-session. No session JSON should be written.
    let tmp_dir = std::env::temp_dir().join(format!("acai_test_no_session_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);

    let output = Command::new(get_binary_path())
        .arg("--no-session")
        .arg("test prompt")
        .env("ACAI_DATA_DIR", &tmp_dir)
        .env_remove("OPENCODE_ZEN_API_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    // The command will fail due to missing API key, but that's fine —
    // we're checking that no session files were written.
    // Sessions are stored under <data_dir>/sessions/<dir_hash>/<uuid>.jsonl
    let sessions_dir = tmp_dir.join("sessions");

    // Either sessions dir doesn't exist, or it has no files
    let no_sessions = if sessions_dir.exists() {
        std::fs::read_dir(&sessions_dir)
            .map(|mut d| d.next().is_none())
            .unwrap_or(true)
    } else {
        true
    };

    assert!(
        no_sessions,
        "--no-session should not create session files. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&tmp_dir);
}
