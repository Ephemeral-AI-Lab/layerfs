//! Immutable length/digest-verified read result across the persistence boundary.
use super::{ObjectKey, PersistenceError};
use crate::location::PackInfo;

/// Complete pack bytes authenticated against their descriptor.
/// Private fields prevent mutation after verification. C2 separately checks the
/// descriptor binding, pack grammar/domain, visibility and canonical identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedPack {
    info: PackInfo,
    body: Vec<u8>,
}
impl PersistedPack {
    /// Verifies a real acquired body before returning it across the read port.
    /// This does not validate C2 framing or relax the caller's read budgets.
    pub fn authenticate(info: PackInfo, body: Vec<u8>) -> Result<Self, PersistenceError> {
        if body.len() != info.length || ObjectKey::for_bytes(&body) != info.key {
            return Err(PersistenceError::Malformed);
        }
        Ok(Self { info, body })
    }
    /// The descriptor bound to these exact immutable bytes.
    pub const fn info(&self) -> PackInfo {
        self.info
    }
    /// Borrows verified bytes without exposing a mutable reference.
    pub fn body(&self) -> &[u8] {
        &self.body
    }
    /// Transfers the authenticated pair to the consuming C2 decoder/cache.
    pub fn into_parts(self) -> (PackInfo, Vec<u8>) {
        (self.info, self.body)
    }
}
