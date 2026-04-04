//! Client implementations for AI model providers.
//!
//! This module contains the [`Agent`] orchestrator, tool definitions, and
//! API-specific request/response handling for interacting with AI backends.
//!
//! # Architecture
//!
//! - [`Agent`] - Main orchestrator that manages conversation loops and tool execution
//! - `tools` - Tool definitions for Bash, Read, Edit, and Write operations
//! - `chat_completions` / `responses` - API-specific request handlers
//!
//! # Example
//!
//! ```no_run
//! use acai::clients::Agent;
//! // Create an agent with a resolved model config and system prompt
//! // let agent = Agent::new(config, "You are a helpful assistant.");
//! ```

mod agent;
mod chat_completions;
mod chat_types;
mod responses;
mod tools;
mod types;

#[doc(inline)]
pub use agent::Agent;
#[doc(inline)]
pub use tools::{set_additional_dirs, summarize_tool_args};
#[doc(inline)]
pub use types::ConversationItem;
