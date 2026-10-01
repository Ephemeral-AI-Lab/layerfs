//! Save ownership outlives short transactions; only final publication exposes it.
use rusqlite::Connection;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use crate::cas::dependencies::Availability;
use crate::cas::owner::{MutationOwner, OutcomeCounters, SaveProfile};
use crate::cas::placement::PendingGroup;
use crate::cas::pool_lane::PoolCounters;
use crate::encoding::delta::candidates::Candidates;
use crate::encoding::delta::read::ChainCounters;
use crate::encoding::delta::select::{DeltaCounters, DepthCache};
use crate::encoding::pool::PoolIndex;
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::PackLane;
use crate::pack::placement::LanePlacement;
use crate::policy::StorageCapacities;
use crate::sqlite::{
    lookup, ownership,
    write::{self, TransactionState},
};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::time::Instant;

impl MutationOwner {
    /// Reserves one save slot, without retaining database write ownership.
    // The captured connection, guard, indexes and credits have distinct custody roles.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn acquire(
        connection: Connection,
        engine: Option<&'static crate::engine::EngineGuard>,
        capacities: StorageCapacities,
        arbitration: Arc<Mutex<()>>,
        published_pool: Arc<Mutex<PoolIndex>>,
        published_candidates: Arc<Mutex<Candidates>>,
        published_index_credit: Arc<super::working::IndexLease>,
        working: super::working::SaveLease,
    ) -> StorageResult<Self> {
        if let Some(guard) = engine {
            crate::sqlite::connection::verify_guarded(
                &connection,
                guard,
                crate::engine::ConnectionClass::ContentWrite,
            )?;
        }
        let mut prepared = super::working_prepared::PreparedSave::new(
            connection,
            engine,
            published_pool,
            published_candidates,
            published_index_credit,
            working,
        );
        // The reservation already exists. No Save row is born before these
        // fallible codec preparations and actual index copies are ready.
        prepared.codecs()?;
        let guard = ownership::lock(&arbitration)?;
        crate::sqlite::connection::verify_profile(&prepared.connection)?;
        let pool = prepared
            .published_pool
            .lock()
            .map_err(|_| StorageError::Integrity("pool index lock"))?
            .clone();
        let content = prepared
            .published_candidates
            .lock()
            .map_err(|_| StorageError::Integrity("candidate index lock"))?
            .clone();
        prepared.pool_index = Some(Arc::new(Mutex::new(pool)));
        prepared.candidates = Some(Arc::new(Mutex::new(content)));
        let baseline_pack_id = lookup::highest_pack_id(&prepared.connection)?;
        let (save_id, _) = match if let Some(engine) = prepared.engine {
            ownership::acquire_participating(&prepared.connection, Some(engine))
        } else {
            ownership::acquire(&prepared.connection)
        } {
            Ok(identity) => identity,
            Err(original) => {
                prepared
                    .working
                    .note(&original, original.is_unknown_outcome());
                drop(guard);
                if original.is_unknown_outcome() {
                    super::working::retain(super::working::RetainedSave::Prepared(prepared));
                }
                return Err(original);
            }
        };
        prepared.working.acknowledged(save_id);
        drop(guard);
        let super::working_prepared::PreparedSave {
            connection,
            engine,
            compression,
            decompression,
            pool_index,
            candidates,
            published_index_credit,
            working,
            published_pool,
            published_candidates,
        } = prepared;
        let compression = compression.unwrap();
        let decompression = decompression.unwrap();
        let pool_index = pool_index.unwrap();
        let candidates = candidates.unwrap();
        Ok(Self {
            connection,
            engine,
            capacities,
            arbitration,
            save_id,
            published_pool,
            published_candidates,
            baseline_pack_id,
            next_pack_id: 0,
            committed_pack_id: 0,
            ceiling: baseline_pack_id,
            placement: std::array::from_fn(|_| LanePlacement::new()),
            groups: std::array::from_fn(|_| PendingGroup::default()),
            queued: Default::default(),
            sealed_rows: Vec::new(),
            transaction: TransactionState::default(),
            transaction_open: false,
            wave_held: false,
            compression,
            decompression,
            terminal: false,
            cleanup_attempted: false,
            cleanup_completed: false,
            quarantined: false,
            counters: OutcomeCounters {
                transactions: 1,
                commits: 1,
                ..OutcomeCounters::default()
            },
            profile: SaveProfile::default(),
            // Enabled only by `LAYERFS_STORAGE_REUSE_PROBE`; `None` otherwise, so
            // an ordinary save carries no probe state at all.
            probe: crate::cas::owner::ReuseProbe::enabled().then(Default::default),
            candidates,
            depths: DepthCache::new(),
            pack_cache: BTreeMap::new(),
            chain: ChainCounters::default(),
            chain_total: ChainCounters::default(),
            delta: DeltaCounters::default(),
            pool_index,
            pool_reader: crate::encoding::pool::PoolReader::new(),
            wave_rows: Vec::new(),
            pending_values: BTreeMap::new(),
            next_ordinal: None,
            ordinal_block_end: None,
            ordinal_reservations: 0,
            pool_synced: false,
            pool: PoolCounters::default(),
            published_index_credit,
            working,
        })
    }

    /// Highest pack identifier observed when this save acquired its slot.
    pub fn baseline_pack_id(&self) -> i64 {
        self.baseline_pack_id
    }

    /// Private connection: its scope excludes every foreign private save.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub(super) fn begin_write(&mut self) -> StorageResult<()> {
        self.validate_engine()?;
        let started = Instant::now();
        write::begin_immediate(&self.connection)?;
        self.transaction_open = true;
        self.transaction = TransactionState::default();
        self.next_pack_id = ownership::next_pack(&self.connection)?;
        // The value this transaction starts from is the value the row holds, so a
        // step that allocates no pack has nothing to write back.
        self.committed_pack_id = self.next_pack_id;
        self.counters.transactions += 1;
        SaveProfile::charge(&mut self.profile.diag.begin_ns, started);
        Ok(())
    }

    /// Writes the pack watermark back only when this transaction moved it.
    ///
    /// The watermark is what stops a second writer handing out a pack id this save
    /// already used, so it must be correct at every step boundary, not merely at
    /// publication: deferring it to publication is exactly the change that would
    /// let a second writer collide, and `tests/pack_watermark.rs` fails on it. It
    /// moves only when `LanePlacement` starts a new pack, and this transaction
    /// re-read the row when it began, so writing the unchanged value back is a
    /// statement whose result the row already holds - one statement, and one
    /// dirtied page on a commit that has nothing to do with pack allocation.
    fn advance_pack_if_moved(&mut self) -> StorageResult<()> {
        if self.next_pack_id != self.committed_pack_id {
            ownership::advance_pack(&self.connection, self.next_pack_id)?;
            self.committed_pack_id = self.next_pack_id;
        }
        Ok(())
    }

    /// Acknowledges one physical group; no transaction survives preparation.
    pub fn maybe_commit(&mut self) -> StorageResult<()> {
        // A wave acknowledges its own transaction once, at its end: a seal inside
        // a wave must not close the transaction the wave opened, or the step
        // becomes a seal again.
        if self.transaction_open && !self.wave_held {
            self.validate_engine()?;
            let whole = Instant::now();
            let started = Instant::now();
            // Multi-writer: SQLite admits one writer per store file, and the other
            // writer must not have to wait for this one's whole upload. A write
            // transaction therefore never outlives the step that opened it under the
            // arbitration lock, so every step commits before that lock is released.
            // Batching stays inside a step; it cannot span steps.
            self.advance_pack_if_moved()?;
            self.validate_engine()?;
            write::commit(&self.connection)?;
            SaveProfile::charge(&mut self.profile.commit_ns, started);
            // Clear the flag before the next step re-acquires: if the lock is lost
            // there, no transaction is open and cleanup proceeds to the deletion
            // pass instead of rolling back a transaction that does not exist.
            self.transaction_open = false;
            self.counters.commits += 1;
            SaveProfile::charge(&mut self.profile.diag.commit_total_ns, whole);
        }
        Ok(())
    }

    pub(super) fn flush_candidates(&mut self) -> StorageResult<()> {
        let arbitration = std::sync::Arc::clone(&self.arbitration);
        let _guard = crate::sqlite::ownership::lock_unless_held(&arbitration, self.wave_held)?;
        // The save's own transaction is normally still open here: bounded commits
        // re-acquire it. Opening a second one would nest, so only a caller that
        // arrives with nothing open starts a transaction, and only that caller may
        // roll it back when the index had nothing new to write.
        let opened = !self.transaction_open;
        if opened {
            self.begin_write()?;
        }
        let written = self
            .candidates
            .lock()
            .map_err(|_| StorageError::Integrity("candidate index lock"))?
            .flush(&self.connection)?;
        if written == 0 && opened {
            self.validate_engine()?;
            write::rollback(&self.connection)?;
            self.transaction_open = false;
            return Ok(());
        }
        self.maybe_commit()
    }

    /// Finishes physical groups, then atomically publishes one save row.
    pub fn finish(&mut self, scope: &TimingScope<'_, Active>) -> StorageResult<OutcomeCounters> {
        if self.terminal {
            return Err(StorageError::Aborted);
        }
        match self.finish_inner(scope) {
            Ok(mut counters) => {
                // Selection and chain counters live beside the resolver while the
                // operation runs; the outcome reports the totals once.
                counters.delta = self.delta;
                counters.chain = self.chain_total;
                counters.pool = self.pool;
                counters.profile = self.profile;
                Ok(counters)
            }
            Err(error) => {
                self.terminal = true;
                Err(error)
            }
        }
    }

    fn finish_inner(&mut self, scope: &TimingScope<'_, Active>) -> StorageResult<OutcomeCounters> {
        self.validate_engine()?;
        let whole = Instant::now();
        let mut availability = Availability::default();
        scope.child("storage.finish.pack_seal").run(|_| {
            for lane in PackLane::ALL {
                self.seal_group(lane, &mut availability)?;
            }
            Ok::<(), StorageError>(())
        })?;
        let arbitration = Arc::clone(&self.arbitration);
        let _guard = ownership::lock(&arbitration)?;
        if !self.transaction_open {
            scope
                .child("storage.finish.transaction_begin")
                .run(|_| self.begin_write())?;
        }
        // W2 (#188d): the content index is written in the transaction that publishes
        // the objects it names, so a save's output and its index become visible
        // together or not at all.
        let started = Instant::now();
        let flushed = scope.child("storage.finish.candidate_flush").run(|_| {
            let mut index = self
                .candidates
                .lock()
                .map_err(|_| StorageError::Integrity("candidate index lock"))?;
            index.flush(&self.connection)
        });
        SaveProfile::charge(&mut self.profile.sql_ns, started);
        flushed?;
        // Pooled groups need a full BLOB reservation while this save may append.
        // At publication no append remains, so release its unused tail inside
        // the same measured transaction before the pack becomes visible.
        let started = Instant::now();
        scope.child("storage.finish.tail_close").run(|_| {
            if let Some((pack_id, used)) =
                self.placement[PackLane::PooledMetadata.index()].close_tail()
            {
                if used < PackLane::PooledMetadata.pack_limit() {
                    write::close_pack_tail(&self.connection, pack_id, used)?;
                }
            }
            Ok::<(), StorageError>(())
        })?;
        SaveProfile::charge(&mut self.profile.sql_ns, started);
        // Multi-writer: the same transaction releases this save's slot by
        // publishing it, and advances the pack allocation watermark so no other
        // writer can hand out a pack id this save already used. Either the save's
        // data, its index and its publication all become visible, or none does.
        let started = Instant::now();
        scope
            .child("storage.finish.ownership_publish")
            .run(|_| ownership::publish(&self.connection, self.save_id))?;
        SaveProfile::charge(&mut self.profile.diag.publish_ns, started);
        // The ordinal block this save still holds went with the save: what it did
        // not hand out is returned to the catalogue, in the transaction that
        // publishes the values that were handed out (`ownership::release_ordinals`).
        scope.child("storage.finish.ordinal_watermark").run(|_| {
            if let (Some(used_end), Some(reserved_end)) =
                (self.next_ordinal, self.ordinal_block_end)
            {
                ownership::release_ordinals(&self.connection, used_end, reserved_end)?;
            }
            self.advance_pack_if_moved()
        })?;
        // Both possible publication copies are funded by the original Save
        // reservation. Complete them before final COMMIT, then only move values.
        let ready_pool = scope.child("storage.finish.pool_clone").run(|_| {
            let private = self
                .pool_index
                .lock()
                .map_err(|_| StorageError::Integrity("pool index lock"))?;
            self.profile.diag.finish_pool_entries = private.len() as u64;
            self.profile.diag.finish_pool_bytes = private.live_bytes() as u64;
            Ok::<_, StorageError>(private.clone())
        })?;
        let ready_candidates = scope.child("storage.finish.candidates_clone").run(|_| {
            let private = self
                .candidates
                .lock()
                .map_err(|_| StorageError::Integrity("candidate index lock"))?;
            self.profile.diag.finish_candidate_entries = private.entries() as u64;
            self.profile.diag.finish_candidate_bytes = private.live_bytes() as u64;
            Ok::<_, StorageError>(private.clone())
        })?;
        let published_pool = Arc::clone(&self.published_pool);
        let published_candidates = Arc::clone(&self.published_candidates);
        let mut shared_pool = published_pool
            .lock()
            .map_err(|_| StorageError::Integrity("pool index lock"))?;
        let mut shared_candidates = published_candidates
            .lock()
            .map_err(|_| StorageError::Integrity("candidate index lock"))?;
        let started = Instant::now();
        self.validate_engine()?;
        scope
            .child("storage.finish.sqlite_commit")
            .run(|_| write::commit(&self.connection))?;
        SaveProfile::charge(&mut self.profile.commit_ns, started);
        self.counters.commits += 1;
        self.transaction_open = false;
        // Known publication transfers ready allocations into already funded
        // shared owners; replacing them actually releases the old index values.
        *shared_pool = ready_pool;
        *shared_candidates = ready_candidates;
        drop(shared_pool);
        drop(shared_candidates);
        SaveProfile::charge(&mut self.profile.diag.finish_total_ns, whole);
        Ok(self.counters)
    }

    /// One definite-failure cleanup, exclusively scoped to this save.
    pub fn abandon(&mut self) -> StorageResult<()> {
        self.validate_engine()?;
        if self.quarantined || self.cleanup_attempted && !self.cleanup_completed {
            return Err(StorageError::Integrity(
                "uncertain/failed Save cleanup denied",
            ));
        }
        if self.cleanup_completed {
            return Ok(());
        }
        self.cleanup_attempted = true;
        self.terminal = true;
        if self.transaction_open {
            let _guard = ownership::lock(&self.arbitration)?;
            ownership::engine_boundary(&self.connection, self.engine)?;
            write::rollback(&self.connection)?;
            self.transaction_open = false;
        }
        crate::sqlite::cleanup::abandon_participating(
            &self.connection,
            self.save_id,
            &self.arbitration,
            self.engine,
        )?;
        self.cleanup_completed = true;
        Ok(())
    }

    /// Stops this save; other saves' candidate state remains independent.
    pub fn mark_terminal(&mut self) {
        self.terminal = true;
    }

    /// Preserves unknown persisted ownership without cleanup or replay.
    pub fn quarantine(&mut self) {
        self.terminal = true;
        self.quarantined = true;
    }
}
