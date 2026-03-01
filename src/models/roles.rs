use serde::{Deserialize, Serialize};

/// Represents the role who sends the message.
///
/// This enum is used to distinguish between different types of roles,
/// such as system, assistant, user, and tool. The roles are serialized
/// and deserialized as lowercase strings.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Represents a system role.
    System,
    /// Represents an assistant.
    Assistant,
    /// Represents a user.
    User,
    /// Represents a tool result.
    Tool,
}
