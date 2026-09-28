//! Retirement and post-Commit physical cleanup for active pages and payloads.
use super::{
    compaction,
    generation::{ActiveBacking, ActivePublication, State},
    reclaim,
};
use crate::WorkspaceError;
use std::collections::BTreeMap;

impl ActiveBacking {
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
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.stopped || state.closed {
            return Err(WorkspaceError::Busy);
        }
        let expected = updates.len();
        let _clone_charge = self.store.budget().reserve(
            updates
                .iter()
                .try_fold(0usize, |bytes, (key, value)| {
                    bytes
                        .checked_add(128 + key.len() + value.as_ref().map_or(0, Vec::len))
                        .ok_or(WorkspaceError::Capacity)
                })?
                .checked_mul(2)
                .ok_or(WorkspaceError::Capacity)?,
        )?;
        let mut updates: BTreeMap<_, _> = updates.iter().cloned().collect();
        if updates.len() != expected {
            return Err(WorkspaceError::InvalidInput);
        }
        let dead = reclaim::prune_dead(&self.index, &mut updates, None)?;
        let dead_large = reclaim::prune_dead_payloads(&self.index, &updates)?;
        self.reserve_retired_large(&mut state, dead_large.len())?;
        let tail = self.pack.tail()?;
        let (generation, prior) = self.index.generation_revision()?;
        let next = prior.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        let plan = compaction::plan(
            self.store.clone(),
            &self.pack,
            &self.index,
            &mut updates,
            generation,
            next,
            usize::MAX,
        )?;
        let scratch_bytes = updates.iter().try_fold(0usize, |bytes, (key, value)| {
            bytes
                .checked_add(64 + key.len() + value.as_ref().map_or(0, Vec::len))
                .ok_or(WorkspaceError::Capacity)
        });
        let _scratch = match scratch_bytes.and_then(|bytes| self.store.budget().reserve(bytes)) {
            Ok(scratch) => scratch,
            Err(error) => return Err(plan.abort().err().unwrap_or(error)),
        };
        let ordered: Vec<_> = updates
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
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
