//! Configuration management for acai.
//!
//! This module provides configuration loading, session management, and data
//! directory handling for the acai CLI. Configuration is loaded from TOML files
//! and can be overridden via command-line arguments.
//!
//! # Key Types
//!
//! - [`DataDir`] - Manages the data directory for session storage
//! - [`Session`] - Represents a conversation session
//! - [`ModelConfig`] - Model provider configuration
//! - [`SettingsLoader`] - Loads settings from TOML files

mod data_dir;
pub mod defaults;
pub mod model;
pub mod session;
pub mod settings;
pub mod worktree;

#[doc(inline)]
pub use data_dir::{AgentsFile, DataDir};
#[doc(inline)]
pub use model::{ModelConfig, ResolvedModelConfig};
#[doc(inline)]
pub use session::Session;
#[doc(inline)]
pub use settings::{ModelDefinition, SettingsLoader};
