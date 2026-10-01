//! Private metadata framing, distinct from canonical mapping encoding.
use super::draft_record::DraftRecord;
use crate::file::mapping::{
    ChildDescriptor, ExtentNode, ExtentSlice, MAX_ENTRIES, MAX_NODE_OBJECT_BYTES,
};
use crate::{
    AdvisoryPredecessors, ContentError, ContentResult, FinalizedObject, ObjectId, ObjectRole,
    PredecessorProvenance,
};
impl DraftRecord {
    /// Private form/role/body for a metadata-only provider; decoded nodes are not hashed.
    pub fn private_body(&self) -> ContentResult<(u8, u8, Vec<u8>)> {
        self.validate()?;
        match self {
            Self::Page(object) => Ok((0, object.role().code(), object.canonical().to_vec())),
            Self::Node(node) => {
                let count = match node {
                    ExtentNode::Leaf { extents, .. } => extents.len(),
                    ExtentNode::Branch { children, .. } => children.len(),
                };
                let mut bytes =
                    Vec::with_capacity(20 + count * if node.level() == 0 { 40 } else { 48 });
                bytes.extend_from_slice(&[1, node.level()]);
                bytes.extend_from_slice(&(count as u16).to_be_bytes());
                bytes.extend_from_slice(&node.logical_len().to_be_bytes());
                bytes.extend_from_slice(&node.extent_count().to_be_bytes());
                match node {
                    ExtentNode::Leaf { extents, .. } => {
                        for extent in extents {
                            bytes.extend_from_slice(extent.payload_object_id().as_bytes());
                            bytes.extend_from_slice(&extent.source_offset().to_be_bytes());
                            bytes.extend_from_slice(&extent.logical_length().to_be_bytes());
                        }
                    }
                    ExtentNode::Branch { children, .. } => {
                        for child in children {
                            bytes.extend_from_slice(&child.cumulative_logical_end.to_be_bytes());
                            bytes.extend_from_slice(&child.cumulative_extent_end.to_be_bytes());
                            bytes.extend_from_slice(child.child_object_id.as_bytes());
                        }
                    }
                }
                Ok((1, if node.level() == 0 { 3 } else { 4 }, bytes))
            }
        }
    }
    /// Exact ordered predecessor metadata, using closed provenance0..2.
    pub fn private_predecessors(&self) -> Vec<(ObjectId, u8)> {
        match self {
            Self::Node(_) => Vec::new(),
            Self::Page(object) => object
                .predecessors()
                .entries()
                .iter()
                .map(|entry| {
                    (
                        entry.id(),
                        match entry.provenance() {
                            PredecessorProvenance::OriginalBase => 0,
                            PredecessorProvenance::UnchangedPrefix => 1,
                            PredecessorProvenance::ReusedRange => 2,
                        },
                    )
                })
                .collect(),
        }
    }
    /// Reconstitute one checked bounded draft and verify exact reference/provenance custody.
    pub fn from_private(
        form: u8,
        role: u8,
        body: Vec<u8>,
        references: Vec<ObjectId>,
        predecessors: Vec<(ObjectId, u8)>,
    ) -> ContentResult<Self> {
        if body.len() > MAX_NODE_OBJECT_BYTES
            || body.capacity() > MAX_NODE_OBJECT_BYTES
            || references.len() > MAX_ENTRIES
            || references.capacity() > MAX_ENTRIES
            || predecessors.len() > 4
        {
            return Err(ContentError::InvalidRecord("draft private capacity"));
        }
        let record = match form {
            0 => {
                let mut hints = AdvisoryPredecessors::new();
                for (id, provenance) in predecessors {
                    hints.push(
                        id,
                        match provenance {
                            0 => PredecessorProvenance::OriginalBase,
                            1 => PredecessorProvenance::UnchangedPrefix,
                            2 => PredecessorProvenance::ReusedRange,
                            _ => {
                                return Err(ContentError::InvalidRecord(
                                    "draft predecessor provenance",
                                ))
                            }
                        },
                    )?;
                }
                Self::Page(
                    FinalizedObject::new(ObjectRole::from_code(role)?, body)?
                        .with_references(references)
                        .with_predecessors(hints),
                )
            }
            1 => {
                if body.len() < 20 || body[0] != 1 || !predecessors.is_empty() {
                    return Err(ContentError::InvalidRecord("draft private header"));
                }
                let level = body[1];
                let count = u16::from_be_bytes(body[2..4].try_into().unwrap()) as usize;
                let bytes = u64::from_be_bytes(body[4..12].try_into().unwrap());
                let extents = u64::from_be_bytes(body[12..20].try_into().unwrap());
                let width = if level == 0 { 40 } else { 48 };
                if count > MAX_ENTRIES
                    || body.len() != 20 + count * width
                    || role != if level == 0 { 3 } else { 4 }
                {
                    return Err(ContentError::InvalidRecord("draft private rows"));
                }
                let node = if level == 0 {
                    let mut rows = Vec::with_capacity(count);
                    for row in body[20..].chunks_exact(40) {
                        rows.push(ExtentSlice::new(
                            ObjectId::from_bytes(&row[..32])?,
                            u32::from_be_bytes(row[32..36].try_into().unwrap()),
                            u32::from_be_bytes(row[36..].try_into().unwrap()),
                        )?);
                    }
                    ExtentNode::Leaf {
                        subtree_logical_bytes: bytes,
                        extents: rows,
                    }
                } else {
                    let mut children = Vec::with_capacity(count);
                    for row in body[20..].chunks_exact(48) {
                        children.push(ChildDescriptor {
                            cumulative_logical_end: u64::from_be_bytes(
                                row[..8].try_into().unwrap(),
                            ),
                            cumulative_extent_end: u64::from_be_bytes(
                                row[8..16].try_into().unwrap(),
                            ),
                            child_object_id: ObjectId::from_bytes(&row[16..])?,
                        });
                    }
                    ExtentNode::Branch {
                        level,
                        subtree_logical_bytes: bytes,
                        subtree_extent_count: extents,
                        children,
                    }
                };
                if node.extent_count() != extents || node.references() != references {
                    return Err(ContentError::InvalidRecord(
                        "draft private reference totals",
                    ));
                }
                Self::Node(node)
            }
            _ => return Err(ContentError::InvalidRecord("draft private form")),
        };
        record.validate()?;
        Ok(record)
    }
}
