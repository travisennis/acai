/// Implements [`serde::Serialize`] and [`serde::Deserialize`] for an enum with corresponding string variants.
///
/// ## Arguments
/// - `$enum_name`: The name of the enum.
/// - `$($variant:ident => $str:expr),*`: The variants of the enum and their corresponding string representations.
macro_rules! impl_enum_string_serialization {
    ($enum_name:ident, $($variant:ident => $str:expr),*) => {
        impl serde::Serialize for $enum_name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                match self {
                    $(
                        $enum_name::$variant => serializer.serialize_str($str),
                    )*
                }
            }
        }

        impl<'de> serde::Deserialize<'de> for $enum_name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct EnumVisitor;

                impl<'de> serde::de::Visitor<'de> for EnumVisitor {
                    type Value = $enum_name;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str(concat!("a string representing a ", stringify!($enum_name)))
                    }

                    fn visit_str<E>(self, value: &str) -> Result<$enum_name, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            $(
                                $str => Ok($enum_name::$variant),
                            )*
                            _ => Err(serde::de::Error::custom("invalid value for enum")),
                        }
                    }
                }

                deserializer.deserialize_str(EnumVisitor)
            }
        }
    };
}

pub(crate) use impl_enum_string_serialization;
