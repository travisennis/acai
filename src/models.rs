use core::fmt;

use crate::macros::impl_enum_string_serialization;

#[derive(Debug, Clone, Copy)]
pub enum Model {
    GPT4Turbo,
    GPT3Turbo,
}

impl_enum_string_serialization!(
    Model,
    GPT4Turbo => "gpt-4-turbo",
    GPT3Turbo => "gpt-3-turbo"
);

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Model::GPT4Turbo => write!(f, "GPT-4-Turbo"),
            Model::GPT3Turbo => write!(f, "GPT-3-Turbo"),
        }
    }
}
