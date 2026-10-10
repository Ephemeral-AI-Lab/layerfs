//! The harness's Store adapter, and the two tables that describe the 2026-10-10 port.
//!
//! **Why an adapter exists.** `layerfs_storage::{Store, SaveOutcome, SaveHandoff,
//! StoreProvider}` and `layerfs_history::sqlite` were removed by `bd9ededba`. The
//! current product composes one Store from two public pieces: a
//! `layerfs_persistence::Handles` (the SQLite session, holding packs **and**
//! history) and a `layerfs_storage::Storage` over its pack port. Every driver in
//! `ops/` needs both, with the same explicit profile, so they are opened here once
//! rather than at ninety call sites. This is harness code: it adds nothing to a
//! product API and calls only public items.
//!
//! **The profile is selected explicitly, every time.** `PersistenceConfig::sqlite`
//! defaults to `Durable`; owner direction 2026-10-07 permits Disposable / WAL /
//! `synchronous = OFF` only. [`config`] is the one place a config is built and it
//! always names `SqlitePersistenceProfile::Disposable`. Durable is never executed.
//!
//! **This is a new identity.** The removed engine was `journal_mode = MEMORY` /
//! `synchronous = OFF` with the history catalogue in a separate database; the
//! current one is WAL / OFF with packs and history in one file. Byte-size and
//! statement-count figures recorded against the removed engine are not continued.
//! [`COUNTER_MAPPINGS`] lists what kept its meaning under a new source and
//! [`UNAVAILABLE_COUNTERS`] lists what has no current source. A name in the second
//! table is **not emitted** — not as zero, not as a different quantity — so a
//! consumer sees "unavailable at this product identity" instead of a number.

use std::path::{Path, PathBuf};

use layerfs_content::ObjectId;
use layerfs_history::{HistoryCatalog, HistoryCatalogConfig};
use layerfs_persistence::{
    ConnectionProfile, Handles, PersistenceConfig, SealedStore, SqlitePersistenceProfile,
};
use layerfs_storage::port::{PackPersistence, PersistenceError};
use layerfs_storage::{
    Diagnostics, Reader, Save, Storage, StorageCapacities, StoragePolicy, StorageResult,
};

use crate::gates::{Gate, GateClass};

/// The persistence profile every Store in this harness is created and opened with.
pub const PROFILE: SqlitePersistenceProfile = SqlitePersistenceProfile::Disposable;

/// The receipt token for [`PROFILE`].
pub const PROFILE_TOKEN: &str = "disposable-wal-synchronous-off";

/// The one place a persistence config is built: SQLite at `path`, [`PROFILE`] named.
pub fn config(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(PROFILE)
}

/// The history authority a C2 or pipeline row's Store is bound to.
///
/// The current Store always carries a history catalogue, so creation needs a
/// binding even for a row that publishes no history. The binding is fixed so a
/// prepared master and every per-sample copy of it open under the same authority.
pub fn harness_history() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"layerfs/fs-bench-pro-storage-content/store/v1".to_vec(),
        incarnation: 1,
        cursor_key: [0x5c; 32],
    }
}

/// One open Store: the SQLite session and the C2 handle over its pack port.
pub struct Store {
    // Declared before `handles` so the C2 handle's clone of the pack port is
    // released before the session that owns it.
    storage: Storage,
    handles: Handles,
    path: PathBuf,
}

impl Store {
    /// Creates a fresh Store under the harness binding. Refuses an existing file.
    pub fn create(path: &Path, policy: StoragePolicy) -> StorageResult<Self> {
        Self::create_with(path, policy, &harness_history())
    }

    /// Creates a fresh Store under an explicit history binding.
    pub fn create_with(
        path: &Path,
        policy: StoragePolicy,
        history: &HistoryCatalogConfig,
    ) -> StorageResult<Self> {
        let handles = Handles::create(config(path), policy, history)?;
        Self::over(handles, path)
    }

    /// Opens an existing Store for writing under the harness binding.
    pub fn open(path: &Path) -> StorageResult<Self> {
        Self::open_with(path, &harness_history())
    }

    /// Opens an existing Store for writing under an explicit history binding.
    pub fn open_with(path: &Path, history: &HistoryCatalogConfig) -> StorageResult<Self> {
        let handles =
            Handles::open_writable(config(path), &history.binding_key, history.cursor_key)?;
        Self::over(handles, path)
    }

    fn over(handles: Handles, path: &Path) -> StorageResult<Self> {
        let storage = Storage::new(handles.storage.clone())?;
        Ok(Self {
            storage,
            handles,
            path: path.to_path_buf(),
        })
    }

    /// The path this Store was created or opened at.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The C2 handle.
    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    /// The history catalogue held in the same database.
    pub fn history(&self) -> &dyn HistoryCatalog {
        &self.handles.history
    }

    /// Existing canonical, pack, wave and chain bounds under the persisted policy.
    pub fn capacities(&self) -> StorageCapacities {
        self.storage.capacities()
    }

    /// Begins one producer's bounded save.
    pub fn begin_save(&self) -> StorageResult<Save<'_>> {
        self.storage.begin_save()
    }

    /// Creates an operation-owned authenticated reader.
    pub fn reader(&self) -> StorageResult<Reader<'_>> {
        self.storage.reader()
    }

    /// Cumulative per-handle operation counts.
    pub fn diagnostics(&self) -> Diagnostics {
        self.storage.diagnostics()
    }

    /// Read-back settings from the actual shared connection.
    pub fn profile(&self) -> &ConnectionProfile {
        self.handles.profile()
    }

    /// The identities of `ids` this Store holds, through the pack port's locator.
    ///
    /// `PackPersistence::locate` takes at most `READ_OBJECT_LIMIT` distinct ids and
    /// omits absence; the caller pages, as [`crate::workload::expected::present_all`]
    /// does. The removed `Store::contains` is the same question; there is no
    /// handle-level presence method today, so the port is asked directly.
    pub fn contains(&self, ids: &[ObjectId]) -> StorageResult<Vec<ObjectId>> {
        let mut located = Vec::with_capacity(ids.len());
        self.handles.storage.locate(ids, &mut located)?;
        Ok(located
            .into_iter()
            .map(|object| object.location.object_id)
            .collect())
    }

    /// Consumes the sole session, checkpoints, closes and verifies one file.
    ///
    /// This is the closed, quiescent state a byte copy or a sidecar gate needs
    /// under WAL: an open or merely dropped session can leave `-wal`/`-shm` beside
    /// the database, and a sealed one cannot. One attempt; a refusal is returned.
    pub fn seal(self) -> Result<SealedStore, PersistenceError> {
        let Self {
            storage, handles, ..
        } = self;
        drop(storage);
        handles.seal()
    }
}

/// One counter that kept its meaning and changed its source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mapping {
    /// The trace names that carry it (`<n>` stands for a state ordinal).
    pub names: &'static str,
    /// The removed source.
    pub removed: &'static str,
    /// The current source.
    pub current: &'static str,
}

/// Counters whose quantity is unchanged and whose source moved.
///
/// Read with [`UNAVAILABLE_COUNTERS`]: a name is in exactly one of the two, or it
/// is harness-computed and in neither.
pub const COUNTER_MAPPINGS: &[Mapping] = &[
    Mapping {
        names: "*.inserted, *.reused, *.full_records, *.prefix_records \
                (reuse, workspace, delta, boundary, small_file, pool, footprint, pipeline, \
                history.state.<n>.inserted, history.state.<n>.save.{reused, full_records, \
                prefix_records}, delta.{reused, inserted, full_records, prefix_records} totals)",
        removed: "SaveOutcome.{inserted, reused, full_records, prefix_records}",
        current: "WriteOutcome.{inserted, reused, full_records, prefix_records}",
    },
    Mapping {
        names: "workspace.packs_created, footprint.packs_created, pipeline.packs_created, \
                history.state.<n>.save.packs_created, delta.packs_created",
        removed: "SaveOutcome.packs_created",
        current: "WriteOutcome.packs (complete immutable packs acknowledged by registration)",
    },
    Mapping {
        names: "pool.{leaves, reused_values, new_values, groups, delta_leaves, full_leaves, trials, \
                work_exceeded}, history.state.<n>.save.pool.*, delta.pool_*",
        removed: "SaveOutcome.pool (PoolCounters)",
        current: "WriteOutcome.pool (PoolCounters)",
    },
    Mapping {
        names: "pipeline.pack_bytes_written",
        removed: "SaveOutcome.pack_bytes_written",
        current: "Storage::diagnostics().pack_write_bytes, after minus before the one save \
                  (physical body bytes acknowledged by publication)",
    },
    Mapping {
        names: "lifecycle.second (begin-save step: pending canonical bytes)",
        removed: "SaveOperation::pending().1",
        current: "Save::pending_canonical_bytes()",
    },
    Mapping {
        names: "history.state.<n>.filesystem.provider.pooled.*",
        removed: "StoreProvider::pooled_read_counters()",
        current: "Reader::pooled_read_counters()",
    },
    Mapping {
        names: "history.canonical_bytes, history.canonical_objects",
        removed: "SQL over table `objects` (object_id, canonical_length)",
        current: "SQL over table `object_location` (object_id, canonical_length)",
    },
    Mapping {
        names: "g1.o1-*-presence gates (the presence oracle)",
        removed: "Store::contains(ids, scope)",
        current: "PackPersistence::locate on Handles.storage, paged at StorageCapacities.read_objects",
    },
    Mapping {
        names: "store lifecycle (create, open, begin_save, accept, finish, abort, C1 handoff, reads)",
        removed: "Store::{create, open, begin_save}, SaveOperation::{accept, finish, abort}, \
                  SaveHandoff, StoreProvider",
        current: "Handles::{create, open_writable, open_read_only} + Storage::new, \
                  Save::{accept, finish} (abort is drop), Save::sink (SaveSink), Storage::reader (Reader)",
    },
];

/// One counter or timer node with no source at the current product identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Unavailable {
    /// The trace names that are no longer emitted (`<n>` stands for a state ordinal).
    pub names: &'static str,
    /// The removed source.
    pub removed: &'static str,
    /// Why nothing current is the same quantity.
    pub reason: &'static str,
}

/// Counters this harness published against the removed engine and cannot publish now.
///
/// **Nothing here is emitted.** A golden row or gate that names one of these stays
/// in the tree unchanged; the golden row is reported unpublished and the gate is
/// reported `INCOMPLETE` by [`unavailable_gate`], never `PASS`.
pub const UNAVAILABLE_COUNTERS: &[Unavailable] = &[
    Unavailable {
        names: "footprint.pack_appends, pipeline.pack_appends, history.state.<n>.save.pack_appends, \
                delta.pack_appends",
        removed: "SaveOutcome.pack_appends",
        reason: "packs are immutable and registered whole; no append operation exists to count",
    },
    Unavailable {
        names: "reuse.presence_queries, pipeline.presence_queries, \
                history.state.<n>.save.presence_queries, delta.presence_queries",
        removed: "SaveOutcome.presence_queries",
        reason: "the save reports no presence-query count; Diagnostics.locate counts batched \
                 locator calls of every kind and is not the same quantity",
    },
    Unavailable {
        names: "reuse.commits, pool.commits, pipeline.commits, history.state.<n>.save.commits, \
                lifecycle.second (finish-empty step)",
        removed: "SaveOutcome.commits",
        reason: "the save reports no commit count; SqlWork.{commits, write_commits} is \
                 session-cumulative over every transaction of the shared session, including \
                 reservations and history, and was not defined as the save's own commits",
    },
    Unavailable {
        names: "reuse.statements, workspace.statements, footprint.statements, pipeline.statements, \
                history.state.<n>.save.statements, delta.statements",
        removed: "SaveOutcome.statements",
        reason: "the save reports no statement count; SqlWork.statements counts every prepared \
                 statement of the session including transaction controls, a different statement set",
    },
    Unavailable {
        names: "delta.prefix_selected, delta.full_losses, delta.no_candidate, delta.work_exceeded, \
                delta.{prepared_full, trials, absent_candidates, ineligible_candidates}, \
                history.state.<n>.save.delta.*",
        removed: "SaveOutcome.delta (DeltaCounters, complete for the save)",
        reason: "Save::delta_counters() is readable only while the Save exists, and \
                 Save::finish both consumes the Save and runs its final wave, where selection \
                 happens; a reading before finish excludes that wave (all of a one-batch save) \
                 and is a different quantity. WriteOutcome carries no delta counters",
    },
    Unavailable {
        names: "delta.chain_max_depth, delta.chain_{objects, edges, encoded_bytes, canonical_bytes, \
                group_decodes}, history.state.<n>.save.chain.*",
        removed: "SaveOutcome.chain (ChainCounters, complete for the save)",
        reason: "Save::chain_counters() has the same lifetime as Save::delta_counters() and \
                 excludes the final wave for the same reason",
    },
    Unavailable {
        names: "lifecycle.first (begin-save step: pending objects)",
        removed: "SaveOperation::pending().0",
        reason: "Save exposes pending canonical bytes only, not a pending object count",
    },
    Unavailable {
        names: "read.wave1_opens, read.wave2_opens, read.provider_opens, \
                history.state.<n>.filesystem.provider.connection_opens, verify.store_connection_opens",
        removed: "StoreReadCounters.opens, StoreProvider::connection_opens()",
        reason: "one session is opened by Handles and shared by every read; no per-wave or \
                 per-provider connection-open count exists",
    },
    Unavailable {
        names: "read.wave1_pages, read.ceiling, read.canonical_bytes",
        removed: "StoreReadCounters.{pages, ceiling, canonical_bytes} (also edges, max_depth)",
        reason: "Reader::read_objects returns bytes only; Diagnostics has no per-wave locator page \
                 count, retained-pack ceiling or canonical-byte count",
    },
    Unavailable {
        names: "history.state.<n>.filesystem.provider.group_decodes, verify.store_group_decodes",
        removed: "StoreProvider::group_decodes()",
        reason: "Reader exposes pooled-lane counters only; Diagnostics.prefetch_group_decodes counts \
                 dependency prefetch during a save, not a reader's ordinary-lane group decodes",
    },
    Unavailable {
        names: "pool.index_entries_before, pool.index_entries, pool.index_bytes",
        removed: "Store::{pool_index_entries, pool_index_bytes} (and content_index_*)",
        reason: "no entry count is public; Save::pooled_index_bytes is readable only before \
                 finish, so it excludes the final wave and is not the after-save figure",
    },
    Unavailable {
        names: "pipeline.profile_*_ns, pipeline.profile_reuse_repeat, pipeline.stored_records, \
                pipeline.diag_*_ns, pipeline.teardown_ns, pipeline.operation_work_ns, \
                history.state.<n>.save.{resolve_ns, reuse_repeat, resolve.*_ns, full_ns, delta_ns, \
                group_ns, place_ns, sql_ns, commit_ns}, delta.profile_*",
        removed: "SaveOutcome.profile (SaveProfile: seven disjoint accept-path buckets, \
                  ResolveProfile, DiagnosticProfile)",
        reason: "the current SaveProfile holds inclusive, overlapping codec-selection spans under \
                 some of the same field names and has no place/sql/commit/reuse/pooled/teardown \
                 fields; publishing its values under the old disjoint-bucket names would be a \
                 different quantity. operation_work_ns was accept span minus the teardown bucket \
                 and falls with it",
    },
    Unavailable {
        names: "pipeline.span_finish_child_ns",
        removed: "harness span around creating the `storage.finish` timer node",
        reason: "Save::finish takes no TimingScope, so no such node is created and there is no \
                 node-creation span to publish",
    },
    Unavailable {
        names: "diagnostic.state.<n>.C2.{allocated, apparent}_bytes, \
                diagnostic.state.<n>.C5.{allocated, apparent}_bytes",
        removed: "separate sample.sqlite (C2) and history.sqlite (C5) files",
        reason: "packs and history share one database file; neither owner has a file of its own. \
                 The shared file is published as owner `C2C5`",
    },
];

/// Timer nodes the removed engine emitted into the harness's timing tree.
///
/// `Store`, `SaveOperation` and `StoreProvider` calls took a `TimingScope` and
/// recorded a node each. No call in `layerfs-storage` or `layerfs-persistence`
/// takes a scope now, so these nodes are absent from `timing.json`. The harness
/// does not wrap the calls in scopes of its own under the same names: a
/// harness-side span around a call is a different measurement from a node the
/// product recorded inside it.
pub const UNAVAILABLE_TIMER_NODES: &[&str] = &[
    "store.create",
    "store.open",
    "storage.begin",
    "storage.begin2",
    "storage.finish",
    "storage.abort",
    "storage.wave1",
    "storage.wave2",
    "presence",
];

/// The reason text every unavailable observation carries.
pub const UNAVAILABLE: &str = "unavailable at this product identity";

/// The gate a decision takes when a counter it reads is in [`UNAVAILABLE_COUNTERS`].
///
/// The gate keeps its class, identity and limit; only its measurement is absent,
/// so it is `INCOMPLETE` — the same answer `history_retained::counter_gates` gives
/// a missing counter — and never `PASS`.
pub fn unavailable_gate(class: GateClass, id: &'static str, observed: &str, limit: &str) -> Gate {
    Gate::incomplete(
        class,
        id,
        &format!("{observed}; {UNAVAILABLE} (ops::store::UNAVAILABLE_COUNTERS)"),
        limit,
    )
}

/// The receipt note a row writes for the names it no longer publishes.
pub fn unavailable_note(names: &[&str]) -> String {
    format!(
        "unavailable_counters: {} ({UNAVAILABLE}; see ops::store::UNAVAILABLE_COUNTERS)",
        names.join(", ")
    )
}

/// The receipt note naming the persistence profile a row ran under.
pub fn profile_note() -> String {
    format!("persistence_profile: {PROFILE_TOKEN} (selected explicitly; Durable is never executed)")
}
