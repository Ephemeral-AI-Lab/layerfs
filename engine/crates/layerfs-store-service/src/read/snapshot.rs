//! Saved snapshots the read calls name, and path lookups in them.
//!
//! A read names `main`, a Branch, or a Commit or Layer identity. `main` is the
//! stack's head Layer and a Branch is its newest node. Reads resolve paths by
//! walking names from the root of the snapshot's immutable tree; nothing here
//! reads a live Workspace.
use crate::history::lookup::{self as args, branch_node, invalid, refuse, short, Answer};
use crate::history::MAIN;
use crate::store::{Store, StorePorts};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::compare::{as_text, EntryFacts, TreeTable, MAXIMUM_TEXT_BYTES};
use layerfs_content::filesystem::path::PathName;
use layerfs_content::filesystem::{FilesystemRootId, SymlinkTarget};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{AuthenticatedObjects, ContentError, ObjectId};
use layerfs_history::{LayerStackRecord, NodeId};
use std::sync::Arc;

/// Width of one identity's hex text.
const NODE_TEXT: usize = 66;

/// What a snapshot name resolved to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Main,
    Branch,
    Commit,
    Layer,
}

/// One resolved snapshot.
pub struct Snap {
    pub kind: Kind,
    pub label: String,
    pub node: NodeId,
    pub root: ObjectId,
    pub tree: TreeTable,
}

/// The stack's object reader.
pub fn reader(store: &Arc<Store>, stack: &LayerStackRecord) -> Arc<StorePorts> {
    store.ports(stack.scope)
}

/// Resolves `main`, a Branch name, or a Commit or Layer identity.
pub fn resolve(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    reader: &StorePorts,
    name: &str,
) -> Result<Snap, SdkRefusal> {
    let (kind, label, node) = if name == MAIN {
        let node = NodeId::Layer(stack.head_layer);
        (Kind::Main, format!("main@{}", short(node)), node)
    } else if name.len() == NODE_TEXT && args::unhex(name).is_some() {
        let node = args::node(name)?;
        let kind = match node {
            NodeId::Commit(_) => Kind::Commit,
            NodeId::Layer(_) => Kind::Layer,
        };
        (kind, short(node), node)
    } else {
        let branch = args::branch(store, stack, name)?;
        let node = branch_node(&branch);
        (Kind::Branch, format!("{name}@{}", short(node)), node)
    };
    let root = args::root(store, node)?;
    let tree = TreeTable::open(reader, FilesystemRootId(root)).map_err(args::content)?;
    Ok(Snap {
        kind,
        label,
        node,
        root,
        tree,
    })
}

/// The entry at a checked absolute path, if any.
pub fn entry(
    reader: &StorePorts,
    tree: &TreeTable,
    path: &str,
) -> Result<Option<EntryFacts>, SdkRefusal> {
    find(reader, tree, path)
}

/// [`entry`] through any object reader, such as a call's memo.
pub fn find(
    reader: &dyn AuthenticatedObjects,
    tree: &TreeTable,
    path: &str,
) -> Result<Option<EntryFacts>, SdkRefusal> {
    let mut serial = tree.root_serial();
    for name in path.split('/').filter(|name| !name.is_empty()) {
        let name = PathName::new(name).map_err(|_| invalid(format!("`{path}` is not valid")))?;
        match tree.lookup(reader, serial, &name).map_err(args::content)? {
            Some(child) => serial = child,
            None => return Ok(None),
        }
    }
    Ok(tree
        .value(reader, serial)
        .map_err(args::content)?
        .map(|value| EntryFacts { serial, value }))
}

/// The entry at a path, refused when absent.
pub fn existing(
    reader: &dyn AuthenticatedObjects,
    snap: &Snap,
    path: &str,
) -> Result<EntryFacts, SdkRefusal> {
    find(reader, &snap.tree, path)?.ok_or_else(|| {
        refuse(
            "NoSuchPath",
            format!("{path} does not exist at {}", snap.label),
            SdkValue::map([("path", SdkValue::text(path))]),
        )
    })
}

/// The SDK name of an inode kind.
pub fn kind_name(kind: InodeKind) -> &'static str {
    match kind {
        InodeKind::RegularFile => "file",
        InodeKind::Directory => "dir",
        InodeKind::Symlink => "symlink",
    }
}

/// Logical size of a file or symlink's content; a directory is 0. A regular
/// file's length is Store metadata: no payload or delta chain is read. A
/// symlink's is its target's length, as a mounted Workspace reports it, not
/// the length of the object that stores the target.
pub fn size(reader: &StorePorts, entry: &EntryFacts) -> Result<u64, SdkRefusal> {
    match entry.value.kind {
        InodeKind::Directory => Ok(0),
        InodeKind::RegularFile => reader
            .file_lengths(&[entry.value.content_root])
            .map_err(args::failed)?
            .pop()
            .ok_or_else(|| args::failed("missing file length")),
        InodeKind::Symlink => {
            layerfs_content::AuthenticatedObjects::read_canonical(reader, entry.value.content_root)
                .and_then(|canonical| SymlinkTarget::decode(&canonical))
                .map(|target| target.as_bytes().len() as u64)
                .map_err(args::content)
        }
    }
}

/// A regular file's text within the text bound; `None` for anything else, a
/// binary file or a larger one.
pub fn text(reader: &StorePorts, entry: Option<EntryFacts>) -> Result<Option<String>, SdkRefusal> {
    let Some(entry) = entry.filter(|entry| entry.value.kind == InodeKind::RegularFile) else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    let read = crate::telemetry::timed("sdk.read", |timing| {
        layerfs_content::file::read_all_bounded(
            reader,
            entry.value.content_root,
            MAXIMUM_TEXT_BYTES,
            &mut bytes,
            timing.child("content.read"),
        )
    });
    match read {
        Ok(_) => Ok(as_text(&bytes).map(str::to_owned)),
        Err(ContentError::BoundedCapacityExceeded { .. }) => Ok(None),
        Err(error) => Err(args::content(error)),
    }
}

/// The stack's reader and the snapshot `name` resolves to, for the reads.
pub fn open(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    name: &str,
) -> Result<(Arc<StorePorts>, Snap), SdkRefusal> {
    let reader = reader(store, stack);
    let snap = resolve(store, stack, &reader, name)?;
    Ok((reader, snap))
}

/// The result shape every read shares.
pub fn header(snap: &Snap, path: &str) -> Vec<(&'static str, SdkValue)> {
    vec![
        ("kind", SdkValue::text("ok")),
        ("snapshot", SdkValue::text(snap.label.as_str())),
        ("root", SdkValue::text(args::hex(snap.root.as_bytes()))),
        ("path", SdkValue::text(path)),
    ]
}

/// A finished map from fields.
pub fn answer(fields: Vec<(&'static str, SdkValue)>) -> Answer {
    Ok(SdkValue::Map(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    ))
}
