//! chmod, utimens and truncate over the portable metadata grammar.
use crate::eval::{refuse, Eval};
use crate::{Refusal, Time, WorkspaceResult};
use layerfs_overlay::{Changes, Inode, InodeKind};

/// Outer None: undecided this round. The flag is false when nothing changes.
pub(crate) fn set(
    eval: &mut Eval<'_>,
    serial: u64,
    mode: Option<u32>,
    mtime: Option<Time>,
    size: Option<u64>,
    now: Time,
) -> WorkspaceResult<Option<(Option<Changes>, Inode)>> {
    // A removed inode with a retained row still accepts descriptor-based
    // attribute changes; only a serial with no inode anywhere is missing.
    let old = match eval.inode(serial)? {
        None => return Ok(None),
        Some(None) => return refuse(Refusal::Missing),
        Some(Some(inode)) => inode,
    };
    let mut new = old.clone();
    if let Some(mode) = mode {
        let allowed = match old.kind {
            InodeKind::Symlink => return refuse(Refusal::Unsupported),
            InodeKind::Directory => 0o1777,
            InodeKind::File => 0o777,
        };
        if mode & !allowed != 0 {
            return refuse(Refusal::NotPermitted);
        }
        new.mode = mode as u16;
    }
    if let Some(size) = size {
        match old.kind {
            InodeKind::Directory => return refuse(Refusal::IsDirectory),
            InodeKind::Symlink => return refuse(Refusal::Invalid),
            InodeKind::File if size > i64::MAX as u64 => return refuse(Refusal::TooLarge),
            InodeKind::File => {}
        }
        // The engine turns a smaller size into a logical cut-off: discarded
        // bytes are hidden at once and never read again after a regrow.
        if size != old.size {
            new.size = size;
            new.mtime_seconds = now.seconds;
            new.mtime_nanoseconds = now.nanoseconds;
        }
    }
    if let Some(time) = mtime {
        if time.nanoseconds >= 1_000_000_000 {
            return refuse(Refusal::Invalid);
        }
        new.mtime_seconds = time.seconds;
        new.mtime_nanoseconds = time.nanoseconds;
    }
    let changes = (new != old).then(|| Changes {
        inodes: vec![new.clone()],
        ..Changes::default()
    });
    Ok(Some((changes, new)))
}
