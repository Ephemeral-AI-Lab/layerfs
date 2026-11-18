//! §8 Phase A: the retained-payload scratch build and its stored totals.

use super::op::quote_ident;
use super::op::scalar;
use super::op::scalar_session;
use super::op::stage;
use super::types::Built;
use super::types::DeadTotals;
use crate::backend::postgresql::session::connection::Session;
use crate::backend::postgresql::session::transaction::Transaction;
use crate::backend::records::Param;
use crate::diagnostic::postgresql_contract::check_table_objects;
use crate::history::GcTuple;
use crate::MarkResult;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use std::sync::Arc;

/// The stored and metadata bytes of the packs the mark retained.
fn retained_pack_totals(session: &Arc<Session>, schema: &str) -> HistoryResult<(i64, i64)> {
    stage(session, |tx| {
        let rows = tx.query(
            &format!(
                "SELECT COALESCE(SUM(p.length)::bigint,0),\
                        COALESCE((SUM(p.length) FILTER (WHERE p.domain=0))::bigint,0) \
                 FROM {schema}.pack p \
                 WHERE EXISTS (SELECT 1 FROM pg_temp.gc_live_pack v WHERE v.pack_id=p.pack_id)"
            ),
            Vec::new(),
        )?;
        let row = rows
            .first()
            .ok_or(HistoryError::Integrity("compaction retained totals"))?;
        Ok((row.get(0)?, row.get(1)?))
    })
}

/// The copy set: every `object_location` row of a visited object, in a session
/// temporary table indexed on `pack_id`.
///
/// The Store's `object_location` has no `pack_id` index, so reading one batch's
/// rows from it scans the whole table, and the copy would cost batches times
/// objects. This is the one scan of the source; each batch then reads its own
/// `pack_id` range through the index. Returns the set's row count, the
/// retained canonical bytes and the set's own size, which is temporary space
/// this run holds until the session ends and so part of its peak.
fn copy_set(session: &Arc<Session>, schema: &str) -> HistoryResult<(i64, i64, u64)> {
    stage(session, |tx| {
        tx.batch(&format!(
            "DROP TABLE IF EXISTS pg_temp.gc_copy;\
             CREATE TEMP TABLE gc_copy ON COMMIT PRESERVE ROWS AS \
              SELECT l.* FROM {schema}.object_location l \
               WHERE EXISTS (SELECT 1 FROM pg_temp.gc_visited v WHERE v.object_id=l.object_id);\
             CREATE INDEX gc_copy_pack ON pg_temp.gc_copy (pack_id);\
             ANALYZE pg_temp.gc_copy;"
        ))?;
        let rows = tx.query(
            "SELECT count(*),COALESCE(SUM(canonical_length)::bigint,0),\
                    pg_total_relation_size('pg_temp.gc_copy'::regclass)::bigint \
             FROM pg_temp.gc_copy",
            Vec::new(),
        )?;
        let row = rows
            .first()
            .ok_or(HistoryError::Integrity("compaction copy set"))?;
        let bytes = u64::try_from(row.get::<i64>(2)?).unwrap_or(0);
        Ok((row.get(0)?, row.get(1)?, bytes))
    })
}

pub(crate) fn relation_bytes(
    session: &Arc<Session>,
    schema: &str,
    names: &[&str],
) -> HistoryResult<u64> {
    stage(session, |tx| relation_bytes_tx(tx, schema, names))
}

/// [`relation_bytes`] on an open transaction, so the swap can measure the
/// relation size it just made live inside the same transaction that records
/// it.
pub(crate) fn relation_bytes_tx(
    tx: &Transaction<'_>,
    schema: &str,
    names: &[&str],
) -> HistoryResult<u64> {
    let list = names
        .iter()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(",");
    let rows = tx.query(
        &format!(
            "SELECT COALESCE(SUM(pg_total_relation_size(c.oid))::bigint,0) FROM pg_class c \
             JOIN pg_namespace n ON n.oid=c.relnamespace \
             WHERE n.nspname=$1 AND c.relname IN ({list})"
        ),
        vec![Param::Text(schema.to_owned())],
    )?;
    let value: i64 = rows[0].get(0)?;
    Ok(u64::try_from(value).unwrap_or(0))
}

pub(crate) fn table_storage(
    tx: &Transaction<'_>,
    schema: &str,
    table: &str,
) -> HistoryResult<(String, Vec<String>)> {
    {
        let rows = tx.query(
            "SELECT COALESCE((SELECT t.spcname FROM pg_tablespace t WHERE t.oid=c.reltablespace),''), \
                    COALESCE(array_to_string(c.reloptions,','),'') \
             FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
             WHERE n.nspname=$1 AND c.relname=$2",
            vec![Param::Text(schema.to_owned()), Param::Text(table.to_owned())],
        )?;
        if rows.len() != 1 {
            return Err(HistoryError::Integrity("compaction source table"));
        }
        let tablespace: String = rows[0].get(0)?;
        let reloptions: String = rows[0].get(1)?;
        let options = reloptions
            .split(',')
            .filter(|option| !option.is_empty())
            .map(str::to_owned)
            .collect();
        Ok((tablespace, options))
    }
}

pub(crate) fn create_scratch(
    session: &Arc<Session>,
    schema: &str,
    scratch: &str,
) -> HistoryResult<()> {
    let exists = scalar_session(
        session,
        "SELECT count(*) FROM pg_namespace WHERE nspname=$1",
        vec![Param::Text(scratch.to_owned())],
    )?;
    if exists != 0 {
        return Err(HistoryError::Exists("compaction scratch schema"));
    }
    stage(session, |tx| {
        tx.batch(&format!("CREATE SCHEMA {scratch}"))?;
        for table in ["pack", "object_location"] {
            let (tablespace, options) = table_storage(tx, schema, table)?;
            let mut sql = format!(
                "CREATE TABLE {scratch}.{table} (LIKE {schema}.{table} INCLUDING STORAGE INCLUDING COMPRESSION INCLUDING CONSTRAINTS)"
            );
            if !options.is_empty() {
                sql.push_str(&format!(" WITH ({})", options.join(", ")));
            }
            if !tablespace.is_empty() {
                sql.push_str(&format!(" TABLESPACE {}", quote_ident(&tablespace)));
            }
            tx.batch(&sql)?;
        }
        tx.batch(&format!(
            "ALTER TABLE {scratch}.pack ADD PRIMARY KEY (pack_id)"
        ))?;
        Ok(())
    })
}

/// Phase A copies about this many retained pack body bytes per transaction:
/// a fixed row count would be unbounded in bytes for large packs.
const PHASE_A_BATCH_BYTES: i64 = 1 << 30;

/// The byte-bounded pack-id batches of the live packs, ascending: the first
/// and last pack id of each and how many live packs it holds. A batch takes
/// the packs whose bodies start inside one [`PHASE_A_BATCH_BYTES`] stretch of
/// the live bodies laid end to end in pack-id order, so it carries about
/// that much, at most one pack more, and at least one pack.
///
/// The server cuts the batches; this process is handed one row a batch and
/// never the list of live packs.
fn byte_batches(session: &Arc<Session>, schema: &str) -> HistoryResult<Vec<(i64, i64, i64)>> {
    stage(session, |tx| {
        let rows = tx.query(
            &format!(
                "SELECT min(b.pack_id),max(b.pack_id),count(*) FROM (\
                   SELECT p.pack_id,\
                          floor((sum(p.length) OVER (ORDER BY p.pack_id) - p.length) / $1::bigint)::bigint AS batch \
                   FROM {schema}.pack p \
                   WHERE EXISTS (SELECT 1 FROM pg_temp.gc_live_pack v WHERE v.pack_id=p.pack_id)) b \
                 GROUP BY b.batch ORDER BY 1"
            ),
            vec![Param::I64(PHASE_A_BATCH_BYTES)],
        )?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            out.push((row.get(0)?, row.get(1)?, row.get(2)?));
        }
        Ok(out)
    })
}

/// The run's peak temp so far: the scratch relations' size plus the WAL this
/// session wrote since `wal_start`. Both terms grow through the copy, so the
/// sample after each batch and the final sample bound the real peak; the
/// post-build size alone under-reports the WAL headroom.
fn peak_sample(session: &Arc<Session>, scratch: &str, wal_start: &str) -> HistoryResult<u64> {
    let scratch_bytes = relation_bytes(session, scratch, &["pack", "object_location"])?;
    let wal_bytes = scalar_session(
        session,
        "SELECT pg_wal_lsn_diff(pg_current_wal_lsn(), $1::text::pg_lsn)::bigint",
        vec![Param::Text(wal_start.to_owned())],
    )?;
    Ok(scratch_bytes.saturating_add(u64::try_from(wal_bytes).unwrap_or(0)))
}

/// Phase A: copy retained rows in pack-id batches, each its own transaction.
/// A batch first deletes its own range, so the copy is idempotent. The
/// location rows come from the copy set, so a batch reads its own rows through
/// an index instead of scanning the Store's table and the visited set.
pub(crate) fn build(
    session: &Arc<Session>,
    schema: &str,
    scratch: &str,
    mark: &MarkResult,
    tuple: &GcTuple,
) -> HistoryResult<Built> {
    create_scratch(session, schema, scratch)?;
    // The byte batches of the live packs, cut by the server: their counts
    // must add up to the packs the mark retained.
    let batches = byte_batches(session, schema)?;
    let live_packs: i64 = batches.iter().map(|(_, _, packs)| *packs).sum();
    if live_packs != mark.objects().packs().len() as i64 {
        return Err(HistoryError::Integrity("compaction live pack list"));
    }
    let wal_start: String = stage(session, |tx| {
        let rows = tx.query("SELECT pg_current_wal_lsn()::text", Vec::new())?;
        rows.first()
            .ok_or(HistoryError::Integrity("compaction wal start"))?
            .get(0)
            .map_err(Into::into)
    })?;
    let visited = mark.objects().len() as i64;
    let (copy_rows, canonical, copy_bytes) = copy_set(session, schema)?;
    // One location row per visited object, or the source holds a duplicate or
    // lacks a locator; the scratch table's own distinct count follows the copy.
    if copy_rows != visited {
        return Err(HistoryError::Integrity("compaction copy set"));
    }
    let mut peak_temp = 0u64;
    let mut copied_rows = 0i64;
    for (lo, hi, expected_packs) in batches {
        let (pack_rows, objects) = stage(session, |tx| {
            // §8 step 3: a batch first deletes its own range. The scratch
            // `object_location` has no `pack_id` index (the Store's has none,
            // and the scratch table becomes the Store's), so this is one scan
            // of the rows copied so far.
            tx.batch(&format!(
                "DELETE FROM {scratch}.pack WHERE pack_id BETWEEN {lo} AND {hi};\
                 DELETE FROM {scratch}.object_location WHERE pack_id BETWEEN {lo} AND {hi};"
            ))?;
            // Each insert reports its own row count: this batch's range holds
            // exactly the live packs and the copy set's rows of that range.
            let pack_rows = scalar(
                tx,
                &format!(
                    "WITH copied AS (INSERT INTO {scratch}.pack SELECT p.* FROM {schema}.pack p \
                      WHERE p.pack_id BETWEEN $1 AND $2 \
                        AND EXISTS (SELECT 1 FROM pg_temp.gc_live_pack v WHERE v.pack_id=p.pack_id) \
                      RETURNING 1) SELECT count(*) FROM copied"
                ),
                vec![Param::I64(lo), Param::I64(hi)],
            )?;
            let objects = scalar(
                tx,
                &format!(
                    "WITH copied AS (INSERT INTO {scratch}.object_location \
                      SELECT c.* FROM pg_temp.gc_copy c WHERE c.pack_id BETWEEN $1 AND $2 \
                      RETURNING 1) SELECT count(*) FROM copied"
                ),
                vec![Param::I64(lo), Param::I64(hi)],
            )?;
            Ok((pack_rows, objects))
        })?;
        if pack_rows != expected_packs {
            return Err(HistoryError::Integrity("compaction batch count"));
        }
        copied_rows = copied_rows.saturating_add(objects);
        peak_temp =
            peak_temp.max(peak_sample(session, scratch, &wal_start)?.saturating_add(copy_bytes));
    }
    // Every row of the copy set was copied once: a visited object in a pack
    // outside the live list would be left out of every range.
    if copied_rows != visited {
        return Err(HistoryError::Integrity("compaction batch count"));
    }
    stage(session, |tx| {
        tx.batch(&format!(
            "CREATE INDEX object_location_prefix ON {scratch}.object_location \
             ((('x' || encode(substring(object_id FROM 1 FOR 8), 'hex'))::bit(64)::bigint));\
             ALTER TABLE {scratch}.object_location \
             ADD FOREIGN KEY (pack_id) REFERENCES {scratch}.pack(pack_id);\
             ANALYZE {scratch}.pack; ANALYZE {scratch}.object_location;"
        ))?;
        Ok(())
    })?;
    // The contract entries for the two payload tables must hold on the scratch
    // tables before the swap: `search_path=scratch` makes the deparsed names
    // match the shipped manifest (the trigger/function entries need the
    // Store-schema function and run in the swap instead).
    let failures = stage(session, |tx| {
        tx.batch(&format!("SET LOCAL search_path TO {scratch}, pg_catalog"))?;
        check_table_objects(tx, &["pack", "object_location"], &["C", "I"])
            .map_err(HistoryError::from)
    })?;
    if !failures.is_empty() {
        return Err(HistoryError::Integrity("compaction scratch contract"));
    }
    peak_temp =
        peak_temp.max(peak_sample(session, scratch, &wal_start)?.saturating_add(copy_bytes));
    let (objects, distinct, pack_rows) = stage(session, |tx| {
        let rows = tx.query(
            &format!(
                "SELECT count(*),count(DISTINCT object_id),(SELECT count(*) FROM {scratch}.pack) \
                 FROM {scratch}.object_location"
            ),
            Vec::new(),
        )?;
        let row = rows
            .first()
            .ok_or(HistoryError::Integrity("compaction object count"))?;
        Ok((row.get::<i64>(0)?, row.get::<i64>(1)?, row.get::<i64>(2)?))
    })?;
    if distinct != mark.objects().len() as i64
        || objects != distinct
        || pack_rows != mark.objects().packs().len() as i64
    {
        return Err(HistoryError::Integrity("compaction object count"));
    }
    let (stored, metadata) = retained_pack_totals(session, schema)?;
    let dead = DeadTotals {
        canonical: tuple.canonical - canonical,
        stored: tuple.stored - stored,
        metadata: tuple.metadata - metadata,
        logical: 0,
    };
    Ok(Built {
        scratch: scratch.to_owned(),
        peak_temp,
        dead,
    })
}
