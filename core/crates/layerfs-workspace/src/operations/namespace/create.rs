//! create, mkdir, symlink and link: one new binding and its parent effects.
use crate::eval::{refuse, touched, Eval};
use crate::{Refusal, Time, WorkspaceResult};
use layerfs_content::filesystem::{PathName, SymlinkTarget};
use layerfs_overlay::{
    Binding, Cell, Changes, DirectoryEntryChange, Inode, InodeKind, CELL_BYTES, MASK_BYTES,
};

/// The new inode's kind-specific values, checked before any owner job.
pub(crate) struct Fresh<'a> {
    pub serial: u64,
    pub kind: InodeKind,
    pub mode: u32,
    pub target: Option<&'a SymlinkTarget>,
}
fn free_name(eval: &mut Eval<'_>, parent: u64, name: &PathName) -> WorkspaceResult<Option<Inode>> {
    let directory = eval.directory(parent)?;
    let layers = eval.layers(parent, name)?;
    let bound = eval.bound(parent, directory.as_ref(), name, layers);
    let (Some(directory), Some(bound)) = (directory, bound) else {
        return Ok(None);
    };
    if bound.is_some() {
        return refuse(Refusal::Exists);
    }
    Ok(Some(directory))
}
fn bind(parent: u64, name: &PathName, serial: u64) -> DirectoryEntryChange {
    DirectoryEntryChange {
        parent,
        name: name.as_bytes().to_vec(),
        binding: Binding::Bound {
            serial,
            inherited: false,
        },
    }
}
pub(crate) fn create(
    eval: &mut Eval<'_>,
    parent: u64,
    name: &PathName,
    fresh: Fresh<'_>,
    now: Time,
) -> WorkspaceResult<Option<(Changes, Inode)>> {
    let allowed = match fresh.kind {
        InodeKind::Directory => 0o1777,
        _ => 0o777,
    };
    if fresh.mode & !allowed != 0 {
        return refuse(Refusal::NotPermitted);
    }
    let target = fresh.target.map(SymlinkTarget::as_bytes).unwrap_or(&[]);
    if fresh.kind == InodeKind::Symlink && target.is_empty() {
        return refuse(Refusal::Missing);
    }
    let Some(directory) = free_name(eval, parent, name)? else {
        return Ok(None);
    };
    let inode = Inode {
        serial: fresh.serial,
        kind: fresh.kind,
        mode: fresh.mode as u16,
        mtime_seconds: now.seconds,
        mtime_nanoseconds: now.nanoseconds,
        nlink: 1,
        size: target.len() as u64,
        inherited_cutoff: 0,
        born: eval.rows.active().number() as u64,
        entries: 0,
    };
    // A symlink target is at most one cell and never changes after creation.
    let cell = (fresh.kind == InodeKind::Symlink).then(|| {
        let mut data = Box::new([0_u8; CELL_BYTES]);
        data[..target.len()].copy_from_slice(target);
        let mut validity = Box::new([0_u8; MASK_BYTES]);
        for bit in 0..target.len() {
            validity[bit / 8] |= 1 << (bit % 8);
        }
        (
            fresh.serial,
            Cell {
                offset: 0,
                data,
                validity,
            },
        )
    });
    let changes = Changes {
        open: None,
        // The serial was reserved for this job: nothing holds a row of it.
        created: Some(fresh.serial),
        detached: None,
        moved_directory: None,
        inodes: vec![inode.clone(), touched(&directory, now, true, false)?],
        directory_entries: vec![bind(parent, name, fresh.serial)],
        cell,
        write: None,
    };
    Ok(Some((changes, inode)))
}
pub(crate) fn link(
    eval: &mut Eval<'_>,
    serial: u64,
    parent: u64,
    name: &PathName,
    now: Time,
) -> WorkspaceResult<Option<(Changes, Inode)>> {
    let file = eval.existing(serial)?;
    if file
        .as_ref()
        .is_some_and(|inode| inode.kind != InodeKind::File)
    {
        return refuse(Refusal::NotPermitted);
    }
    let directory = free_name(eval, parent, name)?;
    let (Some(file), Some(directory)) = (file, directory) else {
        return Ok(None);
    };
    if file.nlink >= i64::MAX as u64 {
        return refuse(Refusal::TooManyLinks);
    }
    // Portable metadata has no separate ctime: a new link leaves mtime alone.
    let file = Inode {
        nlink: file.nlink + 1,
        ..file
    };
    let changes = Changes {
        open: None,
        created: None,
        detached: None,
        moved_directory: None,
        inodes: vec![file.clone(), touched(&directory, now, true, false)?],
        directory_entries: vec![bind(parent, name, serial)],
        cell: None,
        write: None,
    };
    Ok(Some((changes, file)))
}
