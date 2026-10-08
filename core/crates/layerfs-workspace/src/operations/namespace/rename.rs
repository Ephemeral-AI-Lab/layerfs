//! rename: one atomic move, with replacement, parent effects and cycle refusal.
use crate::eval::{refuse, touched, Eval};
use crate::remove::{dereferenced, unbind};
use crate::{Refusal, Time, WorkspaceResult};
use layerfs_content::filesystem::limits::MAXIMUM_PATH_COMPONENTS;
use layerfs_content::filesystem::PathName;
use layerfs_overlay::{Binding, Changes, DirectoryEntryChange, Inode, InodeKind};

pub(crate) struct Move<'a> {
    pub parent: u64,
    pub name: &'a PathName,
    pub new_parent: u64,
    pub new_name: &'a PathName,
    pub replace: bool,
    pub path: Option<&'a [PathName]>,
}
/// Proves the destination parent is not the moved directory or beneath it.
/// Directories have exactly one binding, so resolving the declared path from
/// the root in the current view is the destination's whole ancestry. The walk
/// is bounded by the canonical path-component limit and reads only point rows
/// plus immutable base facts; no reverse index or subtree walk is used.
fn outside(eval: &mut Eval<'_>, moved: u64, action: &Move<'_>) -> WorkspaceResult<Option<()>> {
    let Some(path) = action.path else {
        return refuse(Refusal::AncestryRequired);
    };
    if path.len() > MAXIMUM_PATH_COMPONENTS {
        return refuse(Refusal::Invalid);
    }
    let mut current = eval.root;
    for step in path {
        if current == moved {
            return refuse(Refusal::Invalid);
        }
        let local = eval.rows.inode(current)?;
        let layers = eval.layers(current, step)?;
        match eval.bound(current, local.as_ref(), step, layers) {
            None => return Ok(None),
            Some(None) => return refuse(Refusal::AncestryMismatch),
            Some(Some(next)) => current = next,
        }
    }
    if current != action.new_parent {
        return refuse(Refusal::AncestryMismatch);
    }
    if current == moved {
        return refuse(Refusal::Invalid);
    }
    Ok(Some(()))
}
/// Outer None: undecided this round. Inner None: successful with no change.
pub(crate) fn rename(
    eval: &mut Eval<'_>,
    action: Move<'_>,
    now: Time,
) -> WorkspaceResult<Option<Option<Changes>>> {
    let same = action.parent == action.new_parent;
    let from = eval.directory(action.parent)?;
    let to = if same {
        from.clone()
    } else {
        eval.directory(action.new_parent)?
    };
    let from_layers = eval.layers(action.parent, action.name)?;
    let moved = match eval.bound(action.parent, from.as_ref(), action.name, from_layers) {
        Some(Some(serial)) => eval.target(serial)?,
        Some(None) if from.is_some() => return refuse(Refusal::Missing),
        _ => None,
    };
    let to_layers = eval.layers(action.new_parent, action.new_name)?;
    let replaced: Option<Option<Inode>> =
        match eval.bound(action.new_parent, to.as_ref(), action.new_name, to_layers) {
            Some(Some(serial)) => eval.target(serial)?.map(Some),
            Some(None) => Some(None),
            None => None,
        };
    let inherited = eval.inherited(action.parent, from.as_ref(), action.name, from_layers);
    let proven = match &moved {
        Some(inode) if inode.kind == InodeKind::Directory && !same => {
            outside(eval, inode.serial, &action)?
        }
        _ => Some(()),
    };
    let (Some(from), Some(to), Some(moved), Some(replaced), Some(inherited), Some(())) =
        (from, to, moved, replaced, inherited, proven)
    else {
        return Ok(None);
    };
    if let Some(old) = &replaced {
        // Two names of one inode: POSIX leaves both and reports success.
        if old.serial == moved.serial {
            return Ok(Some(None));
        }
        if !action.replace {
            return refuse(Refusal::Exists);
        }
        match (
            moved.kind == InodeKind::Directory,
            old.kind == InodeKind::Directory,
        ) {
            (true, false) => return refuse(Refusal::NotDirectory),
            (false, true) => return refuse(Refusal::IsDirectory),
            (true, true) if old.entries != 0 => return refuse(Refusal::NotEmpty),
            _ => {}
        }
    }
    let gained = replaced.is_none();
    let mut inodes = if same {
        vec![touched(&from, now, gained, true)?]
    } else {
        vec![
            touched(&from, now, false, true)?,
            touched(&to, now, gained, false)?,
        ]
    };
    if let Some(old) = &replaced {
        inodes.push(dereferenced(old));
    }
    // The moved inode keeps its serial and attributes: no row for it, and no
    // row for anything beneath a moved directory.
    Ok(Some(Some(Changes {
        open: None,
        detached: None,
        moved_directory: (moved.kind == InodeKind::Directory && !same)
            .then_some((moved.serial, action.new_parent)),
        inodes,
        directory_entries: vec![
            unbind(action.parent, action.name, inherited),
            DirectoryEntryChange {
                parent: action.new_parent,
                name: action.new_name.as_bytes().to_vec(),
                binding: Binding::Bound {
                    serial: moved.serial,
                    inherited: false,
                },
            },
        ],
        cell: None,
        write: None,
    })))
}
