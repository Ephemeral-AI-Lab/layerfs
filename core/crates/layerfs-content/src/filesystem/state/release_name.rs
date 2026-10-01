//! Complete checked inline continuation names for external release frames.
use crate::filesystem::PathName;
use crate::{ContentError, ContentResult};
/// One full maximum255-byte UTF-8 component without a heap owner or truncation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReleaseName {
    bytes: [u8; 255],
    length: u16,
}
impl ReleaseName {
    /// Check the same canonical component grammar as PathName, without allocation.
    pub fn new(value: &str) -> ContentResult<Self> {
        Self::from_bytes(value.as_bytes())
    }
    /// Capture every byte after validating the canonical component grammar.
    pub fn from_bytes(value: &[u8]) -> ContentResult<Self> {
        crate::filesystem::path::validate_name(value)?;
        let mut bytes = [0; 255];
        bytes[..value.len()].copy_from_slice(value);
        Ok(Self {
            bytes,
            length: value.len() as u16,
        })
    }
    /// Copy a previously checked PathName without cloning its heap allocation.
    pub fn from_path_name(value: &PathName) -> Self {
        let mut bytes = [0; 255];
        bytes[..value.as_bytes().len()].copy_from_slice(value.as_bytes());
        Self {
            bytes,
            length: value.as_bytes().len() as u16,
        }
    }
    /// Exact original bytes; unused inline bytes remain canonical zero padding.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.length)]
    }
    /// Checked original UTF-8 component.
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(self.as_bytes()).expect("checked inline release name")
    }
    /// Restore one existing listing input; its caller credits the full255 capacity first.
    pub fn to_path_name(&self) -> ContentResult<PathName> {
        let name = PathName::from_bytes(self.as_bytes())?;
        if name.retained_capacity() > 255 {
            return Err(ContentError::InvalidOrderingRecord(
                "release name actual capacity",
            ));
        }
        Ok(name)
    }
}
impl Ord for ReleaseName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_bytes().cmp(other.as_bytes())
    }
}
impl PartialOrd for ReleaseName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
