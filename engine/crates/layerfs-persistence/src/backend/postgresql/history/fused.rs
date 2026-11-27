//! A fused Workspace Commit on PostgreSQL (#768 stage 2): one transaction
//! sent as one pipelined message, one round trip (one more when a statement
//! is not yet prepared on the connection).
//!
//! The requests, in order:
//!
//! 1. `BEGIN READ COMMITTED`; `SET LOCAL` search_path, synchronous_commit and
//!    lock_timeout; `set_config('layerfs.attempt', nonce, true)` and the
//!    generation fence `1/(catalog_incarnation = G)` (simple text).
//! 2. The pack bodies: `INSERT ... FROM unnest`, binary (only with packs),
//!    one statement per run of at most one ordinary pack's bytes
//!    ([`pack_statements`]).
//! 3. The attempt check; `store_lane FOR UPDATE`; the barrier lock and
//!    unlock (simple text).
//! 4. The Branch at the expected head and base (the `branch_snapshot`
//!    filters), its root as expected, and `project_epoch.epoch` kept in
//!    `set_config('layerfs.epoch', .., true)`: one statement, one snapshot.
//! 5. The publication's rows (`publish_rows`, checked; only with rows).
//! 6. The binding hint blocks (only with hints).
//! 7. The next serial and the Commit insert with its attempt, one statement.
//! 8. The Commit's lineage, the parent's line read in SQL.
//! 9. The Branch advance, exactly one row.
//! 10. `project_epoch FOR SHARE`, equal to the kept epoch.
//! 11. `COMMIT` (simple text).
//!
//! Every statement checks the attempt token, a transaction-local setting
//! only this transaction has: if `BEGIN` failed and a statement ran on its
//! own, the check fails and the statement (the only one in its implicit
//! transaction) writes nothing. Each request ends in its own `Sync`, so a
//! refused statement leaves the rest answering `25P02` and the `COMMIT`
//! rolls the transaction back; the first refusal decides:
//!
//! | Request | Refusal | Outcome |
//! | --- | --- | --- |
//! | 1 | `22012` (generation moved) | `StaleStore` |
//! | 3 | `55P03`, `40P01` (lock wait) | `Busy` |
//! | 4 | `22012` (Branch moved, absent or retired; root differs) | race |
//! | 4 | `22003` (no `project_epoch` row) | `Conflict("project epoch")` |
//! | 5 | `22012` (an object already stored: a lost identity) | race |
//! | 5 | `22003` (a value block or signature refused) | `Integrity` |
//! | 7 | `22012` (the Commit identity exists) | race |
//! | 8, 9 | `22012` (no parent line; the Branch moved) | race |
//! | 10 | `22012` (the epoch moved or its row went) | `Conflict("project epoch")` |
//! | any | another SQLSTATE with a definite rule (`rows::sqlstate`) | that refusal |
//! | any | a SQLSTATE without one, `25P02` first, or no reply | `UnknownOutcome`, quarantined |
//!
//! A race refusal wrote nothing and is answered [`Fused::Race`]: the caller
//! continues on the separate path, once, and never sends this message again.
//! An unknown outcome is never followed by that path, and neither is a race
//! refusal whose later replies were lost: that session is quarantined, so
//! the race is answered `Conflict` (definite, nothing written) instead. The
//! outcome is decided, and an unknown one quarantines the session, while
//! the session's lock is still held (`Session::pipeline_judged`).
//!
//! A refusal or cancel after the barrier's lock and before its unlock leaves
//! the key held at session level (#776 P6), which the `COMMIT` does not end:
//! on the failure path only, before the verdict and any fallback, the
//! session releases such a leftover (`barrier_leftover`, one more round
//! trip). An accepted message runs no such scan.
use crate::backend::error::BackendError;
use crate::backend::postgresql::objects::publish_flight::{
    bytes, optional, pack_statements, row_values, Value, PACK, ROWS,
};
use crate::backend::postgresql::session::{
    attempt::nonce,
    connection::Session,
    pipeline::{Reply, Request},
    profile::barrier_leftover,
    rows,
};
use crate::history::Fused;
use crate::storage::blocks::hints::Block;
use layerfs_history::error::HistoryError;
use layerfs_history::records::{CommitOutcome, CommitRecord, CommitRequest};
use layerfs_storage::port::Publication;
use postgres::error::SqlState;
use postgres::types::ToSql;
use std::sync::atomic::Ordering;

const HEAD: &str = include_str!("../../../../sql/postgresql/fused/head.sql");
pub(crate) const HINTS: &str = include_str!("../../../../sql/postgresql/fused/hints.sql");
const COMMIT: &str = include_str!("../../../../sql/postgresql/fused/commit.sql");
const LINEAGE_PARENT: &str = include_str!("../../../../sql/postgresql/fused/lineage_parent.sql");
const LINEAGE_BASE: &str = include_str!("../../../../sql/postgresql/fused/lineage_base.sql");
const ADVANCE: &str = include_str!("../../../../sql/postgresql/fused/advance.sql");
const EPOCH: &str = include_str!("../../../../sql/postgresql/fused/epoch.sql");

/// What a request's refusal means, by its place in the message.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    Begin,
    Packs,
    Lane,
    Head,
    Rows,
    Hints,
    Commit,
    Lineage,
    Advance,
    Epoch,
    End,
}

/// The requests of one message: each one's step, and for a statement its
/// text, values and bound bytes.
#[derive(Default)]
struct Message<'a> {
    steps: Vec<Step>,
    order: Vec<Option<usize>>,
    statements: Vec<(String, Vec<Value<'a>>, u64)>,
}

impl<'a> Message<'a> {
    fn simple(&mut self, step: Step) {
        self.steps.push(step);
        self.order.push(None);
    }
    fn statement(&mut self, step: Step, sql: String, values: Vec<Value<'a>>, bound: u64) {
        self.steps.push(step);
        self.order.push(Some(self.statements.len()));
        self.statements.push((sql, values, bound));
    }
}

/// Makes the fused Commit; see the module documentation.
pub(crate) fn commit(
    session: &Session,
    tail: Option<&Publication>,
    request: &CommitRequest,
    record: CommitRecord,
    blocks: &[Block],
) -> Result<CommitOutcome, Fused> {
    let generation = session.incarnation.load(Ordering::SeqCst);
    if generation == 0 {
        // An open's own check has not set the fence: never fused.
        return Err(Fused::Race);
    }
    let schema = session.settings.schema.as_str();
    let qualified = |sql: &str| sql.replace("{s}", schema);
    let nonce = nonce();
    let begin = format!(
        "{}; SELECT set_config('layerfs.attempt','{nonce}',true),\
         1/(catalog_incarnation={generation})::int FROM {schema}.history_meta WHERE id=1",
        session.settings.publish_begin()
    );
    let lane = format!(
        "SELECT 1/(current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM '{nonce}')::int; {}",
        crate::backend::postgresql::session::profile::Settings::lane_row(schema)
    );
    let attempt: Value<'static> = Box::new(nonce.clone());
    let mut message = Message::default();
    message.simple(Step::Begin);
    let tail = tail.filter(|batch| {
        !(batch.packs.is_empty()
            && batch.objects.is_empty()
            && batch.inode_values.is_empty()
            && batch.signatures.is_empty()
            && batch.release_ordinals.is_none())
    });
    if let Some(batch) = tail.filter(|batch| !batch.packs.is_empty()) {
        for (values, bound) in pack_statements(&batch.packs)? {
            message.statement(Step::Packs, qualified(PACK), with(values, &nonce), bound);
        }
    }
    message.simple(Step::Lane);
    let (branch, stack, base) = (
        request.branch.as_slice(),
        record.stack.as_slice(),
        request.expected_base.as_slice(),
    );
    let head = request.expected_head.map(|id| id.to_bytes().to_vec());
    message.statement(
        Step::Head,
        qualified(HEAD),
        vec![
            bytes(branch),
            bytes(stack),
            bytes(base),
            optional(head.as_deref()),
            bytes(request.expected_root.as_bytes()),
            Box::new(nonce.clone()),
        ],
        17 + 17 + 33 + 33 + 32 + 32,
    );
    if let Some(batch) = tail {
        let (values, bound) = row_values(batch)?;
        let mut values = with(values, &nonce);
        values.push(Box::new(false));
        message.statement(Step::Rows, qualified(ROWS), values, bound);
    }
    if !blocks.is_empty() {
        let groups: Vec<i64> = blocks.iter().map(|block| block.group).collect();
        let bodies: Vec<Vec<u8>> = blocks.iter().map(|block| block.body.clone()).collect();
        let bound = bodies.iter().map(|body| body.len() as u64 + 8).sum();
        message.statement(
            Step::Hints,
            qualified(HINTS),
            vec![Box::new(groups), Box::new(bodies), Box::new(nonce.clone())],
            bound,
        );
    }
    // The 17 row values, then the attempt (#776 R1): the serial is the
    // statement's own `next_history_serial`.
    let mut row = crate::history::commit_insert_params(&record, 0)?;
    let commit_attempt = row
        .pop()
        .ok_or(HistoryError::Integrity("Commit row values"))?;
    row.truncate(17);
    let mut values: Vec<Value<'static>> = row.into_iter().map(boxed).collect();
    values.push(boxed(commit_attempt));
    values.push(Box::new(nonce.clone()));
    message.statement(Step::Commit, qualified(COMMIT), values, 512);
    let id = record.id.to_bytes().to_vec();
    let (lineage, from) = match record.parent {
        Some(parent) => (LINEAGE_PARENT, parent.to_bytes().to_vec()),
        None => (LINEAGE_BASE, base.to_vec()),
    };
    message.statement(
        Step::Lineage,
        qualified(lineage),
        vec![bytes(&id), bytes(&from), Box::new(nonce.clone())],
        66,
    );
    message.statement(
        Step::Advance,
        qualified(ADVANCE),
        vec![
            bytes(&id),
            bytes(branch),
            bytes(base),
            optional(head.as_deref()),
            Box::new(nonce.clone()),
        ],
        17 + 33 + 33 + 33,
    );
    message.statement(
        Step::Epoch,
        qualified(EPOCH),
        vec![bytes(stack), attempt],
        17,
    );
    message.simple(Step::End);
    let refs: Vec<Vec<&(dyn ToSql + Sync)>> = message
        .statements
        .iter()
        .map(|(_, values, _)| values.iter().map(|value| value.as_ref()).collect())
        .collect();
    let mut requests = Vec::with_capacity(message.steps.len());
    for (step, slot) in message.steps.iter().zip(&message.order) {
        requests.push(match (slot, step) {
            (Some(at), _) => Request::Statement {
                sql: &message.statements[*at].0,
                params: &refs[*at],
                bytes: message.statements[*at].2,
            },
            (None, Step::Begin) => Request::Simple(&begin),
            (None, Step::Lane) => Request::Simple(&lane),
            (None, _) => Request::Simple("COMMIT"),
        });
    }
    let steps = &message.steps;
    // Decided while the session's lock is held: an unknown outcome
    // quarantines it before any other caller can use the connection.
    // A refused or cancelled message may have stopped between the barrier's
    // lock and unlock (#776 P6): its failure path releases a leftover key.
    let release = barrier_leftover(schema);
    let outcome = session
        .pipeline_judged(&requests, &release, |replies, trusted| {
            let outcome = decided(decide(steps, &replies, trusted), trusted);
            let unknown = matches!(outcome, Err(Decision::Unknown));
            (outcome, unknown)
        })
        .map_err(|error| Fused::History(error.into()))?;
    session.count_fused(matches!(outcome, Ok(())));
    match outcome {
        Ok(()) => Ok(CommitOutcome::Committed {
            record: Box::new(record),
            displaced: None,
        }),
        Err(Decision::Race) => Err(Fused::Race),
        Err(Decision::Refused(error)) => Err(Fused::History(error)),
        Err(Decision::Unknown) => Err(Fused::History(HistoryError::UnknownOutcome)),
    }
}

/// A race refusal on a session that is no longer trusted (a later reply of
/// the message was lost and the session quarantined): the separate path
/// would start on a writer that answers every call unknown, so the race is
/// answered as the definite refusal it is. The refusal is the server's
/// reply to a statement after an acknowledged `BEGIN`, inside the
/// transaction: that transaction aborted there, so neither its `COMMIT` nor
/// the end of the connection can have applied anything.
fn decided(outcome: Result<(), Decision>, trusted: bool) -> Result<(), Decision> {
    match outcome {
        Err(Decision::Race) if !trusted => Err(Decision::Refused(HistoryError::Conflict(
            "fused race refusal on a lost session",
        ))),
        other => other,
    }
}

/// The values of one statement followed by the attempt token.
fn with<'a>(mut values: Vec<Value<'a>>, nonce: &str) -> Vec<Value<'a>> {
    values.push(Box::new(nonce.to_owned()));
    values
}

fn boxed(param: crate::backend::seam::Param) -> Value<'static> {
    use crate::backend::seam::Param;
    match param {
        Param::I64(value) => Box::new(value),
        Param::Text(value) => Box::new(value),
        Param::Bytes(value) => Box::new(value),
        Param::OptionalBytes(value) => Box::new(value),
        Param::OptionalText(value) => Box::new(value),
    }
}

/// How a fused message ended when it did not commit.
#[derive(Debug)]
enum Decision {
    Race,
    Refused(HistoryError),
    Unknown,
}

/// The outcome of one flight's replies: the first refusal decides (module
/// documentation); `Ok` only when every request, `COMMIT` included,
/// answered without one. `rolled_back`: the session is trusted (no reply
/// lost; a partly sent flight was rolled back).
fn decide(
    steps: &[Step],
    replies: &[Result<Reply, crate::backend::postgresql::session::pipeline::Refusal>],
    rolled_back: bool,
) -> Result<(), Decision> {
    if replies.len() != steps.len() {
        return Err(Decision::Unknown);
    }
    for (step, reply) in steps.iter().zip(replies) {
        let Err(refusal) = reply else { continue };
        let Some(code) = refusal.code.as_ref() else {
            // No SQLSTATE: a lost reply (`Unknown`; the session is
            // quarantined), or a request the client never sent (a definite
            // client refusal: unencodable, or after one that was). Nothing
            // after it went out, and the pipeline rolled back what did; only
            // if that ROLLBACK failed is the outcome unknown.
            return Err(match refusal.error {
                error if !refusal.sent && error != BackendError::Unknown && rolled_back => {
                    Decision::Refused(error.into())
                }
                _ => Decision::Unknown,
            });
        };
        let division = *code == SqlState::DIVISION_BY_ZERO;
        let range = *code == SqlState::NUMERIC_VALUE_OUT_OF_RANGE;
        return Err(match step {
            Step::Begin if division => Decision::Refused(HistoryError::StaleStore),
            Step::Head if division => Decision::Race,
            Step::Head if range => Decision::Refused(HistoryError::Conflict("project epoch")),
            Step::Rows if division => Decision::Race,
            Step::Rows if range => Decision::Refused(HistoryError::Integrity("persistence")),
            Step::Commit | Step::Lineage | Step::Advance if division => Decision::Race,
            Step::Epoch if division => Decision::Refused(HistoryError::Conflict("project epoch")),
            _ if *code == SqlState::IN_FAILED_SQL_TRANSACTION => Decision::Unknown,
            _ => match rows::sqlstate(code.code()) {
                BackendError::Unknown => Decision::Unknown,
                error => Decision::Refused(error.into()),
            },
        });
    }
    Ok(())
}
