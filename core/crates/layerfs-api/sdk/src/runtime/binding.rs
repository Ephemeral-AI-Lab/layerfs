//! Application authority and immutable typed local binding facts.
use super::RuntimeResult;
use layerfs_content::ObjectId;
use layerfs_history::{BranchId, BranchSnapshot, CatalogId, WorkspaceId};

/// Application-supplied authority, called for every operation before its attempt.
///
/// `peer` must come from authenticated transport. This trait authorizes that
/// identity; it does not authenticate a byte string or provide a network route.
pub trait Authorization {
    /// Checks peer/Workspace/Branch service permission, including revocation.
    fn workspace(
        &self,
        peer: [u8; 32],
        workspace: WorkspaceId,
        branch: BranchId,
    ) -> RuntimeResult<()>;
    /// Checks the exact object demand/output refs under that binding.
    fn objects(
        &self,
        peer: [u8; 32],
        workspace: WorkspaceId,
        branch: BranchId,
        ids: &[ObjectId],
    ) -> RuntimeResult<()>;
}

/// Host-created local binding; cannot be reconstructed from untrusted fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    pub(super) owner: [u8; 32],
    pub(super) peer: [u8; 32],
    pub(super) workspace: WorkspaceId,
    pub(super) catalog: CatalogId,
    pub(super) incarnation: u64,
    pub(super) snapshot: BranchSnapshot,
    pub(super) root_serial: u64,
}
impl Binding {
    /// Checked immutable root inode serial, acquired during binding only.
    pub const fn root_serial(&self) -> u64 {
        self.root_serial
    }
    /// Coherent Branch snapshot at bind, retained as construction expectations.
    pub fn snapshot(&self) -> &BranchSnapshot {
        &self.snapshot
    }
    /// Exact Workspace incarnation.
    pub const fn workspace(&self) -> WorkspaceId {
        self.workspace
    }
    /// Authenticated peer identity supplied by the transport/application.
    pub const fn peer(&self) -> [u8; 32] {
        self.peer
    }
}
