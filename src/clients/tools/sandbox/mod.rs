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

use std::path::{Path, PathBuf};

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
#[allow(clippy::struct_field_names, dead_code)]
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
    #[allow(dead_code)]
    pub fn build(cwd: &std::path::Path) -> Self {
        Self::build_with_additional_dirs(cwd, &[], &[], &[])
    }

    /// Build a sandbox configuration with additional directories.
    ///
    /// `additional_dirs` are added as read-only (from `--add-dir`).
    /// `settings_dirs` are added as read-write (from `settings.toml`).
    /// `skill_dirs` are added as read-only (parent dirs of SKILL.md files).
    pub fn build_with_additional_dirs(
        cwd: &std::path::Path,
        additional_dirs: &[std::path::PathBuf],
        settings_dirs: &[std::path::PathBuf],
        skill_dirs: &[std::path::PathBuf],
    ) -> Self {
        let mut read_write = vec![cwd.to_path_buf()];

        // Add temp directories
        let temp_dirs = super::get_temp_directories();
        read_write.extend(temp_dirs.iter().cloned());

        // Add user home toolchain and integration paths
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            Self::extend_with_toolchain_paths(&mut read_write, &home);
        }

        // Add settings directories from settings.toml as read-write
        push_dirs_with_canonical(&mut read_write, settings_dirs);

        // Include both original and canonical paths to handle symlinks
        // (e.g., /tmp → /private/tmp on macOS)
        let read_write = deduplicated_with_canonical(&read_write);

        let read_only_exec = Self::get_system_paths();
        let mut read_only = Self::get_read_only_paths();

        // Add additional directories from --add-dir flag as read-only
        push_dirs_with_canonical(&mut read_only, additional_dirs);

        // Add skill directories as read-only (so scripts like x-fetch.js can execute)
        push_dirs_with_canonical(&mut read_only, skill_dirs);

        Self {
            read_write,
            read_only_exec,
            read_only,
        }
    }

    /// Extend `read_write` with paths needed by common toolchains and CLIs.
    ///
    /// Patterned after the Safehouse macOS profile (see
    /// `examples/safehouse.custom.generated.sb`). The goal is for cake to
    /// work across the languages, package managers, and runtime managers a
    /// coding agent typically encounters, without users having to add per-tool
    /// `--add-dir` flags.
    #[allow(clippy::too_many_lines)]
    fn extend_with_toolchain_paths(read_write: &mut Vec<PathBuf>, home: &Path) {
        // Rust: cargo + rustup (env var overrides honored).
        let cargo_home =
            std::env::var_os("CARGO_HOME").map_or_else(|| home.join(".cargo"), PathBuf::from);
        let rustup_home =
            std::env::var_os("RUSTUP_HOME").map_or_else(|| home.join(".rustup"), PathBuf::from);
        read_write.push(cargo_home);
        read_write.push(rustup_home);
        read_write.extend([
            home.join(".config/cargo"),
            home.join(".cache/cargo"),
            home.join(".cache/sccache"),
        ]);

        // Pre-commit hook caches.
        read_write.push(home.join(".cache/prek"));

        // SCM CLIs: gh and glab.
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

        // Cross-language runtime managers.
        read_write.extend([
            // mise
            home.join(".config/mise"),
            home.join(".local/share/mise"),
            home.join(".local/state/mise"),
            home.join(".cache/mise"),
            home.join(".mise.toml"),
            // volta
            home.join(".volta"),
            // asdf
            home.join(".asdf"),
            home.join(".config/asdf"),
            home.join(".local/share/asdf"),
            home.join(".local/state/asdf"),
            home.join(".cache/asdf"),
            home.join(".asdfrc"),
            home.join(".tool-versions"),
            // proto
            home.join(".proto"),
            home.join(".prototools"),
            // pkgx
            home.join(".pkgx"),
            home.join(".local/share/pkgx"),
            home.join(".cache/pkgx"),
        ]);

        // Node.js ecosystem.
        read_write.extend([
            // version managers
            home.join(".nvm"),
            home.join(".fnm"),
            home.join(".local/share/fnm"),
            home.join(".local/state/fnm"),
            home.join(".local/state/fnm_multishells"),
            home.join(".cache/fnm"),
            // npm
            home.join(".npm"),
            home.join(".config/npm"),
            home.join(".cache/npm"),
            home.join(".cache/node"),
            home.join(".npmrc"),
            // configstore (used by npm and many node CLIs)
            home.join(".config/configstore"),
            // node-gyp
            home.join(".node-gyp"),
            home.join(".cache/node-gyp"),
            // pnpm
            home.join(".config/pnpm"),
            home.join(".pnpm-state"),
            home.join(".pnpm-store"),
            home.join(".local/share/pnpm"),
            home.join(".local/state/pnpm"),
            // yarn (classic + modern)
            home.join(".yarn"),
            home.join(".yarnrc"),
            home.join(".yarnrc.yml"),
            home.join(".config/yarn"),
            home.join(".cache/yarn"),
            // corepack
            home.join(".cache/node/corepack"),
            // turborepo
            home.join(".cache/turbo"),
            // browser automation / test runners (Linux/XDG locations)
            home.join(".cache/puppeteer"),
            home.join(".cache/prisma"),
        ]);

        // Bun.
        read_write.extend([
            home.join(".bun"),
            home.join(".cache/bun"),
            home.join(".local/state/bun"),
            home.join(".local/share/bun"),
            home.join(".bunfig.toml"),
            home.join(".config/bunfig.toml"),
        ]);

        // Deno.
        read_write.extend([home.join(".deno"), home.join(".cache/deno")]);

        // Go (env var overrides honored).
        let gopath = std::env::var_os("GOPATH").map_or_else(|| home.join("go"), PathBuf::from);
        read_write.push(gopath);
        let gomodcache = std::env::var_os("GOMODCACHE").map(PathBuf::from);
        if let Some(p) = gomodcache {
            read_write.push(p);
        }
        let gocache = std::env::var_os("GOCACHE").map(PathBuf::from);
        if let Some(p) = gocache {
            read_write.push(p);
        }
        read_write.extend([
            home.join(".cache/go-build"),
            home.join(".config/go"),
            home.join(".cache/golangci-lint"),
            home.join(".config/golangci-lint"),
            home.join(".local/share/go"),
            home.join(".goenv"),
            home.join(".cache/gopls"),
        ]);

        // Java / JVM toolchains (Maven, Gradle, SBT, Coursier, jenv, sdkman).
        read_write.extend([
            home.join(".m2"),
            home.join(".gradle"),
            home.join(".ivy2"),
            home.join(".sbt"),
            home.join(".jenv"),
            home.join(".sdkman"),
            home.join(".cache/coursier"),
            home.join(".coursier"),
            home.join(".java"),
            home.join(".mavenrc"),
        ]);

        // Python: uv, pip, pipx, poetry, pdm, conda, hatch, ruff, mypy, jupyter, pyenv, etc.
        read_write.extend([
            home.join(".local/bin/uv"),
            home.join(".local/bin/uvx"),
            home.join(".local/share/uv"),
            home.join(".local/state/uv"),
            home.join(".local/pipx"),
            home.join(".cache/uv"),
            home.join(".config/uv"),
            home.join(".cache/pip"),
            home.join(".config/pip"),
            home.join(".cache/pypoetry"),
            home.join(".config/pypoetry"),
            home.join(".local/share/pypoetry"),
            home.join(".cache/pdm"),
            home.join(".config/pdm"),
            home.join(".local/share/pdm"),
            home.join(".cache/pre-commit"),
            home.join(".cache/mypy"),
            home.join(".cache/ruff"),
            home.join(".virtualenvs"),
            home.join(".ipython"),
            home.join(".jupyter"),
            home.join(".pyenv"),
            home.join(".pypirc"),
            home.join(".python_history"),
            // conda / miniconda / miniforge
            home.join(".conda"),
            home.join("miniconda3"),
            home.join("miniforge3"),
            home.join(".condarc"),
            // hatch
            home.join(".cache/hatch"),
            home.join(".config/hatch"),
            home.join(".local/share/hatch"),
        ]);

        // Ruby: rbenv, rvm, gem, bundler, etc.
        read_write.extend([
            home.join(".rbenv"),
            home.join(".rvm"),
            home.join(".rubies"),
            home.join(".bundle"),
            home.join(".gem"),
            home.join(".cache/bundler"),
            home.join(".cache/rubygems"),
            home.join(".gemrc"),
            home.join(".irbrc"),
            home.join(".irb_history"),
            home.join(".pryrc"),
            home.join(".pry_history"),
        ]);

        // Perl: perlbrew, plenv, cpan(m), local::lib.
        read_write.extend([
            home.join(".perlbrew"),
            home.join(".plenv"),
            home.join(".cpan"),
            home.join(".cpanm"),
            home.join(".perl-cpm"),
            home.join("perl5"),
            home.join(".local/lib/perl5"),
            home.join(".cpanrc"),
        ]);

        // PHP / Composer.
        read_write.extend([
            home.join(".composer"),
            home.join(".config/composer"),
            home.join(".cache/composer"),
            home.join(".local/share/composer"),
            home.join(".phpenv"),
            home.join(".config/php"),
            home.join(".cache/php"),
            home.join(".pearrc"),
        ]);

        // macOS-specific cache and application-support locations under ~/Library.
        #[cfg(target_os = "macos")]
        read_write.extend([
            // Rust
            home.join("Library/Caches/cargo"),
            home.join("Library/Caches/sccache"),
            home.join("Library/Application Support/Mozilla.sccache"),
            // Runtime managers
            home.join("Library/Caches/mise"),
            home.join("Library/Caches/asdf"),
            home.join("Library/Caches/pkgx"),
            home.join("Library/Packages"),
            // Node.js ecosystem
            home.join("Library/Caches/fnm"),
            home.join("Library/Application Support/fnm"),
            home.join("Library/Caches/npm"),
            home.join("Library/pnpm"),
            home.join("Library/Caches/pnpm"),
            home.join("Library/Preferences/pnpm"),
            home.join("Library/Caches/Yarn"),
            home.join("Library/Caches/node/corepack"),
            home.join("Library/Caches/ms-playwright"),
            home.join("Library/Caches/Cypress"),
            home.join("Library/Caches/typescript"),
            home.join("Library/Caches/prisma-nodejs"),
            home.join("Library/Caches/checkpoint-nodejs"),
            home.join("Library/Caches/turbo"),
            home.join("Library/Application Support/turborepo"),
            // Bun
            home.join("Library/Caches/bun"),
            // Deno
            home.join("Library/Caches/deno"),
            // Go
            home.join("Library/Caches/go-build"),
            home.join("Library/Caches/golangci-lint"),
            // Java
            home.join("Library/Java"),
            home.join("Library/Application Support/Coursier"),
            home.join("Library/Caches/Coursier"),
            // Python
            home.join("Library/Caches/uv"),
            home.join("Library/Caches/pip"),
            home.join("Library/Caches/pypoetry"),
            // Ruby
            home.join("Library/Caches/bundle"),
            // Perl
            home.join("Library/Caches/cpanm"),
            // PHP
            home.join("Library/Caches/composer"),
        ]);
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
                PathBuf::from("/System/Library"),
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

/// Push each existing directory and its canonical form into the target vector.
/// Both original and canonical paths are pushed so sandbox rules match
/// regardless of symlink resolution (e.g., /tmp and /private/tmp on macOS).
fn push_dirs_with_canonical(target: &mut Vec<PathBuf>, dirs: &[PathBuf]) {
    for dir in dirs {
        if dir.exists() {
            target.push(dir.clone());
            if let Ok(canonical) = dir.canonicalize() {
                target.push(canonical);
            }
        }
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

/// Detect the appropriate sandbox strategy for the current platform.
///
/// If sandboxing is expected on a supported platform but cannot be enforced,
/// return an error instead of silently falling back to unsandboxed execution.
// Linux detection is infallible, but macOS detection can fail closed.
#[allow(clippy::unnecessary_wraps)]
pub(super) fn detect_platform() -> Result<Option<Box<dyn SandboxStrategy>>, String> {
    #[cfg(target_os = "macos")]
    {
        if !std::path::Path::new("/usr/bin/sandbox-exec").exists() {
            return Err(
                "macOS sandbox unavailable: /usr/bin/sandbox-exec was not found. \
                 Set CAKE_SANDBOX=off to run Bash commands without filesystem sandboxing."
                    .to_string(),
            );
        }

        if !MacOsSandbox::can_apply_profile() {
            return Err(
                "macOS sandbox unavailable: sandbox-exec could not apply a Seatbelt profile \
                 in this process context. This commonly happens when cake is already running \
                 inside another sandbox. Set CAKE_SANDBOX=off to run Bash commands without \
                 filesystem sandboxing."
                    .to_string(),
            );
        }

        tracing::debug!("Using macOS sandbox-exec for filesystem sandboxing");
        Ok(Some(Box::new(MacOsSandbox)))
    }

    #[cfg(target_os = "linux")]
    {
        tracing::debug!("Using Linux Landlock LSM for filesystem sandboxing");
        Ok(Some(Box::new(LandlockSandbox)))
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        tracing::warn!(
            "No sandbox available for this platform ({}); bash commands will run unsandboxed",
            std::env::consts::OS
        );
        Ok(None)
    }
}

/// Check if sandboxing should be disabled via environment variable
pub(super) fn is_sandbox_disabled() -> bool {
    match std::env::var("CAKE_SANDBOX").as_deref() {
        Ok("off" | "0" | "false" | "no") => {
            tracing::warn!("Sandbox disabled via CAKE_SANDBOX environment variable");
            true
        },
        Ok("warn") => {
            tracing::warn!("Sandbox 'warn' mode requested; falling back to enforce mode");
            false
        },
        _ => false,
    }
}

/// Check whether this process can enforce the platform sandbox.
#[cfg(all(test, target_os = "macos"))]
pub(super) fn can_enforce_platform_sandbox() -> bool {
    std::path::Path::new("/usr/bin/sandbox-exec").exists() && MacOsSandbox::can_apply_profile()
}

#[cfg(test)]
mod tests {
    use crate::clients::tools::sandbox::SandboxConfig;
    use std::path::PathBuf;

    #[test]
    fn build_allows_fnm_runtime_manager_paths() {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return;
        };

        let config = SandboxConfig::build(std::path::Path::new("/workspace"));

        for expected in [
            home.join(".fnm"),
            home.join(".local/share/fnm"),
            home.join(".local/state/fnm"),
            home.join(".local/state/fnm_multishells"),
            home.join(".cache/fnm"),
        ] {
            assert!(
                config.read_write.contains(&expected),
                "expected read-write sandbox access for {}",
                expected.display()
            );
        }

        #[cfg(target_os = "macos")]
        for expected in [
            home.join("Library/Caches/fnm"),
            home.join("Library/Application Support/fnm"),
        ] {
            assert!(
                config.read_write.contains(&expected),
                "expected read-write sandbox access for {}",
                expected.display()
            );
        }
    }

    /// Smoke test that every major toolchain category from the safehouse
    /// reference profile is represented in the default sandbox config. This
    /// guards against accidentally regressing the broad-coverage approach the
    /// project relies on so cake works across many codebases.
    #[test]
    fn build_covers_common_toolchains() {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return;
        };

        let config = SandboxConfig::build(std::path::Path::new("/workspace"));

        let cross_platform_expected = [
            // Node ecosystem
            home.join(".npm"),
            home.join(".npmrc"),
            home.join(".config/configstore"),
            home.join(".node-gyp"),
            home.join(".pnpm-store"),
            home.join(".yarn"),
            home.join(".cache/node/corepack"),
            home.join(".nvm"),
            home.join(".cache/turbo"),
            // Bun / Deno
            home.join(".bun"),
            home.join(".deno"),
            // Go
            home.join("go"),
            home.join(".cache/go-build"),
            // Java / JVM
            home.join(".m2"),
            home.join(".gradle"),
            home.join(".sdkman"),
            home.join(".cache/coursier"),
            // Python
            home.join(".cache/uv"),
            home.join(".cache/pip"),
            home.join(".local/pipx"),
            home.join(".pyenv"),
            home.join(".cache/ruff"),
            home.join(".cache/mypy"),
            home.join(".virtualenvs"),
            home.join(".conda"),
            // Ruby
            home.join(".rbenv"),
            home.join(".bundle"),
            home.join(".gem"),
            // Perl / PHP
            home.join(".cpanm"),
            home.join(".composer"),
            // Runtime managers
            home.join(".proto"),
            home.join(".pkgx"),
        ];

        for expected in cross_platform_expected {
            assert!(
                config.read_write.contains(&expected),
                "expected read-write sandbox access for {}",
                expected.display()
            );
        }

        #[cfg(target_os = "macos")]
        for expected in [
            home.join("Library/Caches/npm"),
            home.join("Library/pnpm"),
            home.join("Library/Caches/Yarn"),
            home.join("Library/Caches/bun"),
            home.join("Library/Caches/deno"),
            home.join("Library/Caches/go-build"),
            home.join("Library/Java"),
            home.join("Library/Caches/Coursier"),
            home.join("Library/Caches/uv"),
            home.join("Library/Caches/bundle"),
            home.join("Library/Caches/composer"),
        ] {
            assert!(
                config.read_write.contains(&expected),
                "expected read-write sandbox access for {}",
                expected.display()
            );
        }
    }
}
