//! Scoped owning metadata delivery into Workspace stat, with exact errors.
use super::{Binding, LengthReply, RuntimeError, RuntimeResult, Sessions};
use layerfs_content::ObjectId;
use layerfs_workspace::{FileLengths, WorkspaceError, WorkspaceResult};
/// A borrowed authenticated serving-scope length port, not a new Store handle.
/// Authority and local binding checks run for every demand. Whole-file metadata
/// facts do not read or attest payload. No lifetime extension or replay occurs.
pub struct BoundLengths<'s, 'a> {
    sessions: &'s Sessions<'a>,
    binding: &'s Binding,
}
struct Reply {
    expected: ObjectId,
    value: Option<u64>,
}
impl LengthReply for Reply {
    fn file_length(&mut self, id: ObjectId, length: u64) -> RuntimeResult<()> {
        if id != self.expected || self.value.is_some() {
            return Err(RuntimeError::Invalid(
                "file length reply identity/cardinality",
            ));
        }
        self.value = Some(length);
        Ok(())
    }
}
impl<'a> Sessions<'a> {
    /// Creates a borrowed stat metadata adapter over the initialized demand owner.
    pub fn length_port<'s>(&'s self, binding: &'s Binding) -> BoundLengths<'s, 'a> {
        BoundLengths {
            sessions: self,
            binding,
        }
    }
    /// Actual initialized demand-owner counters, without reads or reinitialization.
    pub fn demand_diagnostics(&self) -> layerfs_storage::read::Diagnostics {
        self.demand.diagnostics()
    }
}
impl FileLengths for BoundLengths<'_, '_> {
    fn file_length(&self, id: ObjectId) -> WorkspaceResult<u64> {
        let mut reply = Reply {
            expected: id,
            value: None,
        };
        self.sessions
            .file_lengths(self.binding, &[id], &mut reply)
            .map_err(|error| WorkspaceError::Service(Box::new(error)))?;
        reply.value.ok_or_else(|| {
            WorkspaceError::Service(Box::new(RuntimeError::Invalid("missing file length reply")))
        })
    }
}
