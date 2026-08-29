use crate::{CoreError, CoreResult, ObjectId};

pub const MIN_ENTRIES: usize = 64;
pub const MAX_ENTRIES: usize = 128;
pub const MAX_LEVEL: u8 = 31;
pub const MAX_NODE_OBJECT_BYTES: usize = 8_192;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtentSliceV3 {
    pub payload_object_id: ObjectId,
    pub source_offset: u32,
    pub logical_length: u32,
}

impl ExtentSliceV3 {
    pub fn new(
        payload_object_id: ObjectId,
        source_offset: u32,
        logical_length: u32,
    ) -> CoreResult<Self> {
        if logical_length == 0 || source_offset.checked_add(logical_length).is_none() {
            return Err(CoreError::InvalidRecord("extent slice"));
        }
        Ok(Self {
            payload_object_id,
            source_offset,
            logical_length,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChildDescriptorV3 {
    pub cumulative_logical_end: u64,
    pub cumulative_extent_end: u64,
    pub child_object_id: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtentNodeV3 {
    Leaf {
        subtree_logical_bytes: u64,
        extents: Vec<ExtentSliceV3>,
    },
    Branch {
        level: u8,
        subtree_logical_bytes: u64,
        subtree_extent_count: u64,
        children: Vec<ChildDescriptorV3>,
    },
}

impl ExtentNodeV3 {
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
    pub fn entry_count(&self) -> usize {
        match self {
            Self::Leaf { extents, .. } => extents.len(),
            Self::Branch { children, .. } => children.len(),
        }
    }

    pub fn validate(&self, root: bool) -> CoreResult<()> {
        let count = self.entry_count();
        if count > MAX_ENTRIES || (!root && count < MIN_ENTRIES) {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        match self {
            Self::Leaf {
                subtree_logical_bytes,
                extents,
            } => {
                let mut total = 0_u64;
                let mut previous: Option<ExtentSliceV3> = None;
                for extent in extents {
                    ExtentSliceV3::new(
                        extent.payload_object_id,
                        extent.source_offset,
                        extent.logical_length,
                    )?;
                    if previous.is_some_and(|prior| {
                        prior.payload_object_id == extent.payload_object_id
                            && prior.source_offset.checked_add(prior.logical_length)
                                == Some(extent.source_offset)
                    }) {
                        return Err(CoreError::NonCanonicalPagePartition);
                    }
                    total = total
                        .checked_add(u64::from(extent.logical_length))
                        .ok_or(CoreError::LengthOverflow)?;
                    previous = Some(*extent);
                }
                if total != *subtree_logical_bytes {
                    return Err(CoreError::LengthMismatch {
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
                if *level == 0 || *level > MAX_LEVEL || (root && children.len() < 2) {
                    return Err(CoreError::NonCanonicalPagePartition);
                }
                let mut bytes = 0;
                let mut extents = 0;
                for child in children {
                    if child.cumulative_logical_end <= bytes
                        || child.cumulative_extent_end <= extents
                    {
                        return Err(CoreError::NonCanonicalOrdering);
                    }
                    bytes = child.cumulative_logical_end;
                    extents = child.cumulative_extent_end;
                }
                if bytes != *subtree_logical_bytes || extents != *subtree_extent_count {
                    return Err(CoreError::InvalidRecord("extent branch summary"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileStateV3 {
    pub logical_len: u64,
    pub extent_count: u64,
    pub tree_level: u8,
    pub profile_id: ObjectId,
    pub mapping_root: ObjectId,
}
