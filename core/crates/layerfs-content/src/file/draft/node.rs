//! Exact private provenance; canonical digests are never draft namespace tags.
use crate::file::mapping::{ChildDescriptor, ExtentNode, ExtentSlice, NodeSummary};
use crate::{ContentError, ContentResult, ObjectId};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum EditRef {
    Stored(ObjectId),
    Node(usize),
    Page(ObjectId),
}
impl EditRef {
    pub fn draft_kind(self) -> Option<u32> {
        match self {
            Self::Stored(_) => None,
            Self::Node(_) => Some(1),
            Self::Page(_) => Some(2),
        }
    }
    pub fn key(self) -> [u8; 32] {
        match self {
            Self::Stored(id) | Self::Page(id) => id.to_bytes(),
            Self::Node(number) => {
                let mut key = [0; 32];
                key[24..].copy_from_slice(&(number as u64).to_be_bytes());
                key
            }
        }
    }
    pub fn from_key(kind: u32, key: [u8; 32]) -> ContentResult<Self> {
        match kind {
            1 if key[..24] == [0; 24] => Ok(Self::Node(
                usize::try_from(u64::from_be_bytes(
                    key[24..]
                        .try_into()
                        .map_err(|_| ContentError::InvalidRecord("draft counter"))?,
                ))
                .map_err(|_| ContentError::LengthOverflow)?,
            )),
            2 => Ok(Self::Page(ObjectId::from_bytes(&key)?)),
            _ => Err(ContentError::InvalidRecord("draft key domain")),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Summary {
    pub id: EditRef,
    pub bytes: u64,
    pub extents: u64,
    pub level: u8,
}
impl Summary {
    pub fn from_canonical(value: NodeSummary, page: bool) -> Self {
        Self {
            id: if page {
                EditRef::Page(value.id)
            } else {
                EditRef::Stored(value.id)
            },
            bytes: value.bytes,
            extents: value.extents,
            level: value.level,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Child {
    pub cumulative_logical_end: u64,
    pub cumulative_extent_end: u64,
    pub child_object_id: EditRef,
}
#[derive(Clone, Debug)]
pub(super) enum Node {
    Leaf {
        subtree_logical_bytes: u64,
        extents: Vec<ExtentSlice>,
    },
    Branch {
        level: u8,
        subtree_logical_bytes: u64,
        subtree_extent_count: u64,
        children: Vec<Child>,
    },
}
impl Node {
    pub fn entry_count(&self) -> usize {
        match self {
            Self::Leaf { extents, .. } => extents.len(),
            Self::Branch { children, .. } => children.len(),
        }
    }
    pub fn from_canonical(value: ExtentNode, page: bool) -> Self {
        match value {
            ExtentNode::Leaf {
                subtree_logical_bytes,
                extents,
            } => Self::Leaf {
                subtree_logical_bytes,
                extents,
            },
            ExtentNode::Branch {
                level,
                subtree_logical_bytes,
                subtree_extent_count,
                children,
            } => Self::Branch {
                level,
                subtree_logical_bytes,
                subtree_extent_count,
                children: children
                    .into_iter()
                    .map(|child| Child {
                        cumulative_logical_end: child.cumulative_logical_end,
                        cumulative_extent_end: child.cumulative_extent_end,
                        child_object_id: if page {
                            EditRef::Page(child.child_object_id)
                        } else {
                            EditRef::Stored(child.child_object_id)
                        },
                    })
                    .collect(),
            },
        }
    }
    pub fn level(&self) -> u8 {
        match self {
            Self::Leaf { .. } => 0,
            Self::Branch { level, .. } => *level,
        }
    }
    pub fn logical_len(&self) -> u64 {
        match self {
            Self::Leaf {
                subtree_logical_bytes,
                ..
            }
            | Self::Branch {
                subtree_logical_bytes,
                ..
            } => *subtree_logical_bytes,
        }
    }
    pub fn extent_count(&self) -> u64 {
        match self {
            Self::Leaf { extents, .. } => extents.len() as u64,
            Self::Branch {
                subtree_extent_count,
                ..
            } => *subtree_extent_count,
        }
    }
    pub fn canonical(self) -> ContentResult<ExtentNode> {
        match self {
            Self::Leaf {
                subtree_logical_bytes,
                extents,
            } => Ok(ExtentNode::Leaf {
                subtree_logical_bytes,
                extents,
            }),
            Self::Branch {
                level,
                subtree_logical_bytes,
                subtree_extent_count,
                children,
            } => Ok(ExtentNode::Branch {
                level,
                subtree_logical_bytes,
                subtree_extent_count,
                children: children
                    .into_iter()
                    .map(|child| {
                        let EditRef::Stored(id) = child.child_object_id else {
                            return Err(ContentError::InvalidRecord("unresolved canonical child"));
                        };
                        Ok(ChildDescriptor {
                            cumulative_logical_end: child.cumulative_logical_end,
                            cumulative_extent_end: child.cumulative_extent_end,
                            child_object_id: id,
                        })
                    })
                    .collect::<ContentResult<_>>()?,
            }),
        }
    }
}
