//! Exact comparison orders duplicate admission without lending foreign ownership.
use super::owner::MutationOwner;
use crate::{
    error::{StorageError, StorageResult},
    sqlite::{
        lookup::{self, ObjectLocation},
        ownership,
        write::ObjectRow,
    },
};
use layerfs_content::ObjectRole;
use std::collections::BTreeMap;

impl MutationOwner {
    pub(super) fn validate_candidates(&mut self, rows: &[ObjectRow]) -> StorageResult<()> {
        let saved: (i64, i64) = self.connection.query_row(
            "SELECT save_id,publication FROM temp.layerfs_read_scope",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        for row in rows {
            let candidates = lookup::candidates(&self.connection, &[row.object_id], i64::MAX)?;
            if !candidates.iter().any(|(_, save, _)| *save != self.save_id) {
                continue;
            }
            let current = ObjectLocation {
                object_id: row.object_id,
                role: ObjectRole::from_code(row.role)?,
                canonical_length: row.canonical_length,
                pack_id: row.pack_id,
                group_number: row.group_number,
                record_number: row.record_number,
            };
            let canonical = self.collision_bytes(current)?;
            for (existing, save, _) in candidates {
                if save == self.save_id {
                    continue;
                }
                if existing.role != current.role
                    || existing.canonical_length != current.canonical_length
                {
                    return Err(StorageError::Collision(row.object_id));
                }
                ownership::scope(&self.connection, save, i64::MAX)?;
                let comparison = self.collision_bytes(existing);
                ownership::scope(&self.connection, saved.0, saved.1)?;
                if comparison? != canonical {
                    return Err(StorageError::Collision(row.object_id));
                }
            }
        }
        Ok(())
    }

    fn collision_bytes(&mut self, location: ObjectLocation) -> StorageResult<Vec<u8>> {
        let mut packs = BTreeMap::new();
        // The comparison reads one object once, so its pooled reader starts empty
        // and is dropped with the wave: nothing it retains can outlive this check.
        let mut pool = crate::encoding::pool::read::PoolReader::new();
        let mut groups = crate::encoding::GroupCache::new();
        let mut counters = crate::encoding::delta::read::ChainCounters::default();
        Ok(crate::encoding::delta::read::Resolver::new(
            &self.connection,
            i64::MAX,
            &self.capacities,
            crate::encoding::delta::read::BodyCaches {
                packs: &mut packs,
                pool: &mut pool,
            },
            &mut groups,
            &mut self.decompression,
            &mut counters,
        )
        .resolve_at(location)?
        .0)
    }
}
