//! One typed extended-protocol query for each bounded read unit.
use super::{provider::PgMetadata, rows};
use crate::params::Param;
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{
    location::{LocatedObject, ObjectLocation, PackDomain, SignatureRow},
    policy::{StoragePolicy, DEPENDENCY_PACK_CACHE_BYTES, READ_OBJECT_LIMIT},
    port::*,
};
use std::collections::BTreeSet;
impl PgMetadata {
    pub(super) fn read_policy(&self) -> Result<StoragePolicy, MetadataError> {
        let data = self.client.query(
            include_str!("../../sql/queries/storage/policy.sql"),
            Vec::new(),
            false,
        )?;
        let row = data
            .first()
            .filter(|_| data.len() == 1)
            .ok_or(MetadataError::Missing)?;
        if rows::field::<i16>(row, "schema_version")? != 1 {
            return Err(MetadataError::Refused {
                status: "SCHEMA_IDENTITY".to_owned(),
            });
        }
        let byte = |name| {
            u8::try_from(rows::field::<i16>(row, name)?).map_err(|_| MetadataError::Malformed)
        };
        let threshold = u64::try_from(rows::field::<i64>(row, "small_file_threshold_bytes")?)
            .map_err(|_| MetadataError::Malformed)?;
        StoragePolicy::new(
            byte("format_profile")?,
            threshold,
            byte("whole_file_delta_max_depth")?,
            byte("chunk_delta_max_depth")?,
        )
        .with_metadata_depth(byte("metadata_delta_max_depth")?)
        .validated()
        .map_err(|_| MetadataError::Malformed)
    }
    pub(super) fn read_locations(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), MetadataError> {
        out.clear();
        if ids.len() > READ_OBJECT_LIMIT {
            return Err(MetadataError::Malformed);
        }
        let data = self.client.query(
            include_str!("../../sql/queries/storage/locate.sql"),
            vec![Param::ByteArrays(
                ids.iter().map(|id| id.as_bytes().to_vec()).collect(),
            )],
            false,
        )?;
        for row in data {
            let object_id = rows::object(&rows::field::<Vec<u8>>(&row, "object_id")?)?;
            let role = ObjectRole::from_code(
                u8::try_from(rows::field::<i16>(&row, "role")?)
                    .map_err(|_| MetadataError::Malformed)?,
            )
            .map_err(|_| MetadataError::Malformed)?;
            let canonical_length = rows::positive(rows::field(&row, "canonical_length")?)?;
            let pack = rows::info(&row)?;
            let group_number = rows::nonnegative(rows::field(&row, "group_number")?)?;
            let record_number = rows::nonnegative(rows::field(&row, "record_number")?)?;
            if !ids.contains(&object_id)
                || canonical_length > layerfs_storage::policy::CANONICAL_LIMIT
                || group_number >= layerfs_storage::policy::GROUP_COUNT_LIMIT
                || record_number >= layerfs_storage::policy::RECORD_COUNT_LIMIT
            {
                return Err(MetadataError::Malformed);
            }
            out.push(LocatedObject {
                location: ObjectLocation {
                    object_id,
                    role,
                    canonical_length,
                    pack_id: pack.pack_id,
                    group_number,
                    record_number,
                },
                pack,
            });
        }
        Ok(())
    }
    pub(super) fn read_bodies(
        &self,
        ids: &[i64],
        out: &mut Vec<MetadataPack>,
    ) -> Result<(), MetadataError> {
        out.clear();
        if ids.len() > READ_OBJECT_LIMIT || ids.iter().any(|id| *id <= 0) {
            return Err(MetadataError::Malformed);
        }
        let data = self.client.query(
            include_str!("../../sql/queries/storage/read_packs.sql"),
            vec![Param::I64s(ids.to_vec())],
            false,
        )?;
        for row in data {
            if rows::field::<i16>(&row, "status")? != 0 {
                return Err(MetadataError::Refused {
                    status: "PACK_READ_BOUND".to_owned(),
                });
            }
            let info = rows::info(&row)?;
            let body = rows::field::<Vec<u8>>(&row, "body")?;
            if info.domain != PackDomain::Metadata || body.len() != info.length {
                return Err(MetadataError::Malformed);
            }
            out.push(MetadataPack { info, body });
        }
        if out.len() != ids.iter().collect::<BTreeSet<_>>().len() {
            return Err(MetadataError::Missing);
        }
        if out.iter().map(|row| row.body.len()).sum::<usize>() > DEPENDENCY_PACK_CACHE_BYTES {
            return Err(MetadataError::Malformed);
        }
        Ok(())
    }
    pub(super) fn read_groups(
        &self,
        query: ValueGroupQuery<'_>,
    ) -> Result<ValueGroups, MetadataError> {
        let (sql, params, limit, page) = match query {
            ValueGroupQuery::Ordinals(ids) => {
                if ids.len() > READ_OBJECT_LIMIT || ids.contains(&0) {
                    return Err(MetadataError::Malformed);
                }
                (
                    include_str!("../../sql/queries/storage/value_groups_set.sql"),
                    vec![Param::I64s(ids.iter().map(|id| i64::from(*id)).collect())],
                    ids.len(),
                    false,
                )
            }
            ValueGroupQuery::Page { from, limit } => {
                if from == 0 || limit > READ_OBJECT_LIMIT {
                    return Err(MetadataError::Malformed);
                }
                (
                    include_str!("../../sql/queries/storage/value_groups_page.sql"),
                    vec![Param::I64(i64::from(from)), Param::I64(limit as i64)],
                    limit,
                    true,
                )
            }
        };
        let data = self.client.query(sql, params, false)?;
        let first = data.first().ok_or(MetadataError::Missing)?;
        let window_start = u32::try_from(rows::field::<i64>(first, "metadata_window_start")?)
            .ok()
            .filter(|n| *n > 0)
            .ok_or(MetadataError::Malformed)?;
        let mut found = Vec::new();
        let mut next = None;
        for row in data {
            if let Some(group) = rows::group(&row)? {
                if found.len() < limit {
                    found.push(group);
                } else if page && limit > 0 && next.is_none() {
                    next = Some(group.first_ordinal);
                } else if !page {
                    return Err(MetadataError::Malformed);
                }
            }
        }
        Ok(ValueGroups {
            rows: found,
            window_start,
            next,
        })
    }
    pub(super) fn read_signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), MetadataError> {
        out.clear();
        let data = self.client.query(
            include_str!("../../sql/queries/storage/signatures.sql"),
            Vec::new(),
            false,
        )?;
        if data.len() > 8192 {
            return Err(MetadataError::Malformed);
        }
        for row in data {
            let slot = rows::nonnegative(rows::field(&row, "slot")?)?;
            let stamp = u64::try_from(rows::field::<i64>(&row, "stamp")?)
                .ok()
                .filter(|n| *n > 0)
                .ok_or(MetadataError::Malformed)?;
            if slot >= 8192 || (stamp - 1) % 8192 != slot as u64 {
                return Err(MetadataError::Malformed);
            }
            out.push(SignatureRow {
                slot,
                stamp,
                object_id: rows::object(&rows::field::<Vec<u8>>(&row, "object_id")?)?,
                signature: rows::field::<Vec<u8>>(&row, "signature")?
                    .try_into()
                    .map_err(|_| MetadataError::Malformed)?,
            });
        }
        Ok(())
    }
}
