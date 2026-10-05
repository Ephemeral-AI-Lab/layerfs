//! Atomic component rename through one verified active candidate.
use crate::{runtime::coherence::MutationOrigin, *};
use std::time::Instant;
/// One rename request: both parents, both names and the selected flags.
#[derive(Clone, Copy)]
pub(crate) struct RenameRequest<'a> {
    pub source_parent: u64,
    pub source: &'a [u8],
    pub destination_parent: u64,
    pub destination: &'a [u8],
    pub flags: RenameFlags,
}
impl Workspace {
    /// Moves or renames one name. Ordinary rename replaces an existing
    /// destination; `RenameFlags::noreplace` refuses one instead.
    pub fn rename(
        &self,
        source_parent: u64,
        source: &[u8],
        destination_parent: u64,
        destination: &[u8],
        flags: RenameFlags,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        self.rename_from(
            RenameRequest {
                source_parent,
                source,
                destination_parent,
                destination,
                flags,
            },
            deadline,
            MutationOrigin::Local,
        )
    }
    pub(crate) fn rename_from(
        &self,
        request: RenameRequest<'_>,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<(), WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        super::namespace::check_name(request.source)?;
        super::namespace::check_name(request.destination)?;
        self.rename_active(request, deadline, origin)
    }
}
