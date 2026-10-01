//! Pure complete-header selection before native scratch/receive effects.
use super::error::content;
use layerfs_bridge::contract::{Code, Failure, PreparedChanges, PreparedTotals};
use layerfs_content::{
    filesystem::{
        root::FilesystemRootId,
        rows::SpoolPreparation,
        state::{GraphCapacity, GraphSubject, VerifiedEmptyState},
        InodeScope,
    },
    ObjectId,
};

/// All seven role/population fields are required; D0/B0 alone is insufficient.
pub(crate) fn is_empty(changes: &PreparedChanges) -> Result<bool, Failure> {
    changes.check()?;
    Ok(changes.totals == PreparedTotals::default())
}
/// Build one logical UpdateBase/source association without opening native authority.
pub(crate) fn admit(
    changes: &PreparedChanges,
    preparation: &SpoolPreparation,
    capacity: GraphCapacity,
) -> Result<VerifiedEmptyState, Failure> {
    if !is_empty(changes)? || !preparation.declares_empty() {
        return Err(Code::InvalidInput.into());
    }
    let subject = GraphSubject::new(
        preparation.source_id(),
        InodeScope::from_object(ObjectId::from_bytes(&changes.scope).map_err(content)?),
        Some(FilesystemRootId(
            ObjectId::from_bytes(&changes.base).map_err(content)?,
        )),
        changes.root_serial,
        capacity,
    )
    .map_err(content)?;
    let generation = changes.generation.to_be_bytes();
    let root_serial = changes.root_serial.to_be_bytes();
    let source = subject.source_id().as_bytes();
    let mut context = [0u8; 256];
    let mut used = 0;
    for part in [
        b"layerfs/server-verified-empty/v1\0".as_slice(),
        changes.workspace.as_slice(),
        changes.branch.as_slice(),
        generation.as_slice(),
        changes.base.as_slice(),
        changes.scope.as_slice(),
        root_serial.as_slice(),
        source.as_slice(),
    ] {
        context[used..used + part.len()].copy_from_slice(part);
        used += part.len();
    }
    VerifiedEmptyState::new(*ObjectId::for_bytes(&context[..used]).as_bytes(), subject)
        .map_err(content)
}
