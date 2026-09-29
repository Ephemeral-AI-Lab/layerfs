//! Retirement and post-Commit physical cleanup for active pages and payloads.
use super::{
    compaction,
    generation::{ActiveBacking, ActivePublication, State},
    reclaim,
};
use crate::{
    backing::{budget::Charge, metadata::ProgressFund},
    WorkspaceError,
};
use std::collections::BTreeMap;

impl ActiveBacking {
    pub(crate) fn repair_completion(
        &self,
        fund: &std::sync::Arc<ProgressFund>,
        deadline: std::time::Instant,
    ) -> Result<(), WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.closed {
            return Err(WorkspaceError::Busy);
        }
        self.index.ready_for_retry()?;
        self.store.repair_completion(fund, deadline)
    }

    pub(super) fn reserve_retired_large(
        &self,
        state: &mut State,
        count: usize,
    ) -> Result<(), WorkspaceError> {
        state.retired_large_charge.resize(
            state
                .retired_large
                .len()
                .checked_add(count)
                .and_then(|count| count.checked_mul(96))
                .ok_or(WorkspaceError::Capacity)?,
        )
    }

    pub(super) fn record_retired_large(
        state: &mut State,
        live: Option<u64>,
        dead: Vec<u64>,
        revision: u64,
    ) {
        if let Some(id) = live {
            state.retired_large.remove(&id);
        }
        for id in dead {
            if state.large.contains_key(&id) {
                state.retired_large.insert(id, revision);
            }
        }
        state
            .retired_large_charge
            .resize(state.retired_large.len() * 96)
            .expect("shrinking a retirement charge cannot fail");
    }

    fn maintain_large(&self, state: &mut State) -> Result<(), WorkspaceError> {
        let _charge = self.store.budget().reserve(state.retired_large.len() * 8)?;
        let mut released = Vec::new();
        for (id, revision) in &state.retired_large {
            let owner = state.large.get(id).ok_or(WorkspaceError::Io)?;
            if !self.index.frozen_between(owner.birth, *revision)? {
                released.push(*id);
            }
        }
        for id in released {
            state.retired_large.remove(&id);
            state.large.remove(&id);
        }
        state
            .retired_large_charge
            .resize(state.retired_large.len() * 96)?;
        Ok(())
    }

    pub fn maintain_until(&self, deadline: std::time::Instant) -> Result<(), WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        self.index.maintain()?;
        self.maintain_large(&mut state)?;
        drop(state);
        self.payloads.maintain(deadline)
    }

    /// Publish C5's saved roots with the captured dirty frontier removal.
    /// Physical owners are retired in the same index revision as their last
    /// inverse reference; cleanup errors are reported after publication.
    pub fn publish_reconcile(
        &self,
        updates: &[(Vec<u8>, Option<Vec<u8>>)],
    ) -> Result<ActivePublication, WorkspaceError> {
        let expected = updates.len();
        let charge = self.store.budget().reserve(updates.iter().try_fold(
            0usize,
            |bytes, (key, value)| {
                bytes
                    .checked_add(128 + key.len() + value.as_ref().map_or(0, Vec::len))
                    .ok_or(WorkspaceError::Capacity)
            },
        )?)?;
        let updates: BTreeMap<_, _> = updates.iter().cloned().collect();
        if updates.len() != expected {
            return Err(WorkspaceError::InvalidInput);
        }
        self.publish_reconcile_map(updates, charge, None)
    }

    /// Transfer C5's already charged patch; keys are moved into the sorted
    /// candidate instead of keeping several complete patch clones resident.
    pub(crate) fn publish_reconcile_map(
        &self,
        mut updates: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        mut updates_charge: Charge,
        completion: Option<&std::sync::Arc<ProgressFund>>,
    ) -> Result<ActivePublication, WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.closed {
            return Err(WorkspaceError::Busy);
        }
        // This per-Workspace writer gate excludes ordinary publications while
        // compaction and index pages spend the captured completion fund.
        let _completion = completion
            .map(|fund| self.store.completion(fund))
            .transpose()?;
        let (dead, _dead_charge) = reclaim::prune_dead(&self.index, &mut updates, None)?;
        let (dead_large, _dead_large_charge) = reclaim::prune_dead_payloads(&self.index, &updates)?;
        self.reserve_retired_large(&mut state, dead_large.len())?;
        let tail = self.pack.tail()?;
        let (generation, prior) = self.index.generation_revision()?;
        let next = prior.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        let budget_before_plan = self.store.budget().used();
        let plan = compaction::plan(
            self.store.clone(),
            &self.pack,
            &self.index,
            &mut updates,
            generation,
            next,
            usize::MAX,
        )?;
        let budget_after_plan = self.store.budget().used();
        // Pre-admit the ordered tuple allocation while the map nodes still
        // exist. try_reserve_exact is fallible, and the allocator may return
        // greater capacity: transfer that *actual* tuple capacity before
        // moving any keys or releasing their old map-node charge.
        let scratch_bytes = updates
            .len()
            .checked_mul(std::mem::size_of::<(Vec<u8>, Option<Vec<u8>>)>());
        let ordered_result = (|| -> Result<_, WorkspaceError> {
            let mut ordered_scratch = self
                .store
                .budget()
                .reserve(scratch_bytes.ok_or(WorkspaceError::Capacity)?)?;
            let mut ordered = Vec::new();
            ordered
                .try_reserve_exact(updates.len())
                .map_err(|_| WorkspaceError::Capacity)?;
            ordered_scratch.resize(
                ordered
                    .capacity()
                    .checked_mul(std::mem::size_of::<(Vec<u8>, Option<Vec<u8>>)>())
                    .ok_or(WorkspaceError::Capacity)?,
            )?;
            let map_charge_before = updates_charge.bytes();
            // This exhausts IntoIter. All BTreeMap *nodes* have been freed;
            // every key/value Vec buffer was moved, not cloned or released.
            ordered.extend(updates);
            let (key_capacity, value_capacity) = ordered.iter().try_fold(
                (0usize, 0usize),
                |(keys, values), (key, value)| -> Result<_, WorkspaceError> {
                    Ok((
                        keys.checked_add(key.capacity())
                            .ok_or(WorkspaceError::Capacity)?,
                        values
                            .checked_add(value.as_ref().map_or(0, Vec::capacity))
                            .ok_or(WorkspaceError::Capacity)?,
                    ))
                },
            )?;
            updates_charge.resize(
                key_capacity
                    .checked_add(value_capacity)
                    .ok_or(WorkspaceError::Capacity)?,
            )?;
            if std::env::var_os("LFS_CAPACITY_DIAGNOSTIC").as_deref()
                == Some(std::ffi::OsStr::new("1"))
            {
                eprintln!(
                    "LFS_C5_CHARGE v=1 phase=map_transferred updates={} original_map_charge={} key_capacity={} value_capacity={} ordered_capacity={} ordered_charge={} buffers_charge={} budget_after_transfer={}",
                    ordered.len(), map_charge_before, key_capacity, value_capacity,
                    ordered.capacity(), ordered_scratch.bytes(), updates_charge.bytes(),
                    self.store.budget().used(),
                );
            }
            Ok((ordered, ordered_scratch))
        })();
        let (ordered, ordered_scratch) = match ordered_result {
            Ok(ordered) => ordered,
            Err(error) => return Err(plan.abort().err().unwrap_or(error)),
        };
        if std::env::var_os("LFS_CAPACITY_DIAGNOSTIC").as_deref() == Some(std::ffi::OsStr::new("1"))
        {
            eprintln!(
                "LFS_C5_CHARGE v=1 phase=index_prepare updates={} updates_charge={} ordered_scratch={} budget_before_plan={} budget_after_plan={} budget_before_index={} compaction_sources={}",
                ordered.len(), updates_charge.bytes(), ordered_scratch.bytes(),
                budget_before_plan, budget_after_plan, self.store.budget().used(), plan.retired_len(),
            );
        }
        let candidate = match self.index.prepare(&ordered) {
            Ok(candidate) => candidate,
            Err(error) => return Err(plan.abort().err().unwrap_or(error)),
        };
        if let Err(error) = state.retired.reserve(dead.len() + plan.retired_len()) {
            let candidate_error = candidate.abort().err();
            let plan_error = plan.abort().err();
            return Err(candidate_error.or(plan_error).unwrap_or(error));
        }
        let revision = match candidate.publish() {
            Ok(revision) => revision,
            Err(error) => return Err(plan.abort().err().unwrap_or(error)),
        };
        let compacted = plan.finish();
        Self::record_retired_large(&mut state, None, dead_large, revision);
        let mut cleanup_error = self.index.maintain().err();
        for (logical, page) in dead
            .into_iter()
            .map(|page| {
                (
                    tail.map_or(
                        0,
                        |(logical, current)| if current == page { logical } else { 0 },
                    ),
                    page,
                )
            })
            .chain(compacted)
        {
            if tail.is_some_and(|(_, current)| current == page) {
                if let Err(error) = self.pack.forget_if(logical, page) {
                    cleanup_error.get_or_insert(error);
                }
            }
            if let Err(error) =
                state
                    .retired
                    .retire(&self.store, page, revision, |birth, retire| {
                        self.index.selecting_revision(birth, retire)
                    })
            {
                cleanup_error.get_or_insert(error);
            }
        }
        if cleanup_error.is_some() {
            state.stopped = true;
        }
        Ok(ActivePublication {
            length: 0,
            revision,
            cleanup_error,
            inode: None,
        })
    }
}
