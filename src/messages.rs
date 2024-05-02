use serde::{Deserialize, Serialize};

/// Represents the role who sends the message.
///
/// This enum is used to distinguish between different types of roles,
/// such as system, assistant, and user. The roles are serialized
/// and deserialized as lowercase strings.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Represents a system role.
    System,
    /// Represents an assistant.
    Assistant,
    /// Represents a user.
    User,
}

/// A structure representing a message.
///
/// This struct encapsulates a message, associating it with a specific role
/// to indicate the sender's role.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    /// The role associated with this message, indicating the sender.
    pub role: Role,
    /// The content of the message as a string.
    pub content: String,
}
