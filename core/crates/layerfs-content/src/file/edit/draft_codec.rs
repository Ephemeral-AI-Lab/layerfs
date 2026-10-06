//! Versioned bounded mapping-only temporary records, never canonical draft IDs.
use super::references::{Child, EditRef, Node};
use crate::file::mapping::{
    decode_node_with_context, ExtentSlice, MAX_ENTRIES, MAX_LEVEL, MAX_NODE_OBJECT_BYTES,
};
use crate::{
    AdvisoryPredecessors, ContentError, ContentResult, FinalizedObject, ObjectId, ObjectRole,
    PredecessorProvenance,
};

struct Read<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Read<'a> {
    fn take(&mut self, n: usize) -> ContentResult<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(ContentError::LengthOverflow)?;
        let value = self
            .bytes
            .get(self.at..end)
            .ok_or(ContentError::UnexpectedEof)?;
        self.at = end;
        Ok(value)
    }
    fn byte(&mut self) -> ContentResult<u8> {
        Ok(self.take(1)?[0])
    }
    fn short(&mut self) -> ContentResult<usize> {
        Ok(u16::from_be_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| ContentError::UnexpectedEof)?,
        ) as usize)
    }
    fn long(&mut self) -> ContentResult<u64> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| ContentError::UnexpectedEof)?,
        ))
    }
    fn word(&mut self) -> ContentResult<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| ContentError::UnexpectedEof)?,
        ))
    }
    fn id(&mut self) -> ContentResult<ObjectId> {
        ObjectId::from_bytes(self.take(32)?)
    }
    fn done(self) -> ContentResult<()> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(ContentError::TrailingBytes)
        }
    }
}
pub(super) fn encode(node: &Node) -> ContentResult<Vec<u8>> {
    let count = match node {
        Node::Leaf { extents, .. } => extents.len(),
        Node::Branch { children, .. } => children.len(),
    };
    if count > MAX_ENTRIES || node.level() > MAX_LEVEL {
        return Err(ContentError::InvalidRecord("temporary mapping size"));
    }
    let mut out = Vec::with_capacity(21 + count * if node.level() == 0 { 40 } else { 49 });
    out.extend_from_slice(&[1, u8::from(node.level() != 0), node.level()]);
    out.extend_from_slice(&node.logical_len().to_be_bytes());
    out.extend_from_slice(&node.extent_count().to_be_bytes());
    out.extend_from_slice(&(count as u16).to_be_bytes());
    match node {
        Node::Leaf { extents, .. } => {
            for extent in extents {
                out.extend_from_slice(&extent.payload_object_id().to_bytes());
                out.extend_from_slice(&extent.source_offset().to_be_bytes());
                out.extend_from_slice(&extent.logical_length().to_be_bytes());
            }
        }
        Node::Branch { children, .. } => {
            for child in children {
                out.extend_from_slice(&child.cumulative_logical_end.to_be_bytes());
                out.extend_from_slice(&child.cumulative_extent_end.to_be_bytes());
                out.push(match child.child_object_id {
                    EditRef::Stored(_) => 0,
                    EditRef::Node(_) => 1,
                    EditRef::Page(_) => 2,
                });
                out.extend_from_slice(&child.child_object_id.key());
            }
        }
    }
    // The raw codec is exact: a caller cannot smuggle inconsistent summaries.
    decode(&out)?;
    Ok(out)
}
pub(super) fn decode(bytes: &[u8]) -> ContentResult<Node> {
    let mut input = Read { bytes, at: 0 };
    if input.byte()? != 1 {
        return Err(ContentError::InvalidRecord("temporary mapping version"));
    }
    let tag = input.byte()?;
    let level = input.byte()?;
    let logical = input.long()?;
    let extents = input.long()?;
    let count = input.short()?;
    if count > MAX_ENTRIES || level > MAX_LEVEL {
        return Err(ContentError::InvalidRecord("temporary mapping size"));
    }
    let node = match tag {
        0 if level == 0 => {
            let mut entries = Vec::with_capacity(count);
            let mut total = 0u64;
            for _ in 0..count {
                let id = input.id()?;
                let offset = input.word()?;
                let len = input.word()?;
                let extent = ExtentSlice::new(id, offset, len)?;
                total = total
                    .checked_add(u64::from(len))
                    .ok_or(ContentError::LengthOverflow)?;
                entries.push(extent);
            }
            if total != logical || extents != count as u64 {
                return Err(ContentError::InvalidRecord("temporary leaf summary"));
            }
            Node::Leaf {
                subtree_logical_bytes: logical,
                extents: entries,
            }
        }
        1 if level != 0 && count >= 2 => {
            let mut children = Vec::with_capacity(count);
            let (mut prior_bytes, mut prior_extents) = (0, 0);
            for _ in 0..count {
                let end = input.long()?;
                let extent_end = input.long()?;
                let tag = input.byte()?;
                let key: [u8; 32] = input
                    .take(32)?
                    .try_into()
                    .map_err(|_| ContentError::UnexpectedEof)?;
                if end <= prior_bytes || extent_end <= prior_extents {
                    return Err(ContentError::NonCanonicalOrdering);
                }
                let child_object_id = match tag {
                    0 => EditRef::Stored(ObjectId::from_bytes(&key)?),
                    1 | 2 => EditRef::from_key(u32::from(tag), key)?,
                    _ => return Err(ContentError::InvalidRecord("temporary child tag")),
                };
                children.push(Child {
                    cumulative_logical_end: end,
                    cumulative_extent_end: extent_end,
                    child_object_id,
                });
                prior_bytes = end;
                prior_extents = extent_end;
            }
            if prior_bytes != logical || prior_extents != extents {
                return Err(ContentError::InvalidRecord("temporary branch summary"));
            }
            Node::Branch {
                level,
                subtree_logical_bytes: logical,
                subtree_extent_count: extents,
                children,
            }
        }
        _ => return Err(ContentError::InvalidRecord("temporary mapping tag")),
    };
    input.done()?;
    Ok(node)
}
pub(super) fn page(object: FinalizedObject) -> ContentResult<Vec<u8>> {
    if !matches!(
        object.role(),
        ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch
    ) || object.canonical_len() > MAX_NODE_OBJECT_BYTES
    {
        return Err(ContentError::WrongLogicalRole);
    }
    let node = decode_node_with_context(object.canonical(), true)?;
    if node.references() != object.references()
        || (node.level() == 0) != (object.role() == ObjectRole::ExtentLeaf)
    {
        return Err(ContentError::InvalidRecord("temporary page references"));
    }
    let parts = object.into_parts();
    let mut out = Vec::with_capacity(
        8 + parts.canonical.len() + parts.references.len() * 32 + parts.predecessors.len() * 33,
    );
    out.extend_from_slice(&[1, 2, parts.role.code()]);
    out.extend_from_slice(&(parts.canonical.len() as u16).to_be_bytes());
    out.extend_from_slice(&(parts.references.len() as u16).to_be_bytes());
    out.push(parts.predecessors.len() as u8);
    out.extend_from_slice(&parts.canonical);
    for id in parts.references {
        out.extend_from_slice(&id.to_bytes());
    }
    for predecessor in parts.predecessors.entries() {
        out.extend_from_slice(&predecessor.id().to_bytes());
        out.push(match predecessor.provenance() {
            PredecessorProvenance::OriginalBase => 0,
            PredecessorProvenance::UnchangedPrefix => 1,
            PredecessorProvenance::ReusedRange => 2,
        });
    }
    Ok(out)
}
pub(super) fn decode_page(bytes: &[u8], id: ObjectId) -> ContentResult<FinalizedObject> {
    let mut input = Read { bytes, at: 0 };
    if input.byte()? != 1 || input.byte()? != 2 {
        return Err(ContentError::InvalidRecord("temporary page version"));
    }
    let role = ObjectRole::from_code(input.byte()?)?;
    if !matches!(role, ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch) {
        return Err(ContentError::WrongLogicalRole);
    }
    let canonical = input.short()?;
    let refs = input.short()?;
    let hints = input.byte()? as usize;
    if canonical > MAX_NODE_OBJECT_BYTES || refs > MAX_ENTRIES || hints > 4 {
        return Err(ContentError::InvalidRecord("temporary page size"));
    }
    let object = FinalizedObject::new(role, input.take(canonical)?.to_vec())?;
    if object.id() != id {
        return Err(ContentError::IdentityMismatch);
    }
    let node = decode_node_with_context(object.canonical(), true)?;
    if (node.level() == 0) != (role == ObjectRole::ExtentLeaf) {
        return Err(ContentError::WrongLogicalRole);
    }
    let mut references = Vec::with_capacity(refs);
    for _ in 0..refs {
        references.push(input.id()?);
    }
    if references != node.references() {
        return Err(ContentError::InvalidRecord("temporary page references"));
    }
    let mut predecessors = AdvisoryPredecessors::new();
    for _ in 0..hints {
        let id = input.id()?;
        let provenance = match input.byte()? {
            0 => PredecessorProvenance::OriginalBase,
            1 => PredecessorProvenance::UnchangedPrefix,
            2 => PredecessorProvenance::ReusedRange,
            _ => return Err(ContentError::InvalidRecord("temporary predecessor")),
        };
        predecessors.push(id, provenance)?;
    }
    if predecessors.len() != hints {
        return Err(ContentError::InvalidRecord(
            "temporary predecessor duplicate",
        ));
    }
    input.done()?;
    Ok(object
        .with_references(references)
        .with_predecessors(predecessors))
}
