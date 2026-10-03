//! Checked typed response decoding; no textual database-error parsing.
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{PackDomain, PackInfo, ValueGroupRow},
    port::{MetadataError, ObjectKey},
};
use postgres::{types::FromSql, Row};
pub(super) fn field<'a, T: FromSql<'a>>(row: &'a Row, name: &str) -> Result<T, MetadataError> {
    row.try_get(name).map_err(|_| MetadataError::Malformed)
}
pub(super) fn object(bytes: &[u8]) -> Result<ObjectId, MetadataError> {
    ObjectId::from_bytes(bytes).map_err(|_| MetadataError::Malformed)
}
pub(super) fn positive(value: i64) -> Result<usize, MetadataError> {
    usize::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(MetadataError::Malformed)
}
pub(super) fn nonnegative(value: i64) -> Result<usize, MetadataError> {
    usize::try_from(value).map_err(|_| MetadataError::Malformed)
}
pub(super) fn info(row: &Row) -> Result<PackInfo, MetadataError> {
    let pack_id = field::<i64>(row, "pack_id")?;
    if pack_id <= 0 {
        return Err(MetadataError::Malformed);
    }
    let domain = match field::<i16>(row, "domain")? {
        0 => PackDomain::Metadata,
        1 => PackDomain::Payload,
        _ => return Err(MetadataError::Malformed),
    };
    let digest = field::<Vec<u8>>(row, "digest")?;
    let length = positive(field(row, "length")?)?;
    if !(32..=layerfs_storage::policy::SINGLETON_PACK_LIMIT).contains(&length) {
        return Err(MetadataError::Malformed);
    }
    Ok(PackInfo {
        pack_id,
        domain,
        key: ObjectKey::from_bytes(digest.try_into().map_err(|_| MetadataError::Malformed)?),
        length,
    })
}
pub(super) fn group(row: &Row) -> Result<Option<ValueGroupRow>, MetadataError> {
    let Some(first) = field::<Option<i64>>(row, "first_ordinal")? else {
        return Ok(None);
    };
    let first_ordinal = u32::try_from(first)
        .ok()
        .filter(|first| *first > 0)
        .ok_or(MetadataError::Malformed)?;
    let count = positive(field(row, "count")?)?;
    let pack_id = field(row, "pack_id")?;
    let group_number = nonnegative(field(row, "group_number")?)?;
    if count > layerfs_storage::policy::VALUES_PER_GROUP
        || pack_id <= 0
        || group_number >= layerfs_storage::policy::GROUP_COUNT_LIMIT
        || u64::from(first_ordinal) + count as u64 > u64::from(u32::MAX) + 1
    {
        return Err(MetadataError::Malformed);
    }
    Ok(Some(ValueGroupRow {
        first_ordinal,
        count,
        pack_id,
        group_number,
        digest: object(&field::<Vec<u8>>(row, "digest")?)?,
    }))
}
