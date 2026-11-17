//! Store-side admission for the four GC calls (spec §5).
//!
//! Admission is one Store-DB table and, on SQLite, two short
//! `BEGIN IMMEDIATE` transactions: a mount/Init/merge registers a shared
//! intent, a removal scans the intents in its scope and then writes its
//! marker in the same write transaction, and a compaction registers an
//! exclusive Store intent. A live mount refuses a removal with
//! `MountsActive`; a live operation refuses a mount or another removal with
//! `Busy`; a dead, uncertain owner is `Unresolved`, while a dead, certain one
//! is adopted.
//!
//! PostgreSQL adds transaction-scoped advisory keys to a removal's marker
//! transaction and session-scoped shared keys to a mount, so a removal on one
//! machine sees a mount on another. Its `gc_intent` rows carry the same
//! ownership evidence for `MountsActive`/`Busy`/`Unresolved` reports.

use super::rows::{cell, one, unsigned};
use super::transaction::Transaction;
use super::{branch, layerstack, params, paths, position};
#[cfg(feature = "postgresql")]
use crate::backend::records::Param;
use crate::backend::Engine;
use crate::history::provider::HistoryProvider;
use layerfs_history::error::{
    GcHolder, HistoryError, HistoryResult, Missing, MountHolders, OpConflict,
};
#[cfg(feature = "postgresql")]
use layerfs_history::identity::CatalogId;
use layerfs_history::identity::{BranchId, LayerStackId};
use layerfs_history::path::trace_key;
use layerfs_history::records::{
    GcDeletion, GcIntent, GcIntentKind, GcScopeKind, PathRemovalKind, PathRemovalOutcome,
    PathRemovalRequest,
};
use std::time::{SystemTime, UNIX_EPOCH};

mod removal;
pub(crate) use removal::{delete_stack, intents, remove_branch, remove_path};

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
const EPOCH_LOCK: &str = "gc_epoch_lock";

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
        intent.scope_kind,
        entity(intent),
        intent.phase.clone(),
        hex(&intent.op_id),
        intent.uncertain,
    )
}

/// This process's PID-namespace inode, when the platform names one. A pid
/// is only meaningful inside the namespace that wrote it.
pub(crate) fn current_namespace() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let target = std::fs::read_link("/proc/self/ns/pid").ok()?;
        let text = target.to_str()?;
        text.strip_prefix("pid:[")?.strip_suffix(']')?.parse().ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// This boot's id, parsed from the kernel's textual form.
fn current_boot() -> Option<u128> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
        u128::from_str_radix(text.trim().replace('-', "").as_str(), 16).ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// The state letter and start time `/proc/<pid>/stat` names, when it exists.
fn process_stat(pid: u32) -> Option<(u8, u64)> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let rest = text.rsplit_once(')')?.1.trim_start();
        let mut fields = rest.split_whitespace();
        let state = fields.next()?.as_bytes().first().copied()?;
        // The fields after `state` are ppid, pgrp, session, tty_nr, tpgid,
        // flags, minflt, cminflt, majflt, cmajflt, utime, stime, cutime,
        // cstime, priority, nice, num_threads, itrealvalue, starttime.
        let start = fields.nth(18)?.parse().ok()?;
        Some((state, start))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// The pid, start time and PID namespace an `owner_runtime` token names.
fn parse_runtime(text: &str) -> Option<(u32, u64, Option<u64>)> {
    let body = text.strip_prefix("pid:")?;
    let (facts, namespace) = match body.split_once("@ns:") {
        Some((facts, namespace)) => (facts, Some(namespace.parse().ok()?)),
        None => (body, None),
    };
    let (pid, start) = facts.split_once('@')?;
    Some((pid.parse().ok()?, start.parse().ok()?, namespace))
}

/// The boot id an `owner_boot` token names, before its unique suffix.
fn parse_boot(text: &str) -> Option<u128> {
    u128::from_str_radix(text.strip_prefix("boot:")?.split(':').next()?, 16).ok()
}

/// This machine's owner evidence, captured once per admission: this boot and
/// this process's PID namespace. Read once and compared per intent, so the
/// check is bounded (one small read per admission, one `/proc` stat per
/// same-namespace owner) and never scans the process table.
pub(crate) struct OwnerView {
    boot: Option<u128>,
    namespace: Option<u64>,
}

impl OwnerView {
    pub(crate) fn now() -> Self {
        Self {
            boot: current_boot(),
            namespace: current_namespace(),
        }
    }

    /// True only when this process can prove `intent`'s owner is gone.
    ///
    /// A `machine.stop` or provider SIGKILL leaves the intent row behind, and
    /// the recorded `(pid, start, namespace)` is meaningful only in its own
    /// kernel:
    /// - another boot of a local SQLite Store cannot still run: a SQLite Store
    ///   is admitted only by processes on the machine that holds the file, so
    ///   after that machine reboots every recorded owner is gone and its
    ///   intents are adopted;
    /// - another boot of a shared PostgreSQL Store may be a different machine
    ///   that still runs the owner, and a different PID namespace on this boot
    ///   may belong to a live Sandbox this process cannot inspect soundly
    ///   (`/proc` may be a namespace view, and another user's processes may be
    ///   unreadable). Both stay conservatively live: never adopted, still
    ///   reported as a holder with its `sandbox`, `machine` and `op`, until
    ///   its owner releases it or an operator settles it with
    ///   `gc_mark_uncertain` and `gc_release`.
    fn dead(&self, intent: &GcIntent, shared_store: bool) -> bool {
        let (Some(runtime), Some(boot)) = (
            intent.owner_runtime.as_deref(),
            intent.owner_boot.as_deref(),
        ) else {
            return false;
        };
        let Some((pid, start, Some(namespace))) = parse_runtime(runtime) else {
            return false;
        };
        let Some(recorded_boot) = parse_boot(boot) else {
            return false;
        };
        let Some(here) = self.boot else {
            return false;
        };
        if recorded_boot != here {
            return !shared_store;
        }
        // Another namespace is not probed: this `/proc` may be one namespace's
        // view and another user's processes may be unreadable, so absence
        // there is not proof of death. Conservative and bounded: only this
        // process's own namespace is checked.
        if self.namespace != Some(namespace) {
            return false;
        }
        match process_stat(pid) {
            Some((state, observed)) => observed != start || matches!(state, b'Z' | b'X' | b'x'),
            None => true,
        }
    }

    /// True only when an observer may settle `intent` as dead: it is certain
    /// and its owner is provably gone. An uncertain intent is never settled
    /// by an observer, even when its owner is dead: the owning call (or an
    /// operator's `gc_release`) clears it, so the scope reports `Unresolved`
    /// (§5.1's dead-uncertain cell) instead of deleting the evidence that the
    /// operation may not have finished.
    fn settleable(&self, intent: &GcIntent, shared_store: bool) -> bool {
        !intent.uncertain && self.dead(intent, shared_store)
    }
}

/// Adopts the rows whose owner is provably dead and that are not uncertain:
/// their operation cannot still be running, so their intent is deleted and
/// does not hold the scope. The uncertain rows stay and the caller reports
/// them as `Unresolved`.
fn settle_dead(
    tx: &Transaction<'_>,
    found: Vec<GcIntent>,
    shared_store: bool,
    owners: &OwnerView,
) -> HistoryResult<Vec<GcIntent>> {
    let mut live = Vec::with_capacity(found.len());
    for intent in found {
        if owners.settleable(&intent, shared_store) {
            release_in(tx, &intent.op_id)?;
        } else {
            live.push(intent);
        }
    }
    Ok(live)
}

/// The refusal a non-empty conflict set reports: an uncertain owner is
/// `Unresolved`, a live mount is `MountsActive`, any other live operation is
/// `Busy`.
fn classify(found: &[GcIntent]) -> HistoryError {
    if let Some(dead) = found.iter().find(|intent| intent.uncertain) {
        return HistoryError::Unresolved(Box::new(conflict(dead)));
    }
    // A mount is a holder; an Init or a merge is another operation and is
    // `Busy` with its phase, never `MountsActive` (§5).
    let holders: Vec<GcHolder> = found
        .iter()
        .filter(|intent| intent.kind == GcIntentKind::Mount)
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

/// One registration inside the caller's write transaction.
pub(super) fn register_in(
    tx: &Transaction<'_>,
    intent: &GcIntent,
    postgres: bool,
) -> HistoryResult<()> {
    validate_mount(tx, intent)?;
    let owners = OwnerView::now();
    let found = settle_dead(tx, conflicts_for(tx, intent)?, postgres, &owners)?;
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

/// The advisory keys a PostgreSQL intent names, Store key first, each with the
/// mode the registration takes it in.
///
/// The Store key is shared for every project- or Branch-scoped intent, even an
/// exclusive removal: `project.delete` must not exclude mounts of other
/// projects, and `branch.remove` excludes only its own Branch. Only a
/// Store-scoped exclusive intent (compaction) takes the Store key exclusively.
/// A Branch-scoped removal keeps the project key shared and takes only the
/// Branch key exclusively; a project-scoped one takes the project key
/// exclusively.
#[cfg(feature = "postgresql")]
fn advisory_keys(catalog: CatalogId, intent: &GcIntent) -> Vec<(u8, Vec<u8>, bool)> {
    let scope_shared = intent.kind.shared();
    let store_shared = intent.scope_kind != GcScopeKind::Store || scope_shared;
    let mut keys = vec![(1u8, catalog.as_slice().to_vec(), store_shared)];
    match intent.scope_kind {
        GcScopeKind::Store => {}
        GcScopeKind::Project => keys.push((2, intent.scope_id.clone(), scope_shared)),
        GcScopeKind::Branch => {
            if let Some(project) = &intent.project_id {
                // A Branch-scope removal excludes the Branch, not the project.
                keys.push((2, project.clone(), true));
            }
            keys.push((3, intent.scope_id.clone(), scope_shared));
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
const EMPTY_OP: [u8; 16] = [0u8; 16];

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

/// The intents one advisory-key scope names, in the vocabulary the shared
/// lock failure needs to report a holder.
#[cfg(feature = "postgresql")]
fn scope_intents(tx: &Transaction<'_>, kind: u8, id: &[u8]) -> HistoryResult<Vec<GcIntent>> {
    match kind {
        1 => query(tx, ALL_STORE, params![EMPTY_OP.to_vec()]),
        2 => query(
            tx,
            ALL_PROJECT,
            params![id.to_vec(), Option::<String>::None, EMPTY_OP.to_vec()],
        ),
        3 => query(
            tx,
            ALL_BRANCH,
            params![Option::<Vec<u8>>::None, id.to_vec(), EMPTY_OP.to_vec()],
        ),
        _ => Ok(Vec::new()),
    }
}

/// One live advisory-key holder as `pg_locks`/`pg_stat_activity` report it:
/// pid, `client_addr` and `backend_start`.
#[cfg(feature = "postgresql")]
type AdvisoryHolder = (i64, Option<String>, Option<String>);

/// The live session holding one advisory key.
#[cfg(feature = "postgresql")]
fn advisory_holder(tx: &Transaction<'_>, key: i64) -> HistoryResult<Option<AdvisoryHolder>> {
    // PostgreSQL-only text: `statement()` resolves it through the PG set.
    let rows = tx.query("gc_advisory_holder", params![key])?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    Ok(Some((
        row.get::<i64>(0)?,
        row.get::<Option<String>>(1)?,
        row.get::<Option<String>>(2)?,
    )))
}

/// The refusal a named lock reports: the live intents at that scope (a mount
/// is `MountsActive`, any other operation `Busy` with `{entity,phase,op}`),
/// or, when no row exists, the backend the server names.
#[cfg(feature = "postgresql")]
fn lock_refusal(
    tx: &Transaction<'_>,
    kind: u8,
    id: &[u8],
    key: i64,
) -> HistoryResult<HistoryError> {
    let found = scope_intents(tx, kind, id)?;
    if !found.is_empty() {
        return Ok(classify(&found));
    }
    let scope = match kind {
        2 => GcScopeKind::Project,
        3 => GcScopeKind::Branch,
        _ => GcScopeKind::Store,
    };
    let mut conflict = OpConflict::new(
        if kind == 1 {
            GcIntentKind::Compaction
        } else {
            GcIntentKind::Removal
        },
        scope,
        if kind == 1 {
            "store".to_owned()
        } else {
            hex(id)
        },
        None,
        String::new(),
        false,
    );
    if let Some((pid, client_addr, backend_start)) = advisory_holder(tx, key)? {
        conflict = conflict.with_backend(Some(pid), client_addr, backend_start);
    }
    Ok(HistoryError::BusyOp(Box::new(conflict)))
}

/// PostgreSQL: the Store key a removal shares with every mount, taken first.
#[cfg(feature = "postgresql")]
fn lock_store_shared(tx: &Transaction<'_>, catalog: CatalogId) -> HistoryResult<()> {
    let key = advisory_key(catalog, 1, catalog.as_slice());
    match xact_lock(tx, key, true) {
        Ok(()) => Ok(()),
        Err(_) => Err(lock_refusal(tx, 1, catalog.as_slice(), key)?),
    }
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
    let key = advisory_key(catalog, kind, id);
    match xact_lock(tx, key, shared) {
        Ok(()) => Ok(()),
        Err(_) => Err(lock_refusal(tx, kind, id, key)?),
    }
}

/// PostgreSQL: take the project epoch row a removal marker transaction will
/// bump, `FOR UPDATE`, as the first statement that reads it in that
/// transaction.
///
/// `FOR SHARE` here and `UPDATE ... epoch = epoch + 1` later was a classic
/// lock upgrade: two removals in one project both held the shared row lock and
/// then both waited to upgrade it, so PostgreSQL aborted one with `40P01`
/// (deadlock). The exclusive lock orders them on the row: the loser waits for
/// the winner's commit, then reads (and bumps) the value the winner wrote.
/// SQLite's single `BEGIN IMMEDIATE` writer already excludes another marker
/// transaction, so there the statement only proves the row exists.
fn epoch_hold(tx: &Transaction<'_>, postgres: bool, stack: LayerStackId) -> HistoryResult<()> {
    if !postgres {
        return Ok(());
    }
    let found = tx.query(EPOCH_LOCK, [stack.as_slice()])?;
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

/// The PostgreSQL advisory keys one release unlocks, each with the mode it
/// was taken in.
#[cfg(feature = "postgresql")]
struct ReleaseKeys {
    keys: Vec<(u8, Vec<u8>, bool)>,
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
    })
}

/// The exclusive Store key a compaction takes before it settles a leftover
/// op, so two compactions can never settle the same `gc_op` at once. The
/// registration that follows takes its own key level; dropping the claim
/// leaves that one held for `gc_release`. On SQLite this is a no-op: the lane
/// and the single write transaction are the exclusion.
#[cfg(feature = "postgresql")]
pub(crate) struct StoreClaim<'a> {
    provider: &'a HistoryProvider,
    held: bool,
}

#[cfg(feature = "postgresql")]
impl HistoryProvider {
    /// Takes this Store's exclusive advisory key without registering an intent
    /// row, so a compaction can settle a leftover op under the key.
    pub(crate) fn claim_store_exclusive(&self) -> HistoryResult<StoreClaim<'_>> {
        if let Engine::Postgres(session) = &self.engine {
            let keys = [(1u8, self.catalog_id.as_slice().to_vec(), false)];
            session_lock(session, self.catalog_id, &keys)?;
            return Ok(StoreClaim {
                provider: self,
                held: true,
            });
        }
        Ok(StoreClaim {
            provider: self,
            held: false,
        })
    }
}

#[cfg(feature = "postgresql")]
impl StoreClaim<'_> {
    /// Releases the claim's one key level, leaving the level a registration
    /// took in the meantime held.
    fn unlock(&mut self) {
        if self.held {
            if let Engine::Postgres(session) = &self.provider.engine {
                let keys = [(1u8, self.provider.catalog_id.as_slice().to_vec(), false)];
                let _ = session_unlock(session, self.provider.catalog_id, &keys);
            }
        }
        self.held = false;
    }
}

#[cfg(feature = "postgresql")]
impl Drop for StoreClaim<'_> {
    fn drop(&mut self) {
        self.unlock();
    }
}

#[cfg(feature = "postgresql")]
impl HistoryProvider {
    /// Whether `pid` holds this Store's exclusive advisory key, as `pg_locks`
    /// reports it: the §8 lost-`COMMIT` guard. A recorded owner that still
    /// holds the key is a live compaction, so the next call reports `Busy` and
    /// never terminates it; one that does not hold it is a lost reply, which
    /// the recovery may terminate.
    pub(crate) fn store_key_held_by(&self, pid: i64) -> HistoryResult<bool> {
        let Engine::Postgres(session) = &self.engine else {
            return Ok(false);
        };
        let key = advisory_key(self.catalog_id, 1, self.catalog_id.as_slice());
        session.run(false, |tx| {
            let rows = tx.query(
                "SELECT (count(*) > 0)::int FROM pg_locks l \
                  WHERE l.locktype='advisory' AND l.granted AND l.objsubid=1 \
                    AND l.classid::bigint = (($1::bigint >> 32) & 4294967295) \
                    AND l.objid::bigint = ($1::bigint & 4294967295) \
                    AND l.pid=$2",
                vec![Param::I64(key), Param::I64(pid)],
            )?;
            let held = rows
                .first()
                .map(|row| row.get::<i64>(0))
                .transpose()?
                .unwrap_or(0);
            Ok(held != 0)
        })
    }
}

/// Registers one shared (`mount`, `init`, `merge`) or exclusive
/// (`compaction`) intent.
pub(crate) fn register(provider: &HistoryProvider, intent: &GcIntent) -> HistoryResult<()> {
    match &provider.engine {
        Engine::Sqlite(_) => provider.write(|tx| register_in(tx, intent, false)),
        #[cfg(feature = "postgresql")]
        Engine::Postgres(session) => {
            let keys = advisory_keys(provider.catalog_id, intent);
            session_lock(session, provider.catalog_id, &keys)?;
            let result = session.run(true, |statements| {
                register_in(&Transaction::new(statements), intent, true)
            });
            if result.is_err() {
                let _ = session_unlock(session, provider.catalog_id, &keys);
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
            if let Some(ReleaseKeys { keys }) = found {
                let _ = session_unlock(session, provider.catalog_id, &keys);
            }
            Ok(())
        }
    }
}

/// Marks one intent uncertain; its owner should settle it later.
pub(crate) fn mark_uncertain(provider: &HistoryProvider, op_id: &[u8; 16]) -> HistoryResult<()> {
    provider.write(|tx| uncertain_in(tx, op_id))
}

/// The refusal a transaction-pooled connection gets for a session lock.
#[cfg(feature = "postgresql")]
const POOLED: &str = "transaction-pooled PostgreSQL connection";

#[cfg(feature = "postgresql")]
fn session_lock(
    session: &std::sync::Arc<crate::backend::postgresql::session::connection::Session>,
    catalog: CatalogId,
    keys: &[(u8, Vec<u8>, bool)],
) -> HistoryResult<()> {
    let mut taken: Vec<(u8, Vec<u8>, bool)> = Vec::new();
    for (kind, id, shared) in keys {
        let key = advisory_key(catalog, *kind, id);
        let found = session.read_statement(|statements| {
            Transaction::new(statements).query(
                if *shared {
                    LOCK_SESSION_SHARED
                } else {
                    LOCK_SESSION_EXCLUSIVE
                },
                params![key],
            )
        })?;
        if found.is_empty() {
            let _ = session_unlock(session, catalog, &taken);
            return Err(session_lock_refusal(session, *kind, id, key, *shared)?);
        }
        // Spec §5.2 item 34: a session advisory lock only means presence if the
        // same backend keeps it. The lock statement returns the backend that
        // took it; a transaction-pooling proxy hands the next statement to a
        // different one, so mounts and compaction are `Unsupported` there.
        let pid: i64 = found[0].get(1)?;
        if !session.note_backend(pid) {
            let _ = session_unlock(session, catalog, &taken);
            return Err(HistoryError::Unsupported(POOLED));
        }
        taken.push((*kind, id.clone(), *shared));
    }
    Ok(())
}

/// The refusal a failed session key reports: the intents at its scope, or the
/// backend the server names; never a holderless `MountsActive`/`Busy` when a
/// holder is available.
#[cfg(feature = "postgresql")]
fn session_lock_refusal(
    session: &std::sync::Arc<crate::backend::postgresql::session::connection::Session>,
    kind: u8,
    id: &[u8],
    key: i64,
    shared: bool,
) -> HistoryResult<HistoryError> {
    // A read transaction, not `Mode::Statement`: the intent queries are the
    // search-path ones the write path uses.
    let found = session.run(false, |statements| {
        scope_intents(&Transaction::new(statements), kind, id)
    })?;
    if !found.is_empty() {
        return Ok(classify(&found));
    }
    if shared {
        return Ok(HistoryError::MountsActive(Box::new(MountHolders::new(
            Vec::new(),
        ))));
    }
    let scope = match kind {
        2 => GcScopeKind::Project,
        3 => GcScopeKind::Branch,
        _ => GcScopeKind::Store,
    };
    let mut conflict = OpConflict::new(
        if kind == 1 {
            GcIntentKind::Compaction
        } else {
            GcIntentKind::Removal
        },
        scope,
        if kind == 1 {
            "store".to_owned()
        } else {
            hex(id)
        },
        None,
        String::new(),
        false,
    );
    if let Some((pid, client_addr, backend_start)) = session.run(false, |statements| {
        advisory_holder(&Transaction::new(statements), key)
    })? {
        conflict = conflict.with_backend(Some(pid), client_addr, backend_start);
    }
    Ok(HistoryError::BusyOp(Box::new(conflict)))
}

#[cfg(feature = "postgresql")]
fn session_unlock(
    session: &std::sync::Arc<crate::backend::postgresql::session::connection::Session>,
    catalog: CatalogId,
    keys: &[(u8, Vec<u8>, bool)],
) -> HistoryResult<()> {
    for (kind, id, shared) in keys {
        let key = advisory_key(catalog, *kind, id);
        let found = session.read_statement(|statements| {
            Transaction::new(statements).query(
                if *shared {
                    UNLOCK_SESSION_SHARED
                } else {
                    UNLOCK_SESSION_EXCLUSIVE
                },
                params![key],
            )
        })?;
        if let Some(row) = found.first() {
            let pid: i64 = row.get(1)?;
            if !session.note_backend(pid) {
                return Err(HistoryError::Unsupported(POOLED));
            }
        }
    }
    Ok(())
}
