#![forbid(unsafe_code)]

pub mod backing;
mod checkpoint;
pub mod file_edit;
mod frozen;
pub use frozen::FrozenFrontier;
mod limits;
pub mod namespace;
pub use limits::ResourcePolicy;

use layerfs_content::file::content::FileContentRoot;
use layerfs_content::tree::directory::DirectoryStateRoot;
use layerfs_content::tree::inode::InodeId;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Portable failures retain the native Workspace's error classes and messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidInput(&'static str),
    Integrity(&'static str),
    NotFound(&'static str),
    Core(layerfs_content::CoreError),
}
pub type Result<T> = std::result::Result<T, Error>;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

impl From<layerfs_content::CoreError> for Error {
    fn from(error: layerfs_content::CoreError) -> Self {
        Self::Core(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(pub u64);
pub const ROOT: NodeId = NodeId(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Attr {
    pub node: NodeId,
    pub size: u64,
    pub kind: Kind,
    pub mode: u32,
    pub links: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Data {
    File(FileData),
    Directory(DirectoryData),
    Symlink(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileData {
    Base {
        root: FileContentRoot,
        len: u64,
    },
    Edited {
        base: Option<(FileContentRoot, u64)>,
        spool_high_water: u64,
        pieces: crate::file_edit::PieceTree,
        edits: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryData {
    pub base: Option<DirectoryStateRoot>,
    pub changes: BTreeMap<Vec<u8>, Option<NodeId>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
    pub revision: u64,
    pub canonical: Option<InodeId>,
    pub paths: BTreeSet<String>,
    pub mode: u32,
    pub links: u32,
    pub pins: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub data: Data,
}

impl Node {
    pub fn attr(&self, node: NodeId) -> Attr {
        let (kind, size) = match &self.data {
            Data::File(FileData::Base { len, .. }) => (Kind::File, *len),
            Data::File(FileData::Edited { pieces, .. }) => (Kind::File, pieces.len()),
            Data::Directory(_) => (Kind::Directory, 0),
            Data::Symlink(target) => (Kind::Symlink, target.len() as u64),
        };
        Attr {
            node,
            size,
            kind,
            mode: self.mode,
            links: self.links,
            mtime_seconds: self.mtime_seconds,
            mtime_nanoseconds: self.mtime_nanoseconds,
        }
    }
}

/// Read-only construction input held at one operation cut. The backing owner must
/// retain every referenced range until construction and checkpoint installation finish.
/// The maps may contain only dirty nodes and children named by their directory deltas.
pub struct FrozenWorkspaceChanges<'a> {
    pub nodes: &'a HashMap<NodeId, Node>,
    pub dirty: &'a BTreeSet<NodeId>,
    pub canonical_nodes: &'a HashMap<InodeId, NodeId>,
    pub base_root: layerfs_content::ObjectId,
    pub mutation_generation: u64,
    pub policy: ResourcePolicy,
}

impl FrozenWorkspaceChanges<'_> {
    pub fn attr(&self, node: NodeId) -> Result<Attr> {
        self.nodes
            .get(&node)
            .map(|value| value.attr(node))
            .ok_or(Error::Integrity("frozen node"))
    }
}

impl LiveWorkspace {
    pub fn frozen_changes(&self) -> FrozenWorkspaceChanges<'_> {
        FrozenWorkspaceChanges {
            nodes: &self.nodes,
            dirty: &self.dirty,
            canonical_nodes: &self.canonical_nodes,
            base_root: self.base_root,
            mutation_generation: self.mutation_generation,
            policy: self.policy,
        }
    }
}

/// An immutable owned description. The adapter performs acquisition and physical
/// reads after releasing live-state locks; the retained pieces keep old bytes alive.
pub struct ReadPlan {
    pub requested: u64,
    pub source: ReadSource,
    pub tree_visits: usize,
}

pub enum ReadSource {
    Base(FileContentRoot, u64, u64),
    Edited(Vec<file_edit::Piece>),
}

impl ReadPlan {
    pub fn for_file(data: &FileData, offset: u64, size: usize) -> Result<Self> {
        let len = match data {
            FileData::Base { len, .. } => *len,
            FileData::Edited { pieces, .. } => pieces.len(),
        };
        let offset = offset.min(len);
        let end = len.min(offset.saturating_add(size as u64));
        let (source, tree_visits) = match data {
            FileData::Base { root, .. } => (ReadSource::Base(*root, offset, end), 0),
            FileData::Edited { pieces, .. } => {
                let (ranges, visited) = pieces.range_with_visits(offset, end)?;
                (ReadSource::Edited(ranges), visited)
            }
        };
        Ok(Self {
            requested: end.saturating_sub(offset),
            source,
            tree_visits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_edit::{Piece, PieceTree};

    fn live_file() -> (LiveWorkspace, NodeId) {
        let mut live = LiveWorkspace::new(
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
        );
        let file = NodeId(2);
        let mut node = live.nodes[&ROOT].clone();
        node.data = Data::File(FileData::Edited {
            base: None,
            spool_high_water: 0,
            pieces: PieceTree::empty(),
            edits: 0,
        });
        node.paths = BTreeSet::from(["a".to_owned(), "b".to_owned()]);
        live.nodes.insert(file, node);
        (live, file)
    }

    #[test]
    fn metadata_changes_share_aliases_and_reject_before_generation_overflow() {
        let (mut live, file) = live_file();
        live.chmod(file, 0o6750).unwrap();
        live.set_mtime(file, 123, 456).unwrap();
        assert_eq!(live.attr(file).unwrap().mode, 0o750);
        assert_eq!(live.mutation_generation, 2);
        assert_eq!(live.mutation_paths["a"], live.mutation_paths["b"]);
        let before = live.nodes[&file].clone();
        assert!(live.set_mtime(file, 999, 1_000_000_000).is_err());
        assert_eq!(live.nodes[&file], before);
        live.mutation_generation = u64::MAX;
        assert!(live.chmod(file, 0).is_err());
        assert!(live.set_mtime(file, 999, 0).is_err());
        assert_eq!(live.nodes[&file], before);
        assert_eq!(live.mutation_paths["a"], 2);
    }

    #[test]
    fn prepared_write_is_exact_once_and_unrelated_inode_metadata_can_progress() {
        use crate::backing::{BackingId, BackingRef};
        use crate::file_edit::SpoolSlice;
        let (mut live, file) = live_file();
        let backing = BackingRef::new(BackingId(7), b"owned".to_vec());
        let range = || {
            Some(SpoolSlice {
                segment: backing.clone(),
                offset: 0,
                len: 5,
            })
        };
        let before = live.nodes[&file].clone();
        let prepared = live.prepare_write(file, 3, 5, range()).unwrap();
        assert_eq!(live.nodes[&file], before, "prepare mutated live state");
        assert_eq!(live.spool_bytes, 0);
        live.chmod(ROOT, 0o700).unwrap();
        live.pin(file).unwrap();
        assert_eq!(live.apply_edit(prepared).unwrap(), 5);
        assert_eq!(live.attr(file).unwrap().size, 8);
        assert_eq!(live.spool_bytes, 5);
        let revision = live.nodes[&file].revision;
        let stale = live.prepare_write(file, 0, 5, range()).unwrap();
        let newer = live.prepare_write(file, 0, 1, None).unwrap();
        live.apply_edit(newer).unwrap();
        let installed = live.nodes[&file].clone();
        assert_eq!(
            live.apply_edit(stale),
            Err(Error::Integrity("stale prepared write"))
        );
        assert_eq!(live.nodes[&file], installed);
        assert_eq!(live.nodes[&file].revision, revision + 1);
        assert_eq!(
            live.spool_bytes, 5,
            "unused append never becomes logical data"
        );
        assert!(!backing.is_unique(), "live file retains its backing");
        let stale_metadata = live.prepare_write(file, 0, 5, range()).unwrap();
        let mode = live.attr(file).unwrap().mode;
        live.chmod(file, mode).unwrap();
        assert_eq!(
            live.apply_edit(stale_metadata),
            Err(Error::Integrity("stale prepared write"))
        );
        live.policy.max_spool_bytes = 5;
        assert!(live.prepare_write(file, 0, 5, range()).is_err());
        assert!(live.prepare_write(file, u64::MAX, 5, range()).is_err());
        assert!(live.prepare_write(file, 0, 4, range()).is_err());
    }

    #[test]
    fn truncate_preparation_preserves_exact_inode_ranges_and_edit_counter() {
        use crate::backing::{BackingId, BackingRef};
        use crate::file_edit::SpoolSlice;
        let (mut live, file) = live_file();
        let backing = BackingRef::new(BackingId(1), vec![7; 8]);
        let write = live
            .prepare_write(
                file,
                0,
                8,
                Some(SpoolSlice {
                    segment: backing.clone(),
                    offset: 0,
                    len: 8,
                }),
            )
            .unwrap();
        live.apply_edit(write).unwrap();
        let prepared = live.prepare_truncate(file, 0).unwrap().unwrap();
        assert_eq!(prepared.backing_ranges().next().unwrap().len, 8);
        assert_eq!(live.attr(file).unwrap().size, 8);
        live.set_mtime(file, 42, 0).unwrap();
        assert!(live.apply_edit(prepared).is_err());
        assert_eq!(live.attr(file).unwrap().size, 8);
        let old_read = ReadPlan::for_file(
            match &live.nodes[&file].data {
                Data::File(data) => data,
                _ => unreachable!(),
            },
            0,
            8,
        )
        .unwrap();
        let prepared = live.prepare_truncate(file, 0).unwrap().unwrap();
        live.pin(file).unwrap();
        live.policy.max_spool_bytes = 0;
        live.apply_edit(prepared).unwrap();
        assert_eq!(live.attr(file).unwrap().size, 0);
        assert_eq!(live.spool_bytes, 8, "truncate retains range history charge");
        assert!(!backing.is_unique());
        drop(old_read);
        assert!(backing.is_unique());
        assert!(live.prepare_truncate(file, 0).unwrap().is_none());
        let grow = live.prepare_truncate(file, 8).unwrap().unwrap();
        live.apply_edit(grow).unwrap();
        let Data::File(FileData::Edited { edits, .. }) =
            &mut live.nodes.get_mut(&file).unwrap().data
        else {
            unreachable!()
        };
        // A large cumulative edit count is retained and does not itself reject:
        // only a genuine counter overflow does, and that is checked before any
        // live-state change.
        {
            let Data::File(FileData::Edited { edits, .. }) =
                &mut live.nodes.get_mut(&file).unwrap().data
            else {
                unreachable!()
            };
            *edits = u64::MAX;
        }
        let before = live.nodes[&file].clone();
        assert!(live.prepare_truncate(file, 0).is_err());
        assert_eq!(live.nodes[&file], before);
        {
            let Data::File(FileData::Edited { edits, .. }) =
                &mut live.nodes.get_mut(&file).unwrap().data
            else {
                unreachable!()
            };
            *edits = 1_000_000;
        }
        let prepared = live.prepare_truncate(file, 0).unwrap().unwrap();
        live.apply_edit(prepared).unwrap();
        assert_eq!(live.attr(file).unwrap().size, 0);
    }

    #[test]
    fn pins_reject_overflow_and_keep_unlinked_nodes_until_last_release() {
        let (mut live, file) = live_file();
        assert!(live.unpin(file).is_err());
        live.nodes.get_mut(&file).unwrap().pins = u32::MAX;
        assert!(live.pin(file).is_err());
        assert_eq!(live.nodes[&file].pins, u32::MAX);
        live.nodes.get_mut(&file).unwrap().pins = 0;
        live.pin(file).unwrap();
        live.pin(file).unwrap();
        let value = live.nodes.get_mut(&file).unwrap();
        value.paths.clear();
        value.links = 0;
        assert!(!live.reclaim(file));
        assert!(!live.unpin(file).unwrap());
        assert!(live.nodes.contains_key(&file));
        assert!(live.unpin(file).unwrap());
        assert!(!live.nodes.contains_key(&file));
        assert!(live.unpin(file).is_err());
        assert!(live.pin(ROOT).is_err());
    }

    #[test]
    fn reads_at_and_beyond_eof_are_empty_for_base_and_edited_files() {
        let root = FileContentRoot(layerfs_content::ObjectId::for_bytes(b"base"));
        let edited = FileData::Edited {
            base: None,
            spool_high_water: 0,
            pieces: PieceTree::empty()
                .replace(0, 0, [Piece::Zero { len: 8 }])
                .unwrap(),
            edits: 1,
        };
        for file in [FileData::Base { root, len: 8 }, edited] {
            for offset in [8, 9, u64::MAX] {
                let plan = ReadPlan::for_file(&file, offset, 16).unwrap();
                assert_eq!(plan.requested, 0);
                if let ReadSource::Edited(pieces) = plan.source {
                    assert!(pieces.is_empty());
                }
            }
        }
    }
}

/// The live inode table and its coherent change generation. Native and daemon
/// adapters own one instance per writable Workspace lifetime.
pub struct LiveWorkspace {
    pub base_root: layerfs_content::ObjectId,
    pub canonical_nodes: HashMap<InodeId, NodeId>,
    pub directory_parents: HashMap<NodeId, NodeId>,
    pub(crate) known_names: HashMap<NodeId, namespace::AcquiredNames>,
    pub next_node: u64,
    pub reserved: BTreeSet<NodeId>,
    pub inline_bytes: u64,
    pub piece_allocation_bytes: u64,
    pub spool_bytes: u64,
    pub spool_bytes_peak: u64,
    pub edited_nodes: BTreeSet<NodeId>,
    pub policy: ResourcePolicy,
    pub nodes: HashMap<NodeId, Node>,
    pub dirty: BTreeSet<NodeId>,
    pub mutation_generation: u64,
    pub mutation_paths: BTreeMap<String, u64>,
    /// The active Commit's frozen frontier, if any (one per workspace).
    pub(crate) frozen: Option<Box<FrozenFrontier>>,
    /// The generation whose changes are published in the current base root.
    /// Later generations are live changes; this stays monotonic, unlike the
    /// legacy checkpoint reset to zero.
    pub covered_generation: u64,
}

impl LiveWorkspace {
    pub fn new(root: Node, policy: ResourcePolicy, base_root: layerfs_content::ObjectId) -> Self {
        let canonical_nodes = root
            .canonical
            .map(|inode| (inode, ROOT))
            .into_iter()
            .collect();
        Self {
            base_root,
            canonical_nodes,
            directory_parents: HashMap::from([(ROOT, ROOT)]),
            known_names: HashMap::new(),
            next_node: 2,
            reserved: BTreeSet::new(),
            inline_bytes: 0,
            piece_allocation_bytes: 0,
            spool_bytes: 0,
            spool_bytes_peak: 0,
            edited_nodes: BTreeSet::new(),
            policy,
            nodes: HashMap::from([(ROOT, root)]),
            dirty: BTreeSet::new(),
            mutation_generation: 0,
            mutation_paths: BTreeMap::new(),
            frozen: None,
            covered_generation: 0,
        }
    }

    pub fn attr(&self, node: NodeId) -> Result<Attr> {
        Ok(self
            .nodes
            .get(&node)
            .ok_or(Error::NotFound("node"))?
            .attr(node))
    }

    pub fn next_generation(&self) -> Result<u64> {
        self.mutation_generation
            .checked_add(1)
            .ok_or(Error::Integrity("Workspace mutation generation"))
    }

    pub fn note_mutation(&mut self, paths: impl IntoIterator<Item = String>) -> Result<()> {
        self.mutation_generation = self.next_generation()?;
        for path in paths {
            self.mutation_paths.insert(path, self.mutation_generation);
        }
        Ok(())
    }

    pub fn chmod(&mut self, node: NodeId, mode: u32) -> Result<()> {
        self.protect(node);
        let generation = self.next_generation()?;
        let value = self.nodes.get_mut(&node).ok_or(Error::NotFound("node"))?;
        let revision = value
            .revision
            .checked_add(1)
            .ok_or(Error::Integrity("inode revision"))?;
        value.mode = mode & 0o1777;
        value.revision = revision;
        self.dirty.insert(node);
        self.mutation_generation = generation;
        for path in &value.paths {
            self.mutation_paths.insert(path.clone(), generation);
        }
        Ok(())
    }

    pub fn set_mtime(&mut self, node: NodeId, seconds: i64, nanos: u32) -> Result<()> {
        self.protect(node);
        if nanos > 999_999_999 {
            return Err(Error::InvalidInput("mtime"));
        }
        let generation = self.next_generation()?;
        let value = self.nodes.get_mut(&node).ok_or(Error::NotFound("node"))?;
        let revision = value
            .revision
            .checked_add(1)
            .ok_or(Error::Integrity("inode revision"))?;
        value.revision = revision;
        value.mtime_seconds = seconds;
        value.mtime_nanoseconds = nanos;
        self.dirty.insert(node);
        self.mutation_generation = generation;
        for path in &value.paths {
            self.mutation_paths.insert(path.clone(), generation);
        }
        Ok(())
    }
}
