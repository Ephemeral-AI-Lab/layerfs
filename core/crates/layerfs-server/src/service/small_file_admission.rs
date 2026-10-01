//! Whole-header fixed class selection; body kind/table proof remains mandatory.
use super::error::content;
use layerfs_bridge::contract::{Code, Failure, PreparedChanges};
use layerfs_content::{
    filesystem::{
        root::FilesystemRootId,
        rows::SpoolPreparation,
        state::{GraphCapacity, GraphSubject},
        InodeScope,
    },
    ObjectId,
};
/// Pure complete population recognizer, never an error-driven fallback.
pub(crate) fn selected(changes: &PreparedChanges) -> Result<bool, Failure> {
    changes.check()?;
    let t = changes.totals;
    Ok(t.directories == 0
        && t.names == 0
        && t.name_bytes == 0
        && t.patches == 0
        && t.declarations == 0
        && t.fresh == 0
        && (1..=8).contains(&t.identities))
}
/// Full logical context; no native binding/file/reservation or cleanup is invented.
pub(crate) fn subject(
    changes: &PreparedChanges,
    preparation: &SpoolPreparation,
    capacity: GraphCapacity,
) -> Result<GraphSubject, Failure> {
    if !selected(changes)? {
        return Err(Code::Capacity.into());
    }
    GraphSubject::new(
        preparation.source_id(),
        InodeScope::from_object(ObjectId::from_bytes(&changes.scope).map_err(content)?),
        Some(FilesystemRootId(
            ObjectId::from_bytes(&changes.base).map_err(content)?,
        )),
        changes.root_serial,
        capacity,
    )
    .map_err(content)
}
/// Fixed stack framing of actual captured request context; issuer distinguishes attempts.
pub(crate) fn selector(changes: &PreparedChanges) -> [u8; 32] {
    let mut bytes = [0; 256];
    let mut at = 0;
    let generation = changes.generation.to_be_bytes();
    let root = changes.root_serial.to_be_bytes();
    let identities = changes.totals.identities.to_be_bytes();
    let mut head = [0; 34];
    if let Some(value) = changes.expected_head {
        head[0] = 1;
        head[1..].copy_from_slice(&value);
    }
    for part in [
        b"layerfs/server-small-files/v1\0".as_slice(),
        changes.workspace.as_slice(),
        changes.branch.as_slice(),
        generation.as_slice(),
        changes.expected_base.as_slice(),
        head.as_slice(),
        changes.base.as_slice(),
        changes.scope.as_slice(),
        root.as_slice(),
        identities.as_slice(),
    ] {
        bytes[at..at + part.len()].copy_from_slice(part);
        at += part.len();
    }
    *ObjectId::for_bytes(&bytes[..at]).as_bytes()
}

/// Reserve the actual fixed source/future actor/window shape before Stage Save/body.
pub(crate) fn pending(
    changes: &PreparedChanges,
    preparation: SpoolPreparation,
    capacity: GraphCapacity,
) -> Result<layerfs_content::filesystem::rows::PendingSmallFiles, Failure> {
    if preparation.declared_inodes() as u64 != changes.totals.identities {
        return Err(Code::InvalidInput.into());
    }
    let subject = subject(changes, &preparation, capacity)?;
    layerfs_content::filesystem::rows::PendingSmallFiles::new(
        selector(changes),
        subject,
        preparation,
    )
    .map_err(content)
}
