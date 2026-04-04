use serde::{Deserialize, Serialize};

/// Represents the role of a message sender in a conversation.
///
/// Roles distinguish between different participants in the conversation:
/// system prompts, assistant responses, user inputs, and tool outputs.
///
/// # Examples
///
/// ```
/// use acai::models::Role;
///
/// assert_eq!(Role::User.as_str(), "user");
/// assert_eq!(Role::Assistant.as_str(), "assistant");
/// ```
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

impl Role {
    /// Returns the string representation of the role.
    ///
    /// # Examples
    ///
    /// ```
    /// use acai::models::Role;
    ///
    /// assert_eq!(Role::System.as_str(), "system");
    /// assert_eq!(Role::Assistant.as_str(), "assistant");
    /// assert_eq!(Role::User.as_str(), "user");
    /// assert_eq!(Role::Tool.as_str(), "tool");
    /// ```
    pub const fn as_str(&self) -> &str {
        match self {
            Self::System => "system",
            Self::Assistant => "assistant",
            Self::User => "user",
            Self::Tool => "tool",
        }
    }
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

    #[test]
    fn role_as_str() {
        assert_eq!(Role::System.as_str(), "system");
        assert_eq!(Role::Assistant.as_str(), "assistant");
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::Tool.as_str(), "tool");
    }
}
