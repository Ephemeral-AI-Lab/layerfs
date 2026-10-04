//! Exact bounded lookahead for an empty pooled candidate index.
use super::state::State;
use crate::{
    error::{StorageError, StorageResult},
    policy::ORDINAL_RESERVE_AFTER,
    port::Reserve,
    source::Source,
};
use layerfs_content::{inode_leaf::PoolingLeaf, FinalizedObject, ObjectRole};
use std::collections::BTreeSet;

impl State<'_> {
    pub(super) fn plan_initial_ordinals(
        &mut self,
        objects: &[FinalizedObject],
    ) -> StorageResult<()> {
        if self.ordinal_reservations != 0
            || !objects
                .iter()
                .any(|object| object.role() == ObjectRole::InodeLeaf)
        {
            return Ok(());
        }
        self.sync_pool()?;
        // A nonempty index can reuse values and advance its window. Keep the
        // original per-leaf decision there; this plan never speculates on it.
        if !self.pool_index.is_empty() {
            return Ok(());
        }
        let mut values = BTreeSet::new();
        let mut leaves = 0;
        let mut exact_demands = 0;
        for object in objects {
            if object.role() != ObjectRole::InodeLeaf
                || self.packer.pending(object.id())
                || self.packer.location(object.id()).is_some()
                || self
                    .storage
                    .source
                    .location(object.id(), i64::MAX)?
                    .is_some()
            {
                continue;
            }
            let leaf = PoolingLeaf::decode(object.canonical())?;
            let before = values.len();
            for row in leaf.rows() {
                values.insert(row.value);
            }
            exact_demands += usize::from(values.len() > before);
            leaves += 1;
            if leaves == ORDINAL_RESERVE_AFTER {
                break;
            }
        }
        if !values.is_empty() {
            // At most four leaves /660 distinct values; discard the lookahead
            // before admission. Assignment still follows the ordinary leaf walk.
            self.reserve_ordinals(values.len(), exact_demands)?;
        }
        Ok(())
    }

    pub(super) fn reserve_ordinals(
        &mut self,
        block: usize,
        exact_demands: usize,
    ) -> StorageResult<()> {
        self.storage.source.note(|c| {
            c.reserve += 1;
            c.ordinal_reservations += 1;
        });
        let _work = self.storage.work.span(super::Stage::OrdinalReserve);
        let allocation = self.storage.source.metadata.reserve(Reserve {
            packs: 0,
            ordinals: block,
        })?;
        if allocation.first_ordinal == 0 {
            return Err(StorageError::Integrity("metadata ordinal reservation"));
        }
        self.next_ordinal = u64::from(allocation.first_ordinal);
        self.ordinal_end = self
            .next_ordinal
            .checked_add(block as u64)
            .filter(|end| *end <= u64::from(u32::MAX) + 1)
            .ok_or(StorageError::Integrity("metadata ordinal maximum"))?;
        // The switch to blocks follows exact leaf demands, not the number of
        // durable calls after coalescing. Diagnostics count actual calls above.
        self.ordinal_reservations += exact_demands;
        Ok(())
    }
}
