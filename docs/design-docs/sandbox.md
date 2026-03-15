# Filesystem Sandbox

Acai sandboxes commands executed by the Bash tool to restrict filesystem access. This prevents LLM-generated commands from reading or writing files outside the project directory and essential system paths.

## Overview

When the Bash tool executes a command, acai wraps it in an OS-level sandbox that enforces a deny-default filesystem policy. Only explicitly allowed paths are accessible:

| Access Level | Paths | Purpose |
|---|---|---|
| **Read-write** | Current working directory, temp directories | Project files, build artifacts |
| **Read-only + execute** | `/usr`, `/bin`, `/sbin`, system paths, `~/.cargo`, `~/.rustup` | Running system tools and compilers |
| **Read-only** | `/etc`, `/dev`, `/var` | Configuration and device access |
| **Denied** | Everything else | Home directory, other projects, etc. |

## Platform Support

### macOS — sandbox-exec (Seatbelt)

On macOS, acai uses `sandbox-exec` with a dynamically generated [Seatbelt profile](https://reverse.put.as/wp-content/uploads/2011/09/Apple-Sandbox-Guide-v1.0.pdf). The profile uses a deny-default policy and explicitly allows:

- **Filesystem**: read-write for cwd/temp, read-only+exec for system paths, read-only for config/device paths
- **Process**: `process-fork`, `process-exec` (needed for bash and subcommands)
- **IPC**: `mach-lookup` (needed for dyld, DNS, system frameworks)
- **Signals**: allowed (needed for process management)
- **Network**: fully allowed (the sandbox restricts filesystem only, not network)
- **Devices**: `/dev/null`, `/dev/urandom`, `/dev/random`, `/dev/zero`, `/dev/tty`, `/dev/dtracehelper`
- **System**: `sysctl-read`, `file-ioctl` (needed for terminal operations)

Sandbox profiles are written to temporary files under `$TMPDIR/acai/sandbox_profiles/`.

Requires `/usr/bin/sandbox-exec` (present on all standard macOS installations).

### Linux — Landlock LSM

On Linux, acai uses [Landlock](https://landlock.io/), a Linux Security Module available since kernel 5.13. Landlock allows unprivileged processes to sandbox themselves without root access.

The Landlock sandbox is applied via `pre_exec`, so rules take effect in the child process after `fork()` but before `exec()`.

**Important**: Landlock support must be compiled in explicitly:

```bash
cargo build --release --features landlock
```

Without the `landlock` feature, a warning is logged and commands run without filesystem restrictions.

System paths on Linux include `/usr`, `/bin`, `/sbin`, `/lib`, `/lib64`, `/etc/alternatives`, and `/snap`.

## Configuration

### Disabling the Sandbox

Set the `ACAI_SANDBOX` environment variable to disable sandboxing:

```bash
# Any of these values disable the sandbox
export ACAI_SANDBOX=off
export ACAI_SANDBOX=0
export ACAI_SANDBOX=false
export ACAI_SANDBOX=no
```

When disabled, a warning is logged and all commands run with full filesystem access.

The `warn` value is recognized but currently falls back to enforce mode.

### Additional Read-Write Paths

The sandbox automatically includes:

- The current working directory (and its subtree)
- System temp directories (`$TMPDIR`, `/tmp`, `/var/tmp`)
- User toolchain paths (`$CARGO_HOME` or `~/.cargo`, `$RUSTUP_HOME` or `~/.rustup`)

All read-write paths are canonicalized (symlinks resolved) before being added to the sandbox policy.

## Examples

```bash
# This works — reading files in the project directory
acai instruct --prompt "List the files in this project"
# Bash tool runs: ls -la  ✓

# This is blocked — writing outside the project directory
# Bash tool runs: touch /tmp/acai_test  ✗ (Operation not permitted)

# This is blocked — reading the user's home directory
# Bash tool runs: ls ~/Desktop  ✗ (Operation not permitted)

# This works — running system tools
# Bash tool runs: git status  ✓
# Bash tool runs: cargo build  ✓
```

## Troubleshooting

### Command fails with "Operation not permitted"

The sandbox is blocking access to a path outside the allowed set. Options:

1. Ensure you're running acai from the correct project directory
2. If the command legitimately needs broader access, disable the sandbox with `ACAI_SANDBOX=off`

### "sandbox-exec not found" warning (macOS)

The `sandbox-exec` binary is missing from `/usr/bin/`. This is unusual on standard macOS installations. Commands will run without sandboxing.

### "Landlock feature not enabled" warning (Linux)

Rebuild with the Landlock feature:

```bash
cargo build --release --features landlock
```

### Sandbox not enforced on older Linux kernels

Landlock requires kernel 5.13 or later. On older kernels, Landlock reports `NotEnforced` status and commands run without restrictions. Check your kernel version with `uname -r`.
