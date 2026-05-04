use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::clients::SessionRecord;
use crate::config::Session;
use crate::config::hooks::{HookCommand, HookEvent, LoadedHooks};

const HOOK_OUTPUT_LIMIT: usize = 64 * 1024;

#[derive(Clone)]
pub struct HookRunner {
    loaded: LoadedHooks,
    context: HookContext,
}

#[derive(Clone)]
pub struct HookContext {
    pub session_id: uuid::Uuid,
    pub task_id: uuid::Uuid,
    pub transcript_path: Option<PathBuf>,
    pub cwd: PathBuf,
    pub model: String,
}

#[derive(Debug, Clone)]
pub enum ToolHookPlan {
    Execute {
        arguments: String,
        prefix_notice: Option<String>,
        additional_context: Vec<String>,
    },
    Block {
        reason: String,
        additional_context: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookPermission {
    Allow,
    Deny,
}

#[derive(Debug, Default)]
struct AggregatedHookResult {
    deny_reasons: Vec<String>,
    updated_inputs: Vec<(Value, PathBuf)>,
    additional_context: Vec<String>,
}

#[derive(Debug)]
struct InvocationOutcome {
    command: HookCommand,
    exit_code: Option<i32>,
    duration: Duration,
    stdout: String,
    stderr: String,
    decision: String,
    parsed: Option<HookOutput>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HookOutput {
    #[serde(default)]
    r#continue: Option<bool>,
    stop_reason: Option<String>,
    decision: Option<String>,
    permission: Option<String>,
    reason: Option<String>,
    updated_input: Option<Value>,
    additional_context: Option<String>,
    #[serde(rename = "suppress_output")]
    _suppress_output: Option<bool>,
}

#[derive(Debug, Serialize)]
struct HookRecord<'a> {
    version: u8,
    session_id: String,
    task_id: String,
    transcript_path: Option<&'a PathBuf>,
    cwd: &'a PathBuf,
    hook_event_name: &'static str,
    model: &'a str,
    timestamp: String,
}

impl HookRunner {
    pub const fn new(loaded: LoadedHooks, context: HookContext) -> Self {
        Self { loaded, context }
    }

    pub async fn session_start(
        &self,
        source: &str,
        initial_prompt: &str,
    ) -> anyhow::Result<Vec<String>> {
        let payload = self.payload(
            HookEvent::SessionStart,
            Some(source),
            json!({
                "source": source,
                "initial_prompt": initial_prompt,
            }),
        );
        let result = self
            .run_and_aggregate(HookEvent::SessionStart, Some(source), payload)
            .await?;
        Ok(result.additional_context)
    }

    pub async fn user_prompt_submit(&self, prompt: &str) -> anyhow::Result<Vec<String>> {
        let payload = self.payload(
            HookEvent::UserPromptSubmit,
            None,
            json!({
                "prompt": prompt,
            }),
        );
        let result = self
            .run_and_aggregate(HookEvent::UserPromptSubmit, None, payload)
            .await?;
        Ok(result.additional_context)
    }

    pub async fn pre_tool_use(
        &self,
        tool_name: &str,
        tool_use_id: &str,
        arguments: &str,
    ) -> anyhow::Result<ToolHookPlan> {
        let tool_input = parse_tool_input(arguments);
        let payload = self.payload(
            HookEvent::PreToolUse,
            Some(tool_name),
            json!({
                "tool_name": tool_name,
                "tool_use_id": tool_use_id,
                "tool_input": tool_input,
                "tool_input_json": arguments,
            }),
        );
        let result = self
            .run_and_aggregate(HookEvent::PreToolUse, Some(tool_name), payload)
            .await?;

        if !result.deny_reasons.is_empty() {
            return Ok(ToolHookPlan::Block {
                reason: result.deny_reasons.join("; "),
                additional_context: result.additional_context,
            });
        }

        if let Some((updated, source_file)) = result.updated_inputs.first() {
            if result.updated_inputs.len() > 1 {
                tracing::warn!(
                    target: "cake::hooks",
                    event = "PreToolUse",
                    source = tool_name,
                    first_source_file = %source_file.display(),
                    "Multiple hooks returned updated_input; using first in load order"
                );
            }
            if !updated.is_object() {
                return Ok(ToolHookPlan::Block {
                    reason: format!(
                        "Hook updated_input from {} must be a JSON object",
                        source_file.display()
                    ),
                    additional_context: result.additional_context,
                });
            }
            let new_arguments = serde_json::to_string(updated)
                .context("failed to serialize hook updated_input as JSON")?;
            let notice = format!(
                "Hook updated tool input.\nOriginal arguments: {arguments}\nNew arguments: {new_arguments}\n---\n"
            );
            return Ok(ToolHookPlan::Execute {
                arguments: new_arguments,
                prefix_notice: Some(notice),
                additional_context: result.additional_context,
            });
        }

        Ok(ToolHookPlan::Execute {
            arguments: arguments.to_string(),
            prefix_notice: None,
            additional_context: result.additional_context,
        })
    }

    pub async fn post_tool_use(
        &self,
        tool_name: &str,
        tool_use_id: &str,
        arguments: &str,
        result: &Result<String, String>,
    ) -> anyhow::Result<Option<String>> {
        let (event, result_type, text_result_for_llm) = match result {
            Ok(output) => (HookEvent::PostToolUse, "success", output.as_str()),
            Err(error) => (HookEvent::PostToolUseFailure, "failure", error.as_str()),
        };
        let payload = self.payload(
            event,
            Some(tool_name),
            json!({
                "tool_name": tool_name,
                "tool_use_id": tool_use_id,
                "tool_input": parse_tool_input(arguments),
                "tool_input_json": arguments,
                "tool_result": {
                    "result_type": result_type,
                    "text_result_for_llm": text_result_for_llm,
                }
            }),
        );
        let result = self
            .run_and_aggregate(event, Some(tool_name), payload)
            .await?;
        Ok(join_context(&result.additional_context))
    }

    pub async fn stop(&self, result: Option<&str>) -> anyhow::Result<Option<String>> {
        let payload = self.payload(HookEvent::Stop, None, json!({ "result": result }));
        let result = self
            .run_and_aggregate(HookEvent::Stop, None, payload)
            .await?;
        Ok(join_context(&result.additional_context))
    }

    pub async fn error_occurred(&self, error: &anyhow::Error) -> anyhow::Result<()> {
        let payload = self.payload(
            HookEvent::ErrorOccurred,
            None,
            json!({
                "error": {
                    "message": error.to_string(),
                    "name": "Error",
                }
            }),
        );
        self.run_and_aggregate(HookEvent::ErrorOccurred, None, payload)
            .await?;
        Ok(())
    }

    async fn run_and_aggregate(
        &self,
        event: HookEvent,
        source: Option<&str>,
        payload: Value,
    ) -> anyhow::Result<AggregatedHookResult> {
        let matched = self.loaded.matching_groups(event, source);
        if matched.is_empty() {
            return Ok(AggregatedHookResult::default());
        }

        let mut commands = Vec::new();
        for group in matched {
            for hook in &group.hooks {
                commands.push(hook.clone());
            }
        }

        let futures = commands.into_iter().map(|command| {
            let payload = payload.clone();
            let cwd = self.context.cwd.clone();
            async move { run_command_hook(command, payload, cwd).await }
        });
        let outcomes = futures::future::join_all(futures).await;

        let mut aggregated = AggregatedHookResult::default();
        for outcome in outcomes {
            self.record_outcome(event, source, &outcome);

            if let Some(error) = &outcome.error {
                if outcome.command.fail_closed {
                    if event == HookEvent::PreToolUse {
                        aggregated.deny_reasons.push(format!(
                            "{}: {error}",
                            outcome.command.source_path.display()
                        ));
                        continue;
                    }
                    anyhow::bail!(
                        "Hook failed closed for {event} in {}: {error}",
                        outcome.command.source_path.display()
                    );
                }
                tracing::warn!(
                    target: "cake::hooks",
                    event = event.as_str(),
                    source = source,
                    command = %outcome.command.command,
                    source_file = %outcome.command.source_path.display(),
                    error = %error,
                    "Hook failed open"
                );
                continue;
            }

            let Some(parsed) = &outcome.parsed else {
                continue;
            };
            if parsed.r#continue == Some(false) {
                let reason = parsed
                    .stop_reason
                    .clone()
                    .or_else(|| parsed.reason.clone())
                    .unwrap_or_else(|| "hook requested stop".to_string());
                if event == HookEvent::PreToolUse {
                    aggregated.deny_reasons.push(format!(
                        "{}: {reason}",
                        outcome.command.source_path.display()
                    ));
                } else {
                    anyhow::bail!("Hook requested stop for {event}: {reason}");
                }
            }

            let permission = parsed.permission.as_ref().or(parsed.decision.as_ref());
            if let Some(permission) = permission {
                let parsed_permission = parse_permission(permission);
                if parsed_permission.permission == HookPermission::Deny {
                    let reason = parsed.reason.clone().unwrap_or_else(|| {
                        if permission == "ask" {
                            "interactive ask is not supported yet".to_string()
                        } else {
                            "hook denied action".to_string()
                        }
                    });
                    if event == HookEvent::PreToolUse {
                        aggregated.deny_reasons.push(format!(
                            "{}: {reason}",
                            outcome.command.source_path.display()
                        ));
                    } else {
                        anyhow::bail!("Hook denied {event}: {reason}");
                    }
                }
            }

            if let Some(context) = parsed.additional_context.as_ref()
                && !context.is_empty()
            {
                aggregated.additional_context.push(context.clone());
            }

            if event == HookEvent::PreToolUse
                && let Some(updated_input) = parsed.updated_input.clone()
            {
                aggregated
                    .updated_inputs
                    .push((updated_input, outcome.command.source_path.clone()));
            }
        }

        Ok(aggregated)
    }

    fn payload(&self, event: HookEvent, _source: Option<&str>, extra: Value) -> Value {
        let common = HookRecord {
            version: 1,
            session_id: self.context.session_id.to_string(),
            task_id: self.context.task_id.to_string(),
            transcript_path: self.context.transcript_path.as_ref(),
            cwd: &self.context.cwd,
            hook_event_name: event.as_str(),
            model: &self.context.model,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let mut value = match serde_json::to_value(common) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(target: "cake::hooks", error = %error, "Failed to serialize common hook payload");
                json!({})
            },
        };
        if let (Value::Object(base), Value::Object(extra)) = (&mut value, extra) {
            base.extend(extra);
        }
        value
    }

    fn record_outcome(&self, event: HookEvent, source: Option<&str>, outcome: &InvocationOutcome) {
        let stderr_bytes = outcome.stderr.len();
        let stdout_bytes = outcome.stdout.len();
        let level_error = outcome.command.fail_closed && outcome.error.is_some();
        if level_error {
            tracing::error!(
                target: "cake::hooks",
                event = event.as_str(),
                source = source,
                command = %outcome.command.command,
                source_file = %outcome.command.source_path.display(),
                exit_code = ?outcome.exit_code,
                duration_ms = duration_ms(outcome.duration),
                stderr_bytes,
                stdout_bytes,
                decision = outcome.decision,
                fail_closed = outcome.command.fail_closed,
                "Hook invocation failed closed"
            );
        } else if outcome.error.is_some() {
            tracing::warn!(
                target: "cake::hooks",
                event = event.as_str(),
                source = source,
                command = %outcome.command.command,
                source_file = %outcome.command.source_path.display(),
                exit_code = ?outcome.exit_code,
                duration_ms = duration_ms(outcome.duration),
                stderr_bytes,
                stdout_bytes,
                decision = outcome.decision,
                fail_closed = outcome.command.fail_closed,
                "Hook invocation completed with non-blocking error"
            );
        } else {
            tracing::info!(
                target: "cake::hooks",
                event = event.as_str(),
                source = source,
                command = %outcome.command.command,
                source_file = %outcome.command.source_path.display(),
                exit_code = ?outcome.exit_code,
                duration_ms = duration_ms(outcome.duration),
                stderr_bytes,
                stdout_bytes,
                decision = outcome.decision,
                fail_closed = outcome.command.fail_closed,
                "Hook invocation completed"
            );
        }

        let Some(path) = &self.context.transcript_path else {
            return;
        };
        let record = SessionRecord::HookEvent {
            timestamp: chrono::Utc::now(),
            task_id: self.context.task_id.to_string(),
            event: event.as_str().to_string(),
            source: source.map(ToOwned::to_owned),
            source_file: outcome.command.source_path.clone(),
            command: outcome.command.command.clone(),
            exit_code: outcome.exit_code,
            duration_ms: outcome.duration.as_millis().try_into().unwrap_or(u64::MAX),
            decision: outcome.decision.clone(),
            fail_closed: outcome.command.fail_closed,
            stdout: outcome.stdout.clone(),
            stderr: outcome.stderr.clone(),
        };

        match Session::open_for_append(path)
            .and_then(|mut file| Session::append_record(&mut file, &record))
        {
            Ok(()) => {},
            Err(error) => tracing::warn!(
                target: "cake::hooks",
                path = %path.display(),
                error = %error,
                "Failed to append hook transcript record"
            ),
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn run_command_hook(command: HookCommand, payload: Value, cwd: PathBuf) -> InvocationOutcome {
    let start = Instant::now();
    if let Some(status) = &command.status_message {
        tracing::info!(
            target: "cake::hooks",
            command = %command.command,
            source_file = %command.source_path.display(),
            status_message = %status,
            "Starting hook"
        );
    }

    let mut process = shell_command(&command.command);
    process
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            return InvocationOutcome {
                command,
                exit_code: None,
                duration: start.elapsed(),
                stdout: String::new(),
                stderr: String::new(),
                decision: "error".to_string(),
                parsed: None,
                error: Some(format!("failed to spawn hook command: {error}")),
            };
        },
    };

    if let Some(mut stdin) = child.stdin.take() {
        let input = serde_json::to_vec(&payload).unwrap_or_default();
        if let Err(error) = stdin.write_all(&input).await {
            return InvocationOutcome {
                command,
                exit_code: None,
                duration: start.elapsed(),
                stdout: String::new(),
                stderr: String::new(),
                decision: "error".to_string(),
                parsed: None,
                error: Some(format!("failed to write hook stdin: {error}")),
            };
        }
    }

    let timeout_result = timeout(command.timeout, child.wait_with_output()).await;
    let output = match timeout_result {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return InvocationOutcome {
                command,
                exit_code: None,
                duration: start.elapsed(),
                stdout: String::new(),
                stderr: String::new(),
                decision: "error".to_string(),
                parsed: None,
                error: Some(format!("failed to wait for hook command: {error}")),
            };
        },
        Err(_) => {
            let timeout_secs = command.timeout.as_secs();
            return InvocationOutcome {
                command,
                exit_code: None,
                duration: start.elapsed(),
                stdout: String::new(),
                stderr: String::new(),
                decision: "timeout".to_string(),
                parsed: None,
                error: Some(format!("hook command timed out after {timeout_secs}s")),
            };
        },
    };

    let stdout = capped_text(&output.stdout);
    let stderr = capped_text(&output.stderr);
    let exit_code = output.status.code();

    if exit_code == Some(2) {
        let reason = if stderr.trim().is_empty() {
            parse_hook_output(&stdout)
                .ok()
                .flatten()
                .and_then(|parsed| parsed.reason)
                .unwrap_or_else(|| "hook blocked action".to_string())
        } else {
            stderr.trim().to_string()
        };
        return InvocationOutcome {
            command,
            exit_code,
            duration: start.elapsed(),
            stdout,
            stderr,
            decision: "deny".to_string(),
            parsed: Some(HookOutput {
                r#continue: Some(false),
                stop_reason: Some(reason),
                decision: Some("deny".to_string()),
                permission: None,
                reason: None,
                updated_input: None,
                additional_context: None,
                _suppress_output: None,
            }),
            error: None,
        };
    }

    if exit_code != Some(0) {
        return InvocationOutcome {
            command,
            exit_code,
            duration: start.elapsed(),
            stdout,
            stderr: stderr.clone(),
            decision: "error".to_string(),
            parsed: None,
            error: Some(format!(
                "hook exited with code {}{}",
                exit_code.map_or_else(|| "unknown".to_string(), |code| code.to_string()),
                if stderr.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", stderr.trim())
                }
            )),
        };
    }

    match parse_hook_output(&stdout) {
        Ok(parsed) => {
            let decision = parsed
                .as_ref()
                .and_then(|parsed| {
                    parsed
                        .permission
                        .as_ref()
                        .or(parsed.decision.as_ref())
                        .map(|value| parse_permission(value).decision_label)
                })
                .unwrap_or("none")
                .to_string();
            InvocationOutcome {
                command,
                exit_code,
                duration: start.elapsed(),
                stdout,
                stderr,
                decision,
                parsed,
                error: None,
            }
        },
        Err(error) => InvocationOutcome {
            command,
            exit_code,
            duration: start.elapsed(),
            stdout,
            stderr,
            decision: "error".to_string(),
            parsed: None,
            error: Some(error),
        },
    }
}

fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    }

    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}

fn parse_hook_output(stdout: &str) -> Result<Option<HookOutput>, String> {
    if stdout.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(stdout)
        .map(Some)
        .map_err(|error| format!("hook stdout was not valid JSON: {error}"))
}

struct ParsedPermission {
    permission: HookPermission,
    decision_label: &'static str,
}

fn parse_permission(value: &str) -> ParsedPermission {
    match value {
        "deny" | "block" | "ask" => ParsedPermission {
            permission: HookPermission::Deny,
            decision_label: "deny",
        },
        "allow" => ParsedPermission {
            permission: HookPermission::Allow,
            decision_label: "allow",
        },
        _ => ParsedPermission {
            permission: HookPermission::Allow,
            decision_label: "none",
        },
    }
}

fn parse_tool_input(arguments: &str) -> Value {
    serde_json::from_str(arguments).unwrap_or_else(|_| json!({}))
}

fn join_context(context: &[String]) -> Option<String> {
    if context.is_empty() {
        None
    } else {
        Some(context.join("\n\n"))
    }
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

fn capped_text(bytes: &[u8]) -> String {
    if bytes.len() <= HOOK_OUTPUT_LIMIT {
        return String::from_utf8_lossy(bytes).to_string();
    }
    let omitted = bytes.len() - HOOK_OUTPUT_LIMIT;
    format!(
        "{}... (truncated, {omitted} more bytes)",
        String::from_utf8_lossy(&bytes[..HOOK_OUTPUT_LIMIT])
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::config::hooks::{HookGroup, HookMatcher};

    fn runner(command: &str, fail_closed: bool) -> HookRunner {
        let cwd = std::env::temp_dir();
        let source_path = cwd.join("hooks.json");
        let command = HookCommand {
            command: command.to_string(),
            timeout: Duration::from_secs(2),
            fail_closed,
            status_message: None,
            source_path: source_path.clone(),
        };
        let loaded = LoadedHooks {
            groups: vec![HookGroup {
                source_path,
                event: HookEvent::PreToolUse,
                matcher: HookMatcher::All,
                hooks: vec![command],
            }],
        };
        HookRunner::new(
            loaded,
            HookContext {
                session_id: uuid::Uuid::new_v4(),
                task_id: uuid::Uuid::new_v4(),
                transcript_path: None,
                cwd,
                model: "test-model".to_string(),
            },
        )
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn command_hook_receives_stdin_json() {
        let runner = runner(
            "payload=$(cat); case \"$payload\" in *'\"tool_name\":\"Bash\"'*) printf '{\"permission\":\"allow\"}' ;; *) exit 1 ;; esac",
            false,
        );

        let plan = runner
            .pre_tool_use("Bash", "call-1", r#"{"command":"printf ok"}"#)
            .await
            .unwrap();

        assert!(matches!(plan, ToolHookPlan::Execute { .. }));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn exit_two_blocks_pre_tool_use() {
        let runner = runner("echo blocked >&2; exit 2", false);

        let plan = runner
            .pre_tool_use("Bash", "call-1", r#"{"command":"printf ok"}"#)
            .await
            .unwrap();

        match plan {
            ToolHookPlan::Block { reason, .. } => assert!(reason.contains("blocked")),
            ToolHookPlan::Execute { .. } => panic!("expected block"),
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn invalid_json_fails_open_by_default() {
        let runner = runner("printf not-json", false);

        let plan = runner
            .pre_tool_use("Bash", "call-1", r#"{"command":"printf ok"}"#)
            .await
            .unwrap();

        assert!(matches!(plan, ToolHookPlan::Execute { .. }));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn fail_closed_invalid_json_blocks() {
        let runner = runner("printf not-json", true);

        let plan = runner
            .pre_tool_use("Bash", "call-1", r#"{"command":"printf ok"}"#)
            .await
            .unwrap();

        assert!(matches!(plan, ToolHookPlan::Block { .. }));
    }
}
