//! Frozen generation capture for sandbox-local snapshots.
//!
//! A capture is fixed-size: under the caller's short mutation ordering it
//! moves the live dirty set into an owned frontier and leaves the live
//! workspace a fresh, empty dirty set. Node values are not cloned at
//! capture. The first post-capture mutation of a frontier node copies the
//! node's capture-time value into the frontier (`protect`), so the frozen
//! view stays stable while the live successor mutates in place. Piece trees
//! share immutable treap nodes by `Arc`, so those copies are shallow.
//!
//! The frontier bounds retention: only nodes named by the changed frontier
//! (dirty nodes plus children named by dirty directories' deltas) can be
//! retained, and only one frontier is active per workspace (one active
//! Commit). Releasing the frontier drops the retained copies; payload
//! segments stay alive through the ordinary piece/reference ownership until
//! nothing references them.

use crate::{
    Attr, Data, DirectoryData, Error, FileData, LiveWorkspace, Node, NodeId, Result,
};
use layerfs_content::tree::inode::InodeId;
use layerfs_content::ObjectId;
use std::collections::{BTreeSet, HashMap};

pub struct FrozenFrontier {
    /// The changed frontier at capture: dirty nodes plus children named by
    /// dirty directories' deltas.
    ids: BTreeSet<NodeId>,
    /// Capture-time copies of frontier nodes mutated after capture.
    retained: HashMap<NodeId, Node>,
}

impl LiveWorkspace {
    /// Capture the current changed frontier. Requires no active frontier;
    /// the adapter serializes captures (one active Commit per workspace).
    /// The live dirty set becomes empty; post-capture mutations start a
    /// fresh one. No I/O, no namespace scan, no node copies.
    pub fn capture_frontier(&mut self) -> Result<()> {
        if self.frozen.is_some() {
            return Err(Error::InvalidInput("workspace snapshot busy"));
        }
        let mut ids = std::mem::take(&mut self.dirty);
        for id in ids.clone() {
            if let Some(Node {
                data: Data::Directory(directory),
                ..
            }) = self.nodes.get(&id)
            {
                ids.extend(directory.changes.values().flatten().copied());
            }
        }
        self.frozen = Some(Box::new(FrozenFrontier {
            ids,
            retained: HashMap::new(),
        }));
        Ok(())
    }

    pub fn release_frontier(&mut self) {
        self.frozen = None;
    }

    pub fn frontier_active(&self) -> bool {
        self.frozen.is_some()
    }

    /// The frontier's id set in frontier order, with the live dirty count at
    /// capture time reported by the caller's capture reply separately.
    pub fn frontier_ids(&self) -> Vec<NodeId> {
        self.frozen
            .as_ref()
            .map(|frontier| frontier.ids.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn frontier_len(&self) -> usize {
        self.frozen.as_ref().map_or(0, |frontier| frontier.ids.len())
    }

    /// The frozen view of a frontier node: the capture-time copy if it was
    /// mutated after capture, else the live (unmodified since capture)
    /// value. `None` for nodes outside the frontier.
    pub fn frozen_node(&self, node: NodeId) -> Option<Node> {
        let frontier = self.frozen.as_ref()?;
        if !frontier.ids.contains(&node) {
            return None;
        }
        frontier
            .retained
            .get(&node)
            .cloned()
            .or_else(|| self.nodes.get(&node).cloned())
    }

    /// Retain the capture-time value of a frontier node before its live
    /// value is mutated. Copy-on-first-touch only; exclusive (non-frontier)
    /// nodes mutate in place with no copy.
    pub(crate) fn protect(&mut self, node: NodeId) {
        let Some(frontier) = self.frozen.as_mut() else {
            return;
        };
        if !frontier.ids.contains(&node) || frontier.retained.contains_key(&node) {
            return;
        }
        if let Some(value) = self.nodes.get(&node) {
            frontier.retained.insert(node, value.clone());
        }
    }

    /// `rename` rewrites descendant paths across the node table; retain every
    /// frontier node first. Bounded by the frontier, mirroring the mutation's
    /// own bound.
    pub(crate) fn protect_frontier_paths(&mut self) {
        let Some(frontier) = self.frozen.as_mut() else {
            return;
        };
        let ids: Vec<NodeId> = frontier.ids.iter().copied().collect();
        for id in ids {
            if !frontier.retained.contains_key(&id) {
                if let Some(value) = self.nodes.get(&id) {
                    frontier.retained.insert(id, value.clone());
                }
            }
        }
    }

    /// Apply one completion record for a captured generation. The record
    /// covers `node` as of `revision` (its revision at capture). It is
    /// applied only when the live node is still exactly that version: a node
    /// mutated after capture keeps its live pieces, links and dirty state, so
    /// a delayed or duplicate completion can never clear newer changes.
    /// Returns whether the record was applied.
    pub fn install_covered_record(
        &mut self,
        node: NodeId,
        revision: u64,
        inode: InodeId,
        content: ObjectId,
        attr: Attr,
    ) -> Result<bool> {
        let Some(value) = self.nodes.get_mut(&node) else {
            return Ok(false);
        };
        if value.revision != revision {
            return Ok(false);
        }
        if value.canonical == Some(inode)
            && match &value.data {
                Data::File(FileData::Base { root, len }) => {
                    root.0 == content && *len == attr.size
                }
                Data::Directory(DirectoryData { base, changes }) => {
                    base.is_some_and(|root| root.0 == content) && changes.is_empty()
                }
                _ => false,
            }
        {
            // A duplicate or late completion of an already covered node.
            return Ok(false);
        }
        if (value.paths.is_empty() && value.links == 0)
            || value.attr(node) != attr
            || value.canonical.is_some_and(|old| old != inode)
            || self
                .canonical_nodes
                .get(&inode)
                .is_some_and(|old| *old != node)
        {
            return Err(Error::Integrity("covered record presentation"));
        }
        if matches!(&value.data, Data::File(FileData::Edited { .. }))
            && !self.edited_nodes.contains(&node)
        {
            return Err(Error::Integrity("covered spool descriptor"));
        }
        match &mut value.data {
            Data::File(data) => {
                *data = FileData::Base {
                    root: layerfs_content::file::content::FileContentRoot(content),
                    len: attr.size,
                }
            }
            Data::Directory(DirectoryData { base, changes }) => {
                *base = Some(layerfs_content::tree::directory::DirectoryStateRoot(content));
                changes.clear();
            }
            Data::Symlink(_) => {}
        }
        self.edited_nodes.remove(&node);
        value.canonical = Some(inode);
        self.canonical_nodes.insert(inode, node);
        // A revision-matched node cannot be live-dirty (post-capture
        // mutations bump the revision); the removal is defensive only.
        self.dirty.remove(&node);
        Ok(true)
    }

    /// Finish a covered generation: the workspace is now based on `root`,
    /// all changes up to `covered_generation` are published. Later changes
    /// (live dirty set, retained edited nodes, mutation paths above the
    /// covered generation) survive.
    pub fn finish_covered_generation(&mut self, root: ObjectId, covered_generation: u64) {
        self.spool_bytes = 0;
        self.inline_bytes = 0;
        self.piece_allocation_bytes = 0;
        for id in self.edited_nodes.clone() {
            if let Some(Node {
                data:
                    Data::File(FileData::Edited {
                        spool_high_water,
                        pieces,
                        ..
                    }),
                ..
            }) = self.nodes.get(&id)
            {
                self.spool_bytes = self.spool_bytes.saturating_add(*spool_high_water);
                self.inline_bytes = self.inline_bytes.saturating_add(pieces.inline_len());
                self.piece_allocation_bytes = self
                    .piece_allocation_bytes
                    .saturating_add(pieces.logical_allocation_charge().unwrap_or(0));
            }
        }
        self.base_root = root;
        self.covered_generation = covered_generation;
        self.known_names.clear();
        self.spool_bytes_peak = self.spool_bytes_peak.max(self.spool_bytes);
        self.mutation_paths
            .retain(|_, generation| *generation > covered_generation);
        self.release_frontier();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_edit::{Piece, PieceTree};
    use crate::{Data, DirectoryData, Kind, LiveWorkspace, ResourcePolicy, ROOT};
    use std::collections::{BTreeMap, BTreeSet};

    fn workspace() -> LiveWorkspace {
        LiveWorkspace::new(
            Node {
                revision: 0,
                canonical: None,
                paths: BTreeSet::from([String::new()]),
                mode: 0o755,
                links: 2,
                pins: 0,
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
                data: Data::Directory(DirectoryData {
                    base: None,
                    changes: BTreeMap::new(),
                }),
            },
            ResourcePolicy::default(),
            layerfs_content::ObjectId::for_bytes(b"base namespace"),
        )
    }

    fn write_file(live: &mut LiveWorkspace, node: NodeId, bytes: &[u8]) {
        let prepared = live
            .prepare_splices(
                node,
                vec![(
                    0,
                    live.nodes[&node].attr(node).size,
                    Some(Piece::Inline {
                        bytes: std::sync::Arc::from(bytes),
                        offset: 0,
                        len: bytes.len() as u64,
                    }),
                )],
            )
            .unwrap();
        live.apply_edit(prepared).unwrap();
    }

    fn create(live: &mut LiveWorkspace, name: &str, bytes: &[u8]) -> NodeId {
        let parent = ROOT;
        let revision = live.nodes[&parent].revision;
        let attr = live
            .create_file(
                crate::namespace::ResolvedName {
                    parent,
                    revision,
                    name: layerfs_content::CanonicalName::from_bytes(name.as_bytes()).unwrap(),
                    existing: None,
                },
                0o644,
                None,
            )
            .unwrap();
        let node = attr.node;
        if !bytes.is_empty() {
            write_file(live, node, bytes);
        }
        node
    }

    fn delete(live: &mut LiveWorkspace, name: &str) {
        let parent = ROOT;
        let revision = live.nodes[&parent].revision;
        live.unlink(
            crate::namespace::ResolvedName {
                parent,
                revision,
                name: layerfs_content::CanonicalName::from_bytes(name.as_bytes()).unwrap(),
                existing: Some(live_dirty_node(live, name)),
            },
            false,
            true,
        )
        .unwrap();
    }

    fn live_dirty_node(live: &LiveWorkspace, name: &str) -> NodeId {
        let Data::Directory(directory) = &live.nodes[&ROOT].data else {
            unreachable!()
        };
        directory.changes[name.as_bytes()].unwrap()
    }

    fn file_bytes(live: &LiveWorkspace, node: NodeId) -> Vec<u8> {
        let Data::File(data) = &live.nodes[&node].data else {
            unreachable!()
        };
        match data {
            FileData::Base { .. } => unreachable!(),
            FileData::Edited { pieces, .. } => pieces
                .pieces()
                .into_iter()
                .flat_map(|piece| match piece {
                    Piece::Inline { bytes, .. } => bytes.to_vec(),
                    Piece::Zero { len } => vec![0; len as usize],
                    _ => Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn frozen_frontier_stays_stable_while_live_state_changes() {
        let mut live = workspace();
        let a = create(&mut live, "a", b"AAAA");
        create(&mut live, "b", b"BBBB");
        assert!(!live.frontier_active());
        live.capture_frontier().unwrap();
        assert!(live.frontier_active());
        assert_eq!(live.frontier_len(), 3, "root + a + b");
        assert!(live.dirty.is_empty(), "live dirty set restarts");

        // Modify, rename-overwrite's sibling, and delete through the live
        // successor while the frontier is retained.
        write_file(&mut live, a, b"aaaa");
        live.chmod(a, 0o600).unwrap();
        delete(&mut live, "b");

        // The frozen view sees the capture-time state.
        let frozen_a = live.frozen_node(a).unwrap();
        assert_eq!(file_bytes_from(&frozen_a), b"AAAA");
        assert_eq!(frozen_a.mode, 0o644);
        assert!(live.frozen_node(ROOT).is_some());
        // The live successor sees the newer state.
        assert_eq!(file_bytes(&live, a), b"aaaa");
        assert_eq!(live.nodes[&a].mode, 0o600);
        assert!(!live.nodes.contains_key(&live.frozen_node(a).unwrap().attr(a).node)
            || live.nodes[&live.frozen_node(a).unwrap().attr(a).node].revision > 0);
        assert!(live.dirty.contains(&a), "post-capture edit stays live-dirty");
        let Data::Directory(directory) = &live.nodes[&ROOT].data else {
            unreachable!()
        };
        assert_eq!(directory.changes[b"b".as_slice()], None);

        // Release: no retained copies remain and a later capture starts fresh.
        live.release_frontier();
        assert!(!live.frontier_active());
        live.capture_frontier().unwrap();
        assert_eq!(live.frontier_len(), 2, "root (dirty from delete) + a");
        assert!(live.frontier_ids().contains(&a), "post-capture edit is the new frontier");
        live.release_frontier();
    }

    fn file_bytes_from(node: &Node) -> Vec<u8> {
        let Data::File(data) = &node.data else {
            unreachable!()
        };
        match data {
            FileData::Base { .. } => unreachable!(),
            FileData::Edited { pieces, .. } => pieces
                .pieces()
                .into_iter()
                .flat_map(|piece| match piece {
                    Piece::Inline { bytes, .. } => bytes.to_vec(),
                    Piece::Zero { len } => vec![0; len as usize],
                    _ => Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn covered_record_applies_only_to_unchanged_nodes_and_survives_duplicates() {
        let mut live = workspace();
        let a = create(&mut live, "a", b"AAAA");
        let b = create(&mut live, "b", b"BBBB");
        live.capture_frontier().unwrap();
        let captured_a = live.frozen_node(a).unwrap().revision;
        let captured_b = live.frozen_node(b).unwrap().revision;
        let attr_a = live.frozen_node(a).unwrap().attr(a);
        let attr_b = live.frozen_node(b).unwrap().attr(b);

        // A node mutated after capture keeps its live state.
        write_file(&mut live, a, b"aaaa");
        let inode = layerfs_content::tree::inode::InodeId::from_slice(&[7; 32]).unwrap();
        let content = layerfs_content::ObjectId::for_bytes(b"content");
        assert!(!live
            .install_covered_record(a, captured_a, inode, content, attr_a)
            .unwrap());
        assert!(matches!(
            live.nodes[&a].data,
            crate::Data::File(FileData::Edited { .. })
        ));
        assert!(live.dirty.contains(&a), "G+1 edit stays live-dirty");

        // An untouched frontier node is re-based exactly once; a duplicate
        // late completion is a no-op rather than an error.
        let inode_b = layerfs_content::tree::inode::InodeId::from_slice(&[9; 32]).unwrap();
        let content_b = layerfs_content::ObjectId::for_bytes(b"content-b");
        assert!(live
            .install_covered_record(b, captured_b, inode_b, content_b, attr_b)
            .unwrap());
        assert_eq!(live.nodes[&b].canonical, Some(inode_b));
        assert!(!live
            .install_covered_record(b, captured_b, inode_b, content_b, attr_b)
            .unwrap(),
            "duplicate completion is idempotent");
        // The live successor of a covered node edits the canonical base.
        assert!(matches!(
            live.nodes[&b].data,
            crate::Data::File(FileData::Base { .. })
        ));

        live.finish_covered_generation(content_b, 1);
        assert_eq!(live.covered_generation, 1);
        assert!(!live.frontier_active());
        assert!(live.dirty.contains(&a), "post-cover edit survives");
    }
}

