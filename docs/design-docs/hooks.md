# Command Hooks

cake can run user-configured local commands at session and tool lifecycle events. Hooks are intended for project policy, audit logging, and small automations such as adding context before a task starts or blocking unsafe tool calls.

## Configuration

Hooks are loaded from JSON files in this order:

1. `~/.config/cake/hooks.json`
2. `.cake/hooks.json`
3. `.cake/hooks.local.json`

Missing files are ignored. Malformed files stop cake before the model request and name the offending path. Hooks from later files are appended; they do not replace earlier hooks.

Every file must use `version: 1`:

    {
      "version": 1,
      "hooks": {
        "PreToolUse": [
          {
            "matcher": "Bash|Write",
            "hooks": [
              {
                "type": "command",
                "command": "./.cake/hooks/check-tool.sh",
                "timeout": 5,
                "fail_closed": true
              }
            ]
          }
        ]
      }
    }

Supported events are `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `Stop`, and `ErrorOccurred`.

`matcher` is supported only for `SessionStart`, `PreToolUse`, `PostToolUse`, and `PostToolUseFailure`. Missing `matcher` or `"*"` matches every source. Otherwise, split values with `|`; matches are exact. Tool events use tool names such as `Bash`, `Read`, `Edit`, and `Write`. `SessionStart` uses `startup`, `resume`, or `fork`.

Each command hook runs through the platform shell (`sh -c` on Unix, `cmd /C` on Windows), with the project root as its working directory. Hook scripts must already be executable, for example with `chmod +x ./.cake/hooks/check-tool.sh`.

## Runtime Protocol

cake sends one JSON object to the hook command on stdin and waits for the process to exit. Common fields include:

    {
      "version": 1,
      "session_id": "...",
      "task_id": "...",
      "transcript_path": "/path/to/session.jsonl",
      "cwd": "/path/to/project",
      "hook_event_name": "PreToolUse",
      "model": "glm-5.1",
      "timestamp": "2026-05-04T00:00:00Z"
    }

Tool hooks also receive `tool_name`, `tool_use_id`, `tool_input`, and `tool_input_json`. Post-tool hooks receive `tool_result.result_type` as `success` or `failure`, plus `tool_result.text_result_for_llm`.

Exit code `0` means success. If stdout is non-empty, cake parses it as JSON. Exit code `2` blocks the action. Other exit codes are logged and ignored unless `fail_closed` is true.

Supported stdout fields include:

    {
      "permission": "allow",
      "reason": "optional explanation",
      "updated_input": { "command": "printf safe" },
      "additional_context": "context for the model"
    }

`permission` and `decision` both accept `allow`, `deny`, `block`, and `ask`. `deny`, `block`, and `ask` block in the first implementation. `continue: false` also stops the current action.

`PreToolUse` can return `updated_input`; it must be a JSON object and is passed through the target tool's normal validation. `SessionStart` and `UserPromptSubmit` can return `additional_context`, which cake adds as developer context before the next model request. `PostToolUse` and `PostToolUseFailure` append additional context to the tool result under `Additional hook context:`.

## Observability

Hook activity is logged with the `cake::hooks` tracing target. When sessions are enabled, each hook invocation also appends a `hook_event` record to the session JSONL transcript with event name, source, command, exit code, duration, decision, stdout, and stderr. Stdout and stderr stored by hook records are capped at 64 KiB each.

Hooks are trusted local configuration and run outside the model tool sandbox. Do not install project hooks you do not trust.
