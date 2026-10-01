//! One fixed exact base-name/site-ordinal continuation, independent of depth.
use crate::filesystem::PathName;
use crate::{ContentError, ContentResult};
/// Exact fixed progress encoding and retained width.
pub const ALIAS_PROGRESS_BYTES: usize = 264;
/// Base0, selected Sites1, Complete2; absent fields have canonical zero bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AliasProgress([u8; ALIAS_PROGRESS_BYTES]);
impl AliasProgress {
    /// First base page.
    pub fn initial() -> Self {
        Self([0; ALIAS_PROGRESS_BYTES])
    }
    /// Next exact base name; base EOF switches to selected Sites.
    pub fn base(after: Option<PathName>) -> Self {
        let mut value = Self::initial();
        match after {
            Some(name) => {
                value.0[1] = 1;
                value.0[2..4].copy_from_slice(&(name.as_bytes().len() as u16).to_be_bytes());
                value.0[4..4 + name.as_bytes().len()].copy_from_slice(name.as_bytes());
            }
            None => value.0[0] = 1,
        }
        value
    }
    /// Next selected Site ordinal; EOF completes the current parent.
    pub fn sites(after: Option<u32>, eof: bool) -> Self {
        let mut value = Self::initial();
        value.0[0] = if eof { 2 } else { 1 };
        if !eof {
            if let Some(after) = after {
                value.0[259] = 1;
                value.0[260..].copy_from_slice(&after.to_be_bytes());
            }
        }
        value
    }
    /// Explicit completion when the parent owns no selected Sites.
    pub fn complete() -> Self {
        Self::sites(None, true)
    }
    /// Current listing phase.
    pub const fn stage(&self) -> u8 {
        self.0[0]
    }
    /// Exact retained next base name; no allocation or name hash.
    pub fn name(&self) -> Option<&[u8]> {
        (self.0[1] == 1)
            .then(|| &self.0[4..4 + u16::from_be_bytes([self.0[2], self.0[3]]) as usize])
    }
    /// Exact next selected ordinal, including zero.
    pub fn ordinal(&self) -> Option<u32> {
        (self.0[259] == 1).then(|| u32::from_be_bytes(self.0[260..].try_into().unwrap()))
    }
    /// Checks before/after progress, including strict successful advance.
    pub fn advances_to(&self, next: &Self) -> ContentResult<()> {
        let valid = match (self.stage(), next.stage()) {
            (0, 0) => next
                .name()
                .is_some_and(|name| self.name().is_none_or(|old| name > old)),
            (0, 1) | (1, 2) => true,
            (1, 1) => next
                .ordinal()
                .is_some_and(|n| self.ordinal().is_none_or(|old| n > old)),
            _ => false,
        };
        if !valid {
            return Err(ContentError::InvalidOrderingRecord("alias progress"));
        }
        Ok(())
    }
    /// Canonical fixed continuation.
    pub fn encode(&self) -> [u8; ALIAS_PROGRESS_BYTES] {
        self.0
    }
    /// Requires complete framing, canonical names and zero absent/tail bytes.
    pub fn decode(bytes: &[u8]) -> ContentResult<Self> {
        if bytes.len() != ALIAS_PROGRESS_BYTES || bytes[0] > 2 || bytes[1] > 1 || bytes[259] > 1 {
            return Err(ContentError::InvalidOrderingRecord(
                "alias progress framing",
            ));
        }
        let length = u16::from_be_bytes([bytes[2], bytes[3]]) as usize;
        if length > 255
            || (bytes[1] == 0 && length != 0)
            || bytes[4 + length..259].iter().any(|b| *b != 0)
        {
            return Err(ContentError::InvalidOrderingRecord("alias progress name"));
        }
        if bytes[1] == 1 {
            let name = &bytes[4..4 + length];
            if name.is_empty()
                || std::str::from_utf8(name).is_err()
                || name == b"."
                || name == b".."
                || name.iter().any(|b| matches!(*b, 0 | b'/' | b'\\'))
            {
                return Err(ContentError::InvalidOrderingRecord("alias progress name"));
            }
        }
        if (bytes[259] == 0 && bytes[260..] != [0; 4])
            || (bytes[0] != 0 && bytes[1] != 0)
            || (bytes[0] != 1 && bytes[259] != 0)
        {
            return Err(ContentError::InvalidOrderingRecord("alias progress phase"));
        }
        Ok(Self(bytes.try_into().unwrap()))
    }
}
