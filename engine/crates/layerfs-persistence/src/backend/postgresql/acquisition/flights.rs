//! Write units with no wait, each one pipelined message from `BEGIN` to
//! `COMMIT` (#818 I3b): root windows, `advance`, `discard` and `dispose`.
//!
//! A flight is the unit's transaction as before: `BEGIN` (the lane row and
//! barrier of every acquisition write) with the generation fence and an
//! attempt token, the owner check, the unit's statements, the token check
//! and `COMMIT` as a request of its own, so it also ends an aborted
//! transaction. Each check the unit made between statements is a server
//! assertion before `COMMIT`: a division by zero for `Stale` (a missing
//! owner or charge row) and for a root window that missed an input position
//! (the client then reads `Changed` from that window's reply), a `smallint`
//! overflow for a row the unit would have read as `Integrity` (an unknown
//! phase or a negative count), which classifies as the ordinary refusal
//! does. The first refusal decides. Nothing is resent: a lost reply, or a
//! committed reply that does not validate, is `Unknown` and quarantines the
//! session.
//!
//! Selected before sending (`provider`): a session without a generation, a
//! write nested in the held lane, or an owner or root position that does not
//! convert to signed runs today's transaction instead (`writes`, `cleanup`).
use super::{reads::get, writes::Roots};
use crate::backend::{
    postgresql::{
        history::fused::nonce,
        session::{
            connection::Session,
            pipeline::{Refusal, Reply, Request},
            profile::barrier_leftover,
        },
    },
    records::BackendError,
    sqlite::acquisition::{
        accounting::ROOT_BYTES,
        statements::{signed, unsigned, Failure},
    },
};
use layerfs_content::ObjectId;
use layerfs_storage::port::acquisition::{Discarded, Disposal, Phase};
use postgres::{error::SqlState, types::ToSql, Row};
use std::{collections::BTreeSet, sync::atomic::Ordering};

/// `writes::owner` as an assertion: `Stale` without the live row,
/// `Integrity` for an unknown phase or a negative count.
const OWNER_LIVE: &str = "SELECT CASE WHEN count(*)=0 THEN 1/count(*)::int \
 WHEN bool_and(phase BETWEEN 1 AND 6 AND least(held_rows,held_bytes,peak_rows,peak_bytes,removed_rows,removed_bytes)>=0) THEN 1 \
 ELSE (32768+count(*))::smallint::int END \
 FROM {s}.init_operation WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint";
/// `writes::CHARGE`: `Stale` without the row, `Integrity` for negative rows.
const CHARGED: &str = "WITH c AS (UPDATE {s}.init_operation SET held_rows=held_rows+$3::bigint,held_bytes=held_bytes+$4::bigint,peak_rows=greatest(peak_rows,held_rows+$3::bigint),peak_bytes=greatest(peak_bytes,held_bytes+$4::bigint) WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint RETURNING held_rows) \
 SELECT CASE WHEN count(*)=0 THEN 1/count(*)::int WHEN min(held_rows)>=0 THEN 1 ELSE (32768+count(*))::smallint::int END FROM c";
/// `writes::FILE_ROOTS` returning the recorded positions as one array, with
/// `layerfs.recorded` set to whether every input position was recorded
/// (positions are distinct in a window).
const FILE_ROOTS: &str = "WITH u AS (UPDATE {s}.init_host_file AS t SET file_root=r.root FROM unnest($2::bigint[],$3::bytea[]) AS r(position,root) WHERE t.operation_id=$1::bigint AND t.canonical_position=r.position AND t.file_root IS NULL RETURNING t.canonical_position AS p) \
 SELECT array_agg(p),set_config('layerfs.recorded',(count(*)=cardinality($2::bigint[]))::text,true) FROM u";
const DIRECTORY_ROOTS: &str = "WITH u AS (UPDATE {s}.init_entry AS t SET content_root=r.root FROM unnest($2::bigint[],$3::bytea[]) AS r(position,root) WHERE t.operation_id=$1::bigint AND t.position=r.position AND t.kind=2 AND t.content_root IS NULL RETURNING t.position AS p) \
 SELECT array_agg(p),set_config('layerfs.recorded',(count(*)=cardinality($2::bigint[]))::text,true) FROM u";
/// Aborts the transaction after a root window that missed a position.
const RECORDED: &str = "SELECT 1/(current_setting('layerfs.recorded')::boolean)::int";
const ADVANCED: &str = "WITH a AS (UPDATE {s}.init_operation SET phase=$3::bigint WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint RETURNING phase) SELECT 1/count(*)::int FROM a";
/// One removal job (`cleanup::discard`) in one statement: the inclusive last
/// entry key of at most `$3` lowest keys (the offset key, else the tail) and
/// those entries deleted; then the last of at most `$3` − (entries deleted)
/// lowest host positions and those host files deleted; the removed rows and
/// bytes credited (`writes::CREDIT`). Answers the removed rows and bytes and
/// the rows still held: `Stale` without the record, `Integrity` for
/// negative held rows.
const DISCARD: &str = "WITH ek AS (SELECT parent_position,name FROM ((SELECT parent_position,name,0 AS o FROM {s}.init_entry WHERE operation_id=$1::bigint ORDER BY parent_position,name LIMIT 1 OFFSET greatest($3::bigint-1,0)) UNION ALL (SELECT parent_position,name,1 FROM {s}.init_entry WHERE operation_id=$1::bigint ORDER BY parent_position DESC,name DESC LIMIT 1)) k WHERE $3::bigint>0 ORDER BY o LIMIT 1), \
de AS (DELETE FROM {s}.init_entry e USING ek WHERE e.operation_id=$1::bigint AND (e.parent_position,e.name)<=(ek.parent_position,ek.name) RETURNING (octet_length(e.name)+coalesce(octet_length(e.host_path),0)+coalesce(octet_length(e.host_identity),0)+32+coalesce(octet_length(e.content_root),0))::bigint AS b), \
hk AS (SELECT max(canonical_position) AS p FROM (SELECT canonical_position FROM {s}.init_host_file WHERE operation_id=$1::bigint ORDER BY canonical_position LIMIT greatest($3::bigint-(SELECT count(*) FROM de),0)) l), \
dh AS (DELETE FROM {s}.init_host_file h USING hk WHERE h.operation_id=$1::bigint AND h.canonical_position<=hk.p RETURNING (octet_length(h.host_path)+60+coalesce(octet_length(h.file_root),0))::bigint AS b), \
t AS (SELECT (SELECT count(*) FROM de)+(SELECT count(*) FROM dh) AS r,(coalesce((SELECT sum(b) FROM de),0)+coalesce((SELECT sum(b) FROM dh),0))::bigint AS y), \
c AS (UPDATE {s}.init_operation o SET held_rows=o.held_rows-t.r,held_bytes=o.held_bytes-t.y,removed_rows=o.removed_rows+t.r,removed_bytes=o.removed_bytes+t.y FROM t WHERE o.operation_id=$1::bigint AND o.owner_epoch=$2::bigint RETURNING o.held_rows) \
SELECT t.r,t.y,c.held_rows,CASE WHEN c.held_rows IS NULL THEN 1/(c.held_rows IS NOT NULL)::int WHEN c.held_rows>=0 THEN 1 ELSE (32768+(c.held_rows<0)::int)::smallint::int END FROM t LEFT JOIN c ON true";
/// `cleanup::RELEASE` after a removal job: deletes the record only at zero
/// held rows and sets `layerfs.compact` when no other record remains (the
/// statement's snapshot still holds the deleted row). The record must be
/// gone exactly when it holds no rows; else a division by zero (`Stale`).
const RELEASED: &str = "WITH gone AS (DELETE FROM {s}.init_operation WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint AND held_rows=0 RETURNING operation_id), \
last AS (SELECT NOT EXISTS(SELECT 1 FROM {s}.init_operation o WHERE o.operation_id<>g.operation_id) AS l FROM gone g) \
SELECT (SELECT l FROM last),set_config('layerfs.compact',coalesce((SELECT l FROM last),false)::text,true),\
1/((SELECT count(*) FROM gone)>0 OR coalesce((SELECT held_rows FROM {s}.init_operation WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint),0)<>0)::int";
/// `cleanup::COMPACT` when the release left no record: the same 2 s lock
/// wait, lock order and `TRUNCATE`. Any refusal rolls the block back and the
/// release commits, as `cleanup::KEEP` does.
const COMPACT: &str = "DO $$BEGIN IF current_setting('layerfs.compact',true)='true' THEN \
PERFORM set_config('lock_timeout','2000',true); \
LOCK TABLE {s}.init_operation, {s}.init_entry, {s}.init_host_file IN ACCESS EXCLUSIVE MODE; \
TRUNCATE {s}.init_operation, {s}.init_entry, {s}.init_host_file CONTINUE IDENTITY; \
END IF; EXCEPTION WHEN OTHERS OR query_canceled THEN NULL; END$$";

/// Whether this session's write units go as flights.
pub(super) fn eligible(session: &Session) -> Result<bool, BackendError> {
    Ok(session.incarnation.load(Ordering::SeqCst) != 0 && !session.admission.held_by_caller()?)
}

/// The live owner's signed identity, as the unit binds it.
pub(super) type Identity = (i64, i64);

type Replies = [Result<Reply, Refusal>];

/// One root window: its first index in the unit, positions and roots.
type Window<'a> = (usize, Vec<i64>, Vec<&'a [u8]>);

fn text(sql: &str, schema: &str) -> String {
    sql.replace("{s}", schema)
}

fn rows(reply: &Result<Reply, Refusal>) -> Option<&[Row]> {
    match reply {
        Ok(Reply::Rows(rows)) => Some(rows),
        _ => None,
    }
}

/// Sends one unit: `body` between the owner check and the token check.
/// `divided(i, body)` names the failure a division by zero at body request
/// `i` stands for; `answer(body)` reads a committed unit's replies.
fn send<T>(
    session: &Session,
    (operation, epoch): Identity,
    body: Vec<Request<'_>>,
    divided: impl Fn(usize, &Replies) -> Failure,
    answer: impl FnOnce(&Replies) -> Option<T>,
) -> Result<T, Failure> {
    let generation = session.incarnation.load(Ordering::SeqCst);
    let schema = session.settings.schema.as_str();
    let nonce = nonce();
    let begin = format!(
        "{}; SELECT CASE WHEN g IS NULL THEN (32768 + (g IS NULL)::int)::smallint::int \
         ELSE 1/(g={generation})::int END FROM \
         (SELECT (SELECT catalog_incarnation FROM {schema}.history_meta WHERE id=1) AS g) m; \
         SELECT set_config('layerfs.attempt','{nonce}',true)",
        session.settings.write_begin(true)
    );
    let check = format!(
        "SELECT 1/(current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM '{nonce}')::int"
    );
    let owner_sql = text(OWNER_LIVE, schema);
    let owner: [&(dyn ToSql + Sync); 2] = [&operation, &epoch];
    let count = body.len();
    let mut requests = Vec::with_capacity(count + 4);
    requests.push(Request::Simple(&begin));
    requests.push(Request::Statement {
        sql: &owner_sql,
        params: &owner,
        bytes: 16,
    });
    requests.extend(body);
    requests.push(Request::Simple(&check));
    requests.push(Request::Simple("COMMIT"));
    let release = barrier_leftover(schema);
    let outcome = session.pipeline_judged(&requests, &release, |replies, trusted| {
        let verdict = decide(&replies, trusted, count, &divided, answer);
        let unknown = matches!(verdict, Err(Failure::Backend(BackendError::Unknown)));
        (verdict, unknown)
    })?;
    session.count_fused(outcome.is_ok());
    outcome
}

/// The unit's answer from its replies: the first refusal decides, as the
/// ordinary unit's first failing statement did.
fn decide<T>(
    replies: &Replies,
    trusted: bool,
    count: usize,
    divided: &impl Fn(usize, &Replies) -> Failure,
    answer: impl FnOnce(&Replies) -> Option<T>,
) -> Result<T, Failure> {
    const FIRST: usize = 2;
    let body = FIRST..FIRST + count;
    for (index, reply) in replies.iter().enumerate() {
        let Err(refusal) = reply else { continue };
        let division = refusal.code.as_ref() == Some(&SqlState::DIVISION_BY_ZERO);
        return Err(match index {
            // The generation fence, as the ordinary `BEGIN` batch answers it.
            0 if division => BackendError::Stale.into(),
            1 if division => Failure::Stale,
            i if division && body.contains(&i) => divided(i - FIRST, &replies[body]),
            _ if refusal.code.is_some() => refusal.error.into(),
            _ if !refusal.sent && trusted => refusal.error.into(),
            _ => BackendError::Unknown.into(),
        });
    }
    if !trusted || replies.len() != count + 4 {
        return Err(BackendError::Unknown.into());
    }
    // A malformed receipt after COMMIT may have applied the unit.
    answer(&replies[body]).ok_or(Failure::Backend(BackendError::Unknown))
}

/// `writes::record_roots` in one flight, its windows cut where it cuts them;
/// `None` when a position does not convert to signed (today's path then
/// decides, in its order).
pub(super) fn record_roots(
    session: &Session,
    identity: Identity,
    which: Roots,
    roots: &[(u64, ObjectId)],
) -> Option<Result<(), Failure>> {
    let operation = identity.0;
    let mut windows: Vec<Window<'_>> = Vec::new();
    let mut offset = 0;
    while offset < roots.len() {
        let (mut positions, mut objects) = (Vec::new(), Vec::new());
        let mut distinct = BTreeSet::new();
        for (position, object) in &roots[offset..] {
            let position = signed(*position).ok()?;
            if !distinct.insert(position) {
                break;
            }
            positions.push(position);
            objects.push(object.as_bytes().as_slice());
        }
        let count = positions.len();
        windows.push((offset, positions, objects));
        offset += count;
    }
    let schema = session.settings.schema.as_str();
    let sql = text(
        match which {
            Roots::Files => FILE_ROOTS,
            Roots::Directories => DIRECTORY_ROOTS,
        },
        schema,
    );
    let charged = text(CHARGED, schema);
    let (zero, bytes) = (0_i64, roots.len() as i64 * ROOT_BYTES);
    let params: Vec<[&(dyn ToSql + Sync); 3]> = windows
        .iter()
        .map(|(_, positions, objects)| {
            [
                &operation as &(dyn ToSql + Sync),
                positions as &(dyn ToSql + Sync),
                objects as &(dyn ToSql + Sync),
            ]
        })
        .collect();
    let charge: [&(dyn ToSql + Sync); 4] = [&identity.0, &identity.1, &zero, &bytes];
    let mut body = Vec::with_capacity(windows.len() * 2 + 1);
    for ((_, positions, _), params) in windows.iter().zip(&params) {
        body.push(Request::Statement {
            sql: &sql,
            params,
            bytes: 8 + 40 * positions.len() as u64,
        });
        body.push(Request::Simple(RECORDED));
    }
    let last = body.len();
    body.push(Request::Statement {
        sql: &charged,
        params: &charge,
        bytes: 32,
    });
    let divided = |index: usize, replies: &Replies| -> Failure {
        let window = windows.get(index / 2).filter(|_| index % 2 == 1);
        let Some((start, positions, _)) = window else {
            return match index == last {
                true => Failure::Stale,
                false => BackendError::Integrity.into(),
            };
        };
        let Some(row) = rows(&replies[index - 1]).and_then(<[Row]>::first) else {
            return BackendError::Integrity.into();
        };
        let recorded = match get::<Option<Vec<i64>>>(row, 0) {
            Ok(recorded) => recorded.unwrap_or_default(),
            Err(error) => return error.into(),
        };
        // RETURNING order is not input order: the first input position the
        // window did not record is the sequential check's `Changed`.
        let recorded: BTreeSet<i64> = recorded.into_iter().collect();
        match positions.iter().position(|p| !recorded.contains(p)) {
            Some(missing) => Failure::Changed(roots[start + missing].0),
            None => BackendError::Integrity.into(),
        }
    };
    Some(send(session, identity, body, divided, |_| Some(())))
}

pub(super) fn advance(session: &Session, identity: Identity, phase: Phase) -> Result<(), Failure> {
    let sql = text(ADVANCED, &session.settings.schema);
    let code = i64::from(phase.code());
    let params: [&(dyn ToSql + Sync); 3] = [&identity.0, &identity.1, &code];
    let body = vec![Request::Statement {
        sql: &sql,
        params: &params,
        bytes: 24,
    }];
    send(session, identity, body, |_, _| Failure::Stale, |_| Some(()))
}

/// One removal job (`cleanup::discard`); with `dispose`, the record goes
/// with the operation's last row and the tables are emptied after the last
/// record, as `cleanup::dispose` does, in the same flight.
pub(super) fn dispose(
    session: &Session,
    identity: Identity,
    budget: i64,
    dispose: bool,
) -> Result<Disposal, Failure> {
    let schema = session.settings.schema.as_str();
    let (discard, released, compact) = (
        text(DISCARD, schema),
        text(RELEASED, schema),
        text(COMPACT, schema),
    );
    let job: [&(dyn ToSql + Sync); 3] = [&identity.0, &identity.1, &budget];
    let record: [&(dyn ToSql + Sync); 2] = [&identity.0, &identity.1];
    let mut body = vec![Request::Statement {
        sql: &discard,
        params: &job,
        bytes: 24,
    }];
    if dispose {
        body.push(Request::Statement {
            sql: &released,
            params: &record,
            bytes: 16,
        });
        body.push(Request::Simple(&compact));
    }
    let answer = |replies: &Replies| {
        let row = rows(&replies[0])?.first()?;
        let discarded = Discarded {
            rows: unsigned(get::<i64>(row, 0).ok()?).ok()?,
            bytes: unsigned(get::<i64>(row, 1).ok()?).ok()?,
            remaining_rows: unsigned(get::<Option<i64>>(row, 2).ok()??).ok()?,
        };
        let released = dispose && discarded.remaining_rows == 0;
        if dispose {
            // The record is deleted exactly when no rows remain.
            let gone = get::<Option<bool>>(rows(&replies[1])?.first()?, 0).ok()?;
            if gone.is_some() != released {
                return None;
            }
        }
        Some(Disposal {
            discarded,
            released,
        })
    };
    send(session, identity, body, |_, _| Failure::Stale, answer)
}
