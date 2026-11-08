//! Complete physical group-directory and record-boundary verification.
use super::layout::{self, GroupCodec, PackLane};
use crate::{
    encoding::{framed_record, DecompressionWorkspace},
    StorageError, StorageResult,
};

/// Physical records in every group, including records without a winning locator.
/// Canonical identity/dependency verification remains the owning resolver's job.
/// `proceed` is polled before each bounded group acquisition/decompression.
pub fn validate_records(body: &[u8], proceed: &mut dyn FnMut() -> bool) -> StorageResult<u64> {
    let header = layout::parse_header(body)?;
    let mut workspace = DecompressionWorkspace::new()?;
    let mut records = 0u64;
    for number in 0..header.group_count {
        if !proceed() {
            return Err(StorageError::Aborted);
        }
        let view = layout::group_view(body, header, number)?;
        let selected = body
            .get(view.start..view.end)
            .ok_or(StorageError::Integrity("audit group extent"))?;
        let decoded;
        let bytes = match view.codec {
            GroupCodec::Raw => selected,
            GroupCodec::Zstandard => {
                if header.lane != PackLane::Ordinary {
                    return Err(StorageError::Integrity("audit group codec"));
                }
                decoded = workspace.decompress_group(selected, view.decoded_length)?;
                &decoded
            }
        };
        if bytes.len() != view.decoded_length {
            return Err(StorageError::Integrity("audit group decoded length"));
        }
        // This validates every end offset and the exact final record-area end,
        // not only the requested record's slice.
        framed_record(bytes, 0)?;
        let count = u32::from_le_bytes(
            bytes[..4]
                .try_into()
                .map_err(|_| StorageError::Integrity("audit record count"))?,
        );
        if header.lane == PackLane::Singleton && count != 1 {
            return Err(StorageError::Integrity("audit singleton record count"));
        }
        // All end offsets were checked once above; walk record slices without
        // revalidating the complete directory for every record (quadratic work).
        let offset = 4 + 4 * count as usize;
        let mut previous = 0;
        for index in 0..count as usize {
            if !proceed() {
                return Err(StorageError::Aborted);
            }
            let at = 4 + 4 * index;
            let end = u32::from_le_bytes(
                bytes[at..at + 4]
                    .try_into()
                    .map_err(|_| StorageError::Integrity("audit record end"))?,
            ) as usize;
            let record = bytes
                .get(offset + previous..offset + end)
                .ok_or(StorageError::Integrity("audit record slice"))?;
            previous = end;
            let tag = *record
                .first()
                .ok_or(StorageError::Integrity("audit record tag"))?;
            if header.lane == PackLane::Ordinary {
                match tag {
                    0 => {
                        if layerfs_content::object::decode_bytes_object(&record[1..]).is_err() {
                            crate::encoding::pool::leaf::parse(record)?;
                        }
                    }
                    1 => {
                        crate::encoding::pool::leaf::parse(record)?;
                    }
                    _ => return Err(StorageError::Integrity("audit ordinary record tag")),
                }
            } else {
                crate::encoding::delta::record::validate_framing(header.lane, record)?;
            }
        }
        records = records
            .checked_add(u64::from(count))
            .ok_or(StorageError::Integrity("audit record total"))?;
    }
    Ok(records)
}
