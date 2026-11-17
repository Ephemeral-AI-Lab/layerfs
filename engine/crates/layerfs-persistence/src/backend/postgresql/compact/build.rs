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

pub(crate) fn retained_totals(
    session: &Arc<Session>,
    schema: &str,
) -> HistoryResult<(i64, i64, i64)> {
    stage(session, |tx| {
        let canonical = scalar(
            tx,
            &format!(
                "SELECT COALESCE(SUM(l.canonical_length)::bigint,0) FROM {schema}.object_location l \
                 WHERE EXISTS (SELECT 1 FROM pg_temp.gc_visited v WHERE v.object_id=l.object_id)"
            ),
            Vec::new(),
        )?;
        let stored = scalar(
            tx,
            &format!(
                "SELECT COALESCE(SUM(p.length)::bigint,0) FROM {schema}.pack p \
                 WHERE EXISTS (SELECT 1 FROM pg_temp.gc_live_pack v WHERE v.pack_id=p.pack_id)"
            ),
            Vec::new(),
        )?;
        let metadata = scalar(
            tx,
            &format!(
                "SELECT COALESCE(SUM(p.length)::bigint,0) FROM {schema}.pack p \
                 WHERE p.domain=0 AND EXISTS (SELECT 1 FROM pg_temp.gc_live_pack v WHERE v.pack_id=p.pack_id)"
            ),
            Vec::new(),
        )?;
        Ok((canonical, stored, metadata))
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
const PHASE_A_BATCH_BYTES: u64 = 1 << 30;

/// One byte-bounded pack-id batch: `(pack_id, length)` pairs in ascending
/// pack-id order, split so each batch carries at most about 1 GiB (at least
/// one pack per batch).
fn byte_batches(packs: &[(i64, u64)]) -> Vec<&[(i64, u64)]> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut bytes = 0u64;
    for (index, (_, length)) in packs.iter().enumerate() {
        if index > start && bytes.saturating_add(*length) > PHASE_A_BATCH_BYTES {
            out.push(&packs[start..index]);
            start = index;
            bytes = 0;
        }
        bytes = bytes.saturating_add(*length);
    }
    if start < packs.len() {
        out.push(&packs[start..]);
    }
    out
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

/// Phase A: copy retained rows in pack-id batches, each its own transaction;
/// a retried batch first deletes its own range, so the copy is idempotent.
pub(crate) fn build(
    session: &Arc<Session>,
    schema: &str,
    scratch: &str,
    mark: &MarkResult,
    tuple: &GcTuple,
) -> HistoryResult<Built> {
    create_scratch(session, schema, scratch)?;
    // The live packs with their source lengths, ascending: the byte batches
    // and the per-batch range assertions both read this one list.
    let live_packs: Vec<(i64, u64)> = stage(session, |tx| {
        let rows = tx.query(
            &format!(
                "SELECT p.pack_id,p.length FROM {schema}.pack p \
                 WHERE EXISTS (SELECT 1 FROM pg_temp.gc_live_pack v WHERE v.pack_id=p.pack_id) \
                 ORDER BY p.pack_id"
            ),
            Vec::new(),
        )?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let id: i64 = row.get(0)?;
            let length = u64::try_from(row.get::<i64>(1)?).unwrap_or(0);
            out.push((id, length));
        }
        Ok(out)
    })?;
    if live_packs.len() != mark.objects().packs().len() {
        return Err(HistoryError::Integrity("compaction live pack list"));
    }
    let wal_start: String = stage(session, |tx| {
        let rows = tx.query("SELECT pg_current_wal_lsn()::text", Vec::new())?;
        rows.first()
            .ok_or(HistoryError::Integrity("compaction wal start"))?
            .get(0)
            .map_err(Into::into)
    })?;
    let mut peak_temp = 0u64;
    for batch in byte_batches(&live_packs) {
        let lo = batch[0].0;
        let hi = batch[batch.len() - 1].0;
        let expected_packs = batch.len() as i64;
        stage(session, |tx| {
            tx.batch(&format!(
                "DELETE FROM {scratch}.pack WHERE pack_id BETWEEN {lo} AND {hi};\
                 DELETE FROM {scratch}.object_location WHERE pack_id BETWEEN {lo} AND {hi};"
            ))?;
            tx.query(
                &format!(
                    "INSERT INTO {scratch}.pack SELECT p.* FROM {schema}.pack p \
                     WHERE p.pack_id BETWEEN $1 AND $2 \
                     AND EXISTS (SELECT 1 FROM pg_temp.gc_live_pack v WHERE v.pack_id=p.pack_id)"
                ),
                vec![Param::I64(lo), Param::I64(hi)],
            )?;
            tx.query(
                &format!(
                    "INSERT INTO {scratch}.object_location SELECT l.* FROM {schema}.object_location l \
                     WHERE l.pack_id BETWEEN $1 AND $2 \
                     AND EXISTS (SELECT 1 FROM pg_temp.gc_visited v WHERE v.object_id=l.object_id)"
                ),
                vec![Param::I64(lo), Param::I64(hi)],
            )?;
            // Assert this batch's own pack_id range: the scratch range holds
            // exactly the live packs and visited objects of the source range,
            // and no object is duplicated.
            let pack_rows = scalar(
                tx,
                &format!("SELECT count(*) FROM {scratch}.pack WHERE pack_id BETWEEN {lo} AND {hi}"),
                Vec::new(),
            )?;
            let objects = scalar(
                tx,
                &format!(
                    "SELECT count(*) FROM {scratch}.object_location WHERE pack_id BETWEEN {lo} AND {hi}"
                ),
                Vec::new(),
            )?;
            let distinct = scalar(
                tx,
                &format!(
                    "SELECT count(DISTINCT object_id) FROM {scratch}.object_location WHERE pack_id BETWEEN {lo} AND {hi}"
                ),
                Vec::new(),
            )?;
            let source_objects = scalar(
                tx,
                &format!(
                    "SELECT count(*) FROM {schema}.object_location l \
                     WHERE l.pack_id BETWEEN {lo} AND {hi} \
                     AND EXISTS (SELECT 1 FROM pg_temp.gc_visited v WHERE v.object_id=l.object_id)"
                ),
                Vec::new(),
            )?;
            let source_distinct = scalar(
                tx,
                &format!(
                    "SELECT count(DISTINCT l.object_id) FROM {schema}.object_location l \
                     WHERE l.pack_id BETWEEN {lo} AND {hi} \
                     AND EXISTS (SELECT 1 FROM pg_temp.gc_visited v WHERE v.object_id=l.object_id)"
                ),
                Vec::new(),
            )?;
            if pack_rows != expected_packs
                || objects != source_objects
                || distinct != source_distinct
                || objects != distinct
            {
                return Err(HistoryError::Integrity("compaction batch count"));
            }
            Ok(())
        })?;
        peak_temp = peak_temp.max(peak_sample(session, scratch, &wal_start)?);
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
    peak_temp = peak_temp.max(peak_sample(session, scratch, &wal_start)?);
    let (objects, distinct, pack_rows) = stage(session, |tx| {
        let objects = scalar(
            tx,
            &format!("SELECT count(*) FROM {scratch}.object_location"),
            Vec::new(),
        )?;
        let distinct = scalar(
            tx,
            &format!("SELECT count(DISTINCT object_id) FROM {scratch}.object_location"),
            Vec::new(),
        )?;
        let pack_rows = scalar(
            tx,
            &format!("SELECT count(*) FROM {scratch}.pack"),
            Vec::new(),
        )?;
        Ok((objects, distinct, pack_rows))
    })?;
    if distinct != mark.objects().len() as i64
        || objects != distinct
        || pack_rows != mark.objects().packs().len() as i64
    {
        return Err(HistoryError::Integrity("compaction object count"));
    }
    let (canonical, stored, metadata) = retained_totals(session, schema)?;
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
