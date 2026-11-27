//! The acquisition row payload both engines share: the init_entry metadata
//! blob (encoding and checked decoding), the stored host identity, entry
//! shape validation and exact payload bytes.
//!
//! A row's payload is its variable bytes: name, host path, host identity
//! and object identities. The same amounts are charged when a row is written,
//! adjusted when it changes and read back from the engine when it is removed,
//! so an operation whose rows are all gone holds exactly zero.
use layerfs_content::inode_leaf::InodeKind;
use layerfs_storage::port::acquisition::{HostIdentity, NewEntry};

/// The 32-byte init_entry metadata blob: the inline form's field, mode at
/// 0..4, mtime seconds at 4..12, nanoseconds at 12..16 and 16 zero bytes.
/// Rows are written and read only by the live operation owning them, so the
/// column's content stays private to one running binary.
pub(crate) fn metadata_blob(
    metadata: &layerfs_content::filesystem::attributes::PortableMetadata,
) -> [u8; 32] {
    let mut blob = [0_u8; 32];
    blob[0..4].copy_from_slice(&metadata.mode.to_be_bytes());
    blob[4..12].copy_from_slice(&metadata.mtime_seconds.to_be_bytes());
    blob[12..16].copy_from_slice(&metadata.mtime_nanoseconds.to_be_bytes());
    blob
}

/// Decodes one metadata blob, rejecting nonzero padding and values that fail
/// the portable grammar for the row's kind.
pub(crate) fn metadata_parse(
    blob: &[u8],
    kind: layerfs_content::inode_leaf::InodeKind,
) -> Result<
    layerfs_content::filesystem::attributes::PortableMetadata,
    crate::backend::error::BackendError,
> {
    if blob.len() != 32 || blob[16..].iter().any(|byte| *byte != 0) {
        return Err(crate::backend::error::BackendError::Integrity);
    }
    let metadata = layerfs_content::filesystem::attributes::PortableMetadata {
        mode: u32::from_be_bytes(
            blob[0..4]
                .try_into()
                .map_err(|_| crate::backend::error::BackendError::Integrity)?,
        ),
        mtime_seconds: i64::from_be_bytes(
            blob[4..12]
                .try_into()
                .map_err(|_| crate::backend::error::BackendError::Integrity)?,
        ),
        mtime_nanoseconds: u32::from_be_bytes(
            blob[12..16]
                .try_into()
                .map_err(|_| crate::backend::error::BackendError::Integrity)?,
        ),
    };
    metadata
        .validate(kind)
        .map_err(|_| crate::backend::error::BackendError::Integrity)?;
    Ok(metadata)
}

/// Bytes of one stored object identity.
pub(crate) const ROOT_BYTES: i64 = 32;
/// Bytes of one stored host identity with its evidence.
pub(crate) const HOST_BYTES: i64 = 60;

/// Payload bytes of one entry row as it is inserted.
pub(crate) fn entry_bytes(entry: &NewEntry) -> i64 {
    let stored_path = match entry.kind {
        layerfs_content::inode_leaf::InodeKind::Directory => entry.host_path.as_deref(),
        _ => None,
    };
    let unplaced_host_file = entry.position.is_none() && entry.host.is_some();
    entry.key.name.len() as i64
        + stored_path.map_or(0, |path| path.len() as i64)
        + if unplaced_host_file { HOST_BYTES } else { 0 }
        + ROOT_BYTES
        + entry.target_root.map_or(0, |_| ROOT_BYTES)
}

/// Payload bytes of one first-path host row as it is inserted.
pub(crate) fn host_bytes(path: &[u8]) -> i64 {
    path.len() as i64 + HOST_BYTES
}

/// Shape validation before an individual entry's original attempted mutations.
pub(crate) fn well_formed(entry: &NewEntry) -> bool {
    let path = entry.host_path.as_deref();
    (match entry.kind {
        InodeKind::Directory => path.is_some() && entry.host.is_none(),
        InodeKind::RegularFile => path.is_some() && entry.host.is_some(),
        InodeKind::Symlink => path.is_none() && entry.host.is_none(),
    }) && (entry.kind == InodeKind::Symlink) == entry.target_root.is_some()
        && entry.key.parent.is_some() != (entry.position == Some(0))
        && entry.key.parent.is_some() != entry.key.name.is_empty()
        && (entry.key.parent.is_some() || entry.kind == InodeKind::Directory)
        && entry.metadata.validate(entry.kind).is_ok()
}

/// Device, inode and evidence as one stored value.
pub(crate) fn packed(host: HostIdentity) -> [u8; HOST_BYTES as usize] {
    let mut out = [0; HOST_BYTES as usize];
    out[..8].copy_from_slice(&host.device.to_be_bytes());
    out[8..16].copy_from_slice(&host.inode.to_be_bytes());
    out[16..].copy_from_slice(&host.evidence);
    out
}
