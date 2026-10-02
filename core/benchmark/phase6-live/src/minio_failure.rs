//! Observed upload chronology, independent of object grammar or workload names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UploadFailure {
    /// No HTTP request was sent, or a complete explicit rejection was received.
    Definite(String),
    /// Request transmission started without a complete definite acknowledgment.
    Unknown(String),
}
impl UploadFailure {
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown(_))
    }
}
impl std::fmt::Display for UploadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Definite(e) => write!(f, "definite provider refusal: {e}"),
            Self::Unknown(e) => write!(f, "unknown provider custody: {e}"),
        }
    }
}
impl std::error::Error for UploadFailure {}
impl From<&str> for UploadFailure {
    fn from(value: &str) -> Self {
        Self::Unknown(value.into())
    }
}
impl From<String> for UploadFailure {
    fn from(value: String) -> Self {
        Self::Unknown(value)
    }
}
