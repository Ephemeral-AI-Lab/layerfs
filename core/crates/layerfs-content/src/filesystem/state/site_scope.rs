//! Exact native operation and supplied live source association.
use super::{StateScope, StateTable};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::rows::BindingSourceId;
/// state-scope81/source8.
pub const SITE_SCOPE_BYTES: usize = 89;
/// One exact issued operation/source and implemented BindingSites phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SiteScope {
    state: StateScope,
    source: BindingSourceId,
}
impl SiteScope {
    /// Raw source bytes cannot select this authority.
    pub fn new(state: StateScope, source: BindingSourceId) -> ContentResult<Self> {
        if state.table() != StateTable::BindingSites {
            return Err(ContentError::InvalidOrderingRecord("site table"));
        }
        Ok(Self { state, source })
    }
    /// The exact live native selection/phase/table.
    pub fn state(&self) -> &StateScope {
        &self.state
    }
    /// Issued source associated with this operation.
    pub const fn source_id(&self) -> BindingSourceId {
        self.source
    }
    /// Exact full state association and source bytes.
    pub fn as_bytes(&self) -> [u8; SITE_SCOPE_BYTES] {
        let mut bytes = [0; SITE_SCOPE_BYTES];
        bytes[..81].copy_from_slice(&self.state.as_bytes());
        bytes[81..].copy_from_slice(&self.source.as_bytes());
        bytes
    }
}
