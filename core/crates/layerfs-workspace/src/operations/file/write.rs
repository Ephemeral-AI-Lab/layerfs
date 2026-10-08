//! One bounded byte write: the inode's size and time with its cell window.
use crate::eval::{refuse, Eval};
use crate::{Position, Refusal, Time, WorkspaceResult, WriteData};
use layerfs_overlay::{Changes, Inode, InodeKind, PayloadWrite, WRITE_WINDOW};

/// Outer None: undecided this round. An empty write changes nothing. A file
/// that lost its last name but still has a row accepts descriptor writes.
/// `clip` is the mapped-store rule: bytes at or beyond the current size are
/// dropped, so the size is never changed by the store.
pub(crate) fn write(
    eval: &mut Eval<'_>,
    serial: u64,
    position: Position,
    data: &WriteData,
    now: Time,
    clip: bool,
) -> WorkspaceResult<Option<(Option<Changes>, Inode)>> {
    if data.len() > WRITE_WINDOW {
        return refuse(Refusal::Invalid);
    }
    let old = match eval.inode(serial)? {
        None => return Ok(None),
        Some(None) => return refuse(Refusal::Missing),
        Some(Some(inode)) => inode,
    };
    if !eval.alive(&old) && eval.open_serial != Some(serial) {
        return refuse(Refusal::Missing);
    }
    match old.kind {
        InodeKind::Directory => return refuse(Refusal::IsDirectory),
        InodeKind::Symlink => return refuse(Refusal::Invalid),
        InodeKind::File => {}
    }
    if data.is_empty() {
        return Ok(Some((None, old)));
    }
    let offset = match position {
        Position::At(offset) => offset,
        Position::End => old.size,
    };
    // A mapped store never changes the size: only its bytes below it land.
    let kept = if clip {
        old.size.saturating_sub(offset).min(data.len() as u64) as usize
    } else {
        data.len()
    };
    if kept == 0 {
        return Ok(Some((None, old)));
    }
    let bytes = if kept == data.len() {
        data.0.clone()
    } else {
        data.0[..kept].into()
    };
    let Some(end) = offset
        .checked_add(kept as u64)
        .filter(|end| *end <= i64::MAX as u64)
    else {
        return refuse(Refusal::TooLarge);
    };
    let new = Inode {
        size: old.size.max(end),
        mtime_seconds: now.seconds,
        mtime_nanoseconds: now.nanoseconds,
        ..old
    };
    // No base payload is read: bytes this window does not cover stay
    // inherited through the layer's cut-off.
    let changes = Changes {
        inodes: vec![new.clone()],
        write: Some(PayloadWrite {
            serial,
            offset,
            data: bytes,
        }),
        ..Changes::default()
    };
    Ok(Some((Some(changes), new)))
}
