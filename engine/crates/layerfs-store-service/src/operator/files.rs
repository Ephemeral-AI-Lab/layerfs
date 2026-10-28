//! File revisions from persisted first-parent facts. No reconstructed UI history.
use super::args::{paging, text};
use crate::{
    history::lookup::{self as args, Answer},
    read::snapshot,
    store::{Store, StorePorts},
    write::files::path as checked,
};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::{
    compare::{EntryFacts, TreeTable},
    filesystem::FilesystemRootId,
};
use layerfs_history::NodeId;
use std::sync::Arc;

pub fn revision(
    store: &Arc<Store>,
    node: NodeId,
) -> Result<(layerfs_content::ObjectId, Option<NodeId>, SdkValue), SdkRefusal> {
    Ok(match node {
        NodeId::Commit(id) => {
            let r = args::commit(store, id)?;
            let branch = r
                .meta
                .branch
                .map(|id| store.history().branch(id))
                .transpose()
                .map_err(args::history)?
                .flatten();
            (
                r.root,
                Some(r.parent.map_or(NodeId::Layer(r.base_layer), NodeId::Commit)),
                args::commit_value(&r, branch.as_ref().map(|branch| branch.name.as_str())),
            )
        }
        NodeId::Layer(id) => {
            let r = args::layer(store, id)?;
            (
                r.root,
                r.source_commit
                    .map(NodeId::Commit)
                    .or(r.parent.map(NodeId::Layer)),
                args::layer_value(&r, None),
            )
        }
    })
}

pub fn entry(
    reader: &StorePorts,
    root: layerfs_content::ObjectId,
    path: &str,
) -> Result<Option<EntryFacts>, SdkRefusal> {
    let tree = TreeTable::open(reader, FilesystemRootId(root)).map_err(args::content)?;
    snapshot::entry(reader, &tree, path)
}

pub fn history(store: &Arc<Store>, args: &SdkValue) -> Answer {
    let (_, reader, snap) = super::args::snapshot(store, args)?;
    let path = checked(text(args, "path")?)?;
    let (limit, pos) = paging(args)?;
    let (mut next, mut seen, mut items, mut spent, mut more) =
        (Some(snap.node), 0u64, Vec::new(), 0, false);
    while let Some(node) = next {
        let (root, parent, record) = revision(store, node)?;
        let now = entry(&reader, root, &path)?;
        let before = match parent {
            Some(parent) => entry(&reader, revision(store, parent)?.0, &path)?,
            None => None,
        };
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
                now.map(|e| snapshot::size(&reader, &e))
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
    let path = checked(text(args, "path")?)?;
    let current = snapshot::entry(&reader, &snap.tree, &path)?;
    let size = current
        .map(|entry| snapshot::size(&reader, &entry))
        .transpose()?;
    // The operator explicitly caps this diagnostic, never silently reports a total.
    let mut count = 0i64;
    let mut logical = 0u64;
    let mut next = Some(snap.node);
    while let Some(node) = next {
        let (root, parent, _) = revision(store, node)?;
        let now = entry(&reader, root, &path)?;
        let before = match parent {
            Some(p) => entry(&reader, revision(store, p)?.0, &path)?,
            None => None,
        };
        next = parent;
        if now.map(|e| e.value.content_root) == before.map(|e| e.value.content_root) {
            continue;
        }
        count += 1;
        if let Some(e) = now {
            logical = logical.saturating_add(snapshot::size(&reader, &e)?);
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
