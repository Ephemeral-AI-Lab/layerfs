//! One fenced PostgreSQL flight for an eligible captured Save registration.
use super::super::session::{
    attempt::nonce,
    connection::Session,
    pipeline::{Refusal, Reply, Request},
    profile::{barrier_leftover, Settings},
};
use crate::backend::error::BackendError;
use crate::history::Fused;
use layerfs_content::ObjectId;
use layerfs_history::error::HistoryError;
use layerfs_storage::{
    location::PackDomain,
    port::{Publication, Published, PublishedPack},
};
use postgres::{error::SqlState, types::ToSql};
use std::{collections::BTreeSet, sync::atomic::Ordering};

/// The pack bytes that may be copied into one typed request. Catalog arrays
/// are independently bounded by the existing publication row limit.
const PACK_BYTES: usize = 4 * 1024 * 1024;

pub(crate) fn eligible(session: &Session, batch: &Publication) -> Result<bool, BackendError> {
    Ok(batch
        .packs
        .iter()
        .map(|pack| pack.body().len())
        .sum::<usize>()
        <= PACK_BYTES
        && session.incarnation.load(Ordering::SeqCst) != 0
        && !session.admission.held_by_caller()?)
}

/// Uploads pack bodies before acquiring the existing lane. The final COMMIT
/// remains its own request in the same flight, so it closes an aborted
/// transaction even when the preceding assertion fails.
pub(crate) fn run(session: &Session, batch: &Publication) -> Result<Published, BackendError> {
    send(session, batch, false)
}

/// Uses the existing captured transport with ordinary held-tail custody.
pub(crate) fn run_tail(session: &Session, batch: &Publication) -> Result<Published, BackendError> {
    send(session, batch, true)
}

fn send(
    session: &Session,
    batch: &Publication,
    held_tail: bool,
) -> Result<Published, BackendError> {
    let generation = session.incarnation.load(Ordering::SeqCst);
    let schema = session.settings.schema.as_str();
    let nonce = nonce();
    let begin = format!(
        "{}; SELECT CASE WHEN g IS NULL THEN (32768 + (g IS NULL)::int)::smallint::int \
         ELSE 1/(g={generation})::int END FROM \
         (SELECT (SELECT catalog_incarnation FROM {schema}.history_meta WHERE id=1) AS g) m; \
         SELECT set_config('layerfs.attempt','{nonce}',true)",
        session.settings.publish_begin()
    );
    let check = format!(
        "SELECT 1/(current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM '{nonce}')::int"
    );
    let lane = format!("{check}; {}", Settings::lane_row(schema));
    let pack_sql = PACK.replace("{s}", schema);
    let row_sql = ROWS.replace("{s}", schema);
    // One pack statement per run of at most one ordinary pack's bytes, all
    // in this flight's round trip and transaction (`pack_statements`).
    let built = pack_statements(&batch.packs).map_err(|_| BackendError::Integrity)?;
    let pack_params: Vec<Vec<&(dyn ToSql + Sync)>> = built
        .iter()
        .map(|(values, _)| {
            let mut params: Vec<&(dyn ToSql + Sync)> =
                values.iter().map(|value| &**value).collect();
            params.push(&nonce);
            params
        })
        .collect();
    let mut requests = vec![Request::Simple(&begin)];
    for ((_, pack_bytes), params) in built.iter().zip(&pack_params) {
        requests.push(Request::Statement {
            sql: &pack_sql,
            params,
            bytes: pack_bytes + nonce.len() as u64,
        });
    }
    requests.push(Request::Simple(&lane));

    let (row_values, row_bytes) = row_values(batch).map_err(|_| BackendError::Integrity)?;
    let allow_existing = true;
    let mut rows: Vec<&(dyn ToSql + Sync)> = row_values.iter().map(|value| &**value).collect();
    rows.extend([&nonce as &(dyn ToSql + Sync), &allow_existing]);
    let row_index = requests.len();
    requests.push(Request::Statement {
        sql: &row_sql,
        params: &rows,
        bytes: row_bytes + nonce.len() as u64 + 1,
    });
    requests.push(Request::Simple(&check));
    requests.push(Request::Simple("COMMIT"));

    let release = barrier_leftover(schema);
    let outcome = session.pipeline_judged(&requests, &release, |replies, trusted| {
        // Ordinary standalone publication withholds unused IDs if it
        // cannot confirm rollback, cleanup or completion on the session.
        let verdict = if held_tail && !trusted {
            Err(BackendError::Unknown)
        } else {
            decide(batch, &replies, trusted, row_index)
        };
        let unknown = verdict == Err(BackendError::Unknown);
        (verdict, unknown)
    })?;
    session.count_fused(outcome.is_ok());
    outcome
}

fn decide(
    batch: &Publication,
    replies: &[Result<Reply, Refusal>],
    trusted: bool,
    row_index: usize,
) -> Result<Published, BackendError> {
    for (index, reply) in replies.iter().enumerate() {
        let Err(refusal) = reply else { continue };
        return Err(match &refusal.code {
            Some(code) if index == 0 && *code == SqlState::DIVISION_BY_ZERO => BackendError::Stale,
            Some(_) => refusal.error,
            None if !refusal.sent && trusted => refusal.error,
            None => BackendError::Unknown,
        });
    }
    if !trusted || replies.len() != row_index + 3 {
        return Err(BackendError::Unknown);
    }
    // A malformed receipt after COMMIT may have applied the writes. Only a
    // validated reply lets the Save discard its previous custody.
    let Ok(Reply::Rows(rows)) = &replies[row_index] else {
        return Err(BackendError::Unknown);
    };
    let [row] = rows.as_slice() else {
        return Err(BackendError::Unknown);
    };
    let declared: String = row.try_get(0).map_err(|_| BackendError::Unknown)?;
    let ids: Vec<Vec<u8>> = row.try_get(1).map_err(|_| BackendError::Unknown)?;
    if declared.parse::<usize>().ok() != Some(ids.len()) || ids.len() > batch.objects.len() {
        return Err(BackendError::Unknown);
    }
    let mut inserted = BTreeSet::new();
    for id in ids {
        let id = ObjectId::from_bytes(&id).map_err(|_| BackendError::Unknown)?;
        if !inserted.insert(id) {
            return Err(BackendError::Unknown);
        }
    }
    let lost = batch
        .objects
        .iter()
        .filter_map(|object| (!inserted.remove(&object.object_id)).then_some(object.object_id))
        .collect();
    if !inserted.is_empty() {
        return Err(BackendError::Unknown);
    }
    Ok(Published { lost })
}

pub(crate) const PACK: &str = include_str!("../../../../sql/postgresql/fused/pack.sql");
pub(crate) const ROWS: &str = include_str!("../../../../sql/postgresql/fused/rows.sql");

/// One value bound in binary.
pub(crate) type Value<'a> = Box<dyn ToSql + Sync + 'a>;

pub(crate) fn bytes(value: &[u8]) -> Value<'static> {
    Box::new(value.to_vec())
}

pub(crate) fn optional(value: Option<&[u8]>) -> Value<'static> {
    Box::new(value.map(<[u8]>::to_vec))
}

/// Pack bytes bound by one pack insert statement. The driver encodes each
/// statement into a buffer and copies it into the connection's write
/// buffer, and both keep their grown capacity for the connection's life.
/// Splitting keeps the pack statements' share of that capacity near one
/// ordinary pack, however many packs the batch holds; they stay in the same
/// message and transaction. A singleton pack, up to `SINGLETON_PACK_LIMIT`,
/// cannot be split and is sent alone. The retained capacity is therefore
/// the largest single statement: a singleton pack, or the rows or hint
/// statement at their limits (`TRANSACTION_ROW_LIMIT` rows, `ARRAY_ROWS`
/// hint blocks).
const STATEMENT_PACK_BYTES: usize = layerfs_storage::policy::PACK_LIMIT;

/// The pack insert parameters of `packs`, one entry per statement: the
/// contiguous runs of at most [`STATEMENT_PACK_BYTES`] body bytes, each
/// holding at least one pack, in order. None for no packs.
pub(crate) fn pack_statements(
    packs: &[PublishedPack],
) -> Result<Vec<(Vec<Value<'_>>, u64)>, Fused> {
    let mut statements = Vec::new();
    let (mut start, mut bytes) = (0, 0);
    for (index, pack) in packs.iter().enumerate() {
        let length = pack.body().len();
        if index > start && bytes + length > STATEMENT_PACK_BYTES {
            statements.push(pack_values(&packs[start..index])?);
            (start, bytes) = (index, 0);
        }
        bytes += length;
    }
    if start < packs.len() {
        statements.push(pack_values(&packs[start..])?);
    }
    Ok(statements)
}

/// The pack rows of `packs`, as `objects::publish::packs` writes them.
fn pack_values(packs: &[PublishedPack]) -> Result<(Vec<Value<'_>>, u64), Fused> {
    let mut columns = (
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let mut bound = 0;
    for pack in packs {
        let info = pack.info();
        let body = pack.body();
        let header = layerfs_storage::pack::layout::parse_header(body)
            .map_err(|_| HistoryError::Integrity("pack header"))?;
        let directory = body
            .get(..header.body_offset)
            .ok_or(HistoryError::Integrity("pack directory"))?;
        columns.0.push(info.pack_id);
        columns
            .1
            .push(i64::from(info.domain == PackDomain::Payload));
        columns.2.push(info.key.as_bytes().to_vec());
        columns.3.push(info.length as i64);
        columns.4.push(body.as_slice());
        columns.5.push(directory);
        bound += 56 + body.len() as u64 + directory.len() as u64;
    }
    Ok((
        vec![
            Box::new(columns.0),
            Box::new(columns.1),
            Box::new(columns.2),
            Box::new(columns.3),
            Box::new(columns.4),
            Box::new(columns.5),
        ],
        bound,
    ))
}

/// The 22 columns of `publish_rows`, as `objects::publish::rows` binds them.
pub(crate) fn row_values(batch: &Publication) -> Result<(Vec<Value<'static>>, u64), Fused> {
    use crate::backend::postgresql::objects::publish::prefix;
    let objects = &batch.objects;
    let column = |f: fn(&layerfs_storage::location::ObjectLocation) -> i64| -> Vec<i64> {
        objects.iter().map(f).collect()
    };
    let ids: Vec<Vec<u8>> = objects
        .iter()
        .map(|o| o.object_id.as_bytes().to_vec())
        .collect();
    let metadata: i64 = batch
        .packs
        .iter()
        .filter(|pack| pack.info().domain == PackDomain::Metadata)
        .map(|pack| pack.info().length as i64)
        .sum();
    let stored: i64 = batch.packs.iter().map(|p| p.info().length as i64).sum();
    let values = crate::storage::blocks::values::encode(&batch.inode_values)
        .map_err(|_| HistoryError::Integrity("inode value block"))?;
    let (release, first, count) = match batch.release_ordinals {
        Some((first, count)) => (true, i64::from(first), count as i64),
        None => (false, 0, 0),
    };
    let signatures = &batch.signatures;
    let bound = objects.len() as u64 * 112
        + values.iter().map(|b| b.body.len() as u64 + 16).sum::<u64>()
        + signatures.len() as u64 * 80
        + 48;
    let out: Vec<Value<'static>> = vec![
        Box::new(ids),
        Box::new(column(|o| i64::from(o.role.code()))),
        Box::new(column(|o| o.canonical_length as i64)),
        Box::new(column(|o| o.pack_id)),
        Box::new(column(|o| o.group_number as i64)),
        Box::new(column(|o| o.record_number as i64)),
        Box::new(stored),
        Box::new(metadata),
        Box::new(
            values
                .iter()
                .map(|b| i64::from(b.first))
                .collect::<Vec<_>>(),
        ),
        Box::new(values.iter().map(|b| b.body.clone()).collect::<Vec<_>>()),
        Box::new(first),
        Box::new(count),
        Box::new(release),
        Box::new(signatures.iter().map(|r| r.slot as i64).collect::<Vec<_>>()),
        Box::new(
            signatures
                .iter()
                .map(|r| r.stamp as i64)
                .collect::<Vec<_>>(),
        ),
        Box::new(
            signatures
                .iter()
                .map(|r| r.object_id.as_bytes().to_vec())
                .collect::<Vec<_>>(),
        ),
        Box::new(
            signatures
                .iter()
                .map(|r| r.signature.to_vec())
                .collect::<Vec<_>>(),
        ),
        Box::new(values.iter().map(|b| i64::from(b.last)).collect::<Vec<_>>()),
        Box::new(
            objects
                .iter()
                .map(|o| prefix(&o.object_id))
                .collect::<Vec<_>>(),
        ),
        Box::new(
            signatures
                .iter()
                .map(|r| prefix(&r.object_id))
                .collect::<Vec<_>>(),
        ),
        Box::new(
            objects
                .iter()
                .map(|o| o.base.as_ref().map(|b| b.as_bytes().to_vec()))
                .collect::<Vec<_>>(),
        ),
        Box::new(
            objects
                .iter()
                .map(|o| o.base.as_ref().map(prefix))
                .collect::<Vec<_>>(),
        ),
    ];
    Ok((out, bound))
}
