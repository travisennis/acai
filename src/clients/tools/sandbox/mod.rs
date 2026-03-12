//! OS-level sandboxing for the Bash tool
//!
//! Provides filesystem sandboxing using:
//! - macOS: `sandbox-exec` (Seatbelt profile)
//! - Linux: Landlock LSM (kernel 5.13+)
//!
//! The sandbox restricts filesystem access to:
//! - Read-write: current working directory, temp directories
//! - Read-only + exec: system paths (/usr, /bin, /lib, etc.)
//! - Read-only: config/device paths (/etc, /dev/null, etc.)
//! - Deny: everything else

use std::path::PathBuf;

// =============================================================================
// Platform-specific implementations
// =============================================================================

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(super) use macos::MacOsSandbox;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(super) use linux::LandlockSandbox;

// =============================================================================
// Core Types
// =============================================================================

#[derive(Clone, Debug)]
#[allow(clippy::struct_field_names)]
pub(super) struct SandboxConfig {
    /// Directories with read-write access (cwd, temp dirs)
    pub read_write: Vec<PathBuf>,
    /// Directories with read-only + execute access (system paths)
    pub read_only_exec: Vec<PathBuf>,
    /// Directories with read-only access
    pub read_only: Vec<PathBuf>,
}

impl SandboxConfig {
    /// Build a sandbox configuration for the current context
    #[allow(clippy::unnecessary_wraps)]
    pub fn build(cwd: &std::path::Path) -> Result<Self, String> {
        let mut read_write = vec![cwd.to_path_buf()];

        // Add temp directories
        let temp_dirs = super::get_temp_directories();
        read_write.extend(temp_dirs);

        // Add cargo writable paths (registry downloads, package cache, git deps)
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            let cargo_home =
                std::env::var_os("CARGO_HOME").map_or_else(|| home.join(".cargo"), PathBuf::from);
            for subdir in &["registry", "git", ".package-cache"] {
                let p = cargo_home.join(subdir);
                if p.exists() {
                    read_write.push(p);
                }
            }
        }

        // Include both original and canonical paths to handle symlinks
        // (e.g., /tmp → /private/tmp on macOS)
        let read_write = deduplicated_with_canonical(&read_write);

        let read_only_exec = Self::get_system_paths();
        let read_only = Self::get_read_only_paths();

        Ok(Self {
            read_write,
            read_only_exec,
            read_only,
        })
    }

    /// Get system paths that need read + execute access
    fn get_system_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        #[cfg(target_os = "macos")]
        {
            paths.extend([
                PathBuf::from("/usr"),
                PathBuf::from("/bin"),
                PathBuf::from("/sbin"),
                PathBuf::from("/Library"),
                PathBuf::from("/System"),
                PathBuf::from("/Applications"),
                PathBuf::from("/opt/homebrew"),
                PathBuf::from("/opt/local"),
            ]);
        }

        #[cfg(target_os = "linux")]
        {
            paths.extend([
                PathBuf::from("/usr"),
                PathBuf::from("/bin"),
                PathBuf::from("/sbin"),
                PathBuf::from("/lib"),
                PathBuf::from("/lib64"),
                PathBuf::from("/etc/alternatives"),
                PathBuf::from("/snap"),
            ]);
        }

        // Add user toolchain paths (cargo, rustup, etc.)
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            let cargo_home =
                std::env::var_os("CARGO_HOME").map_or_else(|| home.join(".cargo"), PathBuf::from);
            let rustup_home =
                std::env::var_os("RUSTUP_HOME").map_or_else(|| home.join(".rustup"), PathBuf::from);

            paths.push(cargo_home);
            paths.push(rustup_home);
        }

        paths.into_iter().filter(|p| p.exists()).collect()
    }

    /// Get paths that need read-only access
    fn get_read_only_paths() -> Vec<PathBuf> {
        #[cfg(target_os = "macos")]
        let paths = vec![
            PathBuf::from("/etc"),
            PathBuf::from("/private/etc"),
            PathBuf::from("/private/var"),
            PathBuf::from("/dev"),
            PathBuf::from("/var"),
        ];

        #[cfg(target_os = "linux")]
        let paths = vec![
            PathBuf::from("/etc"),
            PathBuf::from("/dev"),
            PathBuf::from("/proc"),
            PathBuf::from("/sys"),
        ];

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let paths: Vec<PathBuf> = vec![];

        paths.into_iter().filter(|p| p.exists()).collect()
    }
}

/// Include both original and canonical paths, deduplicated.
/// This ensures sandbox rules match regardless of symlink resolution
/// (e.g., /tmp and /private/tmp on macOS).
fn deduplicated_with_canonical(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for p in paths {
        if seen.insert(p.clone()) {
            result.push(p.clone());
        }
        if let Ok(canonical) = p.canonicalize()
            && seen.insert(canonical.clone())
        {
            result.push(canonical);
        }
    }

    result
}

/// Platform-specific sandbox strategy trait
pub(super) trait SandboxStrategy: Send + Sync {
    /// Wrap the given Command with sandbox restrictions.
    ///
    /// On macOS: replace the command with `sandbox-exec -f <profile> bash -c <cmd>`
    /// On Linux: apply Landlock rules before spawning
    fn apply(
        &self,
        command: &mut tokio::process::Command,
        config: &SandboxConfig,
    ) -> Result<(), String>;
}

/// Detect the appropriate sandbox strategy for the current platform
pub(super) fn detect_platform() -> Option<Box<dyn SandboxStrategy>> {
    #[cfg(target_os = "macos")]
    {
        if std::path::Path::new("/usr/bin/sandbox-exec").exists() {
            log::debug!("Using macOS sandbox-exec for filesystem sandboxing");
            return Some(Box::new(MacOsSandbox));
        }
        log::warn!(
            "sandbox-exec not found at /usr/bin/sandbox-exec; bash commands will run unsandboxed"
        );
    }

    #[cfg(target_os = "linux")]
    {
        log::debug!("Using Linux Landlock LSM for filesystem sandboxing");
        return Some(Box::new(LandlockSandbox));
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        log::warn!(
            "No sandbox available for this platform ({}); bash commands will run unsandboxed",
            std::env::consts::OS
        );
    }

    None
}

/// Check if sandboxing should be disabled via environment variable
pub(super) fn is_sandbox_disabled() -> bool {
    match std::env::var("ACAI_SANDBOX").as_deref() {
        Ok("off" | "0" | "false" | "no") => {
            log::warn!("Sandbox disabled via ACAI_SANDBOX environment variable");
            true
        },
        Ok("warn") => {
            log::warn!("Sandbox 'warn' mode requested; falling back to enforce mode");
            false
        },
        _ => false,
    }
}
