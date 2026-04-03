//! Linux sandbox implementation using Landlock LSM
//!
//! Landlock is a Linux Security Module available since kernel 5.13 that
//! allows unprivileged processes to sandbox themselves.
//!
//! This implementation uses `CommandExt::pre_exec` to apply Landlock rules
//! in the child process after `fork()` but before `exec()`.

use crate::clients::tools::sandbox::{SandboxConfig, SandboxStrategy};

/// Linux sandbox strategy using Landlock LSM
#[derive(Debug, Clone, Copy)]
pub struct LandlockSandbox;

impl LandlockSandbox {
    /// Apply Landlock rules in the current process (to be called in `pre_exec`)
    #[cfg(feature = "landlock")]
    fn apply_landlock_rules(config: &SandboxConfig) -> Result<(), std::io::Error> {
        use landlock::{
            ABI, Access, AccessFs, Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetStatus,
        };

        let abi = ABI::V5;

        let mut ruleset = Ruleset::default()
            .handle_access(AccessFs::from_all(abi))
            .map_err(|e| std::io::Error::other(format!("Failed to configure ruleset access: {e}")))?
            .create()
            .map_err(|e| {
                std::io::Error::other(format!("Failed to create Landlock ruleset: {e}"))
            })?;

        // Add read-write rules for cwd and temp dirs
        let rw_access = AccessFs::from_all(abi);
        for path in &config.read_write {
            if path.exists() {
                ruleset = ruleset
                    .add_rules(landlock::path_beneath_rules(&[path], rw_access))
                    .map_err(|e| {
                        std::io::Error::other(format!(
                            "Failed to add rw rule for {}: {e}",
                            path.display()
                        ))
                    })?;
            }
        }

        // Add read-only + exec rules for system paths
        let ro_exec_access = AccessFs::ReadFile | AccessFs::ReadDir | AccessFs::Execute;
        for path in &config.read_only_exec {
            if path.exists() {
                ruleset = ruleset
                    .add_rules(landlock::path_beneath_rules(&[path], ro_exec_access))
                    .map_err(|e| {
                        std::io::Error::other(format!(
                            "Failed to add ro+exec rule for {}: {e}",
                            path.display()
                        ))
                    })?;
            }
        }

        // Add read-only rules
        let read_access = AccessFs::ReadFile | AccessFs::ReadDir;
        for path in &config.read_only {
            if path.exists() {
                ruleset = ruleset
                    .add_rules(landlock::path_beneath_rules(&[path], read_access))
                    .map_err(|e| {
                        std::io::Error::other(format!(
                            "Failed to add ro rule for {}: {e}",
                            path.display()
                        ))
                    })?;
            }
        }

        let status = ruleset.restrict_self().map_err(|e| {
            std::io::Error::other(format!("Failed to restrict process with Landlock: {e}"))
        })?;

        match status.ruleset {
            RulesetStatus::FullyEnforced
            | RulesetStatus::PartiallyEnforced
            | RulesetStatus::NotEnforced => {
                // Can't use log in pre_exec (async-signal-unsafe), just continue
            },
        }

        Ok(())
    }
}

impl SandboxStrategy for LandlockSandbox {
    #[allow(unused_variables)]
    fn apply(
        &self,
        command: &mut tokio::process::Command,
        config: &SandboxConfig,
    ) -> Result<(), String> {
        #[cfg(feature = "landlock")]
        {
            let config = config.clone();
            unsafe {
                command.pre_exec(move || Self::apply_landlock_rules(&config));
            }
        }

        #[cfg(not(feature = "landlock"))]
        {
            let _ = config;
            tracing::warn!(
                "Landlock feature not enabled during compilation; \
                 bash commands will run without filesystem sandboxing. \
                 Rebuild with --features landlock to enable."
            );
        }

        Ok(())
    }
}
