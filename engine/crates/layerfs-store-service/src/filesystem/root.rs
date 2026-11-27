//! Saved snapshots the read calls name, and path lookups in them.
//!
//! A read names `main`, a Branch, or a Commit or Layer identity. `main` is the
//! stack's head Layer and a Branch is its newest node. Reads resolve paths by
//! walking names from the root of the snapshot's immutable tree; nothing here
//! reads a live Workspace.
use crate::filesystem::cache::Cached;
use crate::history::MAIN;
use crate::lookup::{self as args, branch_node, invalid, refuse, short, Answer};
use crate::store::{PortError, Store, StorePorts};
use layerfs_content::compare::{as_text, EntryFacts, TreeTable, MAXIMUM_TEXT_BYTES};
use layerfs_content::filesystem::name::path::PathName;
use layerfs_content::filesystem::{FilesystemRootId, SymlinkTarget};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{AuthenticatedObjects, ContentError, ObjectId};
use layerfs_history::{BranchRecord, LayerStackRecord, NodeId, PathRemovalRecord, ReadContext};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
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
    /// The node's `history_serial`, for the `remove_file` visibility rule.
    pub serial: u64,
}

/// A node's serial with its project's `project.remove_file` markers, read
/// together; `None` when the node has no row.
pub type Removals = Result<Option<(u64, Vec<PathRemovalRecord>)>, Arc<PortError>>;

/// What [`resolve_saved`] read on the way, for the call's later steps
/// (#818 R2): the node's record, from which `at` is built without reading
/// it again, and its serial with the markers, which the call's
/// [`Visibility`](super::visibility::Visibility) takes at the place it read
/// them before, so a failed read still refuses there. `Snap` keeps its
/// fields: the daemon builds one of its own.
pub struct Saved {
    pub record: args::NodeRecord,
    pub removals: Removals,
    /// A Commit's recorded Branch name when the context read it (#818 R5);
    /// `None` when not read, and the answer's `at` reads it then.
    pub recorded: Option<Option<String>>,
}

/// The snapshot a read names, with its history context when one call read
/// it (#818 R5).
pub struct Named {
    pub name: String,
    pub context: Option<ReadContext>,
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
    resolve_in(store, stack, reader, name)
}

/// [`resolve`] with the root's tree opened through `pages`, such as a
/// call's caches (#818 H3).
pub fn resolve_in(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    pages: &dyn AuthenticatedObjects,
    name: &str,
) -> Result<Snap, SdkRefusal> {
    let (snap, _, ()) = resolve_with(store, stack, pages, name, |node| {
        let serial = match store.published(|history| history.node_history_serial(node)) {
            Ok(Some(serial)) => serial,
            _ => 0,
        };
        (serial, ())
    })?;
    Ok(snap)
}

/// [`resolve`] for a read call, which keeps what it read: the node's
/// record, and its serial read with the project's markers in one statement
/// where the serial alone was read (#818 R2). A failed or missing read
/// gives serial 0 here, as [`resolve`] does; [`Saved::removals`] keeps the
/// outcome for the marker step.
///
/// `pages` opens the root's tree: the call's caches, so the call's later
/// demands find what the open read (#818 R4).
pub fn resolve_saved(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    pages: &dyn AuthenticatedObjects,
    name: &str,
) -> Result<(Snap, Saved), SdkRefusal> {
    let (snap, record, removals) = resolve_with(store, stack, pages, name, |node| {
        removals_of(store, stack, node)
    })?;
    let saved = Saved {
        record,
        removals,
        recorded: None,
    };
    Ok((snap, saved))
}

/// [`resolve_saved`] from the history `context` one call read (#818 R5),
/// refused in the same order: the Branch, the node, its project, the
/// root's tree. A context without markers (SQLite's default) reads them
/// here, where [`resolve_saved`] does.
pub fn resolve_context(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    pages: &dyn AuthenticatedObjects,
    name: &str,
    context: ReadContext,
) -> Result<(Snap, Saved), SdkRefusal> {
    let ReadContext {
        branch,
        node: found,
        removals: read,
        recorded,
        ..
    } = context;
    let named = named(stack, name, |wanted| {
        args::branch_in(wanted, |_| Ok(branch))
    })?;
    let record = found.ok_or_else(|| args::absent(named.2))?;
    // A part the context read but could not decode refuses where it was
    // read before: the markers at the marker step (`Visibility::load`), the
    // recorded Branch where the call reads it (a fresh read of that row).
    let (snap, removals) = opened(stack, pages, named, &record, |node| match read {
        Some(found) => {
            let serial = match &found {
                Ok(Some((serial, _))) => *serial,
                _ => 0,
            };
            let found = found.map_err(|error| Arc::new(PortError::History(error)));
            (serial, found)
        }
        None => removals_of(store, stack, node),
    })?;
    let recorded = recorded
        .and_then(Result::ok)
        .map(|branch| branch.map(|b: BranchRecord| b.name.as_str().to_owned()));
    let saved = Saved {
        record,
        removals,
        recorded,
    };
    Ok((snap, saved))
}

/// The node's serial read with the stack's markers in one statement; a
/// failed or missing read gives serial 0, the outcome kept for the marker
/// step.
fn removals_of(store: &Arc<Store>, stack: &LayerStackRecord, node: NodeId) -> (u64, Removals) {
    let removals = store.published(|history| history.node_path_removals(stack.id, node));
    let serial = match &removals {
        Ok(Some((serial, _))) => *serial,
        _ => 0,
    };
    (serial, removals)
}

/// The shared resolve: the name, the node's record (its ownership check
/// and its root, one read), the root's tree, then `position`, the node's
/// serial with whatever the caller reads beside it.
fn resolve_with<T>(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    pages: &dyn AuthenticatedObjects,
    name: &str,
    position: impl FnOnce(NodeId) -> (u64, T),
) -> Result<(Snap, args::NodeRecord, T), SdkRefusal> {
    let named = named(stack, name, |wanted| args::branch(store, stack, wanted))?;
    let record = args::node_record(store, named.2)?;
    let (snap, beside) = opened(stack, pages, named, &record, position)?;
    Ok((snap, record, beside))
}

/// What `name` names: `main`, a full Commit or Layer identity, else a
/// Branch, found by `branch`.
fn named(
    stack: &LayerStackRecord,
    name: &str,
    branch: impl FnOnce(&str) -> Result<BranchRecord, SdkRefusal>,
) -> Result<(Kind, String, NodeId), SdkRefusal> {
    Ok(if name == MAIN {
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
        let branch = branch(name).map_err(|refusal| {
            // A hex-shaped name of 8 or more characters (as long as a
            // commit.get prefix) that is no Branch is a truncated or
            // wrong-length identity, not a missing Branch; a shorter one
            // stays a Branch name.
            let identity_shaped = name.len() >= 8
                && name.chars().all(|c| c.is_ascii_hexdigit())
                && name.len() != NODE_TEXT;
            if refusal.code == "NoSuchBranch" && identity_shaped {
                invalid(format!(
                    "`{name}` is not a Branch, and not a full Commit or Layer \
                     identity (66 hex characters)"
                ))
            } else {
                refusal
            }
        })?;
        let node = branch_node(&branch);
        (Kind::Branch, format!("{name}@{}", short(node)), node)
    })
}

/// The node's tree: its ownership check, its root's tree opened through
/// `pages`, then `position`.
fn opened<T>(
    stack: &LayerStackRecord,
    pages: &dyn AuthenticatedObjects,
    (kind, label, node): (Kind, String, NodeId),
    record: &args::NodeRecord,
    position: impl FnOnce(NodeId) -> (u64, T),
) -> Result<(Snap, T), SdkRefusal> {
    if record.stack() != stack.id {
        return Err(invalid("saved version belongs to another project"));
    }
    let root = record.root();
    let tree = TreeTable::open(pages, FilesystemRootId(root)).map_err(args::content)?;
    let (serial, beside) = position(node);
    let snap = Snap {
        kind,
        label,
        node,
        root,
        tree,
        serial,
    };
    Ok((snap, beside))
}

/// The entry at a checked absolute path, if any.
///
/// Only a component before the last that is a symlink is refused
/// `InvalidInput` (spec 4.3): a symlink is not followed, so a name below it
/// is ill-typed, not absent, and `/link/f` must not read as an ordinary
/// missing path. A regular-file (or removed-file) component cannot contain
/// the next name, so the path resolves as absent (`NoSuchPath`/`NotFound`).
pub fn entry(
    reader: &StorePorts,
    tree: &TreeTable,
    path: &str,
) -> Result<Option<EntryFacts>, SdkRefusal> {
    lookup(reader, tree, path)
}

/// [`entry`] through any object reader, such as a call's memo (#818 R3).
pub fn lookup(
    pages: &dyn AuthenticatedObjects,
    tree: &TreeTable,
    path: &str,
) -> Result<Option<EntryFacts>, SdkRefusal> {
    find_with(pages, tree, path, true, &mut |serial, name| {
        tree.lookup(pages, serial, name)
    })
}

/// [`lookup`] with each name bound by `bind` (directory serial, name) in
/// place of the tree's own lookup: a history call's directory memo
/// (#818 H3). Refusals are [`lookup`]'s.
pub fn lookup_bound(
    pages: &dyn AuthenticatedObjects,
    tree: &TreeTable,
    path: &str,
    bind: &mut dyn FnMut(u64, &PathName) -> layerfs_content::ContentResult<Option<u64>>,
) -> Result<Option<EntryFacts>, SdkRefusal> {
    find_with(pages, tree, path, true, bind)
}

/// [`entry`] through any object reader, such as a call's memo. A component
/// before the last that is not a directory reads as absent, for callers that
/// build a path from the tree's own directory listings.
pub fn find(
    reader: &dyn AuthenticatedObjects,
    tree: &TreeTable,
    path: &str,
) -> Result<Option<EntryFacts>, SdkRefusal> {
    find_with(reader, tree, path, false, &mut |serial, name| {
        tree.lookup(reader, serial, name)
    })
}

/// The shared path walk. `strict` refuses a symlink component before the last
/// instead of reading past it as an absent path; any other non-directory
/// component makes the rest of the walk absent, because it cannot contain the
/// next name.
fn find_with(
    reader: &dyn AuthenticatedObjects,
    tree: &TreeTable,
    path: &str,
    strict: bool,
    bind: &mut dyn FnMut(u64, &PathName) -> layerfs_content::ContentResult<Option<u64>>,
) -> Result<Option<EntryFacts>, SdkRefusal> {
    let mut serial = tree.root_serial();
    let mut names = path.split('/').filter(|name| !name.is_empty()).peekable();
    while let Some(name) = names.next() {
        let name = PathName::new(name).map_err(|_| invalid(format!("`{path}` is not valid")))?;
        match bind(serial, &name).map_err(args::content)? {
            Some(child) => serial = child,
            None => return Ok(None),
        }
        if strict && names.peek().is_some() {
            let value = tree
                .value(reader, serial)
                .map_err(args::content)?
                .ok_or_else(|| args::failed("bound inode record"))?;
            // Spec 4.3: only an intermediate symlink is ill-typed. A regular
            // file cannot contain the next name either, but that is the
            // absent path a removed-file child also reports: `lookup` returns
            // None for a non-directory parent and the walk ends there.
            if value.kind == InodeKind::Symlink {
                return Err(invalid(format!(
                    "`{}` is a symlink; `{path}` cannot be resolved",
                    name.as_str()
                )));
            }
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
    found(snap, path, lookup(reader, &snap.tree, path)?)
}

/// The entry a strict lookup found at `path`, refused as [`existing`]
/// refuses an absent one.
pub fn found(snap: &Snap, path: &str, entry: Option<EntryFacts>) -> Result<EntryFacts, SdkRefusal> {
    entry.ok_or_else(|| {
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

/// [`size`] through the call's caches (#818 R4): a file's length state and
/// a symlink's target can come from a pack the call already holds.
pub fn size_in(cached: &Cached, entry: &EntryFacts) -> Result<u64, SdkRefusal> {
    match entry.value.kind {
        InodeKind::Directory => Ok(0),
        InodeKind::RegularFile => cached
            .lengths(&[entry.value.content_root])?
            .pop()
            .ok_or_else(|| args::failed("missing file length")),
        InodeKind::Symlink => cached
            .read_canonical(entry.value.content_root)
            .and_then(|canonical| SymlinkTarget::decode(&canonical))
            .map(|target| target.as_bytes().len() as u64)
            .map_err(args::content),
    }
}

/// A regular file's text within the text bound; `None` for anything else, a
/// binary file or a larger one.
pub fn text(
    reader: &dyn AuthenticatedObjects,
    entry: Option<EntryFacts>,
) -> Result<Option<String>, SdkRefusal> {
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
