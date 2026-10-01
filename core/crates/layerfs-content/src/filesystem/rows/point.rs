//! Opaque existing source authority and exact issued binding coordinates.
use super::DirectoryHeader;
use crate::error::{ContentError, ContentResult};

/// Existing source issuer width; this is not a persistence or adoption identity.
pub const BINDING_SOURCE_BYTES: usize = 8;
/// issuer8/parent8/header-descriptor8/binding-ordinal4, all big endian.
pub const BINDING_POINT_BYTES: usize = 28;

/// Read-only view of one existing live BindingAuthority, with no raw factory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BindingSourceId(u64);
impl BindingSourceId {
    pub(super) const fn issued(token: u64) -> Self {
        Self(token)
    }
    /// Exact issuer bytes for an already supplied native association.
    pub const fn as_bytes(self) -> [u8; BINDING_SOURCE_BYTES] {
        self.0.to_be_bytes()
    }
}

/// An issued in-range element; actual source membership belongs to its receiver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BindingPoint([u8; BINDING_POINT_BYTES]);
impl BindingPoint {
    /// Empty/EOF/out-of-range headers cannot mint an element or completion.
    pub fn new(header: &DirectoryHeader, ordinal: u32) -> ContentResult<Self> {
        if ordinal >= header.binding_count() {
            return Err(ContentError::InvalidRecord("binding ordinal"));
        }
        let mut bytes = [0; BINDING_POINT_BYTES];
        bytes[..8].copy_from_slice(&header.source_id().as_bytes());
        bytes[8..16].copy_from_slice(&header.parent().to_be_bytes());
        bytes[16..24].copy_from_slice(&header.ordinal().to_be_bytes());
        bytes[24..].copy_from_slice(&ordinal.to_be_bytes());
        Ok(Self(bytes))
    }
    /// Requires the actual issued source; bytes alone never mint that authority.
    pub fn decode_for(source: BindingSourceId, bytes: &[u8]) -> ContentResult<Self> {
        let bytes: [u8; BINDING_POINT_BYTES] = bytes
            .try_into()
            .map_err(|_| ContentError::InvalidRecord("binding point width"))?;
        if bytes[..8] != source.as_bytes() {
            return Err(ContentError::InvalidRecord("binding point issuer"));
        }
        let parent = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        if !super::serial_in_range(parent) {
            return Err(ContentError::InvalidRecord("binding point parent"));
        }
        Ok(Self(bytes))
    }
    /// The fixed private descriptor encoding.
    pub const fn encode(self) -> [u8; BINDING_POINT_BYTES] {
        self.0
    }
    /// Supplied issuer, not an independently decoded raw capability.
    pub fn source_id(self) -> BindingSourceId {
        BindingSourceId::issued(u64::from_be_bytes(self.0[..8].try_into().unwrap()))
    }
    /// Positive parent selected by the original issued header.
    pub fn parent(self) -> u64 {
        u64::from_be_bytes(self.0[8..16].try_into().unwrap())
    }
    /// Original source-local opaque header descriptor.
    pub fn header_descriptor(self) -> u64 {
        u64::from_be_bytes(self.0[16..24].try_into().unwrap())
    }
    /// Original element ordinal, checked against actual source on resolution.
    pub fn binding_ordinal(self) -> u32 {
        u32::from_be_bytes(self.0[24..].try_into().unwrap())
    }
}
