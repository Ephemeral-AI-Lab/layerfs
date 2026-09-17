//! Canonical names and paths: byte grammar, ordering and declared bounds.
//!
//! A name is one nonempty, at-most-255-byte UTF-8 component without NUL, `/` or
//! `\`, and never `.` or `..`. A path is either the empty root or a sequence of
//! such components joined by single separators, at most 4,096 bytes and 256
//! components. Ordering is byte order, which is what the sorted tree engine
//! requires and what the reference uses.

use std::cmp::Ordering;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::limits::{MAXIMUM_NAME_BYTES, MAXIMUM_PATH_BYTES, MAXIMUM_PATH_COMPONENTS};

/// One checked path component.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PathName {
    bytes: Vec<u8>,
}

impl PathName {
    /// Checks and owns one name.
    pub fn new(value: &str) -> ContentResult<Self> {
        Self::from_bytes(value.as_bytes())
    }

    /// Checks and owns one raw name.
    pub fn from_bytes(bytes: &[u8]) -> ContentResult<Self> {
        validate_name(bytes)?;
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    /// Raw bytes of the name.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The name as text; construction proved it is UTF-8.
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes).map_or("", |value| value)
    }
}

impl Ord for PathName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.bytes.cmp(&other.bytes)
    }
}

impl PartialOrd for PathName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl TryFrom<&str> for PathName {
    type Error = ContentError;

    fn try_from(value: &str) -> ContentResult<Self> {
        Self::new(value)
    }
}

/// One checked canonical path.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LogicalPath {
    bytes: Vec<u8>,
}

impl LogicalPath {
    /// Checks and owns one path.
    pub fn new(value: &str) -> ContentResult<Self> {
        Self::from_bytes(value.as_bytes())
    }

    /// Checks and owns one raw path.
    pub fn from_bytes(bytes: &[u8]) -> ContentResult<Self> {
        validate_path(bytes)?;
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    /// The empty root path.
    pub fn root() -> Self {
        Self { bytes: Vec::new() }
    }

    /// True for the root path.
    pub fn is_root(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Raw bytes of the path.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The path as text; construction proved it is UTF-8.
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes).map_or("", |value| value)
    }

    /// Number of components.
    pub fn component_count(&self) -> usize {
        if self.is_root() {
            0
        } else {
            self.bytes.iter().filter(|byte| **byte == b'/').count() + 1
        }
    }

    /// Components in order.
    pub fn components(&self) -> impl Iterator<Item = &[u8]> {
        self.bytes
            .split(|byte| *byte == b'/')
            .filter(|component| !component.is_empty())
    }

    /// Splits off the final component, returning the parent path and the name.
    ///
    /// Both halves are prefixes of an already checked path, so they satisfy the
    /// same grammar and are built without repeating the validation.
    pub fn split_last(&self) -> Option<(Self, PathName)> {
        if self.is_root() {
            return None;
        }
        let separator = self.bytes.iter().rposition(|byte| *byte == b'/');
        let (parent, name) = separator.map_or((&[][..], &self.bytes[..]), |index| {
            (&self.bytes[..index], &self.bytes[index + 1..])
        });
        Some((
            Self {
                bytes: parent.to_vec(),
            },
            PathName {
                bytes: name.to_vec(),
            },
        ))
    }

    /// Joins one checked name onto this path.
    pub fn join(&self, name: &PathName) -> Self {
        let mut bytes = Vec::with_capacity(self.bytes.len() + 1 + name.bytes.len());
        bytes.extend_from_slice(&self.bytes);
        if !self.bytes.is_empty() {
            bytes.push(b'/');
        }
        bytes.extend_from_slice(&name.bytes);
        Self { bytes }
    }
}

fn validate_path(bytes: &[u8]) -> ContentResult<()> {
    if bytes.len() > MAXIMUM_PATH_BYTES {
        return Err(ContentError::PathLimitExceeded);
    }
    if bytes.is_empty() {
        return Ok(());
    }
    if std::str::from_utf8(bytes).is_err() {
        return Err(ContentError::InvalidUtf8);
    }
    let mut components = 0_usize;
    for component in bytes.split(|byte| *byte == b'/') {
        validate_name(component)?;
        components = components
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        if components > MAXIMUM_PATH_COMPONENTS {
            return Err(ContentError::PathLimitExceeded);
        }
    }
    Ok(())
}

fn validate_name(bytes: &[u8]) -> ContentResult<()> {
    if bytes.is_empty() {
        return Err(ContentError::InvalidPath);
    }
    if bytes.len() > MAXIMUM_NAME_BYTES {
        return Err(ContentError::PathLimitExceeded);
    }
    if std::str::from_utf8(bytes).is_err() {
        return Err(ContentError::InvalidUtf8);
    }
    if bytes == b"."
        || bytes == b".."
        || bytes.contains(&0)
        || bytes.contains(&b'/')
        || bytes.contains(&b'\\')
    {
        return Err(ContentError::InvalidPath);
    }
    Ok(())
}
