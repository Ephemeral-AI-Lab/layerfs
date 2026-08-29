use crate::{ResourcePolicy, WorkspaceState};
use layerfs_branch_store::BranchStore;
use layerfs_content::file::rope::{read_all_bounded, state, FileStateRoot, RopeCounters};
use layerfs_content::filesystem::{self as logical, LogicalCounters};
use layerfs_content::object::access::ObjectRead;
use layerfs_content::tree::directory::codec::decode_symlink;
use layerfs_content::tree::directory::{
    directory_lookup, directory_page_after, DirectoryStateRoot, NamespaceCounters,
};
use layerfs_content::tree::inode::codec::decode_inode_record;
use layerfs_content::tree::inode::{
    inode_table_lookup, InodeId, InodeKind, InodeTableCounters, InodeTableRoot,
};
use layerfs_content::tree::metadata::{metadata_lookup, MetadataKey, PortableMetadataV1};
use layerfs_content::{CanonicalName, CanonicalPath};
use layerfs_storage::{BranchId, CommitId, CoreReader, Result, StorageError};
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
    pub(crate) base_root: layerfs_content::ObjectId,
    pub(crate) base_inodes: InodeTableRoot,
    pub(crate) spool: PathBuf,
    pub(crate) spool_bytes: u64,
    pub(crate) mutation_generation: u64,
    pub(crate) policy: ResourcePolicy,
    pub(crate) nodes: HashMap<NodeId, Node>,
    pub(crate) canonical_nodes: HashMap<InodeId, NodeId>,
    pub(crate) dirty: BTreeSet<NodeId>,
    pub(crate) reserved: BTreeSet<NodeId>,
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
        Self::from_snapshot(
            branch,
            branch_id,
            branch_record.head_commit_id,
            base_root,
            spool.as_ref(),
            policy,
        )
    }

    pub(crate) fn clean_copy(&self, spool: impl AsRef<Path>) -> Result<Self> {
        Self::from_snapshot(
            self.branch.clone(),
            self.branch_id,
            self.expected_head,
            self.base_root,
            spool.as_ref(),
            self.policy,
        )
    }

    fn from_snapshot(
        branch: BranchStore,
        branch_id: BranchId,
        expected_head: CommitId,
        base_root: layerfs_content::ObjectId,
        spool: &Path,
        policy: ResourcePolicy,
    ) -> Result<Self> {
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
        let spool = spool.to_owned();
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
            expected_head,
            base_root,
            base_inodes: InodeTableRoot(namespace.inode_table_root),
            spool,
            spool_bytes: 0,
            mutation_generation: 0,
            policy,
            nodes: HashMap::from([(ROOT, root)]),
            canonical_nodes: HashMap::from([(resolved.inode, ROOT)]),
            dirty: BTreeSet::new(),
            reserved: BTreeSet::new(),
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

    pub(crate) fn allocate(&mut self, node: Node) -> NodeId {
        let id = NodeId(self.next_node);
        self.next_node += 1;
        self.nodes.insert(id, node);
        id
    }

    pub(crate) fn reserve_nodes(&mut self, count: u32) -> Result<NodeId> {
        self.ensure_active()?;
        if count == 0 || count > 65_536 {
            return Err(StorageError::InvalidInput("node reservation"));
        }
        let start = self.next_node;
        self.next_node = self
            .next_node
            .checked_add(u64::from(count))
            .ok_or(StorageError::Integrity("node reservation"))?;
        self.reserved.extend((start..self.next_node).map(NodeId));
        Ok(NodeId(start))
    }

    pub(crate) fn create_file_reserved(
        &mut self,
        parent: NodeId,
        name: &[u8],
        mode: u32,
        node: NodeId,
    ) -> Result<Attr> {
        self.ensure_active()?;
        if !self.reserved.remove(&node) {
            return Err(StorageError::Integrity("reserved node"));
        }
        let path = self.child_path(parent, name)?;
        self.new_spool_node_reserved(node, mode & 0o777, path)?;
        self.insert_name(parent, name, node)?;
        self.note_mutation()?;
        self.attr(node)
    }

    pub(crate) fn note_mutation(&mut self) -> Result<()> {
        self.mutation_generation = self
            .mutation_generation
            .checked_add(1)
            .ok_or(StorageError::Integrity("Workspace mutation generation"))?;
        Ok(())
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

pub(crate) fn portable_metadata(
    store: &CoreReader<'_>,
    root: layerfs_content::ObjectId,
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

impl Workspace {
    pub fn create_file(&mut self, parent: NodeId, name: &[u8], mode: u32) -> Result<Attr> {
        self.ensure_active()?;
        let path = self.child_path(parent, name)?;
        let node = self.new_spool_node(mode & 0o777, path.clone())?;
        self.insert_name(parent, name, node)?;
        self.note_mutation()?;
        self.attr(node)
    }

    pub fn mkdir(&mut self, parent: NodeId, name: &[u8], mode: u32) -> Result<Attr> {
        self.ensure_active()?;
        let path = self.child_path(parent, name)?;
        let node = self.allocate(Node {
            canonical: None,
            paths: BTreeSet::from([path.clone()]),
            mode: mode & 0o1777,
            links: 2,
            pins: 0,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            data: Data::Directory(DirectoryData {
                base: None,
                changes: BTreeMap::new(),
            }),
        });
        self.insert_name(parent, name, node)?;
        self.note_mutation()?;
        self.attr(node)
    }

    pub fn symlink(&mut self, parent: NodeId, name: &[u8], target: Vec<u8>) -> Result<Attr> {
        self.ensure_active()?;
        if target.len() > 4096 || target.contains(&0) {
            return Err(StorageError::InvalidInput("symlink"));
        }
        let path = self.child_path(parent, name)?;
        let node = self.allocate(Node {
            canonical: None,
            paths: BTreeSet::from([path.clone()]),
            mode: 0o777,
            links: 1,
            pins: 0,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            data: Data::Symlink(target.clone()),
        });
        self.insert_name(parent, name, node)?;
        self.note_mutation()?;
        self.attr(node)
    }

    pub fn link(&mut self, node: NodeId, parent: NodeId, name: &[u8]) -> Result<Attr> {
        self.ensure_active()?;
        if matches!(
            self.nodes
                .get(&node)
                .ok_or(StorageError::NotFound("node"))?
                .data,
            Data::Directory(_)
        ) {
            return Err(StorageError::InvalidInput("directory link"));
        }
        let target = self.child_path(parent, name)?;
        self.insert_name(parent, name, node)?;
        let value = self.nodes.get_mut(&node).unwrap();
        value.links += 1;
        value.paths.insert(target.clone());
        self.note_mutation()?;
        self.attr(node)
    }

    pub fn unlink(&mut self, parent: NodeId, name: &[u8], directory: bool) -> Result<()> {
        self.ensure_active()?;
        let node = self.lookup_node(parent, name)?;
        let value = self.nodes.get(&node).unwrap();
        if directory != matches!(value.data, Data::Directory(_)) {
            return Err(StorageError::InvalidInput("unlink kind"));
        }
        if directory && !self.directory_entries(node)?.is_empty() {
            return Err(StorageError::InvalidInput("directory not empty"));
        }
        let path = self.child_path(parent, name)?;
        self.directory_mut(parent)?
            .changes
            .insert(name.to_vec(), None);
        let value = self.nodes.get_mut(&node).unwrap();
        value.links = value.links.saturating_sub(1);
        value.paths.remove(&path);
        self.reclaim(node);
        self.note_mutation()?;
        Ok(())
    }

    pub fn rename(
        &mut self,
        parent: NodeId,
        name: &[u8],
        target_parent: NodeId,
        target: &[u8],
        no_replace: bool,
    ) -> Result<()> {
        self.ensure_active()?;
        let node = self.lookup_node(parent, name)?;
        let source = self.child_path(parent, name)?;
        let destination = self.child_path(target_parent, target)?;
        if source == destination {
            return Ok(());
        }
        let source_directory = matches!(self.nodes[&node].data, Data::Directory(_));
        let existing = match self.lookup_node(target_parent, target) {
            Ok(existing) if existing == node => return Ok(()),
            Ok(existing) => Some(existing),
            Err(StorageError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        if no_replace && existing.is_some() {
            return Err(StorageError::InvalidInput("rename target"));
        }
        if let Some(existing) = existing {
            let target_directory = matches!(self.nodes[&existing].data, Data::Directory(_));
            if source_directory != target_directory {
                return Err(StorageError::InvalidInput("rename type"));
            }
        }
        if source_directory {
            let target_parent_path = self.path_of(target_parent)?;
            if target_parent_path == source
                || (target_parent_path.starts_with(&source)
                    && target_parent_path.as_bytes().get(source.len()) == Some(&b'/'))
            {
                return Err(StorageError::InvalidInput("rename descendant"));
            }
        }
        if let Some(existing) = existing {
            if source_directory && !self.directory_is_empty(existing)? {
                return Err(StorageError::InvalidInput("directory not empty"));
            }
        }
        if existing.is_some() {
            self.unlink(target_parent, target, source_directory)?;
        }
        self.directory_mut(parent)?
            .changes
            .insert(name.to_vec(), None);
        self.directory_mut(target_parent)?
            .changes
            .insert(target.to_vec(), Some(node));
        self.replace_path_prefix(&source, &destination);
        self.note_mutation()?;
        Ok(())
    }

    pub fn pin(&mut self, node: NodeId, truncate: bool) -> Result<()> {
        if !matches!(
            self.nodes
                .get(&node)
                .ok_or(StorageError::NotFound("node"))?
                .data,
            Data::File(_)
        ) {
            return Err(StorageError::InvalidInput("open"));
        }
        if truncate {
            self.truncate(node, 0)?;
        }
        self.nodes.get_mut(&node).unwrap().pins += 1;
        Ok(())
    }

    pub fn unpin(&mut self, node: NodeId) -> Result<()> {
        let value = self
            .nodes
            .get_mut(&node)
            .ok_or(StorageError::NotFound("node"))?;
        value.pins = value
            .pins
            .checked_sub(1)
            .ok_or(StorageError::Integrity("node pin"))?;
        self.reclaim(node);
        Ok(())
    }

    pub fn chmod(&mut self, node: NodeId, mode: u32) -> Result<()> {
        self.ensure_active()?;
        self.nodes
            .get_mut(&node)
            .ok_or(StorageError::NotFound("node"))?
            .mode = mode & 0o1777;
        self.note_mutation()?;
        Ok(())
    }

    pub fn set_mtime(&mut self, node: NodeId, seconds: i64, nanos: u32) -> Result<()> {
        self.ensure_active()?;
        if nanos > 999_999_999 {
            return Err(StorageError::InvalidInput("mtime"));
        }
        let value = self
            .nodes
            .get_mut(&node)
            .ok_or(StorageError::NotFound("node"))?;
        value.mtime_seconds = seconds;
        value.mtime_nanoseconds = nanos;
        self.note_mutation()?;
        Ok(())
    }

    fn insert_name(&mut self, parent: NodeId, name: &[u8], node: NodeId) -> Result<()> {
        match self.lookup_node(parent, name) {
            Ok(_) => return Err(StorageError::InvalidInput("name exists")),
            Err(StorageError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
        self.directory_mut(parent)?
            .changes
            .insert(name.to_vec(), Some(node));
        Ok(())
    }

    fn replace_path_prefix(&mut self, source: &str, target: &str) {
        for node in self.nodes.values_mut() {
            node.paths = node
                .paths
                .iter()
                .map(|path| {
                    if path == source {
                        target.to_owned()
                    } else if path.starts_with(source)
                        && path.as_bytes().get(source.len()) == Some(&b'/')
                    {
                        format!("{target}{}", &path[source.len()..])
                    } else {
                        path.clone()
                    }
                })
                .collect();
        }
    }

    fn reclaim(&mut self, node: NodeId) {
        if self
            .nodes
            .get(&node)
            .is_some_and(|value| value.paths.is_empty() && value.pins == 0)
        {
            self.dirty.remove(&node);
            if let Some(value) = self.nodes.remove(&node) {
                if let Some(inode) = value.canonical {
                    self.canonical_nodes.remove(&inode);
                }
                if let Data::File(FileData::Overlay { spool, charged, .. }) = value.data {
                    self.spool_bytes = self
                        .spool_bytes
                        .saturating_sub(charged.iter().map(|(start, end)| end - start).sum());
                    let _ = std::fs::remove_file(spool);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ROOT;
    use layerfs_branch_store::BranchStore;
    use layerfs_layer_store::LayerStore;
    use std::sync::Arc;

    #[derive(Debug, Eq, PartialEq)]
    struct Snapshot {
        nodes: std::collections::HashMap<NodeId, Node>,
        canonical_nodes: std::collections::HashMap<layerfs_content::tree::inode::InodeId, NodeId>,
        dirty: BTreeSet<NodeId>,
        next_node: u64,
        spool_bytes: u64,
    }

    fn snapshot(workspace: &Workspace) -> Snapshot {
        Snapshot {
            nodes: workspace.nodes.clone(),
            canonical_nodes: workspace.canonical_nodes.clone(),
            dirty: workspace.dirty.clone(),
            next_node: workspace.next_node,
            spool_bytes: workspace.spool_bytes,
        }
    }

    fn fixture(label: &str) -> (std::path::PathBuf, Workspace) {
        let root = std::env::temp_dir().join(format!(
            "layerfs-rename-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let layer = Arc::new(LayerStore::create(root.join("layer.sqlite")).unwrap());
        let (_history, genesis) = layer
            .initialize(layerfs_storage::LayerInitialization::Empty)
            .unwrap();
        let branch = BranchStore::create(root.join("branch.sqlite"), layer).unwrap();
        let record = branch
            .create_branch(layerfs_storage::BranchSource::Layer(genesis.id))
            .unwrap();
        let workspace = Workspace::open(branch, record.id, root.join("spool")).unwrap();
        (root, workspace)
    }

    #[test]
    fn rename_validates_every_noop_and_rejection_before_mutation() {
        let (root, mut workspace) = fixture("same-path");
        workspace.create_file(ROOT, b"a", 0o600).unwrap();
        let before = snapshot(&workspace);
        workspace.rename(ROOT, b"a", ROOT, b"a", false).unwrap();
        assert_eq!(snapshot(&workspace), before);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut workspace) = fixture("same-inode");
        let file = workspace.create_file(ROOT, b"a", 0o600).unwrap();
        workspace.link(file.node, ROOT, b"b").unwrap();
        let before = snapshot(&workspace);
        workspace.rename(ROOT, b"a", ROOT, b"b", false).unwrap();
        assert_eq!(snapshot(&workspace), before);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut workspace) = fixture("file-over-directory");
        workspace.create_file(ROOT, b"file", 0o600).unwrap();
        workspace.mkdir(ROOT, b"directory", 0o700).unwrap();
        assert_rejected(&mut workspace, |workspace| {
            workspace.rename(ROOT, b"file", ROOT, b"directory", false)
        });
        assert_rejected(&mut workspace, |workspace| {
            workspace.rename(ROOT, b"directory", ROOT, b"file", false)
        });
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut workspace) = fixture("nonempty-directory");
        workspace.mkdir(ROOT, b"source", 0o700).unwrap();
        let target = workspace.mkdir(ROOT, b"target", 0o700).unwrap();
        workspace.create_file(target.node, b"child", 0o600).unwrap();
        assert_rejected(&mut workspace, |workspace| {
            workspace.rename(ROOT, b"source", ROOT, b"target", false)
        });
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut workspace) = fixture("descendant");
        let source = workspace.mkdir(ROOT, b"source", 0o700).unwrap();
        let child = workspace.mkdir(source.node, b"child", 0o700).unwrap();
        workspace.create_file(child.node, b"target", 0o600).unwrap();
        assert_rejected(&mut workspace, |workspace| {
            workspace.rename(ROOT, b"source", child.node, b"target", false)
        });
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_spool_io_does_not_advance_the_overlay() {
        let (root, mut workspace) = fixture("failed-write");
        let file = workspace.create_file(ROOT, b"file", 0o600).unwrap();
        workspace.write(file.node, 0, b"base").unwrap();
        let Data::File(FileData::Overlay { spool, .. }) = &workspace.nodes[&file.node].data else {
            panic!("expected overlay")
        };
        std::fs::remove_file(spool).unwrap();
        let before = snapshot(&workspace);
        assert!(workspace.write(file.node, 4, b"lost").is_err());
        assert_eq!(snapshot(&workspace), before);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut workspace) = fixture("failed-truncate");
        let file = workspace.create_file(ROOT, b"file", 0o600).unwrap();
        workspace.write(file.node, 0, b"base").unwrap();
        let Data::File(FileData::Overlay { spool, .. }) = &workspace.nodes[&file.node].data else {
            panic!("expected overlay")
        };
        std::fs::remove_file(spool).unwrap();
        let before = snapshot(&workspace);
        assert!(workspace.truncate(file.node, 2).is_err());
        assert_eq!(snapshot(&workspace), before);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn assert_rejected(
        workspace: &mut Workspace,
        operation: impl FnOnce(&mut Workspace) -> Result<()>,
    ) {
        let before = snapshot(workspace);
        assert!(operation(workspace).is_err());
        assert_eq!(snapshot(workspace), before);
    }
}
