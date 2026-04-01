# Tools Framework

The `clients::tools` module provides the tool execution framework that enables AI agents to interact with the filesystem and execute commands safely.

## Overview

Acai provides four built-in tools:

1. **Bash**: Execute shell commands with sandboxing
2. **Read**: Read file contents or list directories
3. **Edit**: Make targeted text replacements in files
4. **Write**: Create new files or overwrite existing ones

Each tool defines:
- A JSON schema for the API (name, description, parameters)
- Validation logic for arguments
- Execution logic with proper error handling

## Tool Definition

Tools are defined using the `Tool` struct:

```rust
pub struct Tool {
    pub(super) type_: String,        // Always "function"
    pub(super) name: String,         // Tool name (Bash, Read, Edit, Write)
    pub(super) description: String,  // Human-readable description
    pub(super) parameters: serde_json::Value,  // JSON Schema for arguments
}
```

Example tool definition (Read):

```rust
pub(super) fn read_tool() -> Tool {
    Tool {
        type_: "function".to_string(),
        name: "Read".to_string(),
        description: "Read a file's contents or list a directory's entries...",
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "..." },
                "start_line": { "type": "integer", ... },
                "end_line": { "type": "integer", ... }
            },
            "required": ["path"]
        }),
    }
}
```

## Tool Execution

The `execute_tool` function dispatches to the appropriate implementation:

```rust
pub(super) async fn execute_tool(name: &str, arguments: &str) -> Result<ToolResult, String>
```

Execution flow:
1. Parse JSON arguments using serde
2. Validate inputs (paths, etc.)
3. Execute the operation
4. Return `ToolResult` with output or error

Results are returned as strings so they can be included in API responses.

## Path Validation

All filesystem tools validate paths before operating:

```rust
pub(super) fn validate_path_in_cwd(path_str: &str) -> Result<PathBuf, String>
```

Validation rules:
- Path must exist and be accessible
- Path must be within the current working directory, OR
- Path must be within allowed temp directories (`/tmp`, `/var/folders`, `TMPDIR`), OR
- Path must be within directories added via `--add-dir` CLI flag (read-only access)

This prevents the AI from accessing sensitive files outside the project.

### Write Tool Path Handling

The Write tool has special handling for new files that don't exist yet:

```rust
fn validate_path_for_write(path_str: &str) -> Result<PathBuf, String>
```

This function:
1. For existing files: uses standard validation
2. For new files: walks up the tree to find an existing parent directory
3. Validates that parent is within allowed directories
4. Reconstructs the full path with the canonicalized parent

This allows creating new files in new subdirectories while maintaining security.

## Individual Tools

### Bash Tool

**Purpose**: Execute shell commands in the host environment.

**Parameters**:
- `command`: The shell command to execute (required)
- `timeout`: Optional timeout in seconds (default: 60)

**Features**:
- OS-level sandboxing (Seatbelt on macOS, Landlock on Linux)
- Configurable via `ACAI_SANDBOX=0` environment variable
- Output streaming with 100KB read cap
- Automatic truncation for large outputs (saved to temp file)
- Metadata footer with exit code and execution time
- Binary output detection and handling

**Output Handling**:
- Small output (≤ 50KB): Returned inline with metadata footer
- Large output (> 50KB): Truncated with head/tail preview (see below)
- Binary output: Written to temp file with helpful message and MIME type detection
- Timeout: Command killed, timeout error returned
- Read cap: Up to 100KB (2× the inline limit) is read from the process; output beyond this is discarded and marked with `[... output truncated at 100000 bytes ...]`

**Truncated Output**:

When command output exceeds the 50KB inline limit, the full output is saved to a temporary file and a head+tail preview is returned. The preview shows the first ~12.5KB and last ~12.5KB of the output:

```
[Output too long — 75000 bytes, 1500 lines.]
Full output saved to: /tmp/acai/bash_output_<uuid>.txt
You can search it with `grep` or view portions with `head`/`tail`.
Consider reformulating the command to produce less output.

--- first ~12500 bytes ---
<first ~12.5KB of output>

--- last ~12500 bytes ---
<last ~12.5KB of output>
[exit:0 | 1.2s]
```

If the temp file cannot be written (e.g., disk full), a fallback inline truncation is used with the first ~25KB and last ~25KB:

```
[Output too long — 75000 bytes, 1500 lines. The command was too verbose; reformulate with less output (e.g. pipe through `head`, `tail`, or `grep`).]

--- first ~25000 bytes ---
<first ~25KB of output>

--- last ~25000 bytes ---
<last ~25KB of output>
[exit:0 | 1.2s]
```

**Metadata Footer**:

All Bash tool output includes a metadata footer showing exit code and execution time:

```
[exit:0 | 12ms]
```

The time display adapts to the duration:
- Durations under 1 second: Shows milliseconds (e.g., `12ms`, `500ms`, `999ms`)
- Durations of 1 second or more: Shows seconds with one decimal place (e.g., `1.0s`, `1.2s`, `15.3s`)

**Binary Output Detection**:

The Bash tool automatically detects binary output and prevents returning corrupted binary data to the LLM. Detection is based on:
- Null byte count: More than 8 null bytes indicates binary
- Non-printable character ratio: More than 30% non-printable characters (excluding common whitespace: `\t`, `\n`, `\r`) indicates binary

When binary output is detected:
1. The data is saved to a temp file in `/tmp/acai/bash_binary_<uuid>`
2. MIME type is detected via magic numbers for common formats:
   - Images: PNG, JPEG, GIF
   - Documents: PDF
   - Archives: ZIP, Gzip, BZip2, TAR
   - Executables: ELF, Mach-O
   - Databases: SQLite
3. A user-friendly message is returned with the file path and suggested tools for inspection (`file`, `hexdump`, `xxd`)

Example binary output message:
```
[Binary output detected - 12345 bytes (12.1 KB)]
Detected type: image/png
Binary data saved to: /tmp/acai/bash_binary_abc123
The command produced binary output which cannot be displayed as text.
You can inspect the file with appropriate tools (e.g., `file`, `hexdump`, `xxd`).
[exit:0 | 15ms]
```

**Destructive Command Blocking**:

The Bash tool includes a pre-execution safety check that blocks known-destructive commands before they reach the sandbox or process spawn. This complements the OS-level sandbox by catching destructive operations that are allowed within the sandbox's permitted zones — for example, destructive git operations inside the repo or remote-affecting operations like force-push.

Blocked git commands:

| Blocked Command | Reason | Allowed Alternative |
|---|---|---|
| `git reset --hard` / `--merge` | Discards uncommitted changes | `git stash` or `git reset --soft` |
| `git checkout -- <file>` | Discards working tree changes | `git restore --staged` |
| `git restore <file>` (without `--staged`), `git restore --worktree` | Discards working tree changes | `git restore --staged` |
| `git clean -f` / `--force` (including combined flags like `-fd`, `-fdx`) | Permanently deletes untracked files | `git clean -n` (dry run) |
| `git push --force` / `-f` | Rewrites remote history | `--force-with-lease` (allowed) |
| `git branch -D` (uppercase) | Force-deletes unmerged branch | `git branch -d` (lowercase, allowed) |
| `git stash drop` / `clear` | Permanently deletes stashed changes | `git stash pop` or `git stash list` |

Blocked filesystem commands:

| Blocked Command | Reason |
|---|---|
| `rm -rf` outside temp directories (`/tmp`, `/var/tmp`, `$TMPDIR`) | Irreversible recursive deletion |

Additional protections:
- **Wrapper detection**: `bash -c` / `sh -c` wrappers are detected and the inner script is recursively checked
- **Command chaining**: Commands joined via `&&`, `||`, `;`, or newlines are split and each segment is checked independently
- **False positive avoidance**: Commit messages, `echo`, `printf`, and similar data contexts are skipped to avoid flagging non-destructive uses

Error format:

When a command is blocked, the tool returns a `BLOCKED` message with structured fields:

```
⚠️ BLOCKED: <summary>
Reason: <why the command is dangerous>
Command: <the matched command fragment>
Tip: <safe alternative>
```

> **Note**: Destructive command blocking is a best-effort guard, not a security boundary. The OS-level sandbox remains the primary enforcement mechanism. See [sandbox.md](./sandbox.md) for details.

### Read Tool

**Purpose**: Read file contents or list directory entries.

**Parameters**:
- `path`: Absolute path to file or directory (required)
- `start_line`: First line to read (1-indexed, default: 1)
- `end_line`: Last line to read (1-indexed, default: 500)

**Features**:
- Line-numbered output for files
- Directory listing with trailing `/` for subdirectories
- Binary file detection (rejects files with null bytes)
- Automatic truncation at 100KB
- Pagination hints ("... X more lines")

**Output Format**:
```
File: /path/to/file
Lines 1-100/500
     1: first line
     2: second line
    ...
[... 400 more lines ...]
```

### Edit Tool

**Purpose**: Make targeted text replacements in existing files.

**Parameters**:
- `path`: Absolute path to the file (required)
- `edits`: Array of edit operations (required, max 10)
  - `old_text`: Exact text to find (required)
  - `new_text`: Replacement text (required)

**Features**:
- Multiple edits per call (up to 10)
- Preflight validation (all edits validated before any changes)
- Overlap detection (prevents conflicting edits)
- Reverse-order application (prevents position shifting)
- Line ending preservation (LF/CRLF)
- UTF-8 BOM handling
- Exact match validation (including whitespace)
- Delete support (empty `newText`)
- Binary file detection
- Unified diff output showing changes

**Error Cases**:
- `old_text` not found (with edit number)
- Multiple matches for `old_text` (must be unique)
- `old_text` == `new_text` (no-op)
- Overlapping edits (with edit numbers)
- Too many edits (> 10)
- No edits provided
- File is binary
- Path is outside working directory

**Example**:
```json
{
  "path": "/path/to/file.rs",
  "edits": [
    { "old_text": "fn old_name()", "new_text": "fn new_name()" },
    { "old_text": "old_name()", "new_text": "new_name()" }
  ]
}
```

### Write Tool

**Purpose**: Create new files or overwrite existing ones.

**Parameters**:
- `file_path`: Absolute path to the file (required)
- `content`: Full content to write (required)

**Features**:
- Automatic parent directory creation
- Distinguishes create vs. overwrite in output
- Warning for overwrites (suggests using Edit instead)
- Byte count reporting

**Best Practices**:
- Use for new files
- Use Edit for modifying existing files (more precise)
- Large files: Consider breaking into multiple writes

## Sandboxing

The Bash tool integrates with the `tools::sandbox` module:

```rust
// Check if sandboxing is disabled
if !super::sandbox::is_sandbox_disabled() {
    if let Some(strategy) = super::sandbox::detect_platform() {
        strategy.apply(&mut command, &sandbox_config)?;
    }
}
```

See [sandbox.md](./sandbox.md) for details on sandbox implementation.

## Error Handling

Tools return `Result<ToolResult, String>` where:
- `Ok(ToolResult { output })`: Success with output string
- `Err(message)`: Error with descriptive message

Error messages are designed to be:
- Actionable (suggest what to do)
- Descriptive (include path, context)
- Safe (don't expose sensitive info)

Examples:
- `"Path '/etc/passwd' is outside the working directory"`
- `"old_text matches 3 locations in file: add more context or set replace_all"`
- `"Binary file detected: cannot edit"`

## Testing

Each tool has comprehensive tests:

- **Bash**: Output streaming, timeout, sandbox blocking, stderr capture, metadata footer formatting, binary output detection, destructive command blocking (git operations, filesystem operations, wrapper detection, command chaining, false positive avoidance)
- **Read**: Small files, line ranges, directories, binary detection
- **Edit**: Multiple edits, overlap detection, line ending preservation, BOM handling, binary files, no-op detection, path validation
- **Write**: Create, overwrite, nested directories, path validation

Tests use `tempfile` for isolation and avoid side effects on the real filesystem.
