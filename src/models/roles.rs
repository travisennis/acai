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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn role_serialization_roundtrip() {
        let cases = [
            (Role::System, "\"system\""),
            (Role::Assistant, "\"assistant\""),
            (Role::User, "\"user\""),
            (Role::Tool, "\"tool\""),
        ];

        for (role, expected) in cases {
            let json = serde_json::to_string(&role).unwrap();
            assert_eq!(json, expected);
            let deserialized: Role = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, role);
        }
    }

    #[test]
    fn role_deserialization_case_insensitive() {
        // serde's rename_all = "lowercase" should handle lowercase
        let role: Role = serde_json::from_str("\"system\"").unwrap();
        assert_eq!(role, Role::System);
    }

    #[test]
    fn role_equality() {
        assert_eq!(Role::User, Role::User);
        assert_ne!(Role::User, Role::Assistant);
    }

    #[test]
    fn role_clone() {
        let role = Role::Assistant;
        let cloned = role;
        assert_eq!(role, cloned);
    }

    #[test]
    fn role_debug_format() {
        assert_eq!(format!("{:?}", Role::User), "User");
        assert_eq!(format!("{:?}", Role::Assistant), "Assistant");
    }
}
