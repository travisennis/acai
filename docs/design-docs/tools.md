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
- Path must be within allowed temp directories (`/tmp`, `/var/folders`, `TMPDIR`)

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
- Exit code reporting

**Output Handling**:
- Small output (< 50KB): Returned inline
- Large output (> 50KB): Written to temp file, preview returned
- Timeout: Command killed, timeout error returned

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

- **Bash**: Output streaming, timeout, sandbox blocking, stderr capture
- **Read**: Small files, line ranges, directories, binary detection
- **Edit**: Multiple edits, overlap detection, line ending preservation, BOM handling, binary files, no-op detection, path validation
- **Write**: Create, overwrite, nested directories, path validation

Tests use `tempfile` for isolation and avoid side effects on the real filesystem.
