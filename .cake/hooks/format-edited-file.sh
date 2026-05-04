#!/bin/sh

if ! command -v python3 >/dev/null 2>&1; then
  exit 0
fi

python3 -c '
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

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

raw_path = tool_input.get("path") or tool_input.get("file_path")
if not raw_path:
    sys.exit(0)

cwd = Path(payload.get("cwd") or os.getcwd()).resolve()
path = Path(str(raw_path)).expanduser()
if not path.is_absolute():
    path = cwd / path

try:
    path = path.resolve()
    path.relative_to(cwd)
except Exception:
    sys.exit(0)

if not path.is_file():
    sys.exit(0)

suffix = path.suffix.lower()
formatter = None
command = None

if suffix == ".rs" and shutil.which("rustfmt"):
    formatter = "rustfmt"
    command = ["rustfmt", str(path)]
elif suffix == ".toml" and shutil.which("taplo"):
    formatter = "taplo"
    command = ["taplo", "format", str(path)]
elif suffix in {".json", ".jsonc", ".md", ".markdown", ".yaml", ".yml"} and shutil.which("prettier"):
    formatter = "prettier"
    command = ["prettier", "--write", str(path)]

if command is None:
    sys.exit(0)

try:
    before = path.read_bytes()
except Exception:
    before = None

try:
    completed = subprocess.run(
        command,
        cwd=str(cwd),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
        timeout=10,
        check=False,
    )
except Exception:
    sys.exit(0)

if completed.returncode != 0:
    message = completed.stderr.strip().splitlines()
    detail = message[0] if message else "formatter failed"
    print(json.dumps({"additional_context": f"Formatter hook could not format {path.relative_to(cwd)} with {formatter}: {detail}"}))
    sys.exit(0)

try:
    after = path.read_bytes()
except Exception:
    after = before

if before is not None and after != before:
    print(json.dumps({"additional_context": f"Formatted edited file with {formatter}: {path.relative_to(cwd)}"}))
'
