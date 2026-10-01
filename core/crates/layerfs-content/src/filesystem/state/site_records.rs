//! Closed exclusive site records and monotone facts, with no arbitrary values.
use super::{SiteScope, STATE_KEY_BYTES};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::rows::BindingPoint;
/// key-length2/key25/value-length4/flags1/point28.
pub const SITE_RECORD_BYTES: usize = 60;
/// version1/scope89/count2/record-bytes4.
pub const SITE_APPEND_HEADER_BYTES: usize = 96;
/// Exact scoped compact exclusive-child key.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SiteKey([u8; STATE_KEY_BYTES]);
impl SiteKey {
    /// Checks one positive local serial under this closed scope.
    pub fn new(scope: &SiteScope, serial: u64) -> ContentResult<Self> {
        if serial == 0 || serial > i64::MAX as u64 {
            return Err(ContentError::InvalidOrderingRecord("site serial"));
        }
        let mut bytes = [0; STATE_KEY_BYTES];
        bytes[..8].copy_from_slice(&scope.state().selection().token().to_be_bytes());
        bytes[8..16].copy_from_slice(&scope.state().phase().to_be_bytes());
        bytes[16] = 3;
        bytes[17..].copy_from_slice(&serial.to_be_bytes());
        Ok(Self(bytes))
    }
    /// Complete width/selected-prefix validation before use.
    pub fn decode(scope: &SiteScope, bytes: &[u8]) -> ContentResult<Self> {
        let bytes: [u8; STATE_KEY_BYTES] = bytes
            .try_into()
            .map_err(|_| ContentError::InvalidOrderingRecord("site key width"))?;
        let expected = Self::new(scope, u64::from_be_bytes(bytes[17..].try_into().unwrap()))?;
        if bytes != expected.0 {
            return Err(ContentError::InvalidOrderingRecord("site key selection"));
        }
        Ok(expected)
    }
    /// Complete fixed key.
    pub const fn as_bytes(&self) -> &[u8; STATE_KEY_BYTES] {
        &self.0
    }
    /// Positive child serial.
    pub fn serial(self) -> u64 {
        u64::from_be_bytes(self.0[17..].try_into().unwrap())
    }
}
/// Immutable point/base bit, plus only the two monotone base-fact bits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SiteRecord {
    key: SiteKey,
    flags: u8,
    point: BindingPoint,
}
impl SiteRecord {
    /// One original exclusive birth, bound to the real issued source point.
    pub fn birth(
        scope: &SiteScope,
        serial: u64,
        point: BindingPoint,
        has_base: bool,
    ) -> ContentResult<Self> {
        if point.source_id() != scope.source_id() {
            return Err(ContentError::InvalidOrderingRecord("site point source"));
        }
        Ok(Self {
            key: SiteKey::new(scope, serial)?,
            flags: u8::from(has_base),
            point,
        })
    }
    /// Exact framing/key/point/source/allowed flags before copying exposed values.
    pub fn decode(scope: &SiteScope, bytes: &[u8]) -> ContentResult<Self> {
        if bytes.len() != SITE_RECORD_BYTES
            || bytes[..2] != 25u16.to_be_bytes()
            || bytes[27..31] != 29u32.to_be_bytes()
            || !matches!(bytes[31], 0 | 1 | 3 | 7)
        {
            return Err(ContentError::InvalidOrderingRecord("site record framing"));
        }
        Ok(Self {
            key: SiteKey::decode(scope, &bytes[2..27])?,
            flags: bytes[31],
            point: BindingPoint::decode_for(scope.source_id(), &bytes[32..])?,
        })
    }
    /// Fixed frame60; declarations and delegation count as product source.
    pub fn encode(self) -> [u8; SITE_RECORD_BYTES] {
        let mut bytes = [0; SITE_RECORD_BYTES];
        bytes[..2].copy_from_slice(&25u16.to_be_bytes());
        bytes[2..27].copy_from_slice(self.key.as_bytes());
        bytes[27..31].copy_from_slice(&29u32.to_be_bytes());
        bytes[31] = self.flags;
        bytes[32..].copy_from_slice(&self.point.encode());
        bytes
    }
    /// Exact child key.
    pub const fn key(self) -> SiteKey {
        self.key
    }
    /// Immutable issued source point.
    pub const fn point(self) -> BindingPoint {
        self.point
    }
    /// Closed flags byte, never supplied arbitrarily by a writer.
    pub const fn flags(self) -> u8 {
        self.flags
    }
    /// Immutable stored-record membership bit.
    pub const fn has_base(self) -> bool {
        self.flags & 1 != 0
    }
    /// At least one relevant nonrestated base binding was observed.
    pub const fn saw_base(self) -> bool {
        self.flags & 2 != 0
    }
    /// At least one such binding was legal under the original OR rule.
    pub const fn any_legal_base(self) -> bool {
        self.flags & 4 != 0
    }
    /// True before facts: only immutable0/1 flags.
    pub const fn is_birth(self) -> bool {
        self.flags <= 1
    }
    /// Normalizes only mutable bits, retaining original point/key/base bit.
    pub const fn birth_projection(self) -> Self {
        Self {
            flags: self.flags & 1,
            ..self
        }
    }
    /// Only monotone facts for a known existing non-file; no point mutation.
    pub fn observe(self, legal: bool) -> ContentResult<Self> {
        if !self.has_base() {
            return Err(ContentError::InvalidOrderingRecord(
                "site fresh observation",
            ));
        }
        Ok(Self {
            flags: self.flags | 2 | if legal { 4 } else { 0 },
            ..self
        })
    }
}
/// Exact key/legal observation; point/key membership remains immutable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SiteObservation {
    key: SiteKey,
    legal: bool,
}
impl SiteObservation {
    /// Associates a typed key with one known legal decision.
    pub const fn new(key: SiteKey, legal: bool) -> Self {
        Self { key, legal }
    }
    /// Selected child key.
    pub const fn key(self) -> SiteKey {
        self.key
    }
    /// True when at least this captured base binding is legal.
    pub const fn legal(self) -> bool {
        self.legal
    }
    /// key25/legal1.
    pub fn encode(self) -> [u8; 26] {
        let mut bytes = [0; 26];
        bytes[..25].copy_from_slice(self.key.as_bytes());
        bytes[25] = u8::from(self.legal);
        bytes
    }
    /// Complete key/byte-class validation.
    pub fn decode(scope: &SiteScope, bytes: &[u8]) -> ContentResult<Self> {
        if bytes.len() != 26 || bytes[25] > 1 {
            return Err(ContentError::InvalidOrderingRecord(
                "site observation framing",
            ));
        }
        Ok(Self::new(
            SiteKey::decode(scope, &bytes[..25])?,
            bytes[25] == 1,
        ))
    }
}
/// Admitted logical site class; native/physical ownership is separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SiteCapacity {
    records: u64,
    encoded_bytes: u64,
}
impl SiteCapacity {
    /// Reports separately admitted rows and complete framed record bytes.
    pub const fn new(records: u64, encoded_bytes: u64) -> ContentResult<Self> {
        Ok(Self {
            records,
            encoded_bytes,
        })
    }
    /// Admitted row population.
    pub const fn records(self) -> u64 {
        self.records
    }
    /// Admitted frame60 population, excluding page headers.
    pub const fn encoded_bytes(self) -> u64 {
        self.encoded_bytes
    }
    /// Declared shape refusal before site effects.
    pub fn check_requested(self, records: usize) -> ContentResult<()> {
        let actual = u64::try_from(records).map_err(|_| ContentError::LengthOverflow)?;
        let bytes = actual.checked_mul(60).ok_or(ContentError::LengthOverflow)?;
        if actual > self.records {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_sites.records",
                limit: self.records,
                actual,
            });
        }
        if bytes > self.encoded_bytes {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "binding_sites.record_bytes",
                limit: self.encoded_bytes,
                actual: bytes,
            });
        }
        Ok(())
    }
}
