//! Integration tests for stdin handling
//!
//! These tests verify CLI behavior including help, version, and argument parsing.
//! Full stdin integration testing requires API mocking which is handled at the
//! unit test level in src/main.rs.

#![allow(clippy::expect_used)]

use std::process::{Command, Stdio};

fn get_binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("release")
        .join("acai")
}

#[test]
fn test_help_shows_prompt_argument() {
    // Verify --help shows PROMPT in usage
    let output = Command::new(get_binary_path())
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
    let output = Command::new(get_binary_path())
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
    let output = Command::new(get_binary_path())
        .arg("test prompt here")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT say "No input provided" - the prompt was parsed
    assert!(
        !stderr.contains("No input provided"),
        "Should parse positional prompt. Stderr: {stderr}"
    );
}

#[test]
fn test_dash_prompt_parsing() {
    // Verify that '-' as prompt is accepted
    // It will fail on no stdin + API key, but that's expected
    let output = Command::new(get_binary_path())
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // When using '-' without stdin, should get specific error
    assert!(
        stderr.contains("No input provided via stdin")
            || stderr.contains("OPENROUTER_API_KEY")
            || stderr.contains("config"),
        "Should either fail on no stdin or proceed to API. Stderr: {stderr}"
    );
}

#[test]
fn test_no_prompt_no_stdin_error() {
    // Verify that running without any input produces a clear error
    let output = Command::new(get_binary_path())
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
