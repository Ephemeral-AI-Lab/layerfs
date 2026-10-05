//! unlink and rmdir: one removed binding, its inode reference and its parent.
use crate::eval::{refuse, touched, Eval};
use crate::{Refusal, Time, WorkspaceResult};
use layerfs_content::filesystem::PathName;
use layerfs_overlay::{Binding, Changes, Inode, InodeKind, NameChange};

pub(crate) fn unbind(parent: u64, name: &PathName, inherited: bool) -> NameChange {
    NameChange {
        parent,
        name: name.as_bytes().to_vec(),
        binding: Binding::Removed { inherited },
    }
}
/// The inode after losing one name. A removed inode keeps its row with no
/// references so serial-addressed access cannot fall through to the base;
/// reclaiming that row once nothing holds it is the lifetime slice's work.
pub(crate) fn dereferenced(inode: &Inode) -> Inode {
    Inode {
        nlink: if inode.kind == InodeKind::Directory {
            0
        } else {
            inode.nlink.saturating_sub(1)
        },
        ..inode.clone()
    }
}
pub(crate) fn remove(
    eval: &mut Eval<'_>,
    parent: u64,
    name: &PathName,
    directory_expected: bool,
    now: Time,
) -> WorkspaceResult<Option<Changes>> {
    let directory = eval.directory(parent)?;
    let layers = eval.layers(parent, name)?;
    let bound = eval.bound(parent, directory.as_ref(), name, layers);
    let target = match bound {
        Some(Some(serial)) => eval.target(serial)?,
        Some(None) if directory.is_some() => return refuse(Refusal::Missing),
        _ => None,
    };
    let inherited = eval.inherited(parent, directory.as_ref(), name, layers);
    let (Some(directory), Some(target), Some(inherited)) = (directory, target, inherited) else {
        return Ok(None);
    };
    match (target.kind == InodeKind::Directory, directory_expected) {
        (true, false) => return refuse(Refusal::IsDirectory),
        (false, true) => return refuse(Refusal::NotDirectory),
        (true, true) if target.entries != 0 => return refuse(Refusal::NotEmpty),
        _ => {}
    }
    Ok(Some(Changes {
        inodes: vec![
            dereferenced(&target),
            touched(&directory, now, false, true)?,
        ],
        names: vec![unbind(parent, name, inherited)],
        cell: None,
        write: None,
    }))
}
