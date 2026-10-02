//! The existing C2 pooled selector with SQL ordinal/group orchestration.
use super::*;
impl Writer {
    pub(super) fn pooled_record(
        &mut self,
        object: &FinalizedObject,
        advisory: &[ObjectId],
    ) -> Result<EncodedRecord, String> {
        let leaf =
            layerfs_content::inode_leaf::InodeLeaf::decode(object.canonical()).map_err(error)?;
        let mut seen = BTreeSet::new();
        let unknown: Vec<_> = leaf
            .rows
            .iter()
            .filter_map(|r| {
                if seen.insert(r.value) {
                    Some(r.value)
                } else {
                    None
                }
            })
            .collect();
        let scope = self.catalog.capture(Some(self.save))?;
        let access = PrivateAccess {
            persisted: Access {
                catalog: self.catalog.as_ref(),
                provider: &self.provider,
                scope,
                logical_use: LogicalUse::MetadataGraph,
            },
            window: &self.window,
            save: self.save,
        };
        self.pool_index
            .sync(
                &access,
                &self.capacities,
                scope.ceiling,
                &mut self.pool,
                &mut self.decoder,
            )
            .map_err(error)?;
        let known = self
            .pool_index
            .find(
                &access,
                &self.capacities,
                scope.ceiling,
                &mut self.pool,
                &mut self.decoder,
                &unknown,
            )
            .map_err(error)?;
        let fresh_count = unknown.iter().filter(|v| !known.contains_key(*v)).count();
        if fresh_count > 0
            && self.ordinal_end.saturating_sub(self.ordinal_next) < fresh_count as u64
        {
            let count =
                if self.ordinal_reservations < layerfs_storage::policy::ORDINAL_RESERVE_AFTER {
                    fresh_count
                } else {
                    fresh_count * layerfs_storage::policy::ORDINAL_BLOCK_LEAVES
                };
            self.ordinal_next = u64::from(self.catalog.reserve_ordinals(self.save, count)?);
            self.ordinal_end = self.ordinal_next + count as u64;
            self.ordinal_reservations += 1;
        }
        let mut memo = BTreeMap::new();
        let mut ordinals = Vec::with_capacity(leaf.rows.len());
        let mut fresh = Vec::new();
        let mut first = 0;
        for row in &leaf.rows {
            let ordinal =
                if let Some(ordinal) = memo.get(&row.value).or_else(|| known.get(&row.value)) {
                    self.pool_counts.reused_values += 1;
                    *ordinal
                } else {
                    let ordinal = u32::try_from(self.ordinal_next).map_err(error)?;
                    self.ordinal_next += 1;
                    if fresh.is_empty() {
                        first = ordinal
                    }
                    fresh.push(row.value);
                    memo.insert(row.value, ordinal);
                    self.pool_counts.new_values += 1;
                    ordinal
                };
            ordinals.push(ordinal);
        }
        if !fresh.is_empty() {
            self.catalog.note_window(self.save, fresh.len(), first)?;
            let canonical = fresh
                .iter()
                .map(value_group::canonical_value)
                .collect::<Result<Vec<_>, _>>()
                .map_err(error)?;
            let built = value_group::build(&canonical, &mut self.encoder).map_err(error)?;
            let key = (PlacementDomain::Metadata, PackLane::PooledMetadata.index());
            self.append_group(key, PackLane::PooledMetadata, built.group, Vec::new())?;
            let tail = self.window.tails.get(&key).ok_or("pooled tail missing")?;
            let row = ValueGroupRow {
                first_ordinal: first,
                count: built.count,
                pack_id: tail.order,
                group_number: tail.groups.len() - 1,
                digest: built.digest,
            };
            self.window.groups.insert(first, row);
            self.pool_index.advance_window(
                self.catalog
                    .metadata_window_start(self.catalog.capture(Some(self.save))?)?,
            );
            self.pool_index.note_group(first, &fresh).map_err(error)?;
            self.pool_counts.groups += 1;
        }
        let body = layerfs_content::inode_leaf::pooled_body(object.canonical(), &ordinals)
            .map_err(error)?;
        let scope = self.catalog.capture(Some(self.save))?;
        let access = PrivateAccess {
            persisted: Access {
                catalog: self.catalog.as_ref(),
                provider: &self.provider,
                scope,
                logical_use: LogicalUse::MetadataGraph,
            },
            window: &self.window,
            save: self.save,
        };
        select_pooled(
            &mut PooledSelectInput {
                access: &access,
                ceiling: scope.ceiling,
                capacities: &self.capacities,
                depths: &mut self.depths,
                reader: &mut self.pool,
                decode: &mut self.decoder,
                counters: &mut self.pool_counts,
                profile: &mut self.profile,
            },
            object.canonical_len(),
            &body,
            advisory,
        )
        .map_err(error)
    }
}
