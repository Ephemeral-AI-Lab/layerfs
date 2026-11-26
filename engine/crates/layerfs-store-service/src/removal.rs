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
//!
//! A merge that cannot materialize a whiteout refuses `Removed` with
//! `{paths: [{path, frontier_serial}]}` (§4.3): a marker that names no valid
//! tree path, or a path the derivation left in place, is listed as a full
//! element sorted by normalized path bytes instead of being left visible.

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
use layerfs_history::error::{RemovedPath, RemovedState};
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
    /// The marker names an entry this tree cannot whiteout: a path that is
    /// not a valid tree path, or one the derivation left in place. A merge
    /// refuses with `Removed` and the full elements (§4.3).
    Unmaterializable(Vec<RemovedPath>),
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
        RemovalError::Unmaterializable(paths) => crate::history::lookup::history(
            HistoryError::Removed(Box::new(RemovedState::new(paths))),
        ),
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
        RemovalError::Unmaterializable(paths) => {
            CommitError::History(HistoryError::Removed(Box::new(RemovedState::new(paths))))
        }
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

/// A node's `history_serial` with its stack's markers
/// ([`layerfs_history::HistoryCatalog::node_path_removals`]), read in one
/// statement; none for an absent node or a provider that keeps no markers.
pub(crate) type NodeRemovals = Option<(u64, Vec<PathRemovalRecord>)>;

/// The whiteout facts of one node a mounted Workspace knows without a read.
///
/// They hold while its mount admission is registered: every marker write
/// (`project.remove_file`, `branch.remove`, `project.delete`) scans the
/// project's live intents in its own write transaction, after its locks, and
/// refuses `MountsActive` (persistence `admission/removal.rs`,
/// `gc_intent_all_project`), and markers are deleted only with their
/// project's stack. So the markers read at mount stay the markers of the
/// bound node, and a Commit this Workspace published hides nothing: a
/// marker's frontier is the Store high-water read in its own transaction,
/// below the serial of any Commit published after it.
///
/// Kept only while they charge at most [`KEPT_REMOVAL_BYTES`]; a bound node
/// with more is not kept, and each Commit reads its markers again on the
/// control reader, as before #768.
pub(crate) struct KnownRemovals {
    node: NodeId,
    found: NodeRemovals,
}

/// Bytes of one node's markers a mounted Workspace keeps ([`KnownRemovals`]),
/// each marker charged its path bytes and its record (`PathRemovalRecord`).
///
/// Owner: one bound Workspace. Bound: this many bytes, one node's markers at
/// a time; a set over it is not kept at all. Live multiplicity: one per
/// mounted Workspace with a registered mount admission. Lifetime: until its
/// next Commit or a materialization replaces it, or the Workspace closes.
/// Release: replaced or dropped with the Workspace. 256 KiB is half the
/// published-value bound a Commit's Storage handle already keeps
/// (`layerfs_storage::policy::POOLED_VALUE_CACHE_BYTES`): about 2,600 markers
/// of 60-byte paths, and at least 63 of the longest paths
/// (`layerfs_history::path::MAXIMUM_PATH_BYTES`). A larger set costs each
/// Commit the control read it had before #768 and never correctness.
pub const KEPT_REMOVAL_BYTES: usize = 256 * 1024;

/// What `found` charges against [`KEPT_REMOVAL_BYTES`].
fn charge(found: &NodeRemovals) -> usize {
    found.as_ref().map_or(0, |(_, markers)| {
        markers
            .iter()
            .map(|marker| marker.path.len() + std::mem::size_of::<PathRemovalRecord>())
            .sum()
    })
}

/// The answer of one [`NodeRemovals`] read: a provider without the marker
/// reads sees none; any other failure refuses.
fn removals(read: Result<NodeRemovals, Arc<PortError>>) -> Result<NodeRemovals, RemovalError> {
    match read {
        Ok(found) => Ok(found),
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

/// The governing maximum frontier of `path`: the maximum over the path and
/// its `/`-boundary ancestors (§4.3).
fn governing_frontier(markers: &[PathRemovalRecord], path: &[u8]) -> u64 {
    markers
        .iter()
        .filter(|other| governs(&other.path, path))
        .map(|other| other.frontier_serial)
        .max()
        .unwrap_or(0)
}

/// The valid tree components of `path`, or `None` when it is not a path a
/// tree can name (a corrupted or legacy marker, which cannot be whiteouted).
fn components(path: &str) -> Option<Vec<PathName>> {
    if path.is_empty() {
        return None;
    }
    path.split('/')
        .filter(|name| !name.is_empty())
        .map(|name| PathName::new(name).ok())
        .collect()
}

/// The paths hidden at `node`: each marker whose governing frontier is at
/// least the node's serial and whose entry exists in `root`, with a path an
/// already selected ancestor governs left out. The full §10 elements carry
/// the governing frontier for the `Removed` refusal. A marker that names no
/// valid tree path is itself unmaterializable and refuses the build.
pub fn hidden_paths(
    store: &Arc<Store>,
    stack: LayerStackId,
    reader: &dyn AuthenticatedObjects,
    root: FilesystemRootId,
    node: NodeId,
) -> Result<Vec<RemovedPath>, RemovalError> {
    let found = removals(store.published(|history| history.node_path_removals(stack, node)))?;
    hidden_in(found, reader, root)
}

/// [`hidden_paths`] over an already read node position and markers.
fn hidden_in(
    found: NodeRemovals,
    reader: &dyn AuthenticatedObjects,
    root: FilesystemRootId,
) -> Result<Vec<RemovedPath>, RemovalError> {
    let Some((serial, markers)) = found else {
        return Ok(Vec::new());
    };
    if markers.is_empty() {
        return Ok(Vec::new());
    }
    let tree = TreeTable::open(reader, root)?;
    let mut paths = Vec::new();
    let mut invalid = Vec::new();
    for marker in &markers {
        if marker.path.is_empty() {
            continue;
        }
        let frontier = governing_frontier(&markers, &marker.path);
        if frontier < serial {
            continue;
        }
        let Ok(path) = std::str::from_utf8(&marker.path) else {
            invalid.push(RemovedPath::new(
                String::from_utf8_lossy(&marker.path),
                frontier,
            ));
            continue;
        };
        let names = match components(path) {
            Some(names) => names,
            None => {
                invalid.push(RemovedPath::new(path.to_owned(), frontier));
                continue;
            }
        };
        if entry_names(reader, &tree, &names)?.is_none() {
            continue;
        }
        paths.push(RemovedPath::new(path.to_owned(), frontier));
    }
    if !invalid.is_empty() {
        invalid.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
        invalid.dedup_by(|left, right| left.path == right.path);
        return Err(RemovalError::Unmaterializable(invalid));
    }
    paths.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    let mut selected: Vec<RemovedPath> = Vec::new();
    for path in paths {
        if selected
            .iter()
            .any(|ancestor| governs(ancestor.path.as_bytes(), path.path.as_bytes()))
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
    let names: Vec<PathName> = match components(path) {
        Some(names) => names,
        None => return Ok(None),
    };
    entry_names(reader, tree, &names)
}

/// The entry at already checked components of one immutable tree.
fn entry_names(
    reader: &dyn AuthenticatedObjects,
    tree: &TreeTable,
    names: &[PathName],
) -> Result<Option<EntryFacts>, ContentError> {
    let mut serial = tree.root_serial();
    for name in names {
        match tree.lookup(reader, serial, name)? {
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
    paths: &[RemovedPath],
) -> Result<Vec<DirectoryUpdate>, ContentError> {
    let mut parents: BTreeMap<u64, Vec<(PathName, Option<u64>)>> = BTreeMap::new();
    for path in paths {
        let names: Vec<String> = path
            .path
            .split('/')
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        let mut serial = tree.root_serial();
        for (index, name) in names.iter().enumerate() {
            let name = PathName::new(name.as_str())
                .map_err(|_| ContentError::InvalidRecord("removed path"))?;
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
    paths: &[RemovedPath],
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
    if paths.is_empty() {
        return Ok(root);
    }
    let derived_root = derived(reader, save, root, &paths)?;
    Ok(derived_root)
}

/// A Commit's candidate root with every entry it inherited unchanged from
/// the bound base removed. A path whose inode serial or value differs from
/// the base was re-created after the binding, so it stays visible. `found`
/// is the bound node's position and markers
/// ([`BoundWorkspace::commit_removals`]).
pub(crate) fn materialize_commit(
    save: &Save<'_>,
    base: FilesystemRootId,
    found: NodeRemovals,
    candidate: FilesystemRootId,
) -> Result<FilesystemRootId, RemovalError> {
    // The candidate's objects are this Save's, not yet published: the Save
    // serves them, falling back to the Store for inherited ones.
    let paths = hidden_in(found, save, base)?;
    if paths.is_empty() {
        return Ok(candidate);
    }
    let base_tree = TreeTable::open(save, base)?;
    let next = TreeTable::open(save, candidate)?;
    let mut remove = Vec::new();
    for path in &paths {
        inherited(save, &base_tree, &next, &path.path, &mut remove)?;
    }
    let selected: Vec<RemovedPath> = remove
        .into_iter()
        .map(|path| RemovedPath::new(path, 0))
        .collect();
    derived(save, save, candidate, &selected)
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
    /// overlay is absent from the base view lookup and is left alone. The
    /// bound node's serial and the markers are one statement, and the tree
    /// lookups are demands, all on this Workspace's own reader (#768).
    ///
    /// With a registered mount admission the answer is kept for this
    /// Workspace's Commits ([`KnownRemovals`]).
    pub fn materialize_removals(&self) -> Result<(), SdkRefusal> {
        self.materialize_removals_with(None)
    }
    /// [`Self::materialize_removals`] with the bound node's removals when
    /// the mount's registration read them after the intent committed
    /// (`HistoryCatalog::gc_register_mount`, #768 step B); `None` reads them
    /// here, on the Workspace reader, as before.
    pub(crate) fn materialize_removals_with(
        &self,
        known: Option<NodeRemovals>,
    ) -> Result<(), SdkRefusal> {
        let snapshot = self.snapshot().map_err(crate::write::mutation::failure)?;
        let (stack, node) = (snapshot.branch.stack, node_of(&snapshot));
        let found = match known {
            Some(found) => found,
            None => removals(self.published(|history| history.node_path_removals(stack, node)))
                .map_err(refusal)?,
        };
        self.keep_removals(node, &found);
        let reader = self.store.ports_for(snapshot.scope, self.identity());
        let paths = hidden_in(found, &*reader, FilesystemRootId(snapshot.effective_root))
            .map_err(refusal)?;
        if paths.is_empty() {
            return Ok(());
        }
        let mut mutation = Mutation::new(self)?;
        for path in paths {
            let absolute = format!("/{}", path.path);
            if mutation.optional(&absolute)?.is_some() {
                mutation.remove(&absolute, true)?;
            }
        }
        mutation.finish()
    }
    /// The position and markers a Commit's whiteout step needs for `node`:
    /// kept ones when this Workspace knows them for that node under its
    /// mount admission ([`KnownRemovals`]); otherwise, and for a catalog
    /// with no admission, read as before, on the control reader.
    pub(crate) fn commit_removals(
        &self,
        stack: LayerStackId,
        node: NodeId,
    ) -> Result<NodeRemovals, RemovalError> {
        if let Some(found) = self.kept_removals(node) {
            return Ok(found);
        }
        removals(
            self.store
                .published(|history| history.node_path_removals(stack, node)),
        )
    }
    /// `node`'s kept markers with its `history_serial`, for a file call's
    /// hidden-path check (#818 W1): kept only under a registered mount
    /// admission, for that exact node and within [`KEPT_REMOVAL_BYTES`]. A
    /// node kept as absent, or one this Workspace published, hides nothing:
    /// serial 0 and no markers. `None` when nothing is kept (no admission,
    /// another node, an over-bound set, a poisoned slot): the caller reads.
    pub(crate) fn kept_markers(&self, node: NodeId) -> Option<(u64, Vec<PathRemovalRecord>)> {
        self.kept_removals(node).map(Option::unwrap_or_default)
    }
    /// The [`KnownRemovals`] answer this Workspace keeps for `node`, if any.
    fn kept_removals(&self, node: NodeId) -> Option<NodeRemovals> {
        self.gc_op?;
        let kept = self.removals.lock().ok()?;
        kept.as_ref()
            .filter(|known| known.node == node)
            .map(|known| known.found.clone())
    }
    /// Records that this Workspace published `node`, a Commit: it hides
    /// nothing ([`KnownRemovals`]).
    pub(crate) fn published_removals(&self, node: NodeId) {
        self.keep_removals(node, &None);
    }
    /// Keeps `found` for `node` only under a registered mount admission and
    /// within [`KEPT_REMOVAL_BYTES`]; otherwise, or in a poisoned slot,
    /// nothing is kept, and the next Commit reads.
    fn keep_removals(&self, node: NodeId, found: &NodeRemovals) {
        if self.gc_op.is_none() {
            return;
        }
        if let Ok(mut kept) = self.removals.lock() {
            *kept = (charge(found) <= KEPT_REMOVAL_BYTES).then(|| KnownRemovals {
                node,
                found: found.clone(),
            });
        }
    }
}
