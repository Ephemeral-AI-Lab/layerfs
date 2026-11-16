//! Namespace entry counts at one immutable pin, under the saved-read budget.
use super::{query::Work, snapshot::Snap, visibility::Visibility, walk::Walker};
use crate::{
    history::{lookup as args, saved},
    store::{Store, StorePorts},
};
use layerfs_bridge::control::SdkValue;
use layerfs_content::object::inode_leaf::InodeKind;
use std::sync::Arc;

/// Root is excluded; links count by path and symlinks are never followed.
/// A path hidden by a `project.remove_file` marker is not part of the visible
/// tree, so it and its descendants are not counted. Logical bytes stay the
/// pinned publication's recorded total, which covers the whole saved tree even
/// when counting is incomplete or a path is hidden.
pub fn count(store: &Arc<Store>, reader: &StorePorts, snap: &Snap, project: &str) -> args::Answer {
    let mut walker = Walker::new(reader, snap, "/")?;
    let seen = Visibility::for_snap(store, snap);
    let mut work = Work::default();
    let (mut files, mut directories, mut symlinks) = (0, 0, 0);
    let mut reason = None;
    while let Some((path, entry)) = walker.next()? {
        if let Err(refusal) = work.step() {
            reason = Some(refusal.message);
            break;
        }
        if seen.hidden(&path).is_some() {
            continue;
        }
        match entry.value.kind {
            InodeKind::RegularFile => files += 1,
            InodeKind::Directory => directories += 1,
            InodeKind::Symlink => symlinks += 1,
        }
    }
    let record = saved::record(store, snap.node)?;
    let mut fields = vec![
        ("project", SdkValue::text(project)),
        ("at", saved::reference(store, snap.node)?),
        ("files", SdkValue::Int(files)),
        ("directories", SdkValue::Int(directories)),
        ("symlinks", SdkValue::Int(symlinks)),
        ("others", SdkValue::Int(0)),
        (
            "logical",
            record
                .get("logical")
                .cloned()
                .ok_or_else(|| args::failed("saved tree has no logical size"))?,
        ),
        ("complete", SdkValue::Bool(reason.is_none())),
    ];
    if let Some(reason) = reason {
        fields.push(("reason", SdkValue::Text(reason)));
    }
    Ok(SdkValue::Map(
        fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
    ))
}
