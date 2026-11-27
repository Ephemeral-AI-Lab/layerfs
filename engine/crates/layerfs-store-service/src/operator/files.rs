//! File revisions from persisted first-parent facts. No reconstructed UI history.
use super::args::{paging, text};
use crate::history::rows::{Revision, Rows};
use crate::read::cached::Cached;
use crate::{
    history::lookup::{self as args, Answer},
    read::snapshot,
    store::{Store, StorePorts},
    write::files::path as checked,
};
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_content::{
    compare::{EntryFacts, TreeTable},
    filesystem::FilesystemRootId,
};
use layerfs_history::NodeId;
use std::sync::Arc;

/// The node's root, parent and record value, read on their own; a walk
/// reads them through one [`Rows`] (#818 H1).
pub fn revision(store: &Arc<Store>, node: NodeId) -> Result<Revision, SdkRefusal> {
    Rows::new(store).revision(node)
}

pub fn entry(
    reader: &StorePorts,
    root: layerfs_content::ObjectId,
    path: &str,
) -> Result<Option<EntryFacts>, SdkRefusal> {
    let tree = TreeTable::open(reader, FilesystemRootId(root)).map_err(args::content)?;
    snapshot::entry(reader, &tree, path)
}

/// A walk's parent side: the node and the path's entry there.
type Carried = Option<(NodeId, Option<EntryFacts>)>;

/// The path's entry at `node` (its `root`) and at its `parent`. The parent's
/// is kept in `carried`: it is the next node's own, read once (#818 H2).
#[allow(clippy::type_complexity)]
fn sides(
    reader: &Cached<'_>,
    rows: &mut Rows<'_>,
    carried: &mut Carried,
    node: NodeId,
    root: layerfs_content::ObjectId,
    parent: Option<NodeId>,
    path: &str,
) -> Result<(Option<EntryFacts>, Option<EntryFacts>), SdkRefusal> {
    let now = match carried.take() {
        Some((held, found)) if held == node => found,
        _ => reader.entry(root, path)?,
    };
    let before = match parent {
        Some(parent) => {
            let found = reader.entry(rows.record(parent)?.root(), path)?;
            *carried = Some((parent, found));
            found
        }
        None => None,
    };
    Ok((now, before))
}

pub fn history(store: &Arc<Store>, args: &SdkValue) -> Answer {
    let (_, reader, snap) = super::args::snapshot(store, args)?;
    // Every version's objects through the call's caches (#818 H3).
    let cached = Cached::new(&reader)?;
    let path = checked(text(args, "path")?)?;
    let (limit, pos) = paging(args)?;
    let (mut next, mut seen, mut items, mut spent, mut more) =
        (Some(snap.node), 0u64, Vec::new(), 0, false);
    let (mut rows, mut carried) = (Rows::new(store), None);
    while let Some(node) = next {
        let (root, parent, record) = rows.revision(node)?;
        let (now, before) = sides(&cached, &mut rows, &mut carried, node, root, parent, &path)?;
        next = parent;
        let same = match (now, before) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                a.value.content_root == b.value.content_root && a.value.kind == b.value.kind
            }
            _ => false,
        };
        if same {
            continue;
        }
        if seen < pos {
            seen += 1;
            continue;
        }
        let cost = record.get_text("message").map_or(0, str::len) + 1024;
        if items.len() == limit || spent + cost > crate::read::window::BYTES {
            more = true;
            break;
        }
        spent += cost;
        items.push(SdkValue::map([
            ("commit", SdkValue::text(args::node_text(node))),
            (
                "message",
                record.get("message").cloned().unwrap_or_default(),
            ),
            (
                "change",
                SdkValue::text(match (before, now) {
                    (None, _) => "Added",
                    (_, None) => "Removed",
                    _ => "Modified",
                }),
            ),
            (
                "size",
                now.map(|e| snapshot::size_in(&cached, &e))
                    .transpose()?
                    .map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
            ),
            (
                "object",
                now.map_or(SdkValue::Null, |e| {
                    SdkValue::text(args::hex(e.value.content_root.as_bytes()))
                }),
            ),
            ("newCanonical", SdkValue::Null),
            ("newStored", SdkValue::Null),
        ]));
    }
    let next = args::next_page(pos, items.len(), more);
    Ok(SdkValue::map([
        ("items", SdkValue::List(items)),
        ("hasMore", SdkValue::Bool(!matches!(next, SdkValue::Null))),
        ("nextPagePos", next),
    ]))
}

pub fn stats(store: &Arc<Store>, args: &SdkValue) -> Answer {
    let (_, reader, snap) = super::args::snapshot(store, args)?;
    let cached = Cached::new(&reader)?;
    let path = checked(text(args, "path")?)?;
    let current = snapshot::entry(&reader, &snap.tree, &path)?;
    let size = current
        .map(|entry| snapshot::size_in(&cached, &entry))
        .transpose()?;
    // The operator explicitly caps this diagnostic, never silently reports a total.
    let mut count = 0i64;
    let mut logical = 0u64;
    let mut next = Some(snap.node);
    let (mut rows, mut carried) = (Rows::new(store), None);
    while let Some(node) = next {
        let (root, parent, _) = rows.revision(node)?;
        let (now, before) = sides(&cached, &mut rows, &mut carried, node, root, parent, &path)?;
        next = parent;
        if now.map(|e| e.value.content_root) == before.map(|e| e.value.content_root) {
            continue;
        }
        count += 1;
        if let Some(e) = now {
            logical = logical.saturating_add(snapshot::size_in(&cached, &e)?);
        }
        if count == 1000 {
            break;
        }
    }
    Ok(SdkValue::map([
        (
            "size",
            size.map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
        ),
        ("revisions", SdkValue::Int(count)),
        ("revisionsMore", SdkValue::Bool(next.is_some())),
        (
            "logical",
            if next.is_some() {
                SdkValue::Null
            } else {
                SdkValue::Int(logical as i64)
            },
        ),
        ("canonical", SdkValue::Null),
        ("stored", SdkValue::Null),
        ("metadata", SdkValue::Null),
    ]))
}
