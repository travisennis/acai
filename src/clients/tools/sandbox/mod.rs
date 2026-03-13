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

        // Add user home toolchain and integration paths
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            // Rust: full read-write to cargo and rustup for builds/installs
            let cargo_home =
                std::env::var_os("CARGO_HOME").map_or_else(|| home.join(".cargo"), PathBuf::from);
            let rustup_home =
                std::env::var_os("RUSTUP_HOME").map_or_else(|| home.join(".rustup"), PathBuf::from);
            read_write.push(cargo_home);
            read_write.push(rustup_home);

            // Rust: sccache paths
            read_write.extend([
                home.join(".cache/sccache"),
                home.join("Library/Caches/sccache"),
            ]);

            // SCM CLIs: gh and glab for PR/issue workflows
            read_write.extend([
                home.join(".config/gh"),
                home.join(".cache/gh"),
                home.join(".local/share/gh"),
                home.join(".local/state/gh"),
                home.join(".config/glab-cli"),
                home.join(".cache/glab-cli"),
                home.join(".local/share/glab-cli"),
                home.join(".local/state/glab-cli"),
            ]);

            // Runtime managers: mise, asdf, volta
            read_write.extend([
                home.join(".config/mise"),
                home.join(".local/share/mise"),
                home.join(".local/state/mise"),
                home.join(".cache/mise"),
                home.join(".asdf"),
                home.join(".volta"),
            ]);

            #[cfg(target_os = "macos")]
            read_write.extend([
                home.join("Library/Caches/mise"),
                home.join("Library/Application Support/Mozilla.sccache"),
            ]);
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

        paths.into_iter().filter(|p| p.exists()).collect()
    }

    /// Get paths that need read-only access
    fn get_read_only_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        #[cfg(target_os = "macos")]
        paths.extend([
            PathBuf::from("/etc"),
            PathBuf::from("/private/etc"),
            PathBuf::from("/private/var"),
            PathBuf::from("/dev"),
            PathBuf::from("/var"),
        ]);

        #[cfg(target_os = "linux")]
        paths.extend([
            PathBuf::from("/etc"),
            PathBuf::from("/dev"),
            PathBuf::from("/proc"),
            PathBuf::from("/sys"),
        ]);

        // Git configuration (read-only)
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            paths.extend([home.join(".config/git"), home.join(".gitattributes")]);
        }

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
