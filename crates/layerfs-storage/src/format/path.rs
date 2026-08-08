//! OD-01 portable path, component, and symlink-target validation.

use core::cmp::Ordering;

use crate::error::{CoreError, CoreResult};

pub const MAX_COMPONENT_BYTES: usize = 255;
pub const MAX_PATH_BYTES: usize = 4_096;
pub const MAX_PATH_DEPTH: usize = 256;
pub const MAX_SYMLINK_TARGET_BYTES: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedComponent<'a>(&'a [u8]);

impl<'a> ValidatedComponent<'a> {
    pub fn new(bytes: &'a [u8]) -> CoreResult<Self> {
        validate_component(bytes)?;
        Ok(Self(bytes))
    }

    pub const fn as_bytes(self) -> &'a [u8] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedPath<'a> {
    bytes: &'a [u8],
    depth: u16,
}

impl<'a> ValidatedPath<'a> {
    pub fn new(bytes: &'a [u8]) -> CoreResult<Self> {
        let depth = validate_path(bytes)?;
        Ok(Self { bytes, depth })
    }

    pub const fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub const fn depth(self) -> u16 {
        self.depth
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedSymlinkTarget<'a>(&'a [u8]);

impl<'a> ValidatedSymlinkTarget<'a> {
    pub fn new(bytes: &'a [u8]) -> CoreResult<Self> {
        validate_symlink_target(bytes)?;
        Ok(Self(bytes))
    }

    pub const fn as_bytes(self) -> &'a [u8] {
        self.0
    }
}

/// Incremental OD-01 whole-path predicate with O(1) retained state.
///
/// Each byte is validated exactly once. Component-local state resets at `/`,
/// and a failure is returned while consuming its first decisive byte or the
/// final end marker; no later byte is inspected by this state machine.
#[derive(Clone, Copy, Debug)]
pub struct PathValidator {
    expected_len: usize,
    bytes_seen: usize,
    depth: usize,
    component_len: usize,
    dot_state: DotState,
    utf8_remaining: u8,
    next_min: u8,
    next_max: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DotState {
    Empty,
    One,
    Two,
    Other,
}

impl PathValidator {
    pub fn new(expected_len: usize) -> CoreResult<Self> {
        if expected_len == 0 || expected_len > MAX_PATH_BYTES {
            return Err(CoreError::Path);
        }
        Ok(Self {
            expected_len,
            bytes_seen: 0,
            depth: 0,
            component_len: 0,
            dot_state: DotState::Empty,
            utf8_remaining: 0,
            next_min: 0x80,
            next_max: 0xbf,
        })
    }

    pub const fn bytes_seen(&self) -> usize {
        self.bytes_seen
    }

    pub fn push(&mut self, byte: u8) -> CoreResult<()> {
        if self.bytes_seen == self.expected_len {
            return Err(CoreError::TrailingBytes);
        }
        self.bytes_seen = self
            .bytes_seen
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;

        if byte == b'/' {
            if self.utf8_remaining != 0 {
                return Err(CoreError::Path);
            }
            self.finish_component()?;
            self.component_len = 0;
            self.dot_state = DotState::Empty;
            return Ok(());
        }

        self.component_len = self
            .component_len
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        if self.component_len > MAX_COMPONENT_BYTES || byte == 0 {
            return Err(CoreError::Path);
        }

        self.advance_dot_state(byte);
        self.advance_utf8(byte)
    }

    pub fn finish(mut self) -> CoreResult<u16> {
        if self.bytes_seen < self.expected_len {
            return Err(CoreError::Truncated);
        }
        if self.utf8_remaining != 0 {
            return Err(CoreError::Path);
        }
        self.finish_component()?;
        u16::try_from(self.depth).map_err(|_| CoreError::IntegerOverflow)
    }

    fn finish_component(&mut self) -> CoreResult<()> {
        if self.component_len == 0 || matches!(self.dot_state, DotState::One | DotState::Two) {
            return Err(CoreError::Path);
        }
        self.depth = self
            .depth
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        if self.depth > MAX_PATH_DEPTH {
            return Err(CoreError::Path);
        }
        Ok(())
    }

    fn advance_dot_state(&mut self, byte: u8) {
        self.dot_state = match (self.dot_state, byte) {
            (DotState::Empty, b'.') => DotState::One,
            (DotState::One, b'.') => DotState::Two,
            _ => DotState::Other,
        };
    }

    fn advance_utf8(&mut self, byte: u8) -> CoreResult<()> {
        if self.utf8_remaining != 0 {
            if !(self.next_min..=self.next_max).contains(&byte) {
                return Err(CoreError::Path);
            }
            self.utf8_remaining -= 1;
            self.next_min = 0x80;
            self.next_max = 0xbf;
            return Ok(());
        }

        match byte {
            0x01..=0x7f => Ok(()),
            0xc2..=0xdf => {
                self.start_utf8(1, 0x80, 0xbf);
                Ok(())
            }
            0xe0 => {
                self.start_utf8(2, 0xa0, 0xbf);
                Ok(())
            }
            0xe1..=0xec | 0xee..=0xef => {
                self.start_utf8(2, 0x80, 0xbf);
                Ok(())
            }
            0xed => {
                self.start_utf8(2, 0x80, 0x9f);
                Ok(())
            }
            0xf0 => {
                self.start_utf8(3, 0x90, 0xbf);
                Ok(())
            }
            0xf1..=0xf3 => {
                self.start_utf8(3, 0x80, 0xbf);
                Ok(())
            }
            0xf4 => {
                self.start_utf8(3, 0x80, 0x8f);
                Ok(())
            }
            _ => Err(CoreError::Path),
        }
    }

    fn start_utf8(&mut self, remaining: u8, next_min: u8, next_max: u8) {
        self.utf8_remaining = remaining;
        self.next_min = next_min;
        self.next_max = next_max;
    }
}

pub fn validate_component(bytes: &[u8]) -> CoreResult<()> {
    if bytes.is_empty()
        || bytes.len() > MAX_COMPONENT_BYTES
        || bytes == b"."
        || bytes == b".."
        || bytes.contains(&0)
        || bytes.contains(&b'/')
        || core::str::from_utf8(bytes).is_err()
    {
        return Err(CoreError::Name);
    }
    Ok(())
}

pub fn validate_path(bytes: &[u8]) -> CoreResult<u16> {
    let mut validator = PathValidator::new(bytes.len())?;
    for &byte in bytes {
        validator.push(byte)?;
    }
    validator.finish()
}

pub fn validate_symlink_target(bytes: &[u8]) -> CoreResult<()> {
    if bytes.is_empty()
        || bytes.len() > MAX_SYMLINK_TARGET_BYTES
        || bytes.contains(&0)
        || core::str::from_utf8(bytes).is_err()
    {
        return Err(CoreError::Target);
    }
    Ok(())
}

/// Canonical unsigned-byte ordering; locale and Unicode collation are absent.
pub fn compare_unsigned(left: &[u8], right: &[u8]) -> Ordering {
    left.cmp(right)
}

/// Hierarchical OD-01 order: compare corresponding components as unsigned
/// bytes, with a parent ordered before any descendant after all shared
/// components are equal.
pub fn compare_paths_unsigned(left: ValidatedPath<'_>, right: ValidatedPath<'_>) -> Ordering {
    let mut left_components = left.as_bytes().split(|&byte| byte == b'/');
    let mut right_components = right.as_bytes().split(|&byte| byte == b'/');

    loop {
        match (left_components.next(), right_components.next()) {
            (Some(left), Some(right)) => match compare_unsigned(left, right) {
                Ordering::Equal => {}
                ordering => return ordering,
            },
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => return Ordering::Equal,
        }
    }
}

pub fn require_strictly_increasing(previous: &[u8], next: &[u8]) -> CoreResult<()> {
    if compare_unsigned(previous, next) == Ordering::Less {
        Ok(())
    } else {
        Err(CoreError::NonCanonicalOrder)
    }
}

pub fn require_strictly_increasing_paths(
    previous: ValidatedPath<'_>,
    next: ValidatedPath<'_>,
) -> CoreResult<()> {
    if compare_paths_unsigned(previous, next) == Ordering::Less {
        Ok(())
    } else {
        Err(CoreError::NonCanonicalOrder)
    }
}
