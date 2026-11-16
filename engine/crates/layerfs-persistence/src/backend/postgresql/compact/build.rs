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
    let list = names
        .iter()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(",");
    stage(session, |tx| {
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
    })
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
    let packs: Vec<i64> = mark.objects().packs().to_vec();
    let chunk = 512usize;
    for batch in packs.chunks(chunk) {
        let lo = batch[0];
        let hi = batch[batch.len() - 1];
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
            Ok(())
        })?;
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
    if distinct != mark.objects().len() as i64 || objects != distinct {
        return Err(HistoryError::Integrity("compaction object count"));
    }
    if pack_rows != mark.objects().packs().len() as i64 {
        return Err(HistoryError::Integrity("compaction pack count"));
    }
    let (canonical, stored, metadata) = retained_totals(session, schema)?;
    let dead = DeadTotals {
        canonical: tuple.canonical - canonical,
        stored: tuple.stored - stored,
        metadata: tuple.metadata - metadata,
        logical: 0,
    };
    let scratch_bytes = relation_bytes(session, scratch, &["pack", "object_location"])?;
    Ok(Built {
        scratch: scratch.to_owned(),
        scratch_bytes,
        dead,
    })
}
