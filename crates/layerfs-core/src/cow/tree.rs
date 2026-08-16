use std::collections::BTreeMap;
use std::sync::Arc;

use crate::identity::{ObjectHashWriter, ObjectId};
use crate::{CanonicalName, CanonicalPath, CoreError, CoreResult, LogicalFile};

pub type NodeId = ObjectId;
pub type RootId = ObjectId;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Metadata {
    mode: u32,
}

impl Metadata {
    pub const fn new(mode: u32) -> Self {
        Self { mode }
    }

    pub const fn mode(self) -> u32 {
        self.mode
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind {
    File,
    Directory,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum NodeData {
    File {
        content: LogicalFile,
        metadata: Metadata,
    },
    Directory {
        entries: BTreeMap<CanonicalName, TreeNode>,
        metadata: Metadata,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeNode(Arc<NodeDataWithId>);

#[derive(Clone, Debug, Eq, PartialEq)]
struct NodeDataWithId {
    id: NodeId,
    data: NodeData,
}

impl TreeNode {
    pub fn file(content: LogicalFile) -> Self {
        Self::file_with_metadata(content, Metadata::default())
    }

    pub fn file_with_metadata(content: LogicalFile, metadata: Metadata) -> Self {
        Self::from_data(NodeData::File { content, metadata })
    }

    pub fn directory<I>(entries: I) -> CoreResult<Self>
    where
        I: IntoIterator<Item = (CanonicalName, TreeNode)>,
    {
        Self::directory_with_metadata(entries, Metadata::default())
    }

    pub fn directory_with_metadata<I>(entries: I, metadata: Metadata) -> CoreResult<Self>
    where
        I: IntoIterator<Item = (CanonicalName, TreeNode)>,
    {
        let mut map = BTreeMap::new();
        for (name, node) in entries {
            if map.len() >= crate::limits::MAX_CHILD_REFERENCES {
                return Err(CoreError::ObjectLimitExceeded);
            }
            if map.insert(name, node).is_some() {
                return Err(CoreError::NameCollision);
            }
        }
        Ok(Self::from_data(NodeData::Directory {
            entries: map,
            metadata,
        }))
    }

    pub fn empty_directory() -> Self {
        Self::from_data(NodeData::Directory {
            entries: BTreeMap::new(),
            metadata: Metadata::default(),
        })
    }

    pub fn kind(&self) -> NodeKind {
        match &self.0.data {
            NodeData::File { .. } => NodeKind::File,
            NodeData::Directory { .. } => NodeKind::Directory,
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(&self.0.data, NodeData::File { .. })
    }

    pub fn is_directory(&self) -> bool {
        matches!(&self.0.data, NodeData::Directory { .. })
    }

    pub fn identity(&self) -> NodeId {
        self.0.id
    }

    pub fn metadata(&self) -> Metadata {
        match &self.0.data {
            NodeData::File { metadata, .. } | NodeData::Directory { metadata, .. } => *metadata,
        }
    }

    pub fn file_content(&self) -> Option<&LogicalFile> {
        match &self.0.data {
            NodeData::File { content, .. } => Some(content),
            NodeData::Directory { .. } => None,
        }
    }

    pub fn entries(&self) -> Option<&BTreeMap<CanonicalName, TreeNode>> {
        match &self.0.data {
            NodeData::File { .. } => None,
            NodeData::Directory { entries, .. } => Some(entries),
        }
    }

    pub fn with_metadata(&self, metadata: Metadata) -> Self {
        if self.metadata() == metadata {
            return self.clone();
        }
        match &self.0.data {
            NodeData::File { content, .. } => Self::file_with_metadata(content.clone(), metadata),
            NodeData::Directory { entries, .. } => Self::from_data(NodeData::Directory {
                entries: entries.clone(),
                metadata,
            }),
        }
    }

    pub fn ptr_eq(left: &Self, right: &Self) -> bool {
        Arc::ptr_eq(&left.0, &right.0)
    }

    fn from_data(data: NodeData) -> Self {
        let id = provisional_id(&data);
        Self(Arc::new(NodeDataWithId { id, data }))
    }

    pub(crate) fn add_child(&self, name: CanonicalName, node: TreeNode) -> CoreResult<Self> {
        let NodeData::Directory { entries, metadata } = &self.0.data else {
            return Err(CoreError::NotDirectory);
        };
        if entries.contains_key(&name) {
            return Err(CoreError::NameCollision);
        }
        if entries.len() >= crate::limits::MAX_CHILD_REFERENCES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        let mut next = entries.clone();
        next.insert(name, node);
        Ok(Self::from_data(NodeData::Directory {
            entries: next,
            metadata: *metadata,
        }))
    }

    pub(crate) fn remove_child(&self, name: &CanonicalName) -> CoreResult<(Self, TreeNode)> {
        let NodeData::Directory { entries, metadata } = &self.0.data else {
            return Err(CoreError::NotDirectory);
        };
        let Some(removed) = entries.get(name).cloned() else {
            return Err(CoreError::PathNotFound);
        };
        let mut next = entries.clone();
        next.remove(name);
        Ok((
            Self::from_data(NodeData::Directory {
                entries: next,
                metadata: *metadata,
            }),
            removed,
        ))
    }

    pub(crate) fn replace_child(&self, name: &CanonicalName, node: TreeNode) -> CoreResult<Self> {
        let NodeData::Directory { entries, metadata } = &self.0.data else {
            return Err(CoreError::NotDirectory);
        };
        if !entries.contains_key(name) {
            return Err(CoreError::PathNotFound);
        }
        let mut next = entries.clone();
        next.insert(name.clone(), node);
        Ok(Self::from_data(NodeData::Directory {
            entries: next,
            metadata: *metadata,
        }))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootHandle {
    directory: TreeNode,
}

impl RootHandle {
    pub fn new(directory: TreeNode) -> CoreResult<Self> {
        if !directory.is_directory() {
            return Err(CoreError::NotDirectory);
        }
        Ok(Self { directory })
    }

    pub fn empty() -> Self {
        Self {
            directory: TreeNode::empty_directory(),
        }
    }

    pub fn from_entries<I>(entries: I) -> CoreResult<Self>
    where
        I: IntoIterator<Item = (CanonicalName, TreeNode)>,
    {
        Self::new(TreeNode::directory(entries)?)
    }

    pub fn id(&self) -> RootId {
        self.directory.identity()
    }

    pub fn node(&self) -> &TreeNode {
        &self.directory
    }

    pub fn metadata(&self) -> Metadata {
        self.directory.metadata()
    }

    pub fn lookup(&self, path: &CanonicalPath) -> CoreResult<Option<&TreeNode>> {
        let mut current = &self.directory;
        for component in path.components() {
            let name = CanonicalName::from_bytes(component)?;
            let Some(entries) = current.entries() else {
                return Err(CoreError::NotDirectory);
            };
            let Some(next) = entries.get(&name) else {
                return Ok(None);
            };
            current = next;
        }
        Ok(Some(current))
    }

    pub fn lookup_required(&self, path: &CanonicalPath) -> CoreResult<&TreeNode> {
        self.lookup(path)?.ok_or(CoreError::PathNotFound)
    }

    pub(crate) fn from_directory(directory: TreeNode) -> Self {
        Self { directory }
    }

    pub(crate) fn directory(&self) -> &TreeNode {
        &self.directory
    }
}

// This fingerprint is deliberately provisional. It is an in-memory identity for
// structural sharing and delta checks, not a frozen tree/object encoding.
fn provisional_id(data: &NodeData) -> NodeId {
    let mut writer = ObjectHashWriter::new();
    writer.update(b"layerfs/provisional-tree-node\0");
    match data {
        NodeData::File { content, metadata } => {
            writer.update(&[0x01]);
            write_u32(&mut writer, metadata.mode());
            write_u64(&mut writer, content.length());
            write_u64(&mut writer, bounded_u64(content.chunks().len()));
            for chunk in content.chunks() {
                writer.update(chunk.id().as_bytes());
                write_u64(&mut writer, chunk.length());
            }
        }
        NodeData::Directory { entries, metadata } => {
            writer.update(&[0x02]);
            write_u32(&mut writer, metadata.mode());
            write_u64(&mut writer, bounded_u64(entries.len()));
            for (name, child) in entries {
                write_u64(&mut writer, bounded_u64(name.as_bytes().len()));
                writer.update(name.as_bytes());
                writer.update(child.identity().as_bytes());
            }
        }
    }
    ObjectId::from_digest(writer.finish())
}

fn write_u32(writer: &mut ObjectHashWriter, value: u32) {
    writer.update(&value.to_be_bytes());
}

fn write_u64(writer: &mut ObjectHashWriter, value: u64) {
    writer.update(&value.to_be_bytes());
}

fn bounded_u64(value: usize) -> u64 {
    value as u64
}
