//! The `project.remove_file` visibility filter over one resolved snapshot.
//!
//! A path is hidden at node `N` when its entry exists at `N` and the marker
//! frontier, the maximum over the path and its `/`-boundary ancestors, is at
//! least `N`'s `history_serial` (`§4.3`). A marked-but-absent path hides
//! nothing, so callers check existence first. Reads and stats refuse with one
//! `Removed` detail; listings and globs omit hidden entries and report one
//! top-level `removed_count` over the queried subtree, filtered before paging.

use super::{super::root, walk};
use crate::lookup;
use crate::store::{PortError, Store};
use layerfs_content::compare::EntryFacts;
use layerfs_content::merge::Rule;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::AuthenticatedObjects;
use layerfs_history::error::{RemovedPath, RemovedState};
use layerfs_history::{HistoryError, LayerStackId, LayerStackRecord, NodeId, PathRemovalRecord};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::collections::BTreeSet;
use std::sync::Arc;

/// True for a provider whose catalog bodies return `Unsupported`; every
/// other marker or serial read failure is a refusal, never "no markers".
pub fn unsupported(error: &Arc<PortError>) -> bool {
    matches!(
        error.as_ref(),
        PortError::History(HistoryError::Unsupported(_))
    )
}

/// The markers and node position one snapshot's reads filter against.
pub struct Visibility {
    serial: u64,
    markers: Vec<PathRemovalRecord>,
}

impl Visibility {
    /// The snapshot's serial and its project's markers, from the one read
    /// [`snapshot::resolve_saved`] made of both (#818 R2). A failure of that
    /// read refuses here, where the marker read refused before, rather than
    /// returning the removed bytes. A node with no row, or a provider without
    /// the combined read (a test double), reads the markers alone, as before:
    /// one that does not implement them either sees none.
    pub fn load(
        store: &Arc<Store>,
        stack: &LayerStackRecord,
        snap: &root::Snap,
        removals: root::Removals,
    ) -> Result<Self, SdkRefusal> {
        match removals {
            Ok(Some((serial, markers))) => Ok(Self { serial, markers }),
            Ok(None) => Self::for_stack(store, stack.id, snap.serial),
            Err(error) if unsupported(&error) => Self::for_stack(store, stack.id, snap.serial),
            Err(error) => Err(lookup::published(error)),
        }
    }

    /// `load` when only the resolved snapshot is at hand.
    pub fn for_snap(store: &Arc<Store>, snap: &root::Snap) -> Result<Self, SdkRefusal> {
        match Self::stack_of(store, snap.node)? {
            Some(stack) => Self::for_stack(store, stack, snap.serial),
            None => Ok(Self {
                serial: snap.serial,
                markers: Vec::new(),
            }),
        }
    }

    /// `for_snap` for a node that has not been resolved into a snapshot.
    pub fn for_node(store: &Arc<Store>, node: NodeId) -> Result<Self, SdkRefusal> {
        match Self::stack_of(store, node)? {
            Some(stack) => Self::for_stack(store, stack, Self::node_serial(store, node)?),
            None => Ok(Self {
                serial: 0,
                markers: Vec::new(),
            }),
        }
    }

    /// The stack a saved node belongs to; a node that cannot be read refuses.
    fn stack_of(store: &Arc<Store>, node: NodeId) -> Result<Option<LayerStackId>, SdkRefusal> {
        match node {
            NodeId::Commit(id) => Ok(Some(lookup::commit(store, id)?.stack)),
            NodeId::Layer(id) => Ok(Some(lookup::layer(store, id)?.stack)),
        }
    }

    /// `load` for a stack id and an explicit `history_serial`, for reads that
    /// walk several nodes and therefore ask `hidden_at` per node. Only a
    /// provider without the marker reads sees none (this is the fail-closed
    /// rule §4.3 needs: a read error must never show a removed path).
    pub fn for_stack(
        store: &Arc<Store>,
        stack: LayerStackId,
        serial: u64,
    ) -> Result<Self, SdkRefusal> {
        let markers = match store.published(|history| history.path_removals(stack)) {
            Ok(markers) => markers,
            Err(error) if unsupported(&error) => Vec::new(),
            Err(error) => return Err(lookup::published(error)),
        };
        Ok(Self { serial, markers })
    }

    /// The filter over markers and a node position the caller already holds,
    /// with no read: a bound Workspace's kept markers (#818 W1). `serial` is
    /// the node's `history_serial`; a node without one, or a provider that
    /// keeps no markers, passes 0 and none, as [`Self::for_node`] reads them.
    pub fn from_markers(serial: u64, markers: Vec<PathRemovalRecord>) -> Self {
        Self { serial, markers }
    }

    /// The node's `history_serial`, or 0 for a provider that keeps none. A
    /// read failure refuses instead of silently treating the node as serial 0.
    pub fn node_serial(store: &Arc<Store>, node: NodeId) -> Result<u64, SdkRefusal> {
        match store.published(|history| history.node_history_serial(node)) {
            Ok(Some(serial)) => Ok(serial),
            Ok(None) => Ok(0),
            Err(error) if unsupported(&error) => Ok(0),
            Err(error) => Err(lookup::published(error)),
        }
    }

    /// The snapshot position the marker frontier is compared against.
    pub fn serial(&self) -> u64 {
        self.serial
    }

    /// The governing frontier for a relative path: the maximum marker frontier
    /// over the path and its `/`-boundary ancestors, if any marker applies.
    fn frontier(&self, relative: &[u8]) -> Option<u64> {
        if relative.is_empty() {
            return None;
        }
        self.markers
            .iter()
            .filter(|marker| layerfs_history::catalog::path::governs(&marker.path, relative))
            .map(|marker| marker.frontier_serial)
            .max()
    }

    /// The governing frontier when the entry at `relative` (without the leading
    /// `/`) is hidden at node `serial`.
    pub fn hidden_relative_at(&self, relative: &[u8], serial: u64) -> Option<u64> {
        let frontier = self.frontier(relative)?;
        (frontier >= serial).then_some(frontier)
    }

    /// The governing frontier when the entry at `relative` is hidden at this
    /// node. Callers that already hold the path bytes (the export walk) avoid a
    /// UTF-8 round trip.
    pub fn hidden_relative(&self, relative: &[u8]) -> Option<u64> {
        self.hidden_relative_at(relative, self.serial)
    }

    /// The governing frontier when the entry at `path` is hidden at `serial`.
    pub fn hidden_at(&self, path: &str, serial: u64) -> Option<u64> {
        self.hidden_relative_at(path.strip_prefix('/').unwrap_or(path).as_bytes(), serial)
    }

    /// The governing frontier when the entry at `path` is hidden at this node.
    pub fn hidden(&self, path: &str) -> Option<u64> {
        self.hidden_at(path, self.serial)
    }

    /// The refusal for one hidden read or stat, `{paths: [{path, frontier_serial}]}`.
    pub fn refusal(path: &str, frontier_serial: u64) -> SdkRefusal {
        crate::lookup::history(HistoryError::Removed(Box::new(RemovedState::new(vec![
            RemovedPath::new(path, frontier_serial),
        ]))))
    }

    /// The number of hidden entries directly inside `dir` whose entries exist
    /// at this snapshot: the `removed_count` of a listing, independent of the
    /// page. A listing returns `dir`'s direct entries, so a marker below one
    /// of them is not a hidden result and is not counted; a glob counts the
    /// matched hidden entries at any depth (`hidden_matches`). Each call
    /// therefore counts exactly the entries its own result would omit.
    ///
    /// Marker existence is checked leniently ([`snapshot::find`], not
    /// [`snapshot::entry`]): a marker whose ancestor has since become a
    /// regular file or a symlink names no entry at this snapshot, so it hides
    /// and counts nothing and the listing or glob pages normally.
    pub fn hidden_children(
        &self,
        pages: &dyn AuthenticatedObjects,
        snap: &root::Snap,
        dir: &str,
    ) -> Result<i64, SdkRefusal> {
        if self.markers.is_empty() {
            return Ok(0);
        }
        let mut count = 0;
        for marker in &self.markers {
            let Ok(relative) = std::str::from_utf8(&marker.path) else {
                continue;
            };
            if relative.is_empty() {
                continue;
            }
            let parent = match relative.rsplit_once('/') {
                Some((parent, _)) => format!("/{parent}"),
                None => "/".to_owned(),
            };
            if parent != dir {
                continue;
            }
            let full = format!("/{relative}");
            if root::find(pages, &snap.tree, &full)?.is_some() && self.hidden(&full).is_some() {
                count += 1;
            }
        }
        Ok(count)
    }

    /// The number of hidden entries at or below `path` that the glob `pattern`
    /// matches: the `removed_count` of a glob, independent of the page. It
    /// counts every hidden entry the glob would otherwise return, at any depth
    /// under the query root, not only the root's direct children: the pattern
    /// decides which descendants are results. A listing's `hidden_children`
    /// does the same for its direct entries. Only hidden subtrees are walked,
    /// so a snapshot with no marker counts nothing.
    pub fn hidden_matches(
        &self,
        pages: &dyn AuthenticatedObjects,
        snap: &root::Snap,
        path: &str,
        pattern: &str,
    ) -> Result<i64, SdkRefusal> {
        if self.markers.is_empty() {
            return Ok(0);
        }
        let rule = Rule::glob(pattern);
        let root = path.trim_start_matches('/');
        let mut found: BTreeSet<String> = BTreeSet::new();
        let mut walked: Vec<String> = Vec::new();
        for marker in &self.markers {
            let Ok(relative) = std::str::from_utf8(&marker.path) else {
                continue;
            };
            if relative.is_empty() {
                continue;
            }
            if Self::under(root, relative).is_none() {
                continue;
            }
            if walked.iter().any(|hidden| {
                layerfs_history::catalog::path::governs(hidden.as_bytes(), relative.as_bytes())
            }) {
                // Already counted when the governing hidden directory was walked.
                continue;
            }
            let full = format!("/{relative}");
            let Some(entry) = root::find(pages, &snap.tree, &full)? else {
                continue;
            };
            if self.hidden(&full).is_none() {
                continue;
            }
            if rule.matches_path(&full) {
                found.insert(full.clone());
            }
            if entry.value.kind == InodeKind::Directory {
                // A hidden directory's descendants are hidden by the same
                // governing frontier and are glob results whenever the pattern
                // reaches them, so walk the hidden subtree and count each
                // match; `found` deduplicates a marker on a descendant.
                walked.push(relative.to_owned());
                walk::walk_within(pages, snap, &full, None, |child, _| {
                    if rule.matches_path(child) {
                        found.insert(child.to_owned());
                    }
                    Ok(true)
                })?;
            }
        }
        Ok(found.len() as i64)
    }

    /// The path `relative` within the query root `root` (both without a
    /// leading `/`; an empty root is the whole tree), or `None` outside it.
    fn under<'p>(root: &str, relative: &'p str) -> Option<&'p str> {
        if root.is_empty() || relative == root {
            Some(relative)
        } else {
            relative
                .strip_prefix(root)
                .and_then(|rest| rest.strip_prefix('/'))
        }
    }
}

/// The entry at `path`, the call's one path walk (#818 R3): strict, through
/// the call's `pages` (its memo), at the place the hidden-path check walked
/// before, so a symlink component or a read failure refuses there; refused
/// `Removed` when the entry exists and is hidden. An absent path is `None`:
/// the read refuses it `NoSuchPath` where it did ([`snapshot::found`]).
pub fn checked(
    pages: &dyn AuthenticatedObjects,
    snap: &root::Snap,
    path: &str,
    visibility: &Visibility,
) -> Result<Option<EntryFacts>, SdkRefusal> {
    let entry = root::lookup(pages, &snap.tree, path)?;
    if entry.is_some() {
        if let Some(refusal) = refuse(path, visibility) {
            return Err(refusal);
        }
    }
    Ok(entry)
}

/// The `Removed` refusal a hidden read or stat reports.
pub fn refuse(path: &str, visibility: &Visibility) -> Option<SdkRefusal> {
    visibility
        .hidden(path)
        .map(|frontier| Visibility::refusal(path, frontier))
}

/// Replaces a list or glob answer: hidden entries are dropped and the one
/// top-level `removed_count` is the caller's page-independent total.
pub fn settle(mut value: SdkValue, visibility: &Visibility, removed: i64) -> SdkValue {
    if let SdkValue::Map(fields) = &mut value {
        if let Some((_, SdkValue::List(items))) = fields.iter_mut().find(|(k, _)| k == "items") {
            items.retain(|item| {
                let path = match item {
                    SdkValue::Text(path) => path.clone(),
                    other => other
                        .get_text("path")
                        .map(str::to_owned)
                        .unwrap_or_default(),
                };
                path.is_empty() || visibility.hidden(&path).is_none()
            });
        }
        fields.push(("removed_count".into(), SdkValue::Int(removed)));
    }
    value
}
