#!/bin/sh

if ! command -v python3 >/dev/null 2>&1; then
  exit 0
fi

python3 -c '
import json
import re
import sys

try:
    payload = json.load(sys.stdin)
except Exception:
    sys.exit(0)

tool_input = payload.get("tool_input") or {}
if not isinstance(tool_input, dict):
    tool_input = {}
if not tool_input:
    raw_tool_input = payload.get("tool_input_json")
    if isinstance(raw_tool_input, str):
        try:
            tool_input = json.loads(raw_tool_input)
        except Exception:
            tool_input = {}
if not isinstance(tool_input, dict):
    tool_input = {}

command = str(tool_input.get("cmd") or tool_input.get("command") or "")
tool_result = payload.get("tool_result") or {}
output = str(tool_result.get("text_result_for_llm") or "")

rust_workflow = re.search(
    r"(^|[;&|\s])(cargo|just)\s+(test|clippy|fmt|build|ci-full|ci|rust-version-check|lint-imports|doc|deny)\b",
    command,
)
rust_error = re.search(
    r"(error\[E[0-9]{4}\]|panicked at|test result: FAILED|clippy|rustfmt|could not compile|failed to run custom build command)",
    output,
    re.IGNORECASE,
)
nonzero_exit = re.search(r"\[exit:(?!0\b)[0-9]+", output)

if not ((rust_workflow and nonzero_exit) or rust_error):
    sys.exit(0)

context = (
    "A Rust workflow command failed in the cake repo. Prefer fixing the first compiler, "
    "clippy, formatting, or test failure in the command output before retrying broader checks. "
    "Before finishing code changes here, run `just ci`; it covers rust-version-check, fmt-check, "
    "clippy-strict, tests, and import lint. If the failure is platform-specific, inspect sandbox "
    "cfg paths and scripts/check-rust-toolchain.sh."
)

print(json.dumps({"additional_context": context}))
'
