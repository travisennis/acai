//! macOS sandbox implementation using `sandbox-exec`
//!
//! Uses the Seatbelt sandbox profile language (Scheme-like syntax) to
//! generate dynamic sandbox profiles that restrict filesystem access.
//! The profile uses a deny-default policy: everything is denied unless
//! explicitly allowed.

use super::{SandboxConfig, SandboxStrategy};
use std::path::Path;
use std::process::Stdio;

/// macOS sandbox strategy using sandbox-exec
#[derive(Debug, Clone, Copy)]
pub struct MacOsSandbox;

impl MacOsSandbox {
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
        lines.push(String::new());

        // Mach services (required for dyld, DNS, system frameworks, etc.)
        lines.push("; Allow mach lookups (needed for basic process operation)".to_string());
        lines.push("(allow mach-lookup)".to_string());
        lines.push(String::new());

        // Signals (needed for process management)
        lines.push("; Allow signals".to_string());
        lines.push("(allow signal)".to_string());
        lines.push(String::new());

        // Sysctl reads (needed by many tools)
        lines.push("; Allow sysctl reads".to_string());
        lines.push("(allow sysctl-read)".to_string());
        lines.push(String::new());

        // Network access (sandbox only restricts filesystem, not network)
        lines.push("; Allow network access".to_string());
        lines.push("(allow network*)".to_string());
        lines.push(String::new());

        // Root directory literal (dyld needs to traverse root)
        lines.push("; Allow reading root directory (needed by dyld)".to_string());
        lines.push("(allow file-read* (literal \"/\"))".to_string());
        lines.push(String::new());

        // Read-write access for working directory and temp dirs
        if !config.read_write.is_empty() {
            lines.push("; Read-write access: working directory and temp dirs".to_string());
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

        // Device access for /dev/null, /dev/urandom, /dev/zero, /dev/tty
        lines.push("; Allow access to standard devices".to_string());
        lines.push("(allow file-read* file-write* (literal \"/dev/null\"))".to_string());
        lines.push("(allow file-read* (literal \"/dev/urandom\"))".to_string());
        lines.push("(allow file-read* (literal \"/dev/random\"))".to_string());
        lines.push("(allow file-read* (literal \"/dev/zero\"))".to_string());
        lines.push("(allow file-read* file-write* (literal \"/dev/tty\"))".to_string());
        lines.push("(allow file-read* file-write* (literal \"/dev/dtracehelper\"))".to_string());
        lines.push(String::new());

        // Allow file-ioctl (needed for terminal operations)
        lines.push("; Allow file-ioctl (needed for terminal operations)".to_string());
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

        log::debug!(
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
        log::debug!("Generated sandbox profile:\n{profile}");

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

        log::debug!("Sandboxed command configured with deny-default profile");

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
        assert!(profile.contains("(allow mach-lookup)"));
        assert!(profile.contains("(allow signal)"));
        assert!(profile.contains("(allow sysctl-read)"));
        assert!(profile.contains("(allow network*)"));
    }

    #[test]
    fn test_profile_allows_standard_devices() {
        let profile = MacOsSandbox::generate_profile(&test_config());

        assert!(profile.contains("/dev/null"));
        assert!(profile.contains("/dev/urandom"));
        assert!(profile.contains("/dev/tty"));
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
}
