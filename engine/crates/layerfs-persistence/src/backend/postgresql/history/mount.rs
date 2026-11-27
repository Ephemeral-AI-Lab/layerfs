//! A Workspace mount's admission on PostgreSQL as one pipelined message
//! (#768 step B), and the statements a mount and an unmount send, prepared
//! when a session opens so neither call prepares one.
//!
//! The message, after the snapshot read (its own autocommit statement):
//!
//! 1. `BEGIN READ COMMITTED`; `SET LOCAL` search_path, synchronous_commit
//!    and lock_timeout; `set_config('layerfs.attempt', nonce, true)` and the
//!    generation fence `1/(catalog_incarnation = G)` (simple text, as the
//!    fused Commit begins).
//! 2. The session keys, all or none (`sql/postgresql/mount/lock.sql`): every
//!    key tried; any refused, the granted ones given back in the statement
//!    before it fails. A session lock outlives the transaction, so the keys
//!    are held from here whatever the transaction does.
//! 3. The intent, inserted only when the Branch exists and no live operation
//!    conflicts in its scope (`sql/postgresql/mount/insert.sql`, built from
//!    the texts the statement-by-statement registration runs), else it fails.
//! 4. `COMMIT` (simple text).
//! 5. The bound node's markers (`paths_node_markers`, fenced autocommit),
//!    only when asked: after the intent committed, on the same connection,
//!    so a removal that committed first is seen and one after it meets the
//!    intent, as the separate read after the registration did.
//!
//! Every statement of the transaction checks the attempt token: if `BEGIN`
//! failed and a statement ran on its own, it takes no key and writes no row.
//! Each request ends in its own `Sync`, so a refusal leaves the rest of the
//! transaction answering `25P02` and the `COMMIT` rolls it back; the first
//! refusal decides:
//!
//! | Request | Refusal | Outcome |
//! | --- | --- | --- |
//! | 1 | `22012` (generation moved) | `StaleStore`, no key taken |
//! | 1 | `55P03` (the lock timeout, on `history_meta`) | `Busy`, no key taken |
//! | 1 | another definite SQLSTATE | [`Admission::Released`]: no key taken |
//! | 2 | `22012` (a key refused) | [`Admission::Released`]: no key held |
//! | 2 | `22003` (another backend, §5.2 item 34) | `Unsupported`, no key taken |
//! | 2 | any other | unknown: the keys it took are unknown |
//! | 3, 4 | a definite SQLSTATE (`rows::sqlstate`) | [`Admission::Holding`]: the keys are held |
//! | 5 | a definite SQLSTATE | admitted, markers not read |
//! | any | a SQLSTATE without a definite rule, `25P02` first, or no reply | unknown, the session quarantined |
//!
//! A reply lost after the `COMMIT` went out, the markers' included, is an
//! unknown outcome although the intent may have committed. The writer is
//! quarantined and nothing is resent; the admission records the op as an
//! orphan (`Session::note_orphan`) inside that turn, and the writer's reopen
//! deletes a row left behind, with the owner release's generation-checked
//! statement, once the old backend is proven gone (`session/reopen.rs`).
//! Until then the row refuses `branch.remove` and compaction
//! (`MountsActive`). Before step B a lost markers reply was the reader's and
//! left the writer able to release the row.
//!
//! A refusal whose session lost a later reply (quarantined, its connection
//! closed, so the server holds none of its keys) is a definite `Conflict`:
//! the transaction aborted at the refusal, and the caller's separate path
//! cannot run on that session. The outcome is decided, and an unknown one
//! quarantines the session, while the session's lock is still held
//! (`Session::decided_turn`).
use super::{fused::nonce, statements};
use crate::backend::postgresql::{
    metadata::{chains, reads},
    objects::read,
    session::{
        connection::{Session, SessionKey},
        pipeline::{Refusal, Reply, Request},
        prepare::fenced,
        rows,
    },
};
use crate::backend::records::{BackendError, Record, Value};
use layerfs_history::error::HistoryError;
use postgres::error::SqlState;
use postgres::types::ToSql;
use std::sync::atomic::Ordering;

const LOCK: &str = include_str!("../../../../sql/postgresql/mount/lock.sql");
const INSERT: &str = include_str!("../../../../sql/postgresql/mount/insert.sql");

/// The refusal a transaction-pooled connection gets for a session lock.
const POOLED: &str = "transaction-pooled PostgreSQL connection";

/// One shipped statement as a subquery: without its closing `;`.
fn subquery(sql: &str) -> &str {
    let sql = sql.trim_end();
    sql.strip_suffix(';').unwrap_or(sql)
}

/// The intent insert of the message: [`INSERT`] with the Branch probe
/// (`branch_branch_by_id`, its `$1` renumbered to the message's `$2`) and
/// the conflict scan (`gc_intent_ops_branch`, unchanged) the
/// statement-by-statement registration runs, so the rule is written once.
pub(crate) fn insert_sql() -> Result<String, BackendError> {
    let branch = statements::statement("branch_branch_by_id")?;
    if branch.contains("$2") || branch.matches("$1").count() != 1 {
        return Err(BackendError::Integrity);
    }
    let conflicts = statements::statement("gc_intent_ops_branch")?;
    Ok(INSERT
        .replace("{branch}", &subquery(branch).replace("$1", "$2"))
        .replace("{conflicts}", subquery(conflicts)))
}

/// A fenced autocommit read of one schema-qualified statement.
fn qualified(key: &str, schema: &str) -> Result<String, BackendError> {
    let sql = statements::qualified(key).ok_or(BackendError::Integrity)?;
    Ok(fenced(&sql.replace("{s}", schema), schema))
}

/// The writer's statements of a mount and of the owner's release: the
/// snapshot by Branch id or by name and the message's lock, insert and
/// markers; the statement-by-statement registration a refusal falls back to
/// (the session keys taken and given back, the Branch probe, the conflict
/// scan, the intent insert); and the release's `DELETE` and its unlock.
pub(crate) fn writer_texts(schema: &str) -> Result<Vec<String>, BackendError> {
    let mut texts = Vec::new();
    for key in [
        "branch_snapshot",
        "branch_snapshot_named",
        "paths_node_markers",
    ] {
        texts.push(qualified(key, schema)?);
    }
    texts.push(LOCK.to_owned());
    texts.push(insert_sql()?);
    for key in [
        "gc_lock_session",
        "gc_unlock_session",
        "gc_intent_release_owner",
    ] {
        let sql = statements::bare(key).ok_or(BackendError::Integrity)?;
        texts.push(sql.replace("{s}", schema));
    }
    for key in [
        "branch_branch_by_id",
        "gc_intent_ops_branch",
        "gc_intent_insert",
    ] {
        texts.push(statements::statement(key)?.to_owned());
    }
    texts.push(super::release::unlock_sql(schema));
    Ok(texts)
}

/// A Workspace reader's statements at mount, each a fenced autocommit read:
/// the first demand of the root (the chain locate, the pack ranges and the
/// inode value blocks). The snapshot and the markers are the writer's.
fn reader_texts(schema: &str) -> Vec<String> {
    vec![
        fenced(&chains::ROOTS.replace("{schema}", schema), schema),
        fenced(&read::ranges_sql(schema), schema),
        fenced(&reads::values_sql(schema), schema),
    ]
}

/// The writer's statements a first create sends after its bind (#818 W4;
/// a create is an SDK write that does not take the fused Commit): the serial
/// reservation's one message (`reserve`), the Save's block allocation, pack
/// `COPY` and object rows, the bindings' insert, and the statements
/// of the Commit's write transaction (`commit::commit_branch`). Reads sent
/// as autocommit statements are not here.
pub(crate) fn create_texts(schema: &str) -> Result<Vec<String>, BackendError> {
    let mut texts = vec![
        super::reserve::text(schema),
        crate::backend::postgresql::metadata::writes::ALLOCATE.to_owned(),
        crate::backend::postgresql::objects::publish::COPY.to_owned(),
        crate::backend::postgresql::objects::publish::ROWS.to_owned(),
        // As `Mechanics::history_arrays` sends it.
        statements::arrays("hint_insert_blocks")
            .ok_or(BackendError::Integrity)?
            .replace("{s}", schema),
    ];
    for key in [
        "commit_snapshot_epoch",
        "position_next_serial",
        "commit_insert_commit",
        "lineage_parent",
        "lineage_insert_commit",
        "commit_advance_branch",
        "gc_epoch_share",
    ] {
        // As a write transaction sends it (`Mechanics::history`).
        texts.push(match statements::qualified(key) {
            Some(sql) => sql.replace("{s}", schema),
            None => statements::statement(key)?.to_owned(),
        });
    }
    Ok(texts)
}

impl Session {
    /// Prepares the writer's create statements ([`create_texts`]) in one
    /// flight: the SDK's Store open sends it (#818 W4), so a first create
    /// prepares none of them.
    pub(crate) fn prepare_sdk_writes(&self) -> Result<(), BackendError> {
        self.prepare(&create_texts(&self.settings.schema)?)?;
        // A reopened writer prepares them again (`Session::reopen`).
        self.sdk_writes.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Prepares the writer's mount and unmount statements in one flight.
    pub(crate) fn prepare_mount_writes(&self) -> Result<(), BackendError> {
        self.prepare(&writer_texts(&self.settings.schema)?)?;
        // A reopened writer prepares them again (`Session::reopen`).
        self.mount_writes.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Prepares a Workspace reader's mount statements in one flight.
    pub(crate) fn prepare_mount_reads(&self) -> Result<(), BackendError> {
        self.prepare(&reader_texts(&self.settings.schema))
    }
}

/// One mount intent's row, as `gc_intent_insert` binds it.
pub(crate) struct IntentRow {
    pub(crate) op_id: Vec<u8>,
    pub(crate) project_id: Vec<u8>,
    pub(crate) branch_id: Vec<u8>,
    pub(crate) kind: &'static str,
    pub(crate) scope_kind: &'static str,
    pub(crate) workspace: Option<String>,
    pub(crate) sandbox: Option<String>,
    pub(crate) machine: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) owner_runtime: Option<String>,
    pub(crate) owner_boot: Option<String>,
    pub(crate) state: &'static str,
    pub(crate) uncertain: i64,
    pub(crate) phase: Option<String>,
    pub(crate) evidence: Option<String>,
    pub(crate) updated_ms: i64,
}

/// How one admission message ended.
pub(crate) enum Admission {
    /// The intent committed and the keys are held. The markers' rows, their
    /// fence column removed, when they were asked for and read at the
    /// session's generation.
    Admitted(Option<Vec<Record>>),
    /// A definite refusal after the keys were granted: nothing was written
    /// and the session still holds every key.
    Holding,
    /// A definite refusal before any key was held (the lock statement
    /// refused a key and gave back its grants, or the transaction's `BEGIN`
    /// batch failed): nothing was written and no key is held.
    Released,
    /// A refusal reported as it is: `StaleStore`, `Busy` for a lock timeout
    /// in the `BEGIN` batch (the statement-by-statement registration's key
    /// statement has no lock timeout, so falling back would wait as long as
    /// the lock is held), a pooled connection, or a refusal on a session
    /// that lost a later reply. Nothing written, no key held.
    Refused(HistoryError),
    /// Unknown; the session is quarantined.
    Unknown,
}

/// What a request's refusal means, by its place in the message.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Begin,
    Lock,
    Insert,
    Commit,
    Markers,
}

/// Sends one mount admission (see the module documentation) for `row`
/// with `keys`, and with `markers` (`(node, stack)`) its markers read.
/// `Err` only when nothing was sent, or the session has no generation yet
/// (an open's own check), which the caller registers separately.
pub(crate) fn admit(
    session: &Session,
    keys: &[SessionKey],
    row: &IntentRow,
    markers: Option<(&[u8], &[u8])>,
    settle: impl FnOnce(&Admission),
) -> Result<(Admission, u64), BackendError> {
    let generation = session.incarnation.load(Ordering::SeqCst);
    if generation == 0 {
        return Err(BackendError::Integrity);
    }
    let schema = session.settings.schema.as_str();
    let nonce = nonce();
    let begin = format!(
        "{}; SELECT set_config('layerfs.attempt','{nonce}',true),\
         1/(catalog_incarnation={generation})::int FROM {schema}.history_meta WHERE id=1",
        session.settings.publish_begin()
    );
    let (ids, modes): (Vec<i64>, Vec<bool>) = keys.iter().copied().unzip();
    let insert = insert_sql()?;
    let markers_sql = qualified("paths_node_markers", schema)?;
    // The insert names its tables as the transaction's `search_path`
    // resolves them: the flight prepares a statement not yet cached inside
    // a read transaction with that `search_path`, in its own turn (one
    // round trip; none when all are cached). The backend the keys must be
    // on is read in that turn, after a quarantined writer reopened, and
    // `settle` records what the session now holds before the turn ends.
    let (admission, epoch) = session.decided_turn(|turn| {
        turn.ready()?;
        let pid = session.backend_pid.load(Ordering::SeqCst);
        let lock_params: [&(dyn ToSql + Sync); 4] = [&ids, &modes, &pid, &nonce];
        let insert_params: [&(dyn ToSql + Sync); 18] = [
            &row.project_id,
            &row.branch_id,
            &row.op_id,
            &row.kind,
            &row.scope_kind,
            &row.workspace,
            &row.sandbox,
            &row.machine,
            &row.project,
            &row.branch,
            &row.owner_runtime,
            &row.owner_boot,
            &row.state,
            &row.uncertain,
            &row.phase,
            &row.evidence,
            &row.updated_ms,
            &nonce,
        ];
        let (node, stack) = markers.unwrap_or_default();
        let markers_params: [&(dyn ToSql + Sync); 2] = [&node, &stack];
        let mut steps = vec![Step::Begin, Step::Lock, Step::Insert, Step::Commit];
        let mut requests = vec![
            Request::Simple(&begin),
            Request::Statement {
                sql: LOCK,
                params: &lock_params,
                bytes: 9 * ids.len() as u64 + 8 + 32,
            },
            Request::Statement {
                sql: &insert,
                params: &insert_params,
                bytes: 512,
            },
            Request::Simple("COMMIT"),
        ];
        if markers.is_some() {
            steps.push(Step::Markers);
            requests.push(Request::Statement {
                sql: &markers_sql,
                params: &markers_params,
                bytes: 34,
            });
        }
        let admission = turn.send(&requests, |replies, trusted| {
            let admission = decide(session, &steps, replies, trusted, generation);
            let unknown = matches!(admission, Admission::Unknown);
            (admission, unknown)
        })?;
        settle(&admission);
        Ok((admission, turn.epoch()))
    })?;
    session.count_fused(matches!(admission, Admission::Admitted(_)));
    Ok((admission, epoch))
}

/// The outcome of one message's replies: the first refusal decides (module
/// documentation). `trusted`: no reply was lost, and a partly sent message
/// was rolled back.
fn decide(
    session: &Session,
    steps: &[Step],
    replies: Vec<Result<Reply, Refusal>>,
    trusted: bool,
    generation: u64,
) -> Admission {
    if replies.len() != steps.len() {
        return Admission::Unknown;
    }
    let mut locked = false;
    let mut markers = None;
    for (step, reply) in steps.iter().zip(replies) {
        let refusal = match reply {
            Ok(Reply::Rows(found)) => {
                match step {
                    Step::Lock => {
                        let pid = found.first().and_then(|row| row.try_get::<_, i64>(0).ok());
                        // The statement checked the session's backend; a
                        // reply that names none, or another, is not a held
                        // key this session can account for.
                        if !pid.is_some_and(|pid| session.note_backend(pid)) {
                            return Admission::Unknown;
                        }
                        locked = true;
                    }
                    Step::Markers => markers = fence(&found, generation),
                    _ => {}
                }
                continue;
            }
            Ok(Reply::Messages(_)) => continue,
            Err(refusal) => refusal,
        };
        let Some(code) = refusal.code.as_ref() else {
            // No SQLSTATE: a lost reply (the session is quarantined), or a
            // request the client never sent: nothing after it went out and
            // the pipeline rolled back what did.
            let definite = !refusal.sent && refusal.error != BackendError::Unknown && trusted;
            return match (definite, step) {
                (true, Step::Markers) => Admission::Admitted(None),
                (true, _) if locked => Admission::Holding,
                (true, _) => Admission::Released,
                (false, _) => Admission::Unknown,
            };
        };
        if *code == SqlState::IN_FAILED_SQL_TRANSACTION {
            return Admission::Unknown;
        }
        let division = *code == SqlState::DIVISION_BY_ZERO;
        let range = *code == SqlState::NUMERIC_VALUE_OUT_OF_RANGE;
        // A SQLSTATE without a definite rule (a terminated backend, a
        // shutdown, an internal failure) leaves the outcome unknown.
        let definite = rows::sqlstate(code.code()) != BackendError::Unknown;
        let outcome = match step {
            Step::Begin if division => Admission::Refused(HistoryError::StaleStore),
            Step::Begin if *code == SqlState::LOCK_NOT_AVAILABLE => {
                Admission::Refused(BackendError::Busy.into())
            }
            Step::Lock if division => Admission::Released,
            Step::Lock if range => Admission::Refused(HistoryError::Unsupported(POOLED)),
            Step::Lock => Admission::Unknown,
            _ if !definite => Admission::Unknown,
            Step::Begin => Admission::Released,
            Step::Insert | Step::Commit => Admission::Holding,
            Step::Markers => Admission::Admitted(None),
        };
        // A lost later reply closed the connection: the keys went with it
        // and no separate path can run on the session.
        return match outcome {
            Admission::Holding | Admission::Released if !trusted => Admission::Refused(
                HistoryError::Conflict("mount admission refusal on a lost session"),
            ),
            outcome => outcome,
        };
    }
    Admission::Admitted(markers)
}

/// The markers' rows with their fence column checked against `generation`
/// and removed; `None` when a row cannot be read or names another
/// generation (the caller then reads the markers itself).
fn fence(found: &[postgres::Row], generation: u64) -> Option<Vec<Record>> {
    let mut records = Vec::with_capacity(found.len());
    for row in found {
        let mut record = rows::record(row).ok()?;
        match record.cells.pop() {
            Some(Value::Integer(seen)) if seen > 0 && seen as u64 == generation => {}
            _ => return None,
        }
        records.push(record);
    }
    Some(records)
}
