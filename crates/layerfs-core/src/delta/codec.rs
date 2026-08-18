//! Candidate authenticated transition bytes.  Semantic ordering remains in `Delta`.

use crate::content::persistence::{canonical_mapping, mapping_bytes, DELTA_INDEX_TAG};
use crate::identity::ObjectId;
use crate::{CoreError, CoreResult};

pub fn encode_genesis(child: ObjectId) -> CoreResult<Vec<u8>> {
    encode_transition(None, child, 0, &[])
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

fn encode_genesis_body(child: ObjectId) -> CoreResult<Vec<u8>> {
    let mut body = Vec::with_capacity(1 + 32 + 8);
    body.push(0);
    body.extend_from_slice(child.as_bytes());
    body.extend_from_slice(&0_u32.to_be_bytes());
    body.extend_from_slice(&0_u32.to_be_bytes());
    Ok(body)
}
