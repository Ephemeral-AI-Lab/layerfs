use std::fmt::{Display, Formatter};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }
    };
}

id_type!(LayerStackId);
id_type!(LayerId);
id_type!(BranchId);
id_type!(CommitId);
id_type!(WorkspaceId);
id_type!(ExecutionId);
id_type!(OperationId);
id_type!(ObjectId);
id_type!(ConflictId);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityName(String);

impl EntityName {
    pub fn parse(value: impl Into<String>) -> Result<Self, &'static str> {
        let value = value.into();
        let bytes = value.as_bytes();
        let valid_edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
        if !(1..=63).contains(&bytes.len())
            || !valid_edge(bytes[0])
            || !valid_edge(bytes[bytes.len() - 1])
            || !bytes.iter().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
        {
            return Err("name must be a 1-63 character lowercase ASCII slug");
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for EntityName {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::EntityName;

    #[test]
    fn validates_shared_names() {
        for valid in ["a", "api-server", "release.2026", "eval_v2"] {
            assert!(EntityName::parse(valid).is_ok(), "{valid}");
        }
        for invalid in ["", "Main", "-bad", "bad-", "feature/x", "two words"] {
            assert!(EntityName::parse(invalid).is_err(), "{invalid}");
        }
        assert!(EntityName::parse("a".repeat(63)).is_ok());
        assert!(EntityName::parse("a".repeat(64)).is_err());
    }
}
