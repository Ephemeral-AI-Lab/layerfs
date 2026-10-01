//! Pure declared admission and a single source issuer before file/body effects.
use super::{BindingAuthority, BindingSourceId, SpoolDeclaration};
use crate::error::{ContentError, ContentResult};

/// Consumed into the actual spool; it cannot recreate or refund the source ID.
pub struct SpoolPreparation {
    pub(super) declaration: SpoolDeclaration,
    pub(super) capacity: u64,
    pub(super) authority: BindingAuthority,
}
impl SpoolPreparation {
    /// Existing declaration/table/byte admission precedes source issuance.
    pub fn new(declaration: SpoolDeclaration, capacity: u64) -> ContentResult<Self> {
        if declaration.required_bytes_upper()? > capacity {
            return Err(ContentError::ResourceUnavailable {
                what: "prepared row spool",
            });
        }
        Ok(Self {
            declaration,
            capacity,
            authority: BindingAuthority::new()?,
        })
    }
    /// Exact captured declaration count, without exposing or recreating its issuer.
    pub const fn declared_inodes(&self) -> usize {
        self.declaration.inodes
    }
    /// Opaque existing source ID, available without native or body effects.
    pub fn source_id(&self) -> BindingSourceId {
        self.authority.source_id()
    }
}
