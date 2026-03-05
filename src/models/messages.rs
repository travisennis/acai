use serde::{Deserialize, Serialize};

use super::Role;

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
