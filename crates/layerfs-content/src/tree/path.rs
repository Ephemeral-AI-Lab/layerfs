use std::cmp::Ordering;

use crate::error::{CoreError, CoreResult};
use crate::limits::{
    MAX_COMPONENT_BYTES, MAX_ENCODED_STRING_BYTES, MAX_PATH_BYTES, MAX_PATH_COMPONENTS,
};

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct CanonicalPath {
    bytes: Vec<u8>,
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct CanonicalName {
    bytes: Vec<u8>,
}

impl CanonicalName {
    pub fn new(value: &str) -> CoreResult<Self> {
        Self::from_bytes(value.as_bytes())
    }

    pub fn from_bytes(bytes: &[u8]) -> CoreResult<Self> {
        validate_name(bytes)?;
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes).map_or("", |value| value)
    }
}

impl std::fmt::Debug for CanonicalName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("CanonicalName")
            .field(&self.as_str())
            .finish()
    }
}

impl Ord for CanonicalName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.bytes.cmp(&other.bytes)
    }
}

impl PartialOrd for CanonicalName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl TryFrom<&str> for CanonicalName {
    type Error = CoreError;

    fn try_from(value: &str) -> CoreResult<Self> {
        Self::new(value)
    }
}

impl TryFrom<&[u8]> for CanonicalName {
    type Error = CoreError;

    fn try_from(value: &[u8]) -> CoreResult<Self> {
        Self::from_bytes(value)
    }
}

impl CanonicalPath {
    pub fn new(value: &str) -> CoreResult<Self> {
        Self::from_bytes(value.as_bytes())
    }

    pub fn from_bytes(bytes: &[u8]) -> CoreResult<Self> {
        validate(bytes)?;
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    pub fn root() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn is_root(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn as_str(&self) -> &str {
        // Construction validates UTF-8, so this conversion cannot fail.
        std::str::from_utf8(&self.bytes).map_or("", |value| value)
    }

    pub fn component_count(&self) -> usize {
        if self.is_root() {
            0
        } else {
            self.bytes.iter().filter(|&&byte| byte == b'/').count() + 1
        }
    }

    pub fn components(&self) -> impl Iterator<Item = &[u8]> {
        self.bytes
            .split(|&byte| byte == b'/')
            .filter(|component| !component.is_empty())
    }
}

impl std::fmt::Debug for CanonicalPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("CanonicalPath")
            .field(&self.as_str())
            .finish()
    }
}

impl Ord for CanonicalPath {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_paths(self, other)
    }
}

impl PartialOrd for CanonicalPath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl TryFrom<&str> for CanonicalPath {
    type Error = CoreError;

    fn try_from(value: &str) -> CoreResult<Self> {
        Self::new(value)
    }
}

impl TryFrom<&[u8]> for CanonicalPath {
    type Error = CoreError;

    fn try_from(value: &[u8]) -> CoreResult<Self> {
        Self::from_bytes(value)
    }
}

fn validate(bytes: &[u8]) -> CoreResult<()> {
    if bytes.len() > MAX_PATH_BYTES || bytes.len() > MAX_ENCODED_STRING_BYTES {
        return Err(CoreError::PathLimitExceeded);
    }
    if bytes.is_empty() {
        return Ok(());
    }
    if std::str::from_utf8(bytes).is_err() {
        return Err(CoreError::InvalidUtf8);
    }
    let mut components = 0_usize;
    for component in bytes.split(|&byte| byte == b'/') {
        validate_name(component)?;
        components = components.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        if components > MAX_PATH_COMPONENTS {
            return Err(CoreError::PathLimitExceeded);
        }
    }
    Ok(())
}

fn validate_name(bytes: &[u8]) -> CoreResult<()> {
    if bytes.is_empty() || bytes.len() > MAX_COMPONENT_BYTES {
        return if bytes.is_empty() {
            Err(CoreError::InvalidPath)
        } else {
            Err(CoreError::PathLimitExceeded)
        };
    }
    if std::str::from_utf8(bytes).is_err() {
        return Err(CoreError::InvalidUtf8);
    }
    if bytes == b"."
        || bytes == b".."
        || bytes.contains(&0)
        || bytes.contains(&b'/')
        || bytes.contains(&b'\\')
    {
        return Err(CoreError::InvalidPath);
    }
    Ok(())
}

pub fn compare_paths(left: &CanonicalPath, right: &CanonicalPath) -> Ordering {
    let mut left_components = left.components();
    let mut right_components = right.components();
    loop {
        match (left_components.next(), right_components.next()) {
            (Some(left), Some(right)) => match left.cmp(right) {
                Ordering::Equal => {}
                ordering => return ordering,
            },
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => return Ordering::Equal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_root_and_canonical_paths() {
        let root = CanonicalPath::root();
        assert!(root.is_root());
        assert_eq!(root.component_count(), 0);
        assert_eq!(root.components().count(), 0);
        let path = CanonicalPath::new("a/é/file").unwrap();
        assert_eq!(path.as_bytes(), "a/é/file".as_bytes());
        assert_eq!(path.component_count(), 3);
        assert_eq!(CanonicalName::new("file").unwrap().as_str(), "file");
    }

    #[test]
    fn rejects_invalid_paths() {
        for value in ["/a", "a/", "a//b", "a/./b", "a/../b", "a\\b", "a\0b"] {
            assert_eq!(CanonicalPath::new(value), Err(CoreError::InvalidPath));
        }
        assert_eq!(
            CanonicalPath::from_bytes(&[0xff]),
            Err(CoreError::InvalidUtf8)
        );
        for value in ["", ".", "..", "a/b"] {
            assert_eq!(CanonicalName::new(value), Err(CoreError::InvalidPath));
        }
        assert_eq!(
            CanonicalName::from_bytes(&[0xff]),
            Err(CoreError::InvalidUtf8)
        );
    }

    #[test]
    fn orders_parents_and_components_by_bytes() {
        let mut paths = [
            CanonicalPath::new("b").unwrap(),
            CanonicalPath::new("a/child").unwrap(),
            CanonicalPath::new("a").unwrap(),
            CanonicalPath::new("a/file").unwrap(),
        ];
        paths.sort();
        let values: Vec<&str> = paths.iter().map(CanonicalPath::as_str).collect();
        assert_eq!(values, ["a", "a/child", "a/file", "b"]);
    }

    #[test]
    fn enforces_path_bounds() {
        let maximum_name = "x".repeat(MAX_COMPONENT_BYTES);
        assert_eq!(
            CanonicalName::new(&maximum_name).unwrap().as_bytes().len(),
            MAX_COMPONENT_BYTES
        );
        let oversized_name = "x".repeat(MAX_COMPONENT_BYTES + 1);
        assert_eq!(
            CanonicalName::new(&oversized_name),
            Err(CoreError::PathLimitExceeded)
        );

        let exact_depth = std::iter::repeat_n("x", MAX_PATH_COMPONENTS)
            .collect::<Vec<_>>()
            .join("/");
        assert_eq!(
            CanonicalPath::new(&exact_depth).unwrap().component_count(),
            MAX_PATH_COMPONENTS
        );
        let too_deep = format!("{exact_depth}/x");
        assert_eq!(
            CanonicalPath::new(&too_deep),
            Err(CoreError::PathLimitExceeded)
        );

        let mut exact_length_components = vec!["x".repeat(MAX_COMPONENT_BYTES); 15];
        exact_length_components.push("x".repeat(MAX_COMPONENT_BYTES - 1));
        exact_length_components.push("x".to_owned());
        let exact_length = exact_length_components.join("/");
        assert_eq!(exact_length.len(), MAX_PATH_BYTES);
        assert_eq!(
            CanonicalPath::new(&exact_length).unwrap().as_bytes().len(),
            MAX_PATH_BYTES
        );
        let too_long = format!("{exact_length}x");
        assert_eq!(
            CanonicalPath::new(&too_long),
            Err(CoreError::PathLimitExceeded)
        );
    }
}
