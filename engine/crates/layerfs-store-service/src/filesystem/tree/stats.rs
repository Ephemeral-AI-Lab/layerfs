//! Namespace entry counts at one immutable pin, under the saved-read budget.
use super::{
    super::{paged::query::Work, root::Snap},
    visibility::Visibility,
    walk::Walker,
};
use crate::{
    history::saved,
    lookup as args,
    store::{Store, StorePorts},
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_protocol::sdk::SdkValue;
use std::sync::Arc;

/// Root is excluded; links count by path and symlinks are never followed.
/// A path hidden by a `project.hide_path` marker is not part of the visible
/// tree, so it and its descendants are not counted. Logical bytes are the
/// pinned publication's recorded total (regular files, once per inode) less
/// the length of each hidden file with one link that the walk reached: the
/// whole visible tree when counting completes, the recorded total less the
/// hidden files seen so far when it stops at its bound.
pub fn count(store: &Arc<Store>, reader: &StorePorts, snap: &Snap, project: &str) -> args::Answer {
    let mut walker = Walker::new(reader, snap, "/")?;
    let seen = Visibility::for_snap(store, snap)?;
    let mut work = Work::default();
    let (mut files, mut directories, mut symlinks) = (0, 0, 0);
    let mut reason = None;
    let (mut hidden, mut hidden_bytes) = (Vec::new(), 0u64);
    while let Some((path, entry)) = walker.next()? {
        if let Err(refusal) = work.step() {
            reason = Some(refusal.message);
            break;
        }
        if seen.hidden(&path).is_some() {
            let value = &entry.value;
            if value.kind == InodeKind::RegularFile && value.namespace_ref_count <= 1 {
                hidden.push(value.content_root);
            }
            if hidden.len() == HIDDEN_WINDOW {
                hidden_bytes += lengths(reader, &mut hidden)?;
            }
            continue;
        }
        match entry.value.kind {
            InodeKind::RegularFile => files += 1,
            InodeKind::Directory => directories += 1,
            InodeKind::Symlink => symlinks += 1,
        }
    }
    hidden_bytes += lengths(reader, &mut hidden)?;
    let record = saved::record(store, snap.node)?;
    let logical = (record.get_int("logical"))
        .ok_or_else(|| args::failed("saved tree has no logical size"))?;
    let logical = logical.saturating_sub(i64::try_from(hidden_bytes).unwrap_or(i64::MAX));
    let mut fields = vec![
        ("project", SdkValue::text(project)),
        ("at", saved::reference(store, snap.node)?),
        ("files", SdkValue::Int(files)),
        ("directories", SdkValue::Int(directories)),
        ("symlinks", SdkValue::Int(symlinks)),
        ("others", SdkValue::Int(0)),
        ("logical", SdkValue::Int(logical.max(0))),
        ("complete", SdkValue::Bool(reason.is_none())),
    ];
    if let Some(reason) = reason {
        fields.push(("reason", SdkValue::Text(reason)));
    }
    Ok(SdkValue::Map(
        fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
    ))
}
/// Hidden file roots whose lengths one demand asks: a bounded window.
const HIDDEN_WINDOW: usize = 256;
/// The summed lengths of the file roots in `roots`, which it empties.
fn lengths(
    reader: &StorePorts,
    roots: &mut Vec<layerfs_content::object::ObjectId>,
) -> Result<u64, layerfs_protocol::sdk::SdkRefusal> {
    if roots.is_empty() {
        return Ok(0);
    }
    let lengths = reader.file_lengths(roots).map_err(args::failed)?;
    roots.clear();
    Ok(lengths.iter().sum())
}
