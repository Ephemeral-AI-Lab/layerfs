//! Original reference Row plus exact touched membership in the count authority.
use super::CanonicalScope;
use crate::filesystem::references::record::{Row, ROW_BYTES};
use crate::{ContentError, ContentResult};
/// Exact original row96 plus one monotone touched-membership byte.
pub const COUNT_VALUE_BYTES: usize = 97;
/// One declared/count/effect row; Count distinction replaces the full declared set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CountRecord {
    /// Original version1 reference tally/value row.
    pub row: Row,
    /// True only after retained/removal/value effects have observed this serial.
    pub touched: bool,
}
impl CountRecord {
    /// Existing original serial, unchanged by count mutations.
    pub const fn serial(self) -> u64 {
        self.row.serial()
    }
    /// Canonical complete97-byte value with exact original reference codec.
    pub fn encode_value(self) -> ContentResult<[u8; 97]> {
        let mut b = [0; 97];
        b[..96].copy_from_slice(&self.row.encode()?);
        b[96] = u8::from(self.touched);
        Ok(b)
    }
    /// Require original row serial to agree with the selected full scoped key.
    pub fn decode(scope: &CanonicalScope, key: &[u8], value: &[u8]) -> ContentResult<Self> {
        if scope.state().table().code() != 19 || value.len() != COUNT_VALUE_BYTES || value[96] > 1 {
            return Err(ContentError::InvalidOrderingRecord("count value framing"));
        }
        let serial = scope.scalar(key)?;
        let row = Row::decode(<&[u8; ROW_BYTES]>::try_from(&value[..96]).unwrap())?;
        if row.serial() != serial {
            return Err(ContentError::InvalidOrderingRecord("count selected serial"));
        }
        Ok(Self {
            row,
            touched: value[96] == 1,
        })
    }
}
/// Closed immutable reference-count epoch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CountEpoch {
    /// All changed-directory and supplied-value effects precede the zero scan.
    Effects = 1,
    /// Every descendant removal is known; final inode rows may be emitted.
    Final = 2,
}
impl CountEpoch {
    /// Exact private epoch byte.
    pub const fn code(self) -> u8 {
        self as u8
    }
}
