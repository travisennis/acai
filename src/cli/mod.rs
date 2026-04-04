//! CLI command runner interface.
//!
//! This module defines the trait for commands that can be executed by the CLI,
//! enabling different command implementations to share common setup and teardown logic.

mod cmd_runner;

#[doc(inline)]
pub use cmd_runner::CmdRunner;
