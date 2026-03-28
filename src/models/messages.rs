use serde::{Deserialize, Serialize};

use crate::models::Role;

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn message_serialization_roundtrip() {
        let msg = Message {
            role: Role::User,
            content: "Hello".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.content, "Hello");
        assert_eq!(deserialized.role, Role::User);
    }

    #[test]
    fn message_serialization_format() {
        let msg = Message {
            role: Role::Assistant,
            content: "Response".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"assistant\""));
        assert!(json.contains("\"content\":\"Response\""));
    }

    #[test]
    fn message_clone() {
        let msg = Message {
            role: Role::User,
            content: "test".to_string(),
        };
        let cloned = msg.clone();
        assert_eq!(msg.role, cloned.role);
        assert_eq!(msg.content, cloned.content);
    }

    #[test]
    fn message_debug_format() {
        let msg = Message {
            role: Role::User,
            content: "test".to_string(),
        };
        let debug = format!("{msg:?}");
        assert!(debug.contains("User"));
        assert!(debug.contains("test"));
    }

    #[test]
    fn message_with_system_role() {
        let msg = Message {
            role: Role::System,
            content: "You are helpful".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, Role::System);
    }

    #[test]
    fn message_with_tool_role() {
        let msg = Message {
            role: Role::Tool,
            content: "Tool output".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, Role::Tool);
    }

    #[test]
    fn message_empty_content() {
        let msg = Message {
            role: Role::User,
            content: String::new(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert!(deserialized.content.is_empty());
    }

    #[test]
    fn message_multiline_content() {
        let msg = Message {
            role: Role::User,
            content: "line1\nline2\nline3".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.content, "line1\nline2\nline3");
    }
}
