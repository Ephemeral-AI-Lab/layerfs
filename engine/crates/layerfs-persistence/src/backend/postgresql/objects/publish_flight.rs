//! One fenced PostgreSQL flight for an eligible captured Save registration.
use super::super::{
    history::fused::{nonce, pack_values, row_values, PACK, ROWS},
    session::{
        connection::Session,
        pipeline::{Refusal, Reply, Request},
        profile::{barrier_leftover, Settings},
    },
};
use crate::backend::records::BackendError;
use layerfs_content::ObjectId;
use layerfs_storage::port::{Publication, Published};
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
    let mut requests = vec![Request::Simple(&begin)];

    let (pack_values, pack_bytes) = pack_values(batch).map_err(|_| BackendError::Integrity)?;
    let mut packs: Vec<&(dyn ToSql + Sync)> = pack_values.iter().map(|value| &**value).collect();
    if !batch.packs.is_empty() {
        packs.push(&nonce);
        requests.push(Request::Statement {
            sql: &pack_sql,
            params: &packs,
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
        let verdict = decide(batch, &replies, trusted, row_index);
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
