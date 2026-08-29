use crate::{ResourcePolicy, WorkspaceState};
use layerfs_branch_store::BranchStore;
use layerfs_core::content::rope::{read_all_bounded, state, FileStateRoot, RopeCounters};
use layerfs_core::inode::{
    inode_table_lookup, InodeId, InodeKind, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::logical::{self, LogicalCounters};
use layerfs_core::metadata::{metadata_lookup, MetadataKey, PortableMetadataV1};
use layerfs_core::namespace::{
    directory_lookup, directory_page_after, DirectoryStateRoot, NamespaceCounters,
};
use layerfs_core::namespace_codec::{decode_inode_record, decode_symlink};
use layerfs_core::object::access::ObjectRead;
use layerfs_core::{CanonicalName, CanonicalPath};
use layerfs_storage_core::internal::StagedChange;
use layerfs_storage_core::{BranchId, CommitId, CoreReader, Result, StorageError};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(pub u64);
pub const ROOT: NodeId = NodeId(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Copy, Debug)]
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
pub(crate) enum Data {
    File(FileData),
    Directory(DirectoryData),
    Symlink(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FileData {
    Base {
        root: FileStateRoot,
        len: u64,
    },
    Overlay {
        base: Option<(FileStateRoot, u64)>,
        spool: PathBuf,
        len: u64,
        dirty: BTreeMap<u64, u64>,
        charged: BTreeMap<u64, u64>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DirectoryData {
    pub base: Option<DirectoryStateRoot>,
    pub changes: BTreeMap<Vec<u8>, Option<NodeId>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Node {
    pub canonical: Option<InodeId>,
    pub paths: BTreeSet<String>,
    pub mode: u32,
    pub links: u32,
    pub pins: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub data: Data,
}

pub struct Workspace {
    pub(crate) branch: BranchStore,
    pub(crate) branch_id: BranchId,
    pub(crate) expected_head: CommitId,
    pub(crate) base_inodes: InodeTableRoot,
    pub(crate) spool: PathBuf,
    pub(crate) spool_bytes: u64,
    pub(crate) policy: ResourcePolicy,
    pub(crate) nodes: HashMap<NodeId, Node>,
    pub(crate) canonical_nodes: HashMap<InodeId, NodeId>,
    pub(crate) dirty: BTreeSet<NodeId>,
    pub(crate) mutations: Vec<StagedChange>,
    pub(crate) next_node: u64,
    pub(crate) state: WorkspaceState,
}

impl Workspace {
    pub fn open(branch: BranchStore, branch_id: BranchId, spool: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_policy(branch, branch_id, spool, ResourcePolicy::default())
    }

    pub fn open_with_policy(
        branch: BranchStore,
        branch_id: BranchId,
        spool: impl AsRef<Path>,
        policy: ResourcePolicy,
    ) -> Result<Self> {
        let (branch_record, base_root) = branch.branch_snapshot(branch_id)?;
        let reader = CoreReader(&branch);
        let namespace = logical::namespace(&reader, base_root)?;
        let resolved = logical::resolve(
            &reader,
            base_root,
            &CanonicalPath::root(),
            &mut LogicalCounters::default(),
        )?;
        let portable =
            portable_metadata(&reader, resolved.record.metadata_root, resolved.record.kind)?;
        let spool = spool.as_ref().to_owned();
        std::fs::create_dir_all(&spool)?;
        let root = Node {
            canonical: Some(resolved.inode),
            paths: BTreeSet::from([String::new()]),
            mode: portable.permission_mode,
            links: 2,
            pins: 0,
            mtime_seconds: portable.mtime_seconds,
            mtime_nanoseconds: portable.mtime_nanoseconds,
            data: Data::Directory(DirectoryData {
                base: Some(DirectoryStateRoot(resolved.record.content_root)),
                changes: BTreeMap::new(),
            }),
        };
        Ok(Self {
            branch,
            branch_id,
            expected_head: branch_record.head_commit_id,
            base_inodes: InodeTableRoot(namespace.inode_table_root),
            spool,
            spool_bytes: 0,
            policy,
            nodes: HashMap::from([(ROOT, root)]),
            canonical_nodes: HashMap::from([(resolved.inode, ROOT)]),
            dirty: BTreeSet::new(),
            mutations: Vec::new(),
            next_node: 2,
            state: WorkspaceState::Active,
        })
    }

    pub fn attr(&self, node: NodeId) -> Result<Attr> {
        let value = self
            .nodes
            .get(&node)
            .ok_or(StorageError::NotFound("node"))?;
        let (kind, size) = match &value.data {
            Data::File(FileData::Base { len, .. }) => (Kind::File, *len),
            Data::File(FileData::Overlay { len, .. }) => (Kind::File, *len),
            Data::Directory(_) => (Kind::Directory, 0),
            Data::Symlink(target) => (Kind::Symlink, target.len() as u64),
        };
        Ok(Attr {
            node,
            size,
            kind,
            mode: value.mode,
            links: value.links,
            mtime_seconds: value.mtime_seconds,
            mtime_nanoseconds: value.mtime_nanoseconds,
        })
    }

    pub fn lookup(&mut self, parent: NodeId, name: &[u8]) -> Result<Attr> {
        let node = self.lookup_node(parent, name)?;
        self.attr(node)
    }

    pub fn readlink(&self, node: NodeId) -> Result<Vec<u8>> {
        match &self
            .nodes
            .get(&node)
            .ok_or(StorageError::NotFound("node"))?
            .data
        {
            Data::Symlink(target) => Ok(target.clone()),
            _ => Err(StorageError::InvalidInput("readlink")),
        }
    }

    pub fn readdir(&mut self, node: NodeId) -> Result<Vec<(NodeId, Kind, Vec<u8>)>> {
        let parent = self.parent_of(node)?;
        let mut output = VecDeque::from([
            (node, Kind::Directory, b".".to_vec()),
            (parent, Kind::Directory, b"..".to_vec()),
        ]);
        for (name, child) in self.directory_entries(node)? {
            output.push_back((child, self.attr(child)?.kind, name));
        }
        Ok(output.into())
    }

    pub(crate) fn pins(&self) -> u64 {
        self.nodes.values().map(|node| u64::from(node.pins)).sum()
    }

    pub(crate) fn allocate(&mut self, node: Node) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;
        self.nodes.insert(id, node);
        id
    }

    pub(crate) fn lookup_node(&mut self, parent: NodeId, name: &[u8]) -> Result<NodeId> {
        validate_name(name)?;
        if let Some(change) = self.directory(parent)?.changes.get(name) {
            return change.ok_or(StorageError::NotFound("name"));
        }
        let Some(base) = self.directory(parent)?.base else {
            return Err(StorageError::NotFound("name"));
        };
        let inode = directory_lookup(
            &CoreReader(&self.branch),
            base,
            &CanonicalName::from_bytes(name)?,
            &mut NamespaceCounters::default(),
        )?
        .ok_or(StorageError::NotFound("name"))?;
        let path = self.child_path(parent, name)?;
        self.materialize(inode, path)
    }

    fn materialize(&mut self, inode: InodeId, path: String) -> Result<NodeId> {
        if let Some(node) = self.canonical_nodes.get(&inode).copied() {
            self.nodes.get_mut(&node).unwrap().paths.insert(path);
            return Ok(node);
        }
        let reader = CoreReader(&self.branch);
        let record_id = inode_table_lookup(
            &reader,
            self.base_inodes,
            inode,
            &mut InodeTableCounters::default(),
        )?
        .ok_or(StorageError::MissingBaseData)?;
        let record = reader.with_authenticated_canonical(record_id, decode_inode_record)?;
        let portable = portable_metadata(&reader, record.metadata_root, record.kind)?;
        let data = match record.kind {
            InodeKind::RegularFile => {
                let state = state(
                    &reader,
                    FileStateRoot(record.content_root),
                    &mut RopeCounters::default(),
                )?;
                Data::File(FileData::Base {
                    root: FileStateRoot(record.content_root),
                    len: state.logical_len,
                })
            }
            InodeKind::Directory => Data::Directory(DirectoryData {
                base: Some(DirectoryStateRoot(record.content_root)),
                changes: BTreeMap::new(),
            }),
            InodeKind::Symlink => Data::Symlink(
                reader
                    .with_authenticated_canonical(record.content_root, decode_symlink)?
                    .target,
            ),
        };
        let node = self.allocate(Node {
            canonical: Some(inode),
            paths: BTreeSet::from([path]),
            mode: portable.permission_mode,
            links: record.namespace_ref_count as u32,
            pins: 0,
            mtime_seconds: portable.mtime_seconds,
            mtime_nanoseconds: portable.mtime_nanoseconds,
            data,
        });
        self.canonical_nodes.insert(inode, node);
        Ok(node)
    }

    pub(crate) fn directory_entries(&mut self, node: NodeId) -> Result<BTreeMap<Vec<u8>, NodeId>> {
        let (base, changes) = {
            let directory = self.directory(node)?;
            (directory.base, directory.changes.clone())
        };
        let prefix = self.path_of(node)?;
        let mut base_entries = Vec::new();
        if let Some(base) = base {
            let mut after = None;
            loop {
                let page = directory_page_after(
                    &CoreReader(&self.branch),
                    base,
                    after.as_ref(),
                    128,
                    256 * 1024,
                    &mut NamespaceCounters::default(),
                )?;
                base_entries.extend(page.entries);
                let Some(next) = page.continuation else { break };
                after = Some(next);
            }
        }
        let mut entries = BTreeMap::new();
        for (name, inode) in base_entries {
            if changes.contains_key(name.as_bytes()) {
                continue;
            }
            let path = join(&prefix, name.as_bytes())?;
            entries.insert(name.as_bytes().to_vec(), self.materialize(inode, path)?);
        }
        for (name, desired) in changes {
            if let Some(child) = desired {
                entries.insert(name, child);
            }
        }
        Ok(entries)
    }

    pub(crate) fn directory_is_empty(&self, node: NodeId) -> Result<bool> {
        let directory = self.directory(node)?;
        if directory.changes.values().any(Option::is_some) {
            return Ok(false);
        }
        let Some(base) = directory.base else {
            return Ok(true);
        };
        let mut after = None;
        loop {
            let page = directory_page_after(
                &CoreReader(&self.branch),
                base,
                after.as_ref(),
                128,
                256 * 1024,
                &mut NamespaceCounters::default(),
            )?;
            if page
                .entries
                .iter()
                .any(|(name, _)| !directory.changes.contains_key(name.as_bytes()))
            {
                return Ok(false);
            }
            let Some(next) = page.continuation else {
                return Ok(true);
            };
            after = Some(next);
        }
    }

    fn directory(&self, node: NodeId) -> Result<&DirectoryData> {
        match &self
            .nodes
            .get(&node)
            .ok_or(StorageError::NotFound("node"))?
            .data
        {
            Data::Directory(directory) => Ok(directory),
            _ => Err(StorageError::InvalidInput("directory")),
        }
    }

    pub(crate) fn directory_mut(&mut self, node: NodeId) -> Result<&mut DirectoryData> {
        match &mut self
            .nodes
            .get_mut(&node)
            .ok_or(StorageError::NotFound("node"))?
            .data
        {
            Data::Directory(directory) => Ok(directory),
            _ => Err(StorageError::InvalidInput("directory")),
        }
    }

    pub(crate) fn path_of(&self, node: NodeId) -> Result<String> {
        self.nodes
            .get(&node)
            .and_then(|node| node.paths.first())
            .cloned()
            .ok_or(StorageError::NotFound("node path"))
    }

    pub(crate) fn child_path(&self, parent: NodeId, name: &[u8]) -> Result<String> {
        validate_name(name)?;
        join(&self.path_of(parent)?, name)
    }

    fn parent_of(&self, node: NodeId) -> Result<NodeId> {
        if node == ROOT {
            return Ok(ROOT);
        }
        let path = self.path_of(node)?;
        let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
        self.nodes
            .iter()
            .find_map(|(id, node)| node.paths.contains(parent).then_some(*id))
            .ok_or(StorageError::MissingBaseData)
    }
}

fn portable_metadata(
    store: &CoreReader<'_>,
    root: layerfs_core::ObjectId,
    kind: InodeKind,
) -> Result<PortableMetadataV1> {
    let value = |name: &[u8], maximum| -> Result<Vec<u8>> {
        let entry = metadata_lookup(
            store,
            root,
            &MetadataKey::new("portable".to_owned(), name.to_vec())?,
        )?
        .ok_or(StorageError::Integrity("portable metadata"))?;
        let mut bytes = Vec::new();
        read_all_bounded(
            store,
            FileStateRoot(entry.value_file_root),
            maximum,
            &mut bytes,
        )?;
        Ok(bytes)
    };
    let mode = value(b"mode", 4)?;
    let mtime = value(b"mtime", 12)?;
    let metadata = PortableMetadataV1 {
        permission_mode: u32::from_be_bytes(
            mode.try_into()
                .map_err(|_| StorageError::Integrity("mode"))?,
        ),
        mtime_seconds: i64::from_be_bytes(
            mtime[..8]
                .try_into()
                .map_err(|_| StorageError::Integrity("mtime"))?,
        ),
        mtime_nanoseconds: u32::from_be_bytes(
            mtime[8..]
                .try_into()
                .map_err(|_| StorageError::Integrity("mtime"))?,
        ),
    };
    metadata.validate(kind)?;
    Ok(metadata)
}

fn join(parent: &str, name: &[u8]) -> Result<String> {
    let name = std::str::from_utf8(name).map_err(|_| StorageError::Integrity("name"))?;
    Ok(if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    })
}

fn validate_name(name: &[u8]) -> Result<()> {
    CanonicalName::from_bytes(name)
        .map(drop)
        .map_err(Into::into)
}
