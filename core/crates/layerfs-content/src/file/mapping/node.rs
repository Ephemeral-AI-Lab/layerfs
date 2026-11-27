//! Typed extent-tree fields and their checked invariants.
//!
//! The extent tree maps logical file offsets onto slices of canonical chunk
//! payloads. Leaves carry payload slices; branches carry strictly increasing
//! summaries of their children. Every field is validated while it is built or
//! decoded; no separate validation pass runs afterwards.

use crate::error::{ContentError, ContentResult};
use crate::object::ObjectId;
use crate::policy::MAX_MAPPING_ENTRIES;

/// Smallest entry count of a non-root page.
pub const MIN_ENTRIES: usize = 64;
/// Largest entry count of any page.
pub const MAX_ENTRIES: usize = MAX_MAPPING_ENTRIES;
/// Largest branch level the grammar can express.
pub const MAX_LEVEL: u8 = 31;
/// Smallest children of a root branch: a root summary is never a single child.
pub const MINIMUM_ROOT_ENTRIES: usize = 2;
/// Smallest entries of a root leaf; the frozen profile records it as zero.
pub const MINIMUM_ROOT_LEAF_ENTRIES: usize = 0;
/// Largest canonical mapping node object.
pub const MAX_NODE_OBJECT_BYTES: usize = 8_192;

/// One slice of a canonical chunk payload used at a logical offset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtentSlice {
    payload_object_id: ObjectId,
    source_offset: u32,
    logical_length: u32,
}

impl ExtentSlice {
    /// Checks that the slice is nonempty and lies inside a canonical chunk payload.
    ///
    /// The upper bound is the frozen chunk grammar's own raw maximum, not the
    /// payload the slice happens to name: a chunk payload can never decode to
    /// more than [`cdc::MAXIMUM_CHUNK_BYTES`] bytes, so a slice reaching past that
    /// is unsatisfiable by every payload that could ever be named here. Checking
    /// it while the slice is constructed rejects a forged or corrupt mapping page
    /// at decode, instead of deferring the refusal to the read that first slices
    /// the payload.
    pub fn new(
        payload_object_id: ObjectId,
        source_offset: u32,
        logical_length: u32,
    ) -> ContentResult<Self> {
        let end = source_offset
            .checked_add(logical_length)
            .ok_or(ContentError::InvalidRecord("extent slice"))?;
        if logical_length == 0 || end > crate::file::cdc::MAXIMUM_CHUNK_BYTES as u32 {
            return Err(ContentError::InvalidRecord("extent slice"));
        }
        Ok(Self {
            payload_object_id,
            source_offset,
            logical_length,
        })
    }

    /// Canonical chunk this slice reads from.
    pub const fn payload_object_id(self) -> ObjectId {
        self.payload_object_id
    }

    /// Offset inside the chunk payload.
    pub const fn source_offset(self) -> u32 {
        self.source_offset
    }

    /// Number of logical bytes this slice contributes.
    pub const fn logical_length(self) -> u32 {
        self.logical_length
    }
}

/// One child of a branch page with its cumulative totals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChildDescriptor {
    /// Logical bytes covered by this child and every earlier sibling.
    pub cumulative_logical_end: u64,
    /// Extents covered by this child and every earlier sibling.
    pub cumulative_extent_end: u64,
    /// Child node identity.
    pub child_object_id: ObjectId,
}

/// One canonical extent-tree page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtentNode {
    /// A page of payload slices.
    Leaf {
        /// Logical bytes covered by this page.
        subtree_logical_bytes: u64,
        /// The slices, in logical order.
        extents: Vec<ExtentSlice>,
    },
    /// A page of child summaries.
    Branch {
        /// Level of this page; children are one level lower.
        level: u8,
        /// Logical bytes covered by this page.
        subtree_logical_bytes: u64,
        /// Extents covered by this page.
        subtree_extent_count: u64,
        /// Child summaries, in logical order.
        children: Vec<ChildDescriptor>,
    },
}

impl ExtentNode {
    /// Level of this page; a leaf is level zero.
    pub fn level(&self) -> u8 {
        match self {
            Self::Leaf { .. } => 0,
            Self::Branch { level, .. } => *level,
        }
    }

    /// Logical bytes covered.
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

    /// Extents covered.
    pub fn extent_count(&self) -> u64 {
        match self {
            Self::Leaf { extents, .. } => extents.len() as u64,
            Self::Branch {
                subtree_extent_count,
                ..
            } => *subtree_extent_count,
        }
    }

    /// Entries on this page.
    pub fn entry_count(&self) -> usize {
        match self {
            Self::Leaf { extents, .. } => extents.len(),
            Self::Branch { children, .. } => children.len(),
        }
    }

    /// Direct child identities, in canonical order.
    pub fn references(&self) -> Vec<ObjectId> {
        match self {
            Self::Leaf { extents, .. } => extents
                .iter()
                .map(|extent| extent.payload_object_id)
                .collect(),
            Self::Branch { children, .. } => {
                children.iter().map(|child| child.child_object_id).collect()
            }
        }
    }

    /// Checks the page partition, entry arithmetic and ordering.
    ///
    /// `root` pages may be short and may be a single-child summary; non-root
    /// pages must hold at least [`MIN_ENTRIES`] entries so the canonical
    /// partition is unique.
    pub fn validate(&self, root: bool) -> ContentResult<()> {
        let count = self.entry_count();
        if count > MAX_ENTRIES || (!root && count < MIN_ENTRIES) {
            return Err(ContentError::NonCanonicalPagePartition);
        }
        match self {
            Self::Leaf {
                subtree_logical_bytes,
                extents,
            } => {
                let mut total = 0_u64;
                let mut previous: Option<ExtentSlice> = None;
                for extent in extents {
                    ExtentSlice::new(
                        extent.payload_object_id,
                        extent.source_offset,
                        extent.logical_length,
                    )?;
                    if previous.is_some_and(|prior| {
                        prior.payload_object_id == extent.payload_object_id
                            && prior.source_offset.checked_add(prior.logical_length)
                                == Some(extent.source_offset)
                    }) {
                        return Err(ContentError::NonCanonicalPagePartition);
                    }
                    total = total
                        .checked_add(u64::from(extent.logical_length))
                        .ok_or(ContentError::LengthOverflow)?;
                    previous = Some(*extent);
                }
                if total != *subtree_logical_bytes {
                    return Err(ContentError::LengthMismatch {
                        expected: *subtree_logical_bytes,
                        actual: total,
                    });
                }
            }
            Self::Branch {
                level,
                subtree_logical_bytes,
                subtree_extent_count,
                children,
            } => {
                if *level == 0
                    || *level > MAX_LEVEL
                    || (root && children.len() < MINIMUM_ROOT_ENTRIES)
                {
                    return Err(ContentError::NonCanonicalPagePartition);
                }
                let mut bytes = 0;
                let mut extents = 0;
                for child in children {
                    if child.cumulative_logical_end <= bytes
                        || child.cumulative_extent_end <= extents
                    {
                        return Err(ContentError::NonCanonicalOrdering);
                    }
                    bytes = child.cumulative_logical_end;
                    extents = child.cumulative_extent_end;
                }
                if bytes != *subtree_logical_bytes || extents != *subtree_extent_count {
                    return Err(ContentError::InvalidRecord("extent branch summary"));
                }
            }
        }
        Ok(())
    }
}

/// Canonical root of a chunked file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileState {
    /// Logical length of the file.
    pub logical_len: u64,
    /// Total extents in the mapping tree.
    pub extent_count: u64,
    /// Level of the mapping root.
    pub tree_level: u8,
    /// Frozen mapping profile identity.
    pub profile_id: ObjectId,
    /// Identity of the mapping root node.
    pub mapping_root: ObjectId,
}

/// A summary of a completed node, retained while its parent is unfinished.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeSummary {
    /// Node identity.
    pub id: ObjectId,
    /// Logical bytes covered.
    pub bytes: u64,
    /// Extents covered.
    pub extents: u64,
    /// Node level.
    pub level: u8,
}
