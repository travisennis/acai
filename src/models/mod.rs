//! Message and role types for conversation handling.
//!
//! This module defines the core types used to represent conversation messages
//! between the user, assistant, and system in the cake CLI.
//!
//! # Key Types
//!
//! - [`Message`] - A single message with a role and content
//! - [`Role`] - The role of a message sender (User, Assistant, System)

mod messages;
mod roles;

#[doc(inline)]
pub use messages::Message;
#[doc(inline)]
pub use roles::Role;
