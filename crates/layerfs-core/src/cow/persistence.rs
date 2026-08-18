//! Candidate directory page/index/wrapper bytes.

use crate::content::persistence::{mapping_bytes, DIR_INDEX_TAG, DIR_METADATA_TAG};
use crate::format::CanonicalName;
use crate::identity::ObjectId;
use crate::object::{encode_object, DirectoryEntry, Object, ObjectKind, ObjectReference};
use crate::{CoreError, CoreResult};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryPageRef {
    pub count: u32,
    pub first_name: Vec<u8>,
    pub object_id: ObjectId,
}

pub fn encode_directory_metadata(mode: u32) -> CoreResult<Vec<u8>> {
    mapping_bytes(DIR_METADATA_TAG, &mode.to_be_bytes())
}

pub fn encode_directory_index(
    total_entries: u32,
    pages: &[DirectoryPageRef],
) -> CoreResult<Vec<u8>> {
    let page_count = u32::try_from(pages.len()).map_err(|_| CoreError::LengthOverflow)?;
    let mut body = Vec::new();
    body.extend_from_slice(&total_entries.to_be_bytes());
    body.extend_from_slice(&page_count.to_be_bytes());
    for page in pages {
        let name_len =
            u16::try_from(page.first_name.len()).map_err(|_| CoreError::LengthOverflow)?;
        body.extend_from_slice(&page.count.to_be_bytes());
        body.extend_from_slice(&name_len.to_be_bytes());
        body.extend_from_slice(&page.first_name);
        body.extend_from_slice(page.object_id.as_bytes());
    }
    mapping_bytes(DIR_INDEX_TAG, &body)
}

pub fn encode_directory_page(entries: &[DirectoryEntry]) -> CoreResult<Vec<u8>> {
    encode_object(&Object::directory(entries.to_vec())?)
}

pub fn encode_directory_wrapper(metadata: ObjectId, index: ObjectId) -> CoreResult<Vec<u8>> {
    let m = CanonicalName::from_bytes(b"m")?;
    let t = CanonicalName::from_bytes(b"t")?;
    encode_object(&Object::Directory(vec![
        DirectoryEntry::new(m, ObjectReference::new(ObjectKind::Bytes, metadata)),
        DirectoryEntry::new(t, ObjectReference::new(ObjectKind::Bytes, index)),
    ]))
}

pub fn parse_directory_index(payload: &[u8]) -> CoreResult<Vec<DirectoryPageRef>> {
    if payload.len() < 8 {
        return Err(CoreError::UnexpectedEof);
    }
    let page_count = usize::try_from(u32::from_be_bytes(
        payload[4..8]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    ))
    .map_err(|_| CoreError::LengthOverflow)?;
    let mut offset = 8usize;
    let mut pages = Vec::with_capacity(page_count.min(1024));
    let mut total = 0_u64;
    for _ in 0..page_count {
        let fixed_end = offset.checked_add(6).ok_or(CoreError::LengthOverflow)?;
        if fixed_end > payload.len() {
            return Err(CoreError::UnexpectedEof);
        }
        let count = u32::from_be_bytes(
            payload[offset..offset + 4]
                .try_into()
                .map_err(|_| CoreError::UnexpectedEof)?,
        );
        let name_len = usize::from(u16::from_be_bytes(
            payload[offset + 4..offset + 6]
                .try_into()
                .map_err(|_| CoreError::UnexpectedEof)?,
        ));
        offset = fixed_end;
        let end = offset
            .checked_add(name_len)
            .and_then(|value| value.checked_add(32))
            .ok_or(CoreError::LengthOverflow)?;
        if end > payload.len() {
            return Err(CoreError::UnexpectedEof);
        }
        pages.push(DirectoryPageRef {
            count,
            first_name: payload[offset..offset + name_len].to_vec(),
            object_id: ObjectId::from_bytes(&payload[offset + name_len..end])?,
        });
        CanonicalName::from_bytes(&pages.last().ok_or(CoreError::UnexpectedEof)?.first_name)?;
        if count == 0 {
            return Err(CoreError::NonCanonicalOrdering);
        }
        total = total
            .checked_add(u64::from(count))
            .ok_or(CoreError::LengthOverflow)?;
        offset = end;
    }
    if offset != payload.len() {
        return Err(CoreError::TrailingBytes);
    }
    if total
        != u64::from(u32::from_be_bytes(
            payload[..4]
                .try_into()
                .map_err(|_| CoreError::UnexpectedEof)?,
        ))
    {
        return Err(CoreError::LengthMismatch {
            expected: u64::from(u32::from_be_bytes(
                payload[..4]
                    .try_into()
                    .map_err(|_| CoreError::UnexpectedEof)?,
            )),
            actual: total,
        });
    }
    if pages
        .windows(2)
        .any(|window| window[0].first_name >= window[1].first_name)
    {
        return Err(CoreError::NonCanonicalOrdering);
    }
    Ok(pages)
}
