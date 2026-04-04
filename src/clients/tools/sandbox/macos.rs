//! macOS sandbox implementation using `sandbox-exec`
//!
//! Uses the Seatbelt sandbox profile language (Scheme-like syntax) to
//! generate dynamic sandbox profiles that restrict filesystem access.
//! The profile uses a deny-default policy: everything is denied unless
//! explicitly allowed.

use crate::clients::tools::sandbox::{SandboxConfig, SandboxStrategy};
use std::path::Path;
use std::process::Stdio;

/// macOS sandbox strategy using sandbox-exec
#[derive(Debug, Clone, Copy)]
pub struct MacOsSandbox;

impl MacOsSandbox {
    /// Append device and PTY rules to the profile
    fn append_device_rules(lines: &mut Vec<String>) {
        lines.push("; Allow access to standard and PTY devices".to_string());
        lines.push("(allow file-read* file-write* (literal \"/dev/null\"))".to_string());
        lines.push("(allow file-read* (literal \"/dev/urandom\"))".to_string());
        lines.push("(allow file-read* (literal \"/dev/random\"))".to_string());
        lines.push("(allow file-read* (literal \"/dev/zero\"))".to_string());
        lines.push("(allow file-read* file-write* (literal \"/dev/tty\"))".to_string());
        lines.push("(allow file-read* file-write* (literal \"/dev/ptmx\"))".to_string());
        lines.push("(allow file-read* file-write* (literal \"/dev/dtracehelper\"))".to_string());
        lines.push("(allow file-read* file-write* (literal \"/dev/stdout\"))".to_string());
        lines.push("(allow file-read* file-write* (literal \"/dev/stderr\"))".to_string());
        lines.push("(allow file-read* file-write* (subpath \"/dev/fd\"))".to_string());
        lines.push("(allow file-read* file-write* (regex #\"^/dev/ttys\"))".to_string());
        lines.push("(allow file-read* file-write* (regex #\"^/dev/pty\"))".to_string());
        lines.push(String::new());
    }

    /// Append git configuration read-only rules to the profile
    fn append_git_rules(lines: &mut Vec<String>) {
        lines.push("; Git configuration (read-only)".to_string());
        if let Ok(home) = std::env::var("HOME") {
            lines.push(format!("(allow file-read* (prefix \"{home}/.gitconfig\"))"));
            lines.push(format!("(allow file-read* (prefix \"{home}/.gitignore\"))"));
            lines.push(format!(
                "(allow file-read* (subpath \"{home}/.config/git\"))"
            ));
            lines.push(format!(
                "(allow file-read* (literal \"{home}/.gitattributes\"))"
            ));
            // Allow reading .ssh directory itself (for listing)
            lines.push(format!("(allow file-read* (literal \"{home}/.ssh\"))"));
            lines.push(format!(
                "(allow file-read* (literal \"{home}/.ssh/config\"))"
            ));
            lines.push(format!(
                "(allow file-read* (literal \"{home}/.ssh/known_hosts\"))"
            ));
        }
        lines.push(String::new());
    }

    /// Append SSH agent socket rules to the profile
    fn append_ssh_agent_rules(lines: &mut Vec<String>) {
        lines.push("; SSH agent sockets (for git push over SSH)".to_string());
        // SSH agent sockets are typically in /tmp/ssh-XXXXXX/agent.XXXXXX
        lines.push("(allow file-read* file-write* (regex #\"^/tmp/ssh-\"))".to_string());
        // On macOS, launchd-managed ssh-agent uses /private/tmp
        lines.push("(allow file-read* file-write* (regex #\"^/private/tmp/ssh-\"))".to_string());
        lines.push(
            "(allow file-read* file-write* (regex #\"^/private/tmp/com\\.apple\\.launchd\\.*/Listeners\"))"
                .to_string(),
        );
        // Allow the actual SSH_AUTH_SOCK path (may be in a non-standard location
        // such as ~/.ssh/agent/). Grant read-write on the parent directory so the
        // sandboxed process can connect to the Unix-domain socket.
        if let Ok(sock) = std::env::var("SSH_AUTH_SOCK") {
            let sock_path = std::path::Path::new(&sock);
            if let Some(parent) = sock_path.parent() {
                let escaped = escape_path(parent);
                lines.push(format!(
                    "(allow file-read* file-write* (subpath \"{escaped}\"))"
                ));
            }
        }
        lines.push(String::new());
    }

    /// Append SCM CLI (gh, glab) configuration, cache, and state rules to the profile.
    fn append_scm_cli_rules(lines: &mut Vec<String>) {
        lines.push("; SCM CLIs: GitHub CLI (gh) and GitLab CLI (glab)".to_string());
        if let Ok(home) = std::env::var("HOME") {
            // GitHub CLI
            lines.push(format!(
                "(allow file-read* file-write* (subpath \"{home}/.config/gh\"))"
            ));
            lines.push(format!(
                "(allow file-read* file-write* (subpath \"{home}/.cache/gh\"))"
            ));
            lines.push(format!(
                "(allow file-read* file-write* (subpath \"{home}/.local/share/gh\"))"
            ));
            lines.push(format!(
                "(allow file-read* file-write* (subpath \"{home}/.local/state/gh\"))"
            ));
            // GitLab CLI
            lines.push(format!(
                "(allow file-read* file-write* (subpath \"{home}/.config/glab-cli\"))"
            ));
            lines.push(format!(
                "(allow file-read* file-write* (subpath \"{home}/.cache/glab-cli\"))"
            ));
            lines.push(format!(
                "(allow file-read* file-write* (subpath \"{home}/.local/share/glab-cli\"))"
            ));
            lines.push(format!(
                "(allow file-read* file-write* (subpath \"{home}/.local/state/glab-cli\"))"
            ));
        }
        lines.push(String::new());
    }

    /// Append macOS Keychain access rules to the profile.
    ///
    /// Note: actual Keychain service access (used by `gh`, `security`, and
    /// SSH passphrase retrieval) is mediated by Security.framework over Mach
    /// IPC, which is covered by the `(allow mach-lookup)` rule above. The
    /// file-level rules here allow tools that read keychain database files
    /// directly (rare, but harmless to permit).
    fn append_keychain_rules(lines: &mut Vec<String>) {
        lines.push(
            "; macOS Keychain file access (supplementary; primary access is via mach-lookup)"
                .to_string(),
        );
        lines.push("(allow file-read* (subpath \"/Library/Keychains\"))".to_string());
        lines.push("(allow file-read* (subpath \"/System/Library/Keychains\"))".to_string());
        if let Ok(home) = std::env::var("HOME") {
            lines.push(format!(
                "(allow file-read* file-write* (subpath \"{home}/Library/Keychains\"))"
            ));
        }
        lines.push(String::new());
    }

    /// Generate a deny-default sandbox profile (.sb file content) from the configuration
    fn generate_profile(config: &SandboxConfig) -> String {
        let mut lines = vec![
            "(version 1)".to_string(),
            "(deny default)".to_string(),
            String::new(),
        ];

        // Process execution (fork/exec needed for bash and subcommands)
        lines.push("; Allow process execution".to_string());
        lines.push("(allow process-fork)".to_string());
        lines.push("(allow process-exec)".to_string());
        lines.push("(allow pseudo-tty)".to_string());
        lines.push(String::new());

        // Process introspection scoped to same sandbox
        lines.push("; Allow process introspection within same sandbox".to_string());
        lines.push("(allow process-info* (target same-sandbox))".to_string());
        lines.push("(allow signal (target same-sandbox))".to_string());
        lines.push("(allow mach-priv-task-port (target same-sandbox))".to_string());
        lines.push(String::new());

        // Mach services (required for dyld, DNS, system frameworks, etc.)
        lines.push("; Allow mach lookups (needed for basic process operation)".to_string());
        lines.push("(allow mach-lookup)".to_string());
        lines.push(String::new());

        // Sysctl reads (needed by many tools)
        lines.push("; Allow sysctl reads".to_string());
        lines.push("(allow sysctl-read)".to_string());
        lines.push(String::new());

        // System socket (needed for kernel event monitoring by network stack)
        lines.push("; Allow system sockets and shared memory".to_string());
        lines.push("(allow system-socket)".to_string());
        lines.push(
            "(allow ipc-posix-shm-read-data (ipc-posix-name \"apple.shm.notification_center\"))"
                .to_string(),
        );
        lines.push(String::new());

        // Network access (sandbox only restricts filesystem, not network)
        lines.push("; Allow network access".to_string());
        lines.push("(allow network*)".to_string());
        lines.push(String::new());

        // Root directory literal (dyld needs to traverse root)
        lines.push("; Allow reading root directory (needed by dyld)".to_string());
        lines.push("(allow file-read* (literal \"/\"))".to_string());
        lines.push(String::new());

        // CWD ancestor directory literals (agents call readdir() on ancestors)
        lines.push("; Allow reading CWD ancestor directories".to_string());
        if let Some(cwd) = config.read_write.first() {
            let mut ancestor = cwd.as_path();
            let mut ancestors = Vec::new();
            while let Some(parent) = ancestor.parent() {
                ancestors.push(parent.to_path_buf());
                ancestor = parent;
            }
            for a in ancestors.into_iter().rev() {
                let escaped = escape_path(&a);
                if escaped != "/" {
                    lines.push(format!("(allow file-read* (literal \"{escaped}\"))"));
                }
            }
        }
        lines.push(String::new());

        // Read-write access for working directory and temp dirs
        if !config.read_write.is_empty() {
            lines.push(
                "; Read-write access: working directory, temp dirs, and toolchains".to_string(),
            );
            for path in &config.read_write {
                let escaped = escape_path(path);
                lines.push(format!(
                    "(allow file-read* file-write* (subpath \"{escaped}\"))"
                ));
            }
            lines.push(String::new());
        }

        // Read + execute access for system paths
        if !config.read_only_exec.is_empty() {
            lines.push("; Read + execute access: system paths".to_string());
            for path in &config.read_only_exec {
                let escaped = escape_path(path);
                lines.push(format!("(allow file-read* (subpath \"{escaped}\"))"));
            }
            lines.push(String::new());
        }

        // Read-only access for config/device paths
        if !config.read_only.is_empty() {
            lines.push("; Read-only access: config and device paths".to_string());
            for path in &config.read_only {
                let escaped = escape_path(path);
                lines.push(format!("(allow file-read* (subpath \"{escaped}\"))"));
            }
            lines.push(String::new());
        }

        Self::append_git_rules(&mut lines);
        Self::append_ssh_agent_rules(&mut lines);
        Self::append_scm_cli_rules(&mut lines);
        Self::append_keychain_rules(&mut lines);
        Self::append_device_rules(&mut lines);

        // Allow file-ioctl scoped to terminal devices
        lines.push("; Allow file-ioctl for terminal operations".to_string());
        lines.push("(allow file-ioctl)".to_string());

        // Allow file locking (needed by cargo and other build tools)
        lines.push("; Allow file locking (needed by cargo and other build tools)".to_string());
        lines.push("(allow file-lock)".to_string());

        lines.join("\n")
    }

    /// Write the profile to a temp file and return its path
    fn write_profile_to_temp(profile: &str) -> Result<tempfile::NamedTempFile, String> {
        use std::io::Write;

        let tmp_dir = std::env::temp_dir().join("acai").join("sandbox_profiles");
        std::fs::create_dir_all(&tmp_dir)
            .map_err(|e| format!("Failed to create sandbox profile directory: {e}"))?;

        let mut temp_file = tempfile::Builder::new()
            .prefix("acai_sandbox_")
            .suffix(".sb")
            .tempfile_in(&tmp_dir)
            .map_err(|e| format!("Failed to create sandbox profile temp file: {e}"))?;

        temp_file
            .write_all(profile.as_bytes())
            .map_err(|e| format!("Failed to write sandbox profile: {e}"))?;

        tracing::debug!(
            "Generated sandbox profile at: {}",
            temp_file.path().display()
        );

        Ok(temp_file)
    }
}

impl SandboxStrategy for MacOsSandbox {
    fn apply(
        &self,
        command: &mut tokio::process::Command,
        config: &SandboxConfig,
    ) -> Result<(), String> {
        let profile = Self::generate_profile(config);
        tracing::debug!("Generated sandbox profile:\n{profile}");

        // Write profile to temp file — persist so sandbox-exec can read it at spawn time
        let temp_file = Self::write_profile_to_temp(&profile)?;
        let profile_path = temp_file.into_temp_path();

        // Get the original command arguments
        let original_args: Vec<String> = command
            .as_std()
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        // Reconfigure the command to use sandbox-exec
        *command = tokio::process::Command::new("/usr/bin/sandbox-exec");

        command.arg("-f").arg(profile_path.as_os_str());

        // Add the original program (bash) and its arguments
        command.arg("bash");
        for arg in original_args {
            command.arg(arg);
        }

        // Re-apply stdio configuration
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Leak the TempPath so the file persists until process exit.
        // The OS will clean up temp files.
        std::mem::forget(profile_path);

        tracing::debug!("Sandboxed command configured with deny-default profile");

        Ok(())
    }
}

/// Escape special characters in paths for the sandbox profile
fn escape_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_config() -> SandboxConfig {
        SandboxConfig {
            read_write: vec![PathBuf::from("/workspace")],
            read_only_exec: vec![PathBuf::from("/usr"), PathBuf::from("/bin")],
            read_only: vec![PathBuf::from("/etc")],
        }
    }

    #[test]
    fn test_profile_uses_deny_default() {
        let profile = MacOsSandbox::generate_profile(&test_config());

        assert!(profile.contains("(version 1)"));
        assert!(profile.contains("(deny default)"));
        assert!(!profile.contains("(allow default)"));
    }

    #[test]
    fn test_profile_allows_root_literal() {
        let profile = MacOsSandbox::generate_profile(&test_config());

        assert!(profile.contains("(allow file-read* (literal \"/\"))"));
    }

    #[test]
    fn test_profile_allows_read_write_paths() {
        let config = SandboxConfig {
            read_write: vec![PathBuf::from("/workspace"), PathBuf::from("/tmp")],
            read_only_exec: vec![],
            read_only: vec![],
        };

        let profile = MacOsSandbox::generate_profile(&config);

        assert!(profile.contains("(allow file-read* file-write* (subpath \"/workspace\"))"));
        assert!(profile.contains("(allow file-read* file-write* (subpath \"/tmp\"))"));
    }

    #[test]
    fn test_profile_allows_read_only_exec_paths() {
        let config = SandboxConfig {
            read_write: vec![],
            read_only_exec: vec![PathBuf::from("/usr"), PathBuf::from("/bin")],
            read_only: vec![],
        };

        let profile = MacOsSandbox::generate_profile(&config);

        assert!(profile.contains("(allow file-read* (subpath \"/usr\"))"));
        assert!(profile.contains("(allow file-read* (subpath \"/bin\"))"));
    }

    #[test]
    fn test_profile_allows_read_only_paths() {
        let config = SandboxConfig {
            read_write: vec![],
            read_only_exec: vec![],
            read_only: vec![PathBuf::from("/etc")],
        };

        let profile = MacOsSandbox::generate_profile(&config);

        assert!(profile.contains("(allow file-read* (subpath \"/etc\"))"));
    }

    #[test]
    fn test_profile_includes_process_and_system_rules() {
        let profile = MacOsSandbox::generate_profile(&test_config());

        assert!(profile.contains("(allow process-fork)"));
        assert!(profile.contains("(allow process-exec)"));
        assert!(profile.contains("(allow pseudo-tty)"));
        assert!(profile.contains("(allow mach-lookup)"));
        assert!(profile.contains("(allow process-info* (target same-sandbox))"));
        assert!(profile.contains("(allow signal (target same-sandbox))"));
        assert!(profile.contains("(allow sysctl-read)"));
        assert!(profile.contains("(allow system-socket)"));
        assert!(profile.contains("(allow network*)"));
    }

    #[test]
    fn test_profile_allows_standard_devices() {
        let profile = MacOsSandbox::generate_profile(&test_config());

        assert!(profile.contains("/dev/null"));
        assert!(profile.contains("/dev/urandom"));
        assert!(profile.contains("/dev/tty"));
        assert!(profile.contains("/dev/ptmx"));
        assert!(profile.contains("/dev/fd"));
    }

    #[test]
    fn test_profile_allows_file_lock() {
        let profile = MacOsSandbox::generate_profile(&test_config());
        assert!(profile.contains("(allow file-lock)"));
    }

    #[test]
    fn test_profile_escaping() {
        let path = PathBuf::from("/path/with\"quote");
        let escaped = escape_path(&path);
        assert_eq!(escaped, "/path/with\\\"quote");
    }

    #[test]
    fn test_profile_allows_ssh_directory_access() {
        let profile = MacOsSandbox::generate_profile(&test_config());

        // Should allow reading .ssh directory itself and specific config files
        assert!(
            profile.contains(".ssh\"))"),
            "Expected profile to allow access to .ssh directory"
        );
    }

    #[test]
    fn test_profile_allows_ssh_agent_sockets() {
        let profile = MacOsSandbox::generate_profile(&test_config());

        // Should allow access to SSH agent sockets in /tmp/ssh-*
        assert!(
            profile.contains("^/tmp/ssh-"),
            "Expected profile to allow access to /tmp/ssh-* sockets"
        );
        // Should allow access to SSH agent sockets in /private/tmp/ssh-*
        assert!(
            profile.contains("^/private/tmp/ssh-"),
            "Expected profile to allow access to /private/tmp/ssh-* sockets"
        );
        // Should allow access to launchd-managed SSH agent sockets
        assert!(
            profile.contains("com\\.apple\\.launchd"),
            "Expected profile to allow access to launchd SSH agent sockets"
        );
    }

    #[test]
    fn test_profile_allows_git_xdg_config() {
        let profile = MacOsSandbox::generate_profile(&test_config());

        assert!(
            profile.contains(".config/git"),
            "Expected profile to allow XDG git config directory"
        );
        assert!(
            profile.contains(".gitattributes"),
            "Expected profile to allow .gitattributes file"
        );
    }

    #[test]
    fn test_profile_allows_ssh_config_and_known_hosts() {
        let profile = MacOsSandbox::generate_profile(&test_config());

        assert!(
            profile.contains(".ssh/config"),
            "Expected profile to allow .ssh/config file"
        );
        assert!(
            profile.contains(".ssh/known_hosts"),
            "Expected profile to allow .ssh/known_hosts file"
        );
    }

    #[test]
    fn test_profile_allows_gh_cli_paths() {
        let profile = MacOsSandbox::generate_profile(&test_config());

        assert!(
            profile.contains(".config/gh"),
            "Expected profile to allow gh config directory"
        );
        assert!(
            profile.contains(".cache/gh"),
            "Expected profile to allow gh cache directory"
        );
        assert!(
            profile.contains(".local/share/gh"),
            "Expected profile to allow gh share directory"
        );
        assert!(
            profile.contains(".local/state/gh"),
            "Expected profile to allow gh state directory"
        );
    }

    #[test]
    fn test_profile_allows_glab_cli_paths() {
        let profile = MacOsSandbox::generate_profile(&test_config());

        assert!(
            profile.contains(".config/glab-cli"),
            "Expected profile to allow glab config directory"
        );
        assert!(
            profile.contains(".cache/glab-cli"),
            "Expected profile to allow glab cache directory"
        );
        assert!(
            profile.contains(".local/share/glab-cli"),
            "Expected profile to allow glab share directory"
        );
        assert!(
            profile.contains(".local/state/glab-cli"),
            "Expected profile to allow glab state directory"
        );
    }

    #[test]
    fn test_profile_does_not_allow_full_ssh_subpath() {
        let profile = MacOsSandbox::generate_profile(&test_config());

        // Should NOT grant broad subpath read to .ssh (only specific files)
        assert!(
            !profile.contains(".ssh\"))")
                || profile.contains("(literal \"") && profile.contains(".ssh/"),
            "Profile should not use subpath for .ssh access"
        );
    }
}
