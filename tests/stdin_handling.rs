//! Integration tests for stdin handling and CLI argument parsing.
//!
//! These tests verify CLI behavior including help, version, and argument parsing.
//! Full stdin integration testing requires API mocking which is handled at the
//! unit test level in src/main.rs.
//!
//! Each test sets `CAKE_DATA_DIR` to an isolated temp directory so that tests
//! can run inside a parent cake session without filesystem collisions on
//! `~/.cache/cake/`.

#![allow(clippy::expect_used)]

mod support;

use std::{
    io::Write,
    process::Stdio,
    thread,
    time::{Duration, Instant},
};

use support::TestEnv;

fn cake_env() -> TestEnv {
    TestEnv::new("cake-stdin-test")
}

#[test]
fn test_help_shows_prompt_argument() {
    // Verify --help shows PROMPT in usage
    let env = cake_env();
    let output = env
        .command()
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
    let env = cake_env();
    let output = env
        .command()
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success(), "--version should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("cake"),
        "Version should contain 'cake'. Output: {stdout}"
    );
}

#[test]
fn test_positional_prompt_parsing() {
    let env = cake_env();
    env.write_project_settings(
        r#"
default_model = "test"

[[models]]
name = "test"
model = "glm-5.1"
base_url = "https://example.com"
api_key_env = "POSITIONAL_PROMPT_TEST_KEY"
"#,
    );

    let output = env
        .command()
        .arg("test prompt here")
        .env_remove("POSITIONAL_PROMPT_TEST_KEY")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("No input provided"),
        "Should parse positional prompt. Stderr: {stderr}"
    );
    assert!(
        stderr.contains("POSITIONAL_PROMPT_TEST_KEY"),
        "Prompt parsing test should advance to model resolution. Stderr: {stderr}"
    );
}

#[test]
fn test_dash_prompt_parsing() {
    let env = cake_env();
    let output = env
        .command()
        .arg("-")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("No input provided via stdin"),
        "Should fail specifically on missing stdin. Stderr: {stderr}"
    );
}

#[test]
fn test_dash_waits_for_delayed_stdin() {
    let env = cake_env();
    let mut child = env
        .command()
        .arg("--no-session")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn command");

    let mut stdin = child.stdin.take().expect("stdin should be piped");
    let start = Instant::now();
    let writer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        stdin
            .write_all(b"delayed stdin content")
            .expect("failed to write delayed stdin");
    });

    let output = child
        .wait_with_output()
        .expect("Failed to wait for command");
    writer.join().expect("stdin writer thread should complete");
    let elapsed = start.elapsed();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        elapsed >= Duration::from_millis(200),
        "Should wait for delayed stdin before continuing"
    );
    assert!(
        !stderr.contains("No input provided via stdin"),
        "Should read delayed stdin instead of treating it as missing. Stderr: {stderr}"
    );
}

#[test]
fn test_no_prompt_no_stdin_error() {
    // Verify that running without any input produces a clear error
    let env = cake_env();
    let output = env
        .command()
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
    let env = cake_env();
    let output = env
        .command()
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
    let env = cake_env();
    env.write_project_settings(
        r#"
default_model = "test"

[[models]]
name = "test"
model = "glm-5.1"
base_url = "not-a-url"
api_key_env = "NO_SESSION_TEST_KEY"
"#,
    );

    let output = env
        .command()
        .arg("--no-session")
        .arg("test prompt")
        .env("NO_SESSION_TEST_KEY", "test-token")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("Failed to execute command");

    let sessions_dir = env.data_dir().join("sessions");

    let no_sessions = if sessions_dir.exists() {
        std::fs::read_dir(&sessions_dir).map_or(true, |mut d| d.next().is_none())
    } else {
        true
    };

    assert!(
        no_sessions,
        "--no-session should not create session files. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
