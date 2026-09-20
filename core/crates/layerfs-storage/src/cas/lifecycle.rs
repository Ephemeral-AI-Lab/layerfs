//! Acquisition, commit cadence, acknowledgement and terminal disposition.
//!
//! Ownership is taken once: a single `BEGIN IMMEDIATE` attempt, a baseline and
//! the operation's cursors read under it, and a profile re-verified rather than
//! trusted. The transaction is bounded by declared row and byte ceilings and
//! acknowledged exactly once at the end, where the publication watermark advances
//! in the same transaction as the writes it names. A failed operation has one
//! cleanup attempt; an unproven one is quarantined instead.

use rusqlite::Connection;

use crate::cas::dependencies::Availability;
use crate::cas::owner::{MutationOwner, OutcomeCounters, SaveProfile};
use crate::cas::placement::PendingGroup;
use crate::cas::pool_lane::PoolCounters;
use crate::encoding::codec::{CompressionWorkspace, DecompressionWorkspace};
use crate::encoding::delta::candidates::Candidates;
use crate::encoding::delta::read::ChainCounters;
use crate::encoding::delta::select::{DeltaCounters, DepthCache};
use crate::error::{StorageError, StorageResult};
use crate::pack::layout::PackLane;
use crate::pack::placement::LanePlacement;
use crate::policy::StorageCapacities;
use crate::sqlite::lookup;
use crate::sqlite::write::{self, TransactionState};
use std::collections::BTreeMap;
use std::time::Instant;

impl MutationOwner {
    /// Acquires ownership once and establishes the operation's cursors.
    pub fn acquire(
        connection: Connection,
        capacities: StorageCapacities,
        pool_index: std::sync::Arc<std::sync::Mutex<crate::encoding::pool::PoolIndex>>,
        candidates: std::sync::Arc<std::sync::Mutex<Candidates>>,
    ) -> StorageResult<Self> {
        // The connection is caller-supplied at this seam, so its profile is
        // re-verified rather than trusted: a write must never run on a
        // connection with another journal mode, synchronous setting,
        // foreign-key enforcement or busy timeout than the declared one.
        crate::sqlite::connection::verify_profile(&connection)?;
        // Ownership first: the baseline and cursor are only meaningful when no
        // other writer can publish a pack between reading them and using them.
        write::begin_immediate(&connection)?;
        let baseline_pack_id = lookup::highest_pack_id(&connection)?;
        let published = crate::sqlite::schema::retained_pack_ceiling(&connection)?;
        if published != baseline_pack_id {
            // Undeleted packs from a save whose cleanup did not complete. Writing
            // now would have to guess their ownership, so the save is refused.
            return Err(StorageError::UninspectedState {
                ceiling: published,
                highest_pack_id: baseline_pack_id,
            });
        }
        let next_pack_id = baseline_pack_id
            .checked_add(1)
            .ok_or(StorageError::Integrity("pack identifier overflow"))?;
        // The Store-owned content index is read back here, not at the call site,
        // because it must be read inside the write acquisition that is about to
        // use it. `Store::open` already loaded it, so this reads the table only
        // after a failed save invalidated the slots.
        {
            let mut index = candidates
                .lock()
                .map_err(|_| StorageError::Integrity("candidate index lock"))?;
            if index.needs_load() {
                index.reload(&connection)?;
            }
        }
        let compression = CompressionWorkspace::new()?;
        let decompression = DecompressionWorkspace::new()?;
        Ok(Self {
            connection,
            capacities,
            baseline_pack_id,
            next_pack_id,
            ceiling: baseline_pack_id,
            placement: [
                LanePlacement::new(),
                LanePlacement::new(),
                LanePlacement::new(),
                LanePlacement::new(),
                LanePlacement::new(),
            ],
            groups: [
                PendingGroup::default(),
                PendingGroup::default(),
                PendingGroup::default(),
                PendingGroup::default(),
                PendingGroup::default(),
            ],
            sealed_rows: Vec::new(),
            transaction: TransactionState { rows: 1, bytes: 0 },
            transaction_open: true,
            compression,
            decompression,
            terminal: false,
            cleanup_attempted: false,
            quarantined: false,
            counters: OutcomeCounters {
                transactions: 1,
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
            pending_values: BTreeMap::new(),
            next_ordinal: None,
            pool_synced: false,
            pool: PoolCounters::default(),
        })
    }

    /// Highest retained pack identifier before this operation.
    pub fn baseline_pack_id(&self) -> i64 {
        self.baseline_pack_id
    }

    /// Connection held by this owner; same-save reads use it directly.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Commits and lazily restarts the shared write transaction when it is full.
    pub fn maybe_commit(&mut self) -> StorageResult<()> {
        if self.transaction_open
            && (self.transaction.rows >= self.capacities.transaction_rows
                || self.transaction.bytes >= self.capacities.transaction_bytes)
        {
            let started = Instant::now();
            write::commit(&self.connection)?;
            SaveProfile::charge(&mut self.profile.commit_ns, started);
            // Clear the flag before the re-acquire: if the lock is lost here, no
            // transaction is open and cleanup must proceed to the deletion pass
            // instead of trying to roll back a transaction that does not exist.
            self.transaction_open = false;
            self.counters.commits += 1;
            let started = Instant::now();
            write::begin_immediate(&self.connection)?;
            SaveProfile::charge(&mut self.profile.commit_ns, started);
            self.transaction_open = true;
            self.transaction = TransactionState { rows: 1, bytes: 0 };
            self.counters.transactions += 1;
        }
        Ok(())
    }

    /// Completes every remaining group and acknowledges the final transaction.
    ///
    /// An operation whose last transaction holds no write at all is released with
    /// `ROLLBACK`: no `COMMIT` is issued for an empty write.
    pub fn finish(&mut self) -> StorageResult<OutcomeCounters> {
        if self.terminal {
            return Err(StorageError::Aborted);
        }
        match self.finish_inner() {
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

    fn finish_inner(&mut self) -> StorageResult<OutcomeCounters> {
        let mut availability = Availability::default();
        for lane in PackLane::ALL {
            self.seal_group(lane, &mut availability)?;
        }
        let pending_rows = self.transaction.rows.saturating_sub(1);
        let has_pending = pending_rows > 0 || self.transaction.bytes > 0;
        // A save that created no pack has nothing to publish. It releases its
        // acquisition with ROLLBACK: no COMMIT is issued for an empty write.
        let publishes = self.ceiling > self.baseline_pack_id;
        if !self.transaction_open {
            return Ok(self.counters);
        }
        if !has_pending && !publishes {
            let started = Instant::now();
            write::rollback(&self.connection)?;
            SaveProfile::charge(&mut self.profile.commit_ns, started);
            self.transaction_open = false;
            return Ok(self.counters);
        }
        if !has_pending {
            // An earlier bounded commit already published this save's last packs.
            // The watermark still needs its own acknowledgement, so the empty
            // acquisition is released and a dedicated final transaction carries
            // the publication. It is the last thing this save does; if it is lost,
            // the packs stay unpublished and the Store is uninspected rather than
            // silently exposed.
            let started = Instant::now();
            write::rollback(&self.connection)?;
            SaveProfile::charge(&mut self.profile.commit_ns, started);
            let started = Instant::now();
            write::begin_immediate(&self.connection)?;
            SaveProfile::charge(&mut self.profile.commit_ns, started);
            self.counters.transactions += 1;
        }
        // **W2 (#188d).** The content index is written in the transaction that
        // publishes the objects it names, and only the entries this save admitted
        // are written. Either the save's output and its index both become visible,
        // or neither does: a rolled-back transaction leaves the table exactly as
        // the last acknowledged save left it.
        let started = Instant::now();
        let flushed = {
            let mut index = self
                .candidates
                .lock()
                .map_err(|_| StorageError::Integrity("candidate index lock"))?;
            index.flush(&self.connection)
        };
        SaveProfile::charge(&mut self.profile.sql_ns, started);
        flushed?;
        // The watermark names the packs this save created. It advances only here,
        // so either the save's output and its watermark both become visible, or
        // neither does.
        let started = Instant::now();
        let advanced =
            crate::sqlite::schema::advance_retained_pack_ceiling(&self.connection, self.ceiling);
        SaveProfile::charge(&mut self.profile.sql_ns, started);
        advanced?;
        let started = Instant::now();
        write::commit(&self.connection)?;
        SaveProfile::charge(&mut self.profile.commit_ns, started);
        self.counters.commits += 1;
        self.transaction_open = false;
        Ok(self.counters)
    }

    /// Ends a failed operation with exactly one cleanup attempt.
    pub fn abandon(&mut self) -> StorageResult<()> {
        if self.cleanup_attempted || self.quarantined {
            return Ok(());
        }
        self.cleanup_attempted = true;
        self.terminal = true;
        if self.transaction_open {
            write::rollback(&self.connection)?;
            self.transaction_open = false;
        }
        crate::sqlite::cleanup::abandon(&self.connection, self.baseline_pack_id)?;
        // An abandoned save published nothing, so the entries it admitted name
        // objects that do not exist. The table is authoritative and was never
        // written; the slots are dropped and read back by the next save.
        self.invalidate_candidates();
        Ok(())
    }

    /// Marks the operation terminal without touching storage.
    ///
    /// A failed save also invalidates the Store-owned pooled index: ordinals it
    /// assigned may never have been committed, so the disposable derivation is
    /// reset and rebuilt from the catalogue by the next save.
    pub fn mark_terminal(&mut self) {
        self.terminal = true;
        if let Ok(mut index) = self.pool_index.lock() {
            index.invalidate();
        }
        self.invalidate_candidates();
    }

    /// Marks an unproven outcome: affected writes stop and nothing is deleted.
    pub fn quarantine(&mut self) {
        self.terminal = true;
        self.quarantined = true;
        if let Ok(mut index) = self.pool_index.lock() {
            index.invalidate();
        }
        self.invalidate_candidates();
    }

    /// Drops the Store-owned content index back to "reload from the table".
    ///
    /// A failed save may have admitted entries for objects it never published, and
    /// the flush that would have written them never committed. The table is
    /// authoritative and the slots are disposable derivation, so the reset is
    /// whole and the next save reads the table back rather than repairing anything.
    fn invalidate_candidates(&mut self) {
        if let Ok(mut index) = self.candidates.lock() {
            index.invalidate();
        }
    }
}
