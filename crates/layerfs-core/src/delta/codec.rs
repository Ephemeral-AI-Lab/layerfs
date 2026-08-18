//! Candidate authenticated transition bytes.  Semantic ordering remains in `Delta`.

use crate::content::persistence::{
    canonical_mapping, decode_mapping, mapping_bytes, DELTA_INDEX_TAG, DELTA_PAGE_TAG,
};
use crate::cow::{RootHandle, TreeNode};
use crate::format::CanonicalPath;
use crate::identity::ObjectId;
use crate::limits::MAX_CHILD_REFERENCES;
use crate::{CoreError, CoreResult};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransitionOperation {
    Add {
        path: Vec<u8>,
        after: ObjectId,
    },
    Remove {
        path: Vec<u8>,
        before: ObjectId,
    },
    Replace {
        path: Vec<u8>,
        before: ObjectId,
        after: ObjectId,
    },
    Metadata {
        path: Vec<u8>,
        before: ObjectId,
        before_mode: u32,
        after: ObjectId,
        after_mode: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedTransition {
    pub parent: Option<ObjectId>,
    pub child: ObjectId,
    pub entry_count: u32,
    pub pages: Vec<ObjectId>,
}

pub fn encode_genesis(child: ObjectId) -> CoreResult<Vec<u8>> {
    encode_transition(None, child, 0, &[])
}

pub fn encode_genesis_with_pages(
    child: ObjectId,
    entry_count: u32,
    pages: &[ObjectId],
) -> CoreResult<Vec<u8>> {
    if entry_count != 0 || !pages.is_empty() {
        return Err(CoreError::DeltaConflict);
    }
    encode_transition(None, child, entry_count, pages)
}

pub fn encode_genesis_with_operations(
    child: ObjectId,
    entry_count: u32,
    pages: &[ObjectId],
    operations: &[TransitionOperation],
) -> CoreResult<Vec<u8>> {
    if !operations.is_empty() || entry_count != 0 || !pages.is_empty() {
        return Err(CoreError::DeltaConflict);
    }
    encode_transition(None, child, entry_count, pages)
}

pub fn encode_change(
    parent: ObjectId,
    child: ObjectId,
    entry_count: u32,
    pages: &[ObjectId],
) -> CoreResult<Vec<u8>> {
    encode_transition(Some(parent), child, entry_count, pages)
}

fn encode_transition(
    parent: Option<ObjectId>,
    child: ObjectId,
    entry_count: u32,
    pages: &[ObjectId],
) -> CoreResult<Vec<u8>> {
    let page_count = u32::try_from(pages.len()).map_err(|_| CoreError::LengthOverflow)?;
    let mut body = Vec::new();
    body.push(u8::from(parent.is_some()));
    if let Some(parent) = parent {
        body.extend_from_slice(parent.as_bytes());
    }
    body.extend_from_slice(child.as_bytes());
    body.extend_from_slice(&entry_count.to_be_bytes());
    body.extend_from_slice(&page_count.to_be_bytes());
    for page in pages {
        body.extend_from_slice(page.as_bytes());
    }
    mapping_bytes(DELTA_INDEX_TAG, &body)
}

pub fn canonical_genesis(child: ObjectId) -> CoreResult<(ObjectId, Vec<u8>)> {
    canonical_mapping(DELTA_INDEX_TAG, &encode_genesis_body(child)?)
}

pub fn decode_transition(payload: &[u8]) -> CoreResult<DecodedTransition> {
    if payload.len() < 1 + 32 + 4 + 4 {
        return Err(CoreError::UnexpectedEof);
    }
    let has_parent = payload[0];
    if has_parent > 1 {
        return Err(CoreError::InvalidMappingTag { tag: has_parent });
    }
    let mut offset = 1_usize;
    let parent = if has_parent == 1 {
        let end = offset.checked_add(32).ok_or(CoreError::LengthOverflow)?;
        let parent = ObjectId::from_bytes(&payload[offset..end])?;
        offset = end;
        Some(parent)
    } else {
        None
    };
    let child_end = offset.checked_add(32).ok_or(CoreError::LengthOverflow)?;
    if child_end > payload.len() {
        return Err(CoreError::UnexpectedEof);
    }
    let child = ObjectId::from_bytes(&payload[offset..child_end])?;
    offset = child_end;
    let fields_end = offset.checked_add(8).ok_or(CoreError::LengthOverflow)?;
    if fields_end > payload.len() {
        return Err(CoreError::UnexpectedEof);
    }
    let entry_count = u32::from_be_bytes(
        payload[offset..offset + 4]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    );
    let page_count = usize::try_from(u32::from_be_bytes(
        payload[offset + 4..fields_end]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    ))
    .map_err(|_| CoreError::LengthOverflow)?;
    if (entry_count == 0) != (page_count == 0) {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    offset = fields_end;
    let bytes = page_count
        .checked_mul(32)
        .ok_or(CoreError::LengthOverflow)?;
    let end = offset.checked_add(bytes).ok_or(CoreError::LengthOverflow)?;
    if end > payload.len() {
        return Err(CoreError::UnexpectedEof);
    }
    let pages = payload[offset..offset + bytes]
        .chunks_exact(32)
        .map(ObjectId::from_bytes)
        .collect::<CoreResult<Vec<_>>>()?;
    offset = end;
    if offset != payload.len() {
        return Err(CoreError::TrailingBytes);
    }
    Ok(DecodedTransition {
        parent,
        child,
        entry_count,
        pages,
    })
}

pub fn decode_mapping_transition(bytes: &[u8]) -> CoreResult<DecodedTransition> {
    let payload = crate::content::persistence::decode_mapping(
        bytes,
        crate::content::persistence::DELTA_INDEX_TAG,
    )?;
    decode_transition(&payload)
}

fn encode_genesis_body(child: ObjectId) -> CoreResult<Vec<u8>> {
    let mut body = Vec::with_capacity(1 + 32 + 8);
    body.push(0);
    body.extend_from_slice(child.as_bytes());
    body.extend_from_slice(&0_u32.to_be_bytes());
    body.extend_from_slice(&0_u32.to_be_bytes());
    Ok(body)
}

pub fn encode_delta_page(entries: &[TransitionOperation]) -> CoreResult<Vec<u8>> {
    let count = u32::try_from(entries.len()).map_err(|_| CoreError::LengthOverflow)?;
    let mut body = Vec::new();
    body.extend_from_slice(&count.to_be_bytes());
    for entry in entries {
        encode_entry(entry, &mut body)?;
    }
    mapping_bytes(DELTA_PAGE_TAG, &body)
}

pub fn decode_mapping_delta_page(bytes: &[u8]) -> CoreResult<Vec<TransitionOperation>> {
    let payload = decode_mapping(bytes, DELTA_PAGE_TAG)?;
    decode_delta_page(&payload)
}

pub fn decode_delta_page(payload: &[u8]) -> CoreResult<Vec<TransitionOperation>> {
    if payload.len() < 4 {
        return Err(CoreError::UnexpectedEof);
    }
    let count = usize::try_from(u32::from_be_bytes(
        payload[..4]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    ))
    .map_err(|_| CoreError::LengthOverflow)?;
    if count == 0 {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    if count > MAX_CHILD_REFERENCES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    if count > payload.len().saturating_sub(4) {
        return Err(CoreError::UnexpectedEof);
    }
    let mut offset = 4_usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = *payload.get(offset).ok_or(CoreError::UnexpectedEof)?;
        offset = offset.checked_add(1).ok_or(CoreError::LengthOverflow)?;
        let path_len_end = offset.checked_add(4).ok_or(CoreError::LengthOverflow)?;
        if path_len_end > payload.len() {
            return Err(CoreError::UnexpectedEof);
        }
        let path_len = usize::try_from(u32::from_be_bytes(
            payload[offset..path_len_end]
                .try_into()
                .map_err(|_| CoreError::UnexpectedEof)?,
        ))
        .map_err(|_| CoreError::LengthOverflow)?;
        offset = path_len_end;
        let path_end = offset
            .checked_add(path_len)
            .ok_or(CoreError::LengthOverflow)?;
        if path_end > payload.len() {
            return Err(CoreError::UnexpectedEof);
        }
        let path = payload[offset..path_end].to_vec();
        CanonicalPath::from_bytes(&path)?;
        offset = path_end;
        let entry = match kind {
            0x01 => TransitionOperation::Add {
                path,
                after: read_id(payload, &mut offset)?,
            },
            0x02 => TransitionOperation::Remove {
                path,
                before: read_id(payload, &mut offset)?,
            },
            0x03 => TransitionOperation::Replace {
                path,
                before: read_id(payload, &mut offset)?,
                after: read_id(payload, &mut offset)?,
            },
            0x04 => TransitionOperation::Metadata {
                path,
                before: read_id(payload, &mut offset)?,
                before_mode: read_u32(payload, &mut offset)?,
                after: read_id(payload, &mut offset)?,
                after_mode: read_u32(payload, &mut offset)?,
            },
            _ => return Err(CoreError::InvalidMappingTag { tag: kind }),
        };
        entries.push(entry);
    }
    if offset != payload.len() {
        return Err(CoreError::TrailingBytes);
    }
    Ok(entries)
}

fn encode_entry(entry: &TransitionOperation, output: &mut Vec<u8>) -> CoreResult<()> {
    match entry {
        TransitionOperation::Add { path, after } => {
            encode_path(output, 0x01, path)?;
            output.extend_from_slice(after.as_bytes());
        }
        TransitionOperation::Remove { path, before } => {
            encode_path(output, 0x02, path)?;
            output.extend_from_slice(before.as_bytes());
        }
        TransitionOperation::Replace {
            path,
            before,
            after,
        } => {
            encode_path(output, 0x03, path)?;
            output.extend_from_slice(before.as_bytes());
            output.extend_from_slice(after.as_bytes());
        }
        TransitionOperation::Metadata {
            path,
            before,
            before_mode,
            after,
            after_mode,
        } => {
            encode_path(output, 0x04, path)?;
            output.extend_from_slice(before.as_bytes());
            output.extend_from_slice(&before_mode.to_be_bytes());
            output.extend_from_slice(after.as_bytes());
            output.extend_from_slice(&after_mode.to_be_bytes());
        }
    }
    Ok(())
}

fn encode_path(output: &mut Vec<u8>, kind: u8, path: &[u8]) -> CoreResult<()> {
    CanonicalPath::from_bytes(path)?;
    output.push(kind);
    output.extend_from_slice(
        &u32::try_from(path.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    output.extend_from_slice(path);
    Ok(())
}

fn read_id(bytes: &[u8], offset: &mut usize) -> CoreResult<ObjectId> {
    let end = offset.checked_add(32).ok_or(CoreError::LengthOverflow)?;
    if end > bytes.len() {
        return Err(CoreError::UnexpectedEof);
    }
    let id = ObjectId::from_bytes(&bytes[*offset..end])?;
    *offset = end;
    Ok(id)
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> CoreResult<u32> {
    let end = offset.checked_add(4).ok_or(CoreError::LengthOverflow)?;
    if end > bytes.len() {
        return Err(CoreError::UnexpectedEof);
    }
    let value = u32::from_be_bytes(
        bytes[*offset..end]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    );
    *offset = end;
    Ok(value)
}

pub fn replay_durable_transition<F, G>(
    transition: &DecodedTransition,
    entries: &[TransitionOperation],
    parent: &RootHandle,
    parent_durable: ObjectId,
    mut load_node: F,
    mut durable_id: G,
) -> CoreResult<crate::delta::Delta>
where
    F: FnMut(ObjectId) -> CoreResult<TreeNode>,
    G: FnMut(&TreeNode) -> CoreResult<ObjectId>,
{
    if transition.parent != Some(parent_durable)
        || transition.entry_count
            != u32::try_from(entries.len()).map_err(|_| CoreError::LengthOverflow)?
    {
        return Err(CoreError::DeltaParentMismatch {
            expected: parent_durable,
            actual: transition.parent.unwrap_or(parent_durable),
        });
    }
    if durable_id(parent.node())? != parent_durable {
        return Err(CoreError::DeltaParentMismatch {
            expected: parent_durable,
            actual: durable_id(parent.node())?,
        });
    }
    let mut current = parent.clone();
    let mut decoded_entries = Vec::with_capacity(entries.len());
    for operation in entries {
        let entry = match operation {
            TransitionOperation::Add { path, after } => {
                let path = CanonicalPath::from_bytes(path)?;
                let node = load_node(*after)?;
                if durable_id(&node)? != *after {
                    return Err(CoreError::IdentityMismatch);
                }
                super::DeltaEntry::Add { path, node }
            }
            TransitionOperation::Remove { path, before } => {
                let path = CanonicalPath::from_bytes(path)?;
                let node = current.lookup_required(&path)?;
                if durable_id(node)? != *before {
                    return Err(CoreError::DeltaConflict);
                }
                super::DeltaEntry::Remove {
                    path: path.clone(),
                    before: node.identity(),
                }
            }
            TransitionOperation::Replace {
                path,
                before,
                after,
            } => {
                let path = CanonicalPath::from_bytes(path)?;
                let current_node = current.lookup_required(&path)?;
                if durable_id(current_node)? != *before {
                    return Err(CoreError::DeltaConflict);
                }
                let node = load_node(*after)?;
                if durable_id(&node)? != *after {
                    return Err(CoreError::IdentityMismatch);
                }
                super::DeltaEntry::Replace {
                    path: path.clone(),
                    before: current_node.identity(),
                    node,
                }
            }
            TransitionOperation::Metadata {
                path,
                before,
                before_mode,
                after,
                after_mode,
            } => {
                let path = CanonicalPath::from_bytes(path)?;
                let current_node = current.lookup_required(&path)?;
                if durable_id(current_node)? != *before
                    || current_node.metadata().mode() != *before_mode
                {
                    return Err(CoreError::DeltaConflict);
                }
                let updated = current_node.with_metadata(crate::cow::Metadata::new(*after_mode));
                if durable_id(&updated)? != *after {
                    return Err(CoreError::DeltaConflict);
                }
                super::DeltaEntry::Metadata {
                    path: path.clone(),
                    before: current_node.identity(),
                    before_metadata: current_node.metadata(),
                    after: updated.identity(),
                    after_metadata: updated.metadata(),
                }
            }
        };
        current = crate::cow::apply_delta_entry(&current, &entry)?;
        decoded_entries.push(entry);
    }
    if durable_id(current.node())? != transition.child {
        return Err(CoreError::DeltaChildMismatch {
            expected: transition.child,
            actual: durable_id(current.node())?,
        });
    }
    crate::delta::Delta::new(parent.id(), current.id(), decoded_entries)
}
