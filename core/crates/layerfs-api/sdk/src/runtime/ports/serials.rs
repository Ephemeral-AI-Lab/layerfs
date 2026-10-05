//! Scoped owning inode-serial reservations for Workspace creation.
use crate::runtime::{Binding, RuntimeError, RuntimeResult, Sessions};
use layerfs_history::{HistoryCatalog, ReserveRequest};
use layerfs_workspace::{InodeSerials, WorkspaceError, WorkspaceResult};

/// Largest range one request may consume from a scope. An admission window per
/// call, not a limit on the inodes a Workspace can create over its lifetime.
pub const SERIAL_WINDOW: u64 = 65_536;

/// A borrowed authenticated serving-scope serial allocator, not a new catalog
/// handle. Authority and local binding checks run for every reservation.
pub struct BoundSerials<'s, 'a> {
    sessions: &'s Sessions<'a>,
    binding: &'s Binding,
}
impl<'a> Sessions<'a> {
    /// Consumes one half-open range `[start, start + count)` of the bound
    /// Branch's allocation scope through the owning history catalog. One
    /// attempt: after a failed or unknown call the range is simply never used,
    /// because a reservation is consumed whether or not its serials are.
    pub fn reserve_serials(&self, binding: &Binding, count: u64) -> RuntimeResult<(u64, u64)> {
        self.check_binding(binding)?;
        if count == 0 || count > SERIAL_WINDOW {
            return Err(RuntimeError::Invalid("inode serial window"));
        }
        let scope = binding.snapshot.scope;
        let reserved = self
            .history
            .reserve_inodes(&ReserveRequest { scope, count })?;
        if reserved.scope != scope || reserved.count != count || reserved.start == 0 {
            return Err(RuntimeError::Invalid("inode serial reservation reply"));
        }
        Ok((reserved.start, reserved.count))
    }
    /// Creates a borrowed serial allocator port for Workspace creation.
    pub fn serial_port<'s>(&'s self, binding: &'s Binding) -> BoundSerials<'s, 'a> {
        BoundSerials {
            sessions: self,
            binding,
        }
    }
}
impl InodeSerials for BoundSerials<'_, '_> {
    fn reserve(&self, count: u64) -> WorkspaceResult<(u64, u64)> {
        self.sessions
            .reserve_serials(self.binding, count)
            .map_err(|error| WorkspaceError::Service(Box::new(error)))
    }
}
