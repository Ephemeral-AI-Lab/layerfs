//! Versioned C5 companion to the existing history construction driver.
//! State 1 is genesis; every later state stages, commits and publishes one Layer.
//!
//! **Ported 2026-10-10.** `layerfs_history::sqlite` (a standalone catalogue
//! database) was removed by `bd9ededba`; the catalogue is now the
//! `HistoryCatalog` a `layerfs_persistence::Handles` holds in the **same**
//! database as the packs. The history driver therefore publishes into the Store
//! it grows ([`RetainedHistory::create_in`], [`RetainedHistory::publish_in`],
//! [`verify_in`]), and there is no `history.sqlite` beside `sample.sqlite` any
//! more. The path-taking entry points ([`RetainedHistory::create`],
//! [`RetainedHistory::publish`], [`verify`]) keep their signatures and open a
//! Store of their own at the path they are handed. Every open selects
//! Disposable / WAL / `synchronous = OFF` explicitly through `ops::store::config`.
//!
//! [`storage_gate`] is **unchanged**: it still sums `sample.sqlite` and
//! `history.sqlite` against the ceilings recorded for the removed two-file
//! layout. With one combined Store it finds no `history.sqlite` and reports
//! `INCOMPLETE`; the old byte ceilings are not continued under the new identity.
use std::path::Path;

use layerfs_content::{filesystem::root::profile_id, InodeScope, ObjectId};
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitId, CommitStagedOutcome, CommitStagedRequest,
    ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryError, HistoryName,
    HistoryResult, LayerId, LayerStackId, StackInitialization, StageRequest, WorkspaceId,
};
use layerfs_persistence::Handles;
use layerfs_storage::port::PersistenceError;
use layerfs_storage::StoragePolicy;

/// Explicit opt-in; historical C2-only operations retain their original behavior.
pub fn enabled() -> bool {
    matches!(std::env::var("LAYERFS_HISTORY_RETAINED_CATALOG").as_deref(), Ok("1" | "2" | "3" | "4"))
}

/// The explicitly selected compound workload identity; absent means legacy v1.
pub fn version() -> &'static str {
    match std::env::var("LAYERFS_HISTORY_RETAINED_CATALOG").as_deref() {
        Ok("4") => "v4",
        Ok("3") => "v3",
        Ok("2") => "v2",
        _ => "v1",
    }
}

/// Publishes the complete canonical identity counters from the actual closed C2.
pub fn record_counters(
    path: &Path,
    trace: &mut crate::support::trace::TraceWriter,
) -> Result<(), super::OpError> {
    use crate::support::trace::Kind;
    let read = || -> Result<(i64, i64), rusqlite::Error> {
        let db = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        // Table `objects` became `object_location` (`object_id` is its primary key).
        db.query_row("SELECT COUNT(*), SUM(n) FROM (SELECT object_id, MAX(canonical_length) n FROM object_location GROUP BY object_id)",
            [], |row| Ok((row.get(0)?, row.get(1)?)))
    };
    let (objects, bytes) = read().map_err(|error| super::OpError::Io(error.to_string()))?;
    trace.write_number(
        Kind::Counter,
        "history.canonical_bytes",
        i128::from(bytes),
        "bytes",
        "distinct at-run C2 identities",
    )?;
    trace.write_number(
        Kind::Counter,
        "history.canonical_objects",
        i128::from(objects),
        "objects",
        "distinct at-run C2 identities",
    )?;
    Ok(())
}

/// Compound O3 replaces the unrelated 217-row golden-table lookup.
/// Missing counters are incomplete; v1/v2 retain the old strict constants,
/// while v3 pins the independent same-producer reference.
pub fn counter_gates(
    row: crate::workload::history::Row,
    values: &[(String, i128)],
) -> Vec<crate::gates::Gate> {
    use crate::{
        gates::{self, Gate, GateClass},
        workload::history::Row,
    };
    let (bytes, objects) = match row {
        Row::Stride10 if version() != "v1" => (380_559_460, 51_689),
        Row::Stride10 => (380_921_328, 52_032),
        Row::Stride3 if matches!(version(), "v3" | "v4") => (589_480_854, 73_447),
        Row::Stride3 => (589_423_458, 73_476),
        Row::Stride1 if matches!(version(), "v3" | "v4") => (871_337_620, 104_618),
        Row::Stride1 => (871_588_115, 104_705),
    };
    let expected = [
        ("history.canonical_bytes", bytes),
        ("history.canonical_objects", objects),
    ];
    expected
        .iter()
        .map(
            |(key, expected)| match values.iter().find(|(name, _)| name == key) {
                None => Gate::incomplete(
                    GateClass::Correctness,
                    "g1.o3-pinned-counters",
                    key,
                    "complete canonical identity pins",
                ),
                Some((_, value)) => gates::require(
                    GateClass::Correctness,
                    "g1.o3-pinned-counters",
                    value == expected,
                    &format!("{key}={value}"),
                    &format!("{expected}"),
                ),
            },
        )
        .collect()
}

/// The history binding every `history.*` Store is created and opened under.
///
/// The Store always carries a catalogue now, so the row's Store is bound with this
/// config whether or not the compound profile publishes into it.
pub fn config() -> HistoryCatalogConfig {
    // v4 changes the storage gate, not the retained-history workload identity.
    let identity_version = if version() == "v4" { "v3" } else { version() };
    HistoryCatalogConfig {
        binding_key: format!("layerfs/issue286/retained-history/{identity_version}").into_bytes(),
        incarnation: match identity_version { "v3" => 3, "v2" => 2, _ => 1 },
        cursor_key: [0x28; 32],
    }
}

fn stack() -> LayerStackId {
    LayerStackId::from_authority([0x28; 16])
}
fn branch(ordinal: usize) -> BranchId {
    let mut body = [0x29; 16];
    body[15] = ordinal as u8;
    BranchId::from_authority(body)
}
fn workspace() -> WorkspaceId {
    WorkspaceId::from_authority([0x2a; 32]).expect("fixed nonzero authority")
}

/// C5 identity returned by one timed state publication.
#[derive(Clone, Copy, Debug)]
pub struct RetainedState {
    /// Genesis has no Commit; all subsequent states have one.
    pub commit: Option<CommitId>,
    /// Every selected state has a persisted Layer.
    pub layer: LayerId,
}

/// A persistence refusal, as the history error class it is.
fn persistence_error(error: PersistenceError) -> HistoryError {
    match error {
        PersistenceError::Busy => HistoryError::Busy,
        PersistenceError::Uncertain => HistoryError::UnknownOutcome,
        PersistenceError::Missing => {
            HistoryError::Missing(layerfs_history::error::Missing::Catalog)
        }
        PersistenceError::BackendUnavailable => HistoryError::Unsupported("persistence backend"),
        PersistenceError::Refused { .. } | PersistenceError::Malformed => {
            HistoryError::Integrity("retained Store refused")
        }
    }
}

/// One fresh catalog, owned by the measured invocation rather than preparation.
pub struct RetainedHistory {
    /// The Store this value created for itself ([`RetainedHistory::create`]), or
    /// `None` when the chain publishes into a Store its caller holds.
    owned: Option<Handles>,
    head: LayerId,
    next_ordinal: usize,
}

impl RetainedHistory {
    /// Creates a Store at `path`, then the genesis Layer and Branch in its catalog.
    pub fn create(path: &Path, scope: InodeScope, root: ObjectId) -> HistoryResult<Self> {
        let handles = Handles::create(
            super::store::config(path),
            StoragePolicy::frozen_default(),
            &config(),
        )
        .map_err(persistence_error)?;
        let mut created = Self::create_in(&handles.history, scope, root)?;
        created.owned = Some(handles);
        Ok(created)
    }

    /// Creates the genesis Layer and Branch in a catalog the caller holds, inside
    /// state 1's timer.
    pub fn create_in(
        catalog: &dyn HistoryCatalog,
        scope: InodeScope,
        root: ObjectId,
    ) -> HistoryResult<Self> {
        let initialized = catalog.initialize_layerstack(&StackInitialization {
            stack: stack(),
            name: HistoryName::new("retained")?,
            scope: scope.object(),
            profile: profile_id(),
            genesis_root: root,
        })?;
        catalog.fork(&ForkRequest {
            stack: stack(),
            branch: branch(1),
            name: HistoryName::new("state-1")?,
            source: ForkSource::Layer(initialized.head_layer),
        })?;
        Ok(Self {
            owned: None,
            head: initialized.head_layer,
            next_ordinal: 2,
        })
    }

    /// Returns genesis without creating a duplicate first-state Commit.
    pub fn genesis(&self) -> RetainedState {
        RetainedState {
            commit: None,
            layer: self.head,
        }
    }

    /// Saves one retained state into the Store this value created for itself.
    pub fn publish(&mut self, ordinal: usize, root: ObjectId) -> HistoryResult<RetainedState> {
        let Self {
            owned,
            head,
            next_ordinal,
        } = self;
        let handles = owned
            .as_ref()
            .ok_or(HistoryError::InvalidInput("retained catalog owner"))?;
        advance(&handles.history, head, next_ordinal, ordinal, root)
    }

    /// Saves one retained state into a catalog the caller holds.
    pub fn publish_in(
        &mut self,
        catalog: &dyn HistoryCatalog,
        ordinal: usize,
        root: ObjectId,
    ) -> HistoryResult<RetainedState> {
        advance(
            catalog,
            &mut self.head,
            &mut self.next_ordinal,
            ordinal,
            root,
        )
    }
}

/// Saves one real retained state through public C5 transitions, without retry.
fn advance(
    catalog: &dyn HistoryCatalog,
    head: &mut LayerId,
    next_ordinal: &mut usize,
    ordinal: usize,
    root: ObjectId,
) -> HistoryResult<RetainedState> {
    {
        if ordinal != *next_ordinal {
            return Err(HistoryError::InvalidInput("retained state order"));
        }
        // C5 Branch bases are immutable. Each publication forks the prior Layer.
        catalog.fork(&ForkRequest {
            stack: stack(),
            branch: branch(ordinal),
            name: HistoryName::new(&format!("state-{ordinal}"))?,
            source: ForkSource::Layer(*head),
        })?;
        let old = catalog
            .branch_snapshot(branch(ordinal))?
            .ok_or(HistoryError::Integrity("retained Branch"))?;
        let stage = catalog.stage_changes(&StageRequest {
            workspace: workspace(),
            branch: branch(ordinal),
            expected_head: old.branch.head_commit,
            expected_base: old.branch.base_layer,
            expected_root: old.effective_root,
            construction_base_root: old.effective_root,
            intended_commit_base: old.branch.base_layer,
            candidate_root: root,
            profile: old.profile,
            scope: old.scope,
            generation: ordinal as u64,
        })?;
        let commit = match catalog.commit_staged(&CommitStagedRequest {
            workspace: workspace(),
            token: stage.token,
        })? {
            CommitStagedOutcome::Committed(commit) => commit,
            _ => {
                return Err(HistoryError::Integrity(
                    "retained state requires a new Commit",
                ))
            }
        };
        let layer = match catalog.add_layer(&AddLayerRequest {
            stack: stack(),
            branch: branch(ordinal),
            commit: commit.id,
            expected_stack_head: *head,
            expected_branch_base: old.branch.base_layer,
        })? {
            AddLayerOutcome::Added(layer) => layer,
            _ => {
                return Err(HistoryError::Integrity(
                    "retained state requires a new Layer",
                ))
            }
        };
        *head = layer.id;
        *next_ordinal += 1;
        Ok(RetainedState {
            commit: Some(commit.id),
            layer: layer.id,
        })
    }
}

/// Reopens the Store at `path` read-only and queries EVERY selected root through
/// public C5 APIs.
/// This proves C5 custody against the performance roots; independent O1 pins
/// and the corpus tree/byte oracle remain separate mandatory checks.
pub fn verify(path: &Path, scope: InodeScope, roots: &[ObjectId]) -> HistoryResult<usize> {
    let settings = config();
    let handles = Handles::open_read_only(
        super::store::config(path),
        &settings.binding_key,
        settings.cursor_key,
    )
    .map_err(persistence_error)?;
    verify_in(&handles.history, scope, roots)
}

/// Queries EVERY selected root through a catalog the caller holds.
pub fn verify_in(
    catalog: &dyn HistoryCatalog,
    scope: InodeScope,
    roots: &[ObjectId],
) -> HistoryResult<usize> {
    let first = *roots
        .first()
        .ok_or(HistoryError::InvalidInput("retained roots"))?;
    let genesis = LayerId::derive(stack(), None, first);
    let mut parent = None;
    let mut last_commit = None;
    let mut last_base = genesis;
    for (index, root) in roots.iter().copied().enumerate() {
        let id = LayerId::derive(stack(), parent, root);
        let layer = catalog
            .layer(id)?
            .ok_or(HistoryError::Integrity("retained Layer"))?;
        if layer.root != root || layer.stack != stack() || layer.parent != parent {
            return Err(HistoryError::Integrity("retained Layer context"));
        }
        let source = if index == 0 {
            None
        } else {
            let base = parent.expect("non-genesis has a parent");
            let commit_id = CommitId::derive(root, None, base);
            let commit = catalog
                .commit(commit_id)?
                .ok_or(HistoryError::Integrity("retained Commit"))?;
            if commit.root != root
                || commit.stack != stack()
                || commit.parent.is_some()
                || commit.base_layer != base
            {
                return Err(HistoryError::Integrity("retained Commit context"));
            }
            last_commit = Some(commit_id);
            last_base = base;
            Some(commit_id)
        };
        if layer.source_commit != source || layer.source_branch != source.map(|_| branch(index + 1))
        {
            return Err(HistoryError::Integrity("retained publication source"));
        }
        parent = Some(id);
    }
    let snapshot = catalog
        .branch_snapshot(branch(roots.len()))?
        .ok_or(HistoryError::Integrity("retained Branch"))?;
    let held_stack = catalog
        .layer_stack(stack())?
        .ok_or(HistoryError::Integrity("retained stack"))?;
    if snapshot.effective_root != *roots.last().expect("nonempty")
        || snapshot.branch.head_commit != last_commit
        || snapshot.branch.base_layer != last_base
        || snapshot.scope != scope.object()
        || snapshot.profile != profile_id()
        || held_stack.head_layer != parent.expect("nonempty")
        || catalog.stage(workspace())?.is_some()
    {
        return Err(HistoryError::Integrity("retained final context"));
    }
    Ok(roots.len())
}

/// Strict compound storage gate on the closed, newly created at-run files.
/// Embedded indexes are included once in their owning database allocation.
pub fn storage_gate(directory: &Path, row: crate::workload::history::Row) -> crate::gates::Gate {
    use crate::gates::{self, Gate, GateClass};
    use crate::workload::history::Row;
    use std::os::unix::fs::MetadataExt;
    let ceiling = match (version(), row) {
        ("v4", Row::Stride10) => 54_278_964,
        ("v4", Row::Stride3) => 70_427_034,
        ("v4", Row::Stride1) => 92_342_273,
        (_, Row::Stride10) => 49_344_512,
        (_, Row::Stride3) => 64_024_576,
        (_, Row::Stride1) => 83_947_520,
    };
    let id = match version() {
        "v4" => "g1.o6-total-retained-within-110pct-v016-v4",
        "v3" => "g1.o6-total-retained-below-v016-v3",
        "v2" => "g1.o6-total-retained-below-v016-v2",
        _ => "g1.o6-total-retained-below-v016-v1",
    };
    let limit = format!("exclusive C2+C5 allocated bytes < {ceiling}");
    let mut total = 0u64;
    for owner in ["sample.sqlite", "history.sqlite"] {
        let path = directory.join(owner);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                return Gate::incomplete(
                    GateClass::Resource,
                    id,
                    &format!("{owner}: {error}"),
                    &limit,
                )
            }
        };
        if !metadata.is_file() || metadata.nlink() != 1 {
            return Gate::ineligible(
                GateClass::Resource,
                id,
                &format!("{owner}: nonexclusive file"),
                &limit,
            );
        }
        let Some(next) = metadata
            .blocks()
            .checked_mul(512)
            .and_then(|n| total.checked_add(n))
        else {
            return Gate::incomplete(GateClass::Resource, id, "allocation overflow", &limit);
        };
        total = next;
    }
    gates::require(
        GateClass::Resource,
        id,
        total < ceiling,
        &format!("{total} B"),
        &limit,
    )
}

/// Independent O1 ledger gate. The candidate trace never supplies expected roots.
pub fn root_pin_gate(row: crate::workload::history::Row, roots: &[ObjectId]) -> crate::gates::Gate {
    use crate::gates::{Gate, GateClass};
    let id = "g1.o1-state-root";
    let limit = "every retained root equals the prospectively sealed independent reference pin";
    let read = || -> Result<Vec<ObjectId>, String> {
        let path =
            std::env::var("LAYERFS_HISTORY_ROOT_PINS").map_err(|_| "root pin ledger absent")?;
        let expected = std::env::var("LAYERFS_HISTORY_ROOT_PINS_SHA256")
            .map_err(|_| "root pin ledger seal absent")?;
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        if crate::workload::digest::hex(&crate::workload::digest::sha256(&bytes)) != expected {
            return Err("root pin ledger seal mismatch".into());
        }
        let text = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
        let mut lines = text.lines();
        let header = if version() != "v1" {
            "layerfs-history-root-pins-v2\t6b22835dd57d76ea53bd44561a68f50e0aab756f"
        } else {
            "layerfs-history-root-pins-v1\t2f07f1f37af3e06a92a00880c68882b3c91923ef"
        };
        if lines.next() != Some(header)
        {
            return Err("unregistered independent reference source".into());
        }
        let mut pins = Vec::new();
        for (index, line) in lines.enumerate() {
            let (ordinal, root) = line.split_once('\t').ok_or("root pin row shape")?;
            if ordinal.parse::<usize>().ok() != Some(index + 1) {
                return Err("root pin ledger order".into());
            }
            pins.push(root.parse().map_err(|_| "root pin identity")?);
        }
        if pins.len() != row.states() {
            return Err("root pin ledger cardinality".into());
        }
        Ok(pins)
    };
    match read() {
        Err(error) => Gate::incomplete(GateClass::Correctness, id, &error, limit),
        Ok(pins) => crate::gates::require(
            GateClass::Correctness,
            id,
            pins == roots,
            &format!("{} independent roots; observed {}", pins.len(), roots.len()),
            limit,
        ),
    }
}
