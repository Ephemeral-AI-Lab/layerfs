//! Shared active inode and clock facts for namespace publications.
use crate::{
    backing::active::{Extent, HotInode},
    NodeAttributes, NodeKind, WorkspaceError,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn now() -> Result<(i64, u32), WorkspaceError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| WorkspaceError::Io)?;
    Ok((
        i64::try_from(elapsed.as_secs()).map_err(|_| WorkspaceError::Capacity)?,
        elapsed.subsec_nanos(),
    ))
}

pub(super) fn hot(
    attr: NodeAttributes,
    base: [u8; 32],
    metadata: [u8; 32],
    generation: u64,
    revision: u64,
    fresh: bool,
) -> HotInode {
    HotInode {
        revision,
        generation,
        length: attr.size,
        kind: attr.kind,
        fresh,
        storage: u8::from(attr.kind == NodeKind::File && attr.size > 0),
        mode: attr.mode,
        seconds: attr.mtime_seconds,
        nanos: attr.mtime_nanoseconds,
        links: u32::try_from(attr.references).unwrap_or(u32::MAX),
        base,
        metadata,
        inline: if attr.kind == NodeKind::File && attr.size > 0 {
            [Some(Extent::base(0, attr.size)), None, None, None]
        } else {
            [None; 4]
        },
    }
}
