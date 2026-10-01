//! One bounded draft representation; decoded private bytes are not canonical bytes.
use crate::file::mapping::{
    ChildDescriptor, ExtentNode, ExtentSlice, MAX_ENTRIES, MAX_NODE_OBJECT_BYTES,
};
use crate::{ContentError, ContentResult, FinalizedObject, ObjectId, ObjectRole};

/// A single bounded metadata body; payload objects are never draft records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftRecord {
    /// A finalized immutable mapping page, with ordered references/predecessors.
    Page(FinalizedObject),
    /// An unfinished node with private draft references, encoded only at finality.
    Node(ExtentNode),
}
impl DraftRecord {
    /// Validate the real bounded representation before retention or provider effects.
    pub fn validate(&self) -> ContentResult<()> {
        let node = match self {
            Self::Node(node) => {
                let capacity = match node {
                    ExtentNode::Leaf { extents, .. } => extents.capacity(),
                    ExtentNode::Branch { children, .. } => children.capacity(),
                };
                if capacity > MAX_ENTRIES {
                    return Err(ContentError::InvalidRecord("draft decoded owner capacity"));
                }
                node.clone()
            }
            Self::Page(object) => {
                if !matches!(
                    object.role(),
                    ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch
                ) || object.canonical_len() > MAX_NODE_OBJECT_BYTES
                    || object.canonical_capacity() > MAX_NODE_OBJECT_BYTES
                    || object.references().len() > MAX_ENTRIES
                    || object.reference_capacity() > MAX_ENTRIES
                {
                    return Err(ContentError::InvalidRecord("draft page shape"));
                }
                let node =
                    crate::file::mapping::decode_node_with_context(object.canonical(), true)?;
                if node.references() != object.references()
                    || (node.level() == 0) != (object.role() == ObjectRole::ExtentLeaf)
                {
                    return Err(ContentError::InvalidRecord("draft page references"));
                }
                node
            }
        };
        node.validate(true)?;
        Ok(())
    }
    /// Decode one boundary only when the edit actually requires it.
    pub fn node(&self, root: bool) -> ContentResult<ExtentNode> {
        match self {
            Self::Node(node) => {
                node.validate(root)?;
                Ok(node.clone())
            }
            Self::Page(object) => {
                crate::file::mapping::decode_node_with_context(object.canonical(), root)
            }
        }
    }
    /// Direct mapping child keys only, in exact descriptor order.
    pub fn children(&self) -> ContentResult<Vec<ObjectId>> {
        match self.node(true)? {
            ExtentNode::Branch { children, .. } => Ok(children
                .into_iter()
                .map(|child| child.child_object_id)
                .collect()),
            ExtentNode::Leaf { .. } => Ok(Vec::new()),
        }
    }
    /// All direct reference facts, including leaf payload identities.
    pub fn references(&self) -> Vec<ObjectId> {
        match self {
            Self::Node(node) => node.references(),
            Self::Page(object) => object.references().to_vec(),
        }
    }
    /// Historical logical unfinished-body charge; not allocated capacity or full metadata.
    pub fn deferred_body_charge(&self) -> usize {
        match self {
            Self::Page(object) => object.canonical_len().saturating_add(128),
            Self::Node(node) => {
                let entries = match node {
                    ExtentNode::Leaf { extents, .. } => extents
                        .len()
                        .saturating_mul(std::mem::size_of::<ExtentSlice>()),
                    ExtentNode::Branch { children, .. } => children
                        .len()
                        .saturating_mul(std::mem::size_of::<ChildDescriptor>()),
                };
                std::mem::size_of::<ExtentNode>()
                    .saturating_add(entries)
                    .saturating_add(128)
            }
        }
    }
    /// Framed aggregate metadata charge, including counts/jobs/associated facts.
    pub fn charge(&self) -> usize {
        let body = match self {
            Self::Page(object) => object.canonical_len(),
            Self::Node(ExtentNode::Leaf { extents, .. }) => 20 + extents.len() * 40,
            Self::Node(ExtentNode::Branch { children, .. }) => 20 + children.len() * 48,
        };
        let predecessors = match self {
            Self::Page(object) => object.predecessors().len(),
            Self::Node(_) => 0,
        };
        body + 55 + 151 + 63 + 71 + self.references().len() * 89 + predecessors * 90
    }
}
