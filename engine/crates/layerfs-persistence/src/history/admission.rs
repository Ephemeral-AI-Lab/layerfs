//! Store-side admission for the four GC calls (spec §5).
//!
//! Admission is one Store-DB table and, on SQLite, two short
//! `BEGIN IMMEDIATE` transactions: a mount/Init/merge registers a shared
//! intent, a removal scans the intents in its scope and then writes its
//! marker in the same write transaction, and a compaction registers an
//! exclusive Store intent. A live mount refuses a removal with
//! `MountsActive`; a live operation refuses a mount or another removal with
//! `Busy`; a dead or uncertain owner is `Unresolved`.
//!
//! PostgreSQL adds transaction-scoped advisory keys to a removal's marker
//! transaction and session-scoped shared keys to a mount, so a removal on one
//! machine sees a mount on another. Its `gc_intent` rows carry the same
//! ownership evidence for `MountsActive`/`Busy`/`Unresolved` reports.

use super::rows::{cell, one, unsigned};
use super::transaction::Transaction;
use super::{branch, layerstack, params, paths, position};
use crate::backend::Engine;
use crate::history::provider::HistoryProvider;
use layerfs_history::error::{
    GcHolder, HistoryError, HistoryResult, Missing, MountHolders, OpConflict,
};
#[cfg(feature = "postgresql")]
use layerfs_history::identity::CatalogId;
use layerfs_history::identity::{BranchId, LayerStackId};
use layerfs_history::path::path_key;
use layerfs_history::records::{
    GcDeletion, GcIntent, GcIntentKind, GcScopeKind, PathRemovalKind, PathRemovalOutcome,
    PathRemovalRequest,
};
use std::time::{SystemTime, UNIX_EPOCH};

const INSERT: &str = "gc_intent_insert";
#[cfg(feature = "postgresql")]
const GET: &str = "gc_intent_get";
const RELEASE: &str = "gc_intent_release";
const UNCERTAIN: &str = "gc_intent_uncertain";
const LIST: &str = "gc_intent_list";
const OPS_BRANCH: &str = "gc_intent_ops_branch";
const OPS_PROJECT: &str = "gc_intent_ops_project";
const OPS_STORE: &str = "gc_intent_ops_store";
const ALL_BRANCH: &str = "gc_intent_all_branch";
const ALL_PROJECT: &str = "gc_intent_all_project";
const ALL_STORE: &str = "gc_intent_all_store";
#[cfg(feature = "postgresql")]
const BRANCH_STACK: &str = "gc_branch_stack_any";
const EPOCH_SHARE: &str = "gc_epoch_share";

#[cfg(feature = "postgresql")]
const LOCK_XACT_SHARED: &str = "gc_lock_xact_shared";
#[cfg(feature = "postgresql")]
const LOCK_XACT_EXCLUSIVE: &str = "gc_lock_xact_exclusive";
#[cfg(feature = "postgresql")]
const LOCK_SESSION_SHARED: &str = "gc_lock_session_shared";
#[cfg(feature = "postgresql")]
const LOCK_SESSION_EXCLUSIVE: &str = "gc_lock_session_exclusive";
#[cfg(feature = "postgresql")]
const UNLOCK_SESSION_SHARED: &str = "gc_unlock_session_shared";
#[cfg(feature = "postgresql")]
const UNLOCK_SESSION_EXCLUSIVE: &str = "gc_unlock_session_exclusive";

/// True when this provider is backed by PostgreSQL.
fn postgres(provider: &HistoryProvider) -> bool {
    #[cfg(feature = "postgresql")]
    {
        matches!(provider.engine, Engine::Postgres(_))
    }
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = provider;
        false
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn kind_of(text: &str) -> HistoryResult<GcIntentKind> {
    Ok(match text {
        "mount" => GcIntentKind::Mount,
        "init" => GcIntentKind::Init,
        "merge" => GcIntentKind::Merge,
        "removal" => GcIntentKind::Removal,
        "compaction" => GcIntentKind::Compaction,
        _ => return Err(HistoryError::Integrity("gc intent kind")),
    })
}

fn scope_of(text: &str) -> HistoryResult<GcScopeKind> {
    Ok(match text {
        "store" => GcScopeKind::Store,
        "project" => GcScopeKind::Project,
        "branch" => GcScopeKind::Branch,
        _ => return Err(HistoryError::Integrity("gc intent scope")),
    })
}

fn workspace_of(text: Option<String>) -> HistoryResult<Option<[u8; 32]>> {
    let Some(text) = text else {
        return Ok(None);
    };
    if text.len() != 64 {
        return Err(HistoryError::Integrity("gc intent workspace"));
    }
    let mut body = [0u8; 32];
    for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        let digit = |byte: u8| -> HistoryResult<u8> {
            match byte {
                b'0'..=b'9' => Ok(byte - b'0'),
                b'a'..=b'f' => Ok(byte - b'a' + 10),
                b'A'..=b'F' => Ok(byte - b'A' + 10),
                _ => Err(HistoryError::Integrity("gc intent workspace")),
            }
        };
        body[index] = (digit(pair[0])? << 4) | digit(pair[1])?;
    }
    Ok(Some(body))
}

/// Decodes the fourteen columns every intent query returns, in order:
/// op_id, kind, scope_kind, scope_id, workspace, sandbox, machine, project,
/// branch, owner_runtime, owner_boot, state, uncertain, phase.
fn intent_row(row: &crate::backend::records::Record) -> HistoryResult<GcIntent> {
    let op_id: Vec<u8> = cell(row, 0)?;
    let op_id: [u8; 16] = op_id
        .as_slice()
        .try_into()
        .map_err(|_| HistoryError::Integrity("gc intent op_id"))?;
    let state: String = cell(row, 11)?;
    let uncertain_flag: i64 = cell(row, 12)?;
    Ok(GcIntent {
        op_id,
        kind: kind_of(&cell::<String>(row, 1)?)?,
        scope_kind: scope_of(&cell::<String>(row, 2)?)?,
        scope_id: cell(row, 3)?,
        workspace: workspace_of(cell::<Option<String>>(row, 4)?)?,
        sandbox: cell(row, 5)?,
        machine: cell(row, 6)?,
        project: cell(row, 7)?,
        branch: cell(row, 8)?,
        owner_runtime: cell(row, 9)?,
        owner_boot: cell(row, 10)?,
        project_id: None,
        phase: cell(row, 13)?,
        uncertain: state == "uncertain" || uncertain_flag != 0,
    })
}

fn entity(intent: &GcIntent) -> String {
    match (&intent.project, &intent.branch, intent.scope_kind) {
        (_, Some(branch), _) => branch.clone(),
        (Some(project), None, _) => project.clone(),
        (None, None, GcScopeKind::Store) => "store".to_owned(),
        (None, None, _) => hex(&intent.scope_id),
    }
}

fn holder(intent: &GcIntent) -> GcHolder {
    GcHolder {
        kind: intent.kind,
        entity: entity(intent),
        workspace: intent.workspace.as_ref().map(|bytes| hex(bytes)),
        sandbox: intent.sandbox.clone(),
        machine: intent.machine.clone(),
        op: hex(&intent.op_id),
    }
}

fn conflict(intent: &GcIntent) -> OpConflict {
    OpConflict::new(
        intent.kind,
        entity(intent),
        intent.phase.clone(),
        hex(&intent.op_id),
        intent.uncertain,
    )
}

/// The refusal a non-empty conflict set reports: an uncertain owner is
/// `Unresolved`, a live mount is `MountsActive`, any other live operation is
/// `Busy`.
fn classify(found: &[GcIntent]) -> HistoryError {
    if let Some(dead) = found.iter().find(|intent| intent.uncertain) {
        return HistoryError::Unresolved(Box::new(conflict(dead)));
    }
    let holders: Vec<GcHolder> = found
        .iter()
        .filter(|intent| intent.kind.shared())
        .map(holder)
        .collect();
    if !holders.is_empty() {
        return HistoryError::MountsActive(Box::new(MountHolders::new(holders)));
    }
    match found.first() {
        Some(operation) => HistoryError::BusyOp(Box::new(conflict(operation))),
        None => HistoryError::Busy,
    }
}

fn query(
    tx: &Transaction<'_>,
    key: &str,
    params: Vec<crate::backend::records::Param>,
) -> HistoryResult<Vec<GcIntent>> {
    tx.query(key, params)?.iter().map(intent_row).collect()
}

fn conflicts_for(tx: &Transaction<'_>, intent: &GcIntent) -> HistoryResult<Vec<GcIntent>> {
    let shared = intent.kind.shared();
    match intent.scope_kind {
        GcScopeKind::Store => query(
            tx,
            if shared { OPS_STORE } else { ALL_STORE },
            params![intent.op_id.to_vec()],
        ),
        GcScopeKind::Project => query(
            tx,
            if shared { OPS_PROJECT } else { ALL_PROJECT },
            params![
                intent.scope_id.clone(),
                intent.project.clone(),
                intent.op_id.to_vec()
            ],
        ),
        GcScopeKind::Branch => query(
            tx,
            if shared { OPS_BRANCH } else { ALL_BRANCH },
            params![
                intent.project_id.clone(),
                intent.scope_id.clone(),
                intent.op_id.to_vec()
            ],
        ),
    }
}

fn validate_mount(tx: &Transaction<'_>, intent: &GcIntent) -> HistoryResult<()> {
    if intent.kind != GcIntentKind::Mount {
        return Ok(());
    }
    match intent.scope_kind {
        GcScopeKind::Store => Ok(()),
        GcScopeKind::Project => {
            let stack = LayerStackId::from_slice(&intent.scope_id)
                .map_err(|_| HistoryError::InvalidInput("GC project scope"))?;
            if layerstack::layer_stack(tx, stack)?.is_none() {
                return Err(HistoryError::Missing(Missing::LayerStack));
            }
            Ok(())
        }
        GcScopeKind::Branch => {
            let branch = BranchId::from_slice(&intent.scope_id)
                .map_err(|_| HistoryError::InvalidInput("GC Branch scope"))?;
            if branch::branch(tx, branch)?.is_none() {
                return Err(HistoryError::Missing(Missing::Branch));
            }
            Ok(())
        }
    }
}

/// One registration in its own write transaction.
fn register_in(tx: &Transaction<'_>, intent: &GcIntent, _postgres: bool) -> HistoryResult<()> {
    validate_mount(tx, intent)?;
    let found = conflicts_for(tx, intent)?;
    if !found.is_empty() {
        return Err(classify(&found));
    }
    let state = if intent.uncertain {
        "uncertain"
    } else {
        "live"
    };
    let uncertain = i64::from(intent.uncertain);
    tx.execute(
        INSERT,
        params![
            intent.op_id.to_vec(),
            intent.kind.as_str(),
            intent.scope_kind.as_str(),
            intent.scope_id.clone(),
            intent.workspace.map(|bytes| hex(&bytes)),
            intent.sandbox.clone(),
            intent.machine.clone(),
            intent.project.clone(),
            intent.branch.clone(),
            intent.owner_runtime.clone(),
            intent.owner_boot.clone(),
            state,
            uncertain,
            intent.phase.clone(),
            now_ms(),
        ],
    )?;
    Ok(())
}

/// The advisory keys a PostgreSQL intent names, Store key first.
#[cfg(feature = "postgresql")]
fn advisory_keys(catalog: CatalogId, intent: &GcIntent) -> Vec<(u8, Vec<u8>)> {
    let mut keys = vec![(1u8, catalog.as_slice().to_vec())];
    match intent.scope_kind {
        GcScopeKind::Store => {}
        GcScopeKind::Project => keys.push((2, intent.scope_id.clone())),
        GcScopeKind::Branch => {
            if let Some(project) = &intent.project_id {
                keys.push((2, project.clone()));
            }
            keys.push((3, intent.scope_id.clone()));
        }
    }
    keys
}

#[cfg(feature = "postgresql")]
fn advisory_key(catalog: CatalogId, kind: u8, id: &[u8]) -> i64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/advisory/v1\0");
    hasher.update(catalog.as_slice());
    hasher.update(&[kind]);
    hasher.update(id);
    let digest = hasher.finalize();
    let mut head = [0u8; 8];
    head.copy_from_slice(&digest.as_bytes()[..8]);
    i64::from_be_bytes(head)
}

#[cfg(feature = "postgresql")]
fn xact_lock(tx: &Transaction<'_>, key: i64, shared: bool) -> HistoryResult<()> {
    let found = tx.query(
        if shared {
            LOCK_XACT_SHARED
        } else {
            LOCK_XACT_EXCLUSIVE
        },
        params![key],
    )?;
    if found.is_empty() {
        return Err(if shared {
            HistoryError::MountsActive(Box::new(MountHolders::new(Vec::new())))
        } else {
            HistoryError::Busy
        });
    }
    Ok(())
}

/// PostgreSQL: the Store key a removal shares with every mount, taken first.
#[cfg(feature = "postgresql")]
fn lock_store_shared(tx: &Transaction<'_>, catalog: CatalogId) -> HistoryResult<()> {
    xact_lock(tx, advisory_key(catalog, 1, catalog.as_slice()), true)
}

/// PostgreSQL: one project or Branch key.
#[cfg(feature = "postgresql")]
fn lock_scope(
    tx: &Transaction<'_>,
    catalog: CatalogId,
    kind: u8,
    id: &[u8],
    shared: bool,
) -> HistoryResult<()> {
    xact_lock(tx, advisory_key(catalog, kind, id), shared)
}

fn epoch_share(tx: &Transaction<'_>, postgres: bool, stack: LayerStackId) -> HistoryResult<()> {
    if !postgres {
        return Ok(());
    }
    let found = tx.query(EPOCH_SHARE, [stack.as_slice()])?;
    if found.len() != 1 {
        return Err(HistoryError::Conflict("project epoch"));
    }
    Ok(())
}

fn release_in(tx: &Transaction<'_>, op_id: &[u8; 16]) -> HistoryResult<()> {
    tx.execute(RELEASE, [op_id.as_slice()])?;
    Ok(())
}

fn uncertain_in(tx: &Transaction<'_>, op_id: &[u8; 16]) -> HistoryResult<()> {
    tx.execute(UNCERTAIN, params![now_ms(), op_id.as_slice()])?;
    Ok(())
}

#[cfg(feature = "postgresql")]
fn get_in(tx: &Transaction<'_>, op_id: &[u8; 16]) -> HistoryResult<Option<GcIntent>> {
    one(tx, GET, [op_id.as_slice()], intent_row)
}

/// The PostgreSQL advisory keys one release unlocks, and whether its intent
/// was shared.
#[cfg(feature = "postgresql")]
struct ReleaseKeys {
    keys: Vec<(u8, Vec<u8>)>,
    shared: bool,
}

#[cfg(feature = "postgresql")]
fn keys_for_release(
    tx: &Transaction<'_>,
    catalog: CatalogId,
    intent: &GcIntent,
) -> HistoryResult<ReleaseKeys> {
    let mut intent = intent.clone();
    if intent.scope_kind == GcScopeKind::Branch && intent.project_id.is_none() {
        intent.project_id = one(tx, BRANCH_STACK, [intent.scope_id.as_slice()], |row| {
            cell::<Vec<u8>>(row, 0)
        })?;
    }
    Ok(ReleaseKeys {
        keys: advisory_keys(catalog, &intent),
        shared: intent.kind.shared(),
    })
}

/// Registers one shared (`mount`, `init`, `merge`) or exclusive
/// (`compaction`) intent.
pub(crate) fn register(provider: &HistoryProvider, intent: &GcIntent) -> HistoryResult<()> {
    match &provider.engine {
        Engine::Sqlite(_) => provider.write(|tx| register_in(tx, intent, false)),
        #[cfg(feature = "postgresql")]
        Engine::Postgres(session) => {
            let keys = advisory_keys(provider.catalog_id, intent);
            let shared = intent.kind.shared();
            session_lock(session, provider.catalog_id, &keys, shared)?;
            let result = session.run(true, |statements| {
                register_in(&Transaction::new(statements), intent, true)
            });
            if result.is_err() {
                let _ = session_unlock(session, provider.catalog_id, &keys, shared);
            }
            result
        }
    }
}

/// Deletes one intent, and on PostgreSQL releases the session keys it held.
pub(crate) fn release(provider: &HistoryProvider, op_id: &[u8; 16]) -> HistoryResult<()> {
    match &provider.engine {
        Engine::Sqlite(_) => provider.write(|tx| release_in(tx, op_id)),
        #[cfg(feature = "postgresql")]
        Engine::Postgres(session) => {
            let found = provider.write(|tx| {
                let Some(intent) = get_in(tx, op_id)? else {
                    return Ok(None);
                };
                release_in(tx, op_id)?;
                Ok(Some(keys_for_release(tx, provider.catalog_id, &intent)?))
            })?;
            if let Some(ReleaseKeys { keys, shared }) = found {
                let _ = session_unlock(session, provider.catalog_id, &keys, shared);
            }
            Ok(())
        }
    }
}

/// Marks one intent uncertain; its owner should settle it later.
pub(crate) fn mark_uncertain(provider: &HistoryProvider, op_id: &[u8; 16]) -> HistoryResult<()> {
    provider.write(|tx| uncertain_in(tx, op_id))
}

/// `branch.remove`: scan the Branch scope and write the marker in one write
/// transaction.
pub(crate) fn remove_branch(
    provider: &HistoryProvider,
    branch: BranchId,
    removed_ms: i64,
    intent: &GcIntent,
) -> HistoryResult<()> {
    let postgres = postgres(provider);
    let catalog = provider.catalog_id;
    let _ = catalog;
    provider.write(|tx| {
        let record = branch::branch(tx, branch)?.ok_or(HistoryError::Missing(Missing::Branch))?;
        #[cfg(feature = "postgresql")]
        if postgres {
            lock_store_shared(tx, catalog)?;
            lock_scope(tx, catalog, 2, record.stack.as_slice(), true)?;
            let found = query(
                tx,
                ALL_BRANCH,
                params![
                    Some(record.stack.as_slice().to_vec()),
                    branch.as_slice().to_vec(),
                    intent.op_id.to_vec()
                ],
            )?;
            if !found.is_empty() {
                return Err(classify(&found));
            }
            lock_scope(tx, catalog, 3, branch.as_slice(), false)?;
        }
        epoch_share(tx, postgres, record.stack)?;
        let found = query(
            tx,
            ALL_BRANCH,
            params![
                Some(record.stack.as_slice().to_vec()),
                branch.as_slice().to_vec(),
                intent.op_id.to_vec()
            ],
        )?;
        if !found.is_empty() {
            return Err(classify(&found));
        }
        super::removal::mark_branch_removed(tx, branch, removed_ms)
    })
}

/// `project.delete`: the same-home retry classification, then admission and
/// the marker in one write transaction.
pub(crate) fn delete_stack(
    provider: &HistoryProvider,
    stack: LayerStackId,
    deleted_ms: i64,
    intent: &GcIntent,
) -> HistoryResult<GcDeletion> {
    let postgres = postgres(provider);
    let catalog = provider.catalog_id;
    let _ = catalog;
    provider.write(|tx| {
        if let Some(recorded) = layerstack::stack_deleted_ms(tx, stack)? {
            return Ok(GcDeletion::Retired {
                deleted_ms: recorded,
            });
        }
        let live = layerstack::layer_stack(tx, stack)?;
        let Some(record) = live else {
            return Ok(match layerstack::stack_deleted_ms(tx, stack)? {
                Some(recorded) => GcDeletion::Retired {
                    deleted_ms: recorded,
                },
                None => GcDeletion::Absent,
            });
        };
        #[cfg(feature = "postgresql")]
        if postgres {
            lock_store_shared(tx, catalog)?;
            let found = query(
                tx,
                ALL_PROJECT,
                params![
                    stack.as_slice().to_vec(),
                    Some(record.name.as_str().to_owned()),
                    intent.op_id.to_vec()
                ],
            )?;
            if !found.is_empty() {
                return Err(classify(&found));
            }
            lock_scope(tx, catalog, 2, stack.as_slice(), false)?;
        }
        epoch_share(tx, postgres, stack)?;
        let found = query(
            tx,
            ALL_PROJECT,
            params![
                stack.as_slice().to_vec(),
                Some(record.name.as_str().to_owned()),
                intent.op_id.to_vec()
            ],
        )?;
        if !found.is_empty() {
            return Err(classify(&found));
        }
        layerstack::mark_stack_deleted(tx, stack, deleted_ms)?;
        Ok(GcDeletion::Marked { deleted_ms })
    })
}

/// `project.remove_file`: admission first, then the marker and epoch bump in
/// the same write transaction.
pub(crate) fn remove_path(
    provider: &HistoryProvider,
    request: &PathRemovalRequest,
    intent: &GcIntent,
) -> HistoryResult<PathRemovalOutcome> {
    let postgres = postgres(provider);
    let catalog = provider.catalog_id;
    let _ = catalog;
    provider.write(|tx| {
        if layerstack::layer_stack(tx, request.stack)?.is_none() {
            return Err(HistoryError::Missing(Missing::LayerStack));
        }
        #[cfg(feature = "postgresql")]
        if postgres {
            lock_store_shared(tx, catalog)?;
            let found = query(
                tx,
                ALL_PROJECT,
                params![
                    request.stack.as_slice().to_vec(),
                    intent.project.clone(),
                    intent.op_id.to_vec()
                ],
            )?;
            if !found.is_empty() {
                return Err(classify(&found));
            }
            lock_scope(tx, catalog, 2, request.stack.as_slice(), false)?;
        }
        epoch_share(tx, postgres, request.stack)?;
        let found = query(
            tx,
            ALL_PROJECT,
            params![
                request.stack.as_slice().to_vec(),
                intent.project.clone(),
                intent.op_id.to_vec()
            ],
        )?;
        if !found.is_empty() {
            return Err(classify(&found));
        }
        let key = path_key(&request.path);
        let stored = one(
            tx,
            "paths_marker",
            [request.stack.as_slice(), key.as_slice()],
            |row| {
                Ok((
                    cell::<Vec<u8>>(row, 0)?,
                    unsigned(row, 1)?,
                    cell::<i64>(row, 2)?,
                ))
            },
        )?;
        let stack_high = position::stack_high_water(tx, request.stack)?;
        if let Some((stored_path, frontier, removed)) = &stored {
            if stored_path != &request.path {
                return Err(HistoryError::Integrity("path marker key"));
            }
            if stack_high <= *frontier {
                return Ok(PathRemovalOutcome {
                    kind: PathRemovalKind::UpToDate,
                    stack: request.stack,
                    path: stored_path.clone(),
                    frontier_serial: *frontier,
                    removed_ms: *removed,
                });
            }
        }
        let frontier = position::high_water(tx)?;
        paths::mark(
            tx,
            request.stack,
            &request.path,
            frontier,
            request.removed_ms,
        )?;
        let advanced = tx.execute("paths_advance_epoch", [request.stack.as_slice()])?;
        if advanced != 1 {
            return Err(HistoryError::Conflict("project epoch"));
        }
        Ok(PathRemovalOutcome {
            kind: PathRemovalKind::Removed,
            stack: request.stack,
            path: request.path.clone(),
            frontier_serial: frontier,
            removed_ms: request.removed_ms,
        })
    })
}

/// Every live intent recorded at one scope, for `store.status` and tests.
pub(crate) fn intents(
    provider: &HistoryProvider,
    scope_kind: GcScopeKind,
    scope_id: &[u8],
) -> HistoryResult<Vec<GcIntent>> {
    provider.read(|tx| query(tx, LIST, params![scope_kind.as_str(), scope_id.to_vec()]))
}

#[cfg(feature = "postgresql")]
fn session_lock(
    session: &std::sync::Arc<crate::backend::postgresql::session::connection::Session>,
    catalog: CatalogId,
    keys: &[(u8, Vec<u8>)],
    shared: bool,
) -> HistoryResult<()> {
    let mut taken: Vec<(u8, Vec<u8>)> = Vec::new();
    for (kind, id) in keys {
        let key = advisory_key(catalog, *kind, id);
        let found = session.read_statement(|statements| {
            Transaction::new(statements).query(
                if shared {
                    LOCK_SESSION_SHARED
                } else {
                    LOCK_SESSION_EXCLUSIVE
                },
                params![key],
            )
        })?;
        if found.is_empty() {
            let _ = session_unlock(session, catalog, &taken, shared);
            return Err(if shared {
                HistoryError::MountsActive(Box::new(MountHolders::new(Vec::new())))
            } else {
                HistoryError::Busy
            });
        }
        taken.push((*kind, id.clone()));
    }
    Ok(())
}

#[cfg(feature = "postgresql")]
fn session_unlock(
    session: &std::sync::Arc<crate::backend::postgresql::session::connection::Session>,
    catalog: CatalogId,
    keys: &[(u8, Vec<u8>)],
    shared: bool,
) -> HistoryResult<()> {
    for (kind, id) in keys {
        let key = advisory_key(catalog, *kind, id);
        let _ = session.read_statement(|statements| {
            Transaction::new(statements).query(
                if shared {
                    UNLOCK_SESSION_SHARED
                } else {
                    UNLOCK_SESSION_EXCLUSIVE
                },
                params![key],
            )
        })?;
    }
    Ok(())
}
