//! One acknowledged allocation or atomic first-wins registration unit.
use super::{provider::PgMetadata, rows};
use crate::params::Param;
use layerfs_storage::{location::PackDomain, policy, port::*};
use std::collections::BTreeSet;
impl PgMetadata {
    pub(super) fn allocate(&self, request: Reserve) -> Result<Reserved, MetadataError> {
        if request.packs > policy::TRANSACTION_ROW_LIMIT as usize
            || request.ordinals > policy::METADATA_INDEX_VALUES
        {
            return Err(MetadataError::Malformed);
        }
        let data = self.client.query(
            include_str!("../../sql/queries/storage/reserve.sql"),
            vec![
                Param::I64(request.packs as i64),
                Param::I64(request.ordinals as i64),
            ],
            true,
        )?;
        let row = data
            .first()
            .filter(|_| data.len() == 1)
            .ok_or(MetadataError::Malformed)?;
        let first_pack_id = rows::field::<i64>(row, "first_pack_id")?;
        let first_ordinal = u32::try_from(rows::field::<i64>(row, "first_ordinal")?)
            .map_err(|_| MetadataError::Malformed)?;
        if request.packs > 0 && first_pack_id <= 0 || request.ordinals > 0 && first_ordinal == 0 {
            return Err(MetadataError::Malformed);
        }
        Ok(Reserved {
            first_pack_id,
            first_ordinal,
        })
    }
    pub(super) fn write_registration(
        &self,
        batch: &Registration,
    ) -> Result<Registered, MetadataError> {
        validate(batch)?;
        let params = vec![
            Param::I64s(batch.packs.iter().map(|row| row.info.pack_id).collect()),
            Param::I16s(
                batch
                    .packs
                    .iter()
                    .map(|row| match row.info.domain {
                        PackDomain::Metadata => 0,
                        PackDomain::Payload => 1,
                    })
                    .collect(),
            ),
            Param::ByteArrays(
                batch
                    .packs
                    .iter()
                    .map(|row| row.info.key.as_bytes().to_vec())
                    .collect(),
            ),
            Param::I64s(
                batch
                    .packs
                    .iter()
                    .map(|row| row.info.length as i64)
                    .collect(),
            ),
            Param::NullableByteArrays(batch.packs.iter().map(|row| row.body.clone()).collect()),
            Param::ByteArrays(
                batch
                    .objects
                    .iter()
                    .map(|row| row.object_id.as_bytes().to_vec())
                    .collect(),
            ),
            Param::I16s(
                batch
                    .objects
                    .iter()
                    .map(|row| i16::from(row.role.code()))
                    .collect(),
            ),
            Param::I64s(
                batch
                    .objects
                    .iter()
                    .map(|row| row.canonical_length as i64)
                    .collect(),
            ),
            Param::I64s(batch.objects.iter().map(|row| row.pack_id).collect()),
            Param::I64s(
                batch
                    .objects
                    .iter()
                    .map(|row| row.group_number as i64)
                    .collect(),
            ),
            Param::I64s(
                batch
                    .objects
                    .iter()
                    .map(|row| row.record_number as i64)
                    .collect(),
            ),
            Param::I64s(
                batch
                    .value_groups
                    .iter()
                    .map(|row| i64::from(row.first_ordinal))
                    .collect(),
            ),
            Param::I64s(
                batch
                    .value_groups
                    .iter()
                    .map(|row| row.count as i64)
                    .collect(),
            ),
            Param::I64s(batch.value_groups.iter().map(|row| row.pack_id).collect()),
            Param::I64s(
                batch
                    .value_groups
                    .iter()
                    .map(|row| row.group_number as i64)
                    .collect(),
            ),
            Param::ByteArrays(
                batch
                    .value_groups
                    .iter()
                    .map(|row| row.digest.as_bytes().to_vec())
                    .collect(),
            ),
            Param::I64s(batch.signatures.iter().map(|row| row.slot as i64).collect()),
            Param::I64s(
                batch
                    .signatures
                    .iter()
                    .map(|row| row.stamp as i64)
                    .collect(),
            ),
            Param::ByteArrays(
                batch
                    .signatures
                    .iter()
                    .map(|row| row.object_id.as_bytes().to_vec())
                    .collect(),
            ),
            Param::ByteArrays(
                batch
                    .signatures
                    .iter()
                    .map(|row| row.signature.to_vec())
                    .collect(),
            ),
            Param::OptionalI64(batch.window_start.map(i64::from)),
            Param::OptionalI64(batch.release_ordinals.map(|(first, _)| i64::from(first))),
            Param::OptionalI64(batch.release_ordinals.map(|(_, count)| count as i64)),
        ];
        let data = self.client.query(
            include_str!("../../sql/queries/storage/register.sql"),
            params,
            true,
        )?;
        let row = data
            .first()
            .filter(|_| data.len() == 1)
            .ok_or(MetadataError::Malformed)?;
        let lost = rows::field::<Vec<Vec<u8>>>(row, "lost")?
            .iter()
            .map(|bytes| rows::object(bytes))
            .collect::<Result<Vec<_>, _>>()?;
        if lost.iter().collect::<BTreeSet<_>>().len() != lost.len()
            || lost
                .iter()
                .any(|id| !batch.objects.iter().any(|row| row.object_id == *id))
        {
            return Err(MetadataError::Malformed);
        }
        Ok(Registered { lost })
    }
}
fn validate(batch: &Registration) -> Result<(), MetadataError> {
    let count = batch.packs.len()
        + batch.objects.len()
        + batch.value_groups.len()
        + batch.signatures.len()
        + usize::from(batch.window_start.is_some())
        + usize::from(batch.release_ordinals.is_some());
    if count > policy::TRANSACTION_ROW_LIMIT as usize {
        return Err(MetadataError::Malformed);
    }
    let bytes = batch
        .packs
        .iter()
        .filter_map(|row| row.body.as_ref())
        .map(Vec::len)
        .chain(batch.objects.iter().map(|row| row.canonical_length))
        .try_fold(0usize, |total, n| total.checked_add(n))
        .ok_or(MetadataError::Malformed)?;
    let singleton = batch.objects.len() == 1
        && batch.packs.len() == 1
        && batch.packs[0].info.domain == PackDomain::Payload;
    if bytes > policy::TRANSACTION_CANONICAL_BYTES_LIMIT as usize && !singleton {
        return Err(MetadataError::Malformed);
    }
    let mut packs = BTreeSet::new();
    for row in &batch.packs {
        let info = row.info;
        if info.pack_id <= 0
            || !packs.insert(info.pack_id)
            || !(32..=policy::SINGLETON_PACK_LIMIT).contains(&info.length)
            || match (info.domain, &row.body) {
                (PackDomain::Metadata, Some(body)) => {
                    body.len() != info.length || info.length > policy::PACK_LIMIT
                }
                (PackDomain::Payload, None) => false,
                _ => true,
            }
        {
            return Err(MetadataError::Malformed);
        }
    }
    let mut objects = BTreeSet::new();
    for row in &batch.objects {
        if !objects.insert(row.object_id)
            || row.pack_id <= 0
            || row.canonical_length == 0
            || row.canonical_length > policy::CANONICAL_LIMIT
            || row.group_number >= policy::GROUP_COUNT_LIMIT
            || row.record_number >= policy::RECORD_COUNT_LIMIT
        {
            return Err(MetadataError::Malformed);
        }
    }
    let mut groups = BTreeSet::new();
    for row in &batch.value_groups {
        if !groups.insert(row.first_ordinal)
            || row.first_ordinal == 0
            || row.count == 0
            || row.count > policy::VALUES_PER_GROUP
            || row.pack_id <= 0
            || row.group_number >= policy::GROUP_COUNT_LIMIT
            || u64::from(row.first_ordinal) + row.count as u64 > u64::from(u32::MAX) + 1
        {
            return Err(MetadataError::Malformed);
        }
    }
    let mut slots = BTreeSet::new();
    for row in &batch.signatures {
        if !slots.insert(row.slot)
            || row.slot >= 8192
            || row.stamp == 0
            || row.stamp > i64::MAX as u64
            || (row.stamp - 1) % 8192 != row.slot as u64
        {
            return Err(MetadataError::Malformed);
        }
    }
    if batch.window_start == Some(0)
        || batch.release_ordinals.is_some_and(|(first, count)| {
            first == 0
                || count > policy::METADATA_INDEX_VALUES
                || u64::from(first) + count as u64 > u64::from(u32::MAX) + 1
        })
    {
        return Err(MetadataError::Malformed);
    }
    Ok(())
}
