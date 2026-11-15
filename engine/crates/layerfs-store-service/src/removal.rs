//! Whiteout materialization for `project.remove_file` (§4.3).
//!
//! A marker hides an entry at a node at or below its frontier. A tree built
//! from such a node must record the *absence* explicitly, because the new
//! node is above the frontier and the read filter no longer hides the
//! inherited entry. This module derives that tree:
//!
//! - [`derive_root`] removes every path the marker hides at `node`; a merge
//!   applies it to its base and to each side before the three-way merge, so a
//!   side's hidden content is absent there and a post-frontier re-creation is
//!   an ordinary addition that wins.
//! - [`materialize_commit`] compares the bound base with the candidate a
//!   Commit constructed and removes only entries the candidate inherited
//!   unchanged (same inode serial and value); a re-created path keeps its new
//!   inode and stays visible.
//! - [`BoundWorkspace::materialize_removals`] writes the whiteout into a
//!   mounted Workspace's overlay, so its view hides the path and a one-shot
//!   `write`/`rm`/`mv`/`cp` sees the same absence the committed tree has.

use crate::store::{BoundWorkspace, CommitError, PortError, Store};
use crate::write::mutation::Mutation;
use layerfs_bridge::control::SdkRefusal;
use layerfs_content::compare::tree::join;
use layerfs_content::compare::{list_directory, same_value, EntryFacts, TreeTable};
use layerfs_content::filesystem::{
    update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources,
    FilesystemRootId, PathName,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{AuthenticatedObjects, ContentError};
use layerfs_history::path::governs;
use layerfs_history::{BranchSnapshot, HistoryError, LayerStackId, NodeId, PathRemovalRecord};
use layerfs_storage::Save;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Why a derived tree could not be built.
#[derive(Debug)]
pub enum RemovalError {
    /// The marker or position lookup was refused.
    History(HistoryError),
    /// The Store reader was refused.
    Ports(Arc<PortError>),
    /// The tree update was refused.
    Content(ContentError),
}

impl From<ContentError> for RemovalError {
    fn from(error: ContentError) -> Self {
        Self::Content(error)
    }
}

/// The refusal a materialization failure is to a public call.
pub fn refusal(error: RemovalError) -> SdkRefusal {
    match error {
        RemovalError::History(error) => crate::history::lookup::history(error),
        RemovalError::Ports(error) => crate::history::lookup::failed(error),
        RemovalError::Content(error) => crate::history::lookup::content(error),
    }
}

/// The refusal a materialization failure is to a Commit.
pub fn commit_error(error: RemovalError) -> CommitError {
    match error {
        RemovalError::History(error) => CommitError::History(error),
        RemovalError::Ports(provider) => CommitError::Content {
            error: ContentError::ProviderFailure {
                what: "path removal markers",
            },
            provider: Some(provider),
        },
        RemovalError::Content(error) => CommitError::Content {
            error,
            provider: None,
        },
    }
}

/// The node a Branch snapshot's effective tree belongs to.
pub fn node_of(snapshot: &BranchSnapshot) -> NodeId {
    snapshot
        .branch
        .head_commit
        .map(NodeId::Commit)
        .unwrap_or(NodeId::Layer(snapshot.branch.base_layer))
}

/// The markers of `stack`, or none for a provider that keeps no markers.
fn markers(
    store: &Arc<Store>,
    stack: LayerStackId,
) -> Result<Vec<PathRemovalRecord>, RemovalError> {
    match store.published(|history| history.path_removals(stack)) {
        Ok(markers) => Ok(markers),
        Err(error)
            if matches!(
                error.as_ref(),
                PortError::History(HistoryError::Unsupported(_))
            ) =>
        {
            Ok(Vec::new())
        }
        Err(error) => Err(RemovalError::Ports(error)),
    }
}

/// The `history_serial` of `node`, or none for a provider that keeps none.
fn node_serial(store: &Arc<Store>, node: NodeId) -> Result<Option<u64>, RemovalError> {
    match store.published(|history| history.node_history_serial(node)) {
        Ok(serial) => Ok(serial),
        Err(error)
            if matches!(
                error.as_ref(),
                PortError::History(HistoryError::Unsupported(_))
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(RemovalError::Ports(error)),
    }
}

/// The paths hidden at `node`: each marker whose governing frontier is at
/// least the node's serial and whose entry exists in `root`, with a path an
/// already selected ancestor governs left out.
pub fn hidden_paths(
    store: &Arc<Store>,
    stack: LayerStackId,
    reader: &dyn AuthenticatedObjects,
    root: FilesystemRootId,
    node: NodeId,
) -> Result<Vec<String>, RemovalError> {
    let Some(serial) = node_serial(store, node)? else {
        return Ok(Vec::new());
    };
    let markers = markers(store, stack)?;
    if markers.is_empty() {
        return Ok(Vec::new());
    }
    let tree = TreeTable::open(reader, root)?;
    let mut paths = Vec::new();
    for marker in &markers {
        if marker.path.is_empty() {
            continue;
        }
        let Ok(path) = std::str::from_utf8(&marker.path) else {
            continue;
        };
        let frontier = markers
            .iter()
            .filter(|other| governs(&other.path, &marker.path))
            .map(|other| other.frontier_serial)
            .max()
            .unwrap_or(0);
        if frontier < serial || entry(reader, &tree, path)?.is_none() {
            continue;
        }
        paths.push(path.to_owned());
    }
    paths.sort_by_key(String::len);
    let mut selected: Vec<String> = Vec::new();
    for path in paths {
        if selected
            .iter()
            .any(|ancestor| governs(ancestor.as_bytes(), path.as_bytes()))
        {
            continue;
        }
        selected.push(path);
    }
    Ok(selected)
}

/// The entry at a checked path of one immutable tree.
fn entry(
    reader: &dyn AuthenticatedObjects,
    tree: &TreeTable,
    path: &str,
) -> Result<Option<EntryFacts>, ContentError> {
    let mut serial = tree.root_serial();
    for name in path.split('/').filter(|name| !name.is_empty()) {
        let name = PathName::new(name).map_err(|_| ContentError::InvalidRecord("removed path"))?;
        match tree.lookup(reader, serial, &name)? {
            Some(child) => serial = child,
            None => return Ok(None),
        }
    }
    Ok(tree
        .value(reader, serial)?
        .map(|value| EntryFacts { serial, value }))
}

/// One parent's final binding changes: the named entry is absent.
fn updates(
    reader: &dyn AuthenticatedObjects,
    tree: &TreeTable,
    paths: &[String],
) -> Result<Vec<DirectoryUpdate>, ContentError> {
    let mut parents: BTreeMap<u64, Vec<(PathName, Option<u64>)>> = BTreeMap::new();
    for path in paths {
        let names: Vec<&str> = path.split('/').filter(|name| !name.is_empty()).collect();
        let mut serial = tree.root_serial();
        for (index, name) in names.iter().enumerate() {
            let name =
                PathName::new(name).map_err(|_| ContentError::InvalidRecord("removed path"))?;
            let Some(child) = tree.lookup(reader, serial, &name)? else {
                break;
            };
            if index + 1 == names.len() {
                parents.entry(serial).or_default().push((name, None));
            } else {
                serial = child;
            }
        }
    }
    Ok(parents
        .into_iter()
        .map(|(parent, mut changes)| {
            changes.sort_by(|left, right| left.0.cmp(&right.0));
            changes.dedup_by(|left, right| left.0 == right.0);
            DirectoryUpdate { parent, changes }
        })
        .collect())
}

/// One immutable root with `paths` removed, or `root` when there is none.
fn derived(
    reader: &dyn AuthenticatedObjects,
    save: &Save<'_>,
    root: FilesystemRootId,
    paths: &[String],
) -> Result<FilesystemRootId, RemovalError> {
    if paths.is_empty() {
        return Ok(root);
    }
    let tree = TreeTable::open(reader, root)?;
    let directories = updates(reader, &tree, paths)?;
    if directories.is_empty() {
        return Ok(root);
    }
    let input = FilesystemInput {
        base: Some(root),
        scope: tree.root.scope(),
        root_serial: tree.root_serial(),
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let mut sink = save.sink();
    let mut objects = FilesystemObjects::new(reader, &mut sink);
    Ok(update_filesystem(&mut objects, &input, None)?.root)
}

/// The root of `node` with every entry the markers hide there removed.
///
/// A node above the frontier hides nothing, so this is the side's own root
/// and the three-way merge proceeds unchanged. A hidden side becomes absent;
/// a post-frontier re-creation on the other side is then an addition that
/// wins, deterministically, in either merge order.
pub fn derive_root(
    store: &Arc<Store>,
    stack: LayerStackId,
    reader: &dyn AuthenticatedObjects,
    save: &Save<'_>,
    node: NodeId,
    root: FilesystemRootId,
) -> Result<FilesystemRootId, RemovalError> {
    let paths = hidden_paths(store, stack, reader, root, node)?;
    derived(reader, save, root, &paths)
}

/// A Commit's candidate root with every entry it inherited unchanged from
/// the bound base removed. A path whose inode serial or value differs from
/// the base was re-created after the binding, so it stays visible.
pub fn materialize_commit(
    store: &Arc<Store>,
    stack: LayerStackId,
    save: &Save<'_>,
    base: FilesystemRootId,
    node: NodeId,
    candidate: FilesystemRootId,
) -> Result<FilesystemRootId, RemovalError> {
    // The candidate's objects are this Save's, not yet published: the Save
    // serves them, falling back to the Store for inherited ones.
    let paths = hidden_paths(store, stack, save, base, node)?;
    if paths.is_empty() {
        return Ok(candidate);
    }
    let base_tree = TreeTable::open(save, base)?;
    let next = TreeTable::open(save, candidate)?;
    let mut remove = Vec::new();
    for path in &paths {
        inherited(save, &base_tree, &next, path, &mut remove)?;
    }
    derived(save, save, candidate, &remove)
}

/// Collects the entries of `path` that `next` inherited unchanged from
/// `base`: the whole entry when serial and value are equal, else the equal
/// children below two directories. A changed file or a new inode is a
/// re-creation and stays.
fn inherited(
    reader: &dyn AuthenticatedObjects,
    base: &TreeTable,
    next: &TreeTable,
    path: &str,
    remove: &mut Vec<String>,
) -> Result<(), ContentError> {
    let Some(base_entry) = entry(reader, base, path)? else {
        return Ok(());
    };
    let Some(next_entry) = entry(reader, next, path)? else {
        return Ok(());
    };
    if base_entry.serial == next_entry.serial && same_value(&base_entry.value, &next_entry.value) {
        remove.push(path.to_owned());
        return Ok(());
    }
    if base_entry.value.kind != InodeKind::Directory
        || next_entry.value.kind != InodeKind::Directory
    {
        return Ok(());
    }
    let mut children = Vec::new();
    list_directory(reader, base_entry.value.content_root, |name, _| {
        children.push(join(path, name));
        Ok(true)
    })?;
    for child in children {
        inherited(reader, base, next, &child, remove)?;
    }
    Ok(())
}

impl BoundWorkspace {
    /// Writes one whiteout into this Workspace's overlay for every path the
    /// markers hide at its bound node, so a mounted view hides it and the
    /// next Commit records a real deletion. A path already re-created in the
    /// overlay is absent from the base view lookup and is left alone.
    pub fn materialize_removals(&self) -> Result<(), SdkRefusal> {
        let snapshot = self.snapshot().map_err(crate::write::mutation::failure)?;
        let reader = self.store.ports(snapshot.scope);
        let paths = hidden_paths(
            &self.store,
            snapshot.branch.stack,
            &*reader,
            FilesystemRootId(snapshot.effective_root),
            node_of(&snapshot),
        )
        .map_err(refusal)?;
        if paths.is_empty() {
            return Ok(());
        }
        let mut mutation = Mutation::new(self)?;
        for path in paths {
            let absolute = format!("/{path}");
            if mutation.optional(&absolute)?.is_some() {
                mutation.remove(&absolute)?;
            }
        }
        mutation.finish()
    }
}
