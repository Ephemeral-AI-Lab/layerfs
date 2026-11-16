//! §8 PostgreSQL Store compaction: Phase M mark, Phase A build, Phase B swap
//! and Phase C cleanup.
//!
//! One call takes the T6 Store-scoped exclusive intent (a session advisory
//! key held through cleanup), records the §8 mark tuple as its first
//! statement, runs the T7.2 mark, and decides from the wholly dead pack
//! bytes. A `rebuild` copies only retained `pack`/`object_location` rows into
//! a scratch schema, swaps the two tables in one short transaction at
//! `G -> G + 1`, and deletes only the ids the mark persisted in `gc_dead`.
//! The op row in `gc_op` names the scratch schema before it is created, so a
//! crash (or a lost COMMIT reply) is judged by the next `store.compact`:
//! generation `G` removes the artifact, `G + 1` resumes cleanup.

use super::access::compaction_missing;
use super::session::{
    connection::Session,
    transaction::{Mode, Transaction},
};
use crate::backend::records::Param;
use crate::backend::sqlite::compact::{
    CompactFailpoint, CompactKeptWaste, CompactPath, CompactReceipt, CompactRequest,
    CompactResidue, CompactZeroReason,
};
use crate::compact_metrics::{
    self, CompactOutcome, CompactPhase, CompactRefusal, CompactRunObservation,
};
use crate::history::GcTuple;
use crate::{MarkResult, PostgresHandles};
use layerfs_history::error::{HistoryError, HistoryResult, OpConflict};
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::HistoryCatalog;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// The §3 rebuild threshold: rebuild when dead * 10 >= stored.
const THRESHOLD: u64 = 10;
/// `lfs_gc_<hex>`: the deterministic scratch schema name.
const SCRATCH_PREFIX: &str = "lfs_gc_";

/// The three wholly dead domains the swap records before COMMIT.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DeadTotals {
    canonical: i64,
    stored: i64,
    metadata: i64,
    logical: i64,
}

/// One compaction op row, as the next call judges it.
#[derive(Clone, Debug)]
struct OpRow {
    generation: i64,
    phase: String,
    artifact: Option<String>,
    dead: DeadTotals,
    totals_applied: bool,
    pid: Option<i64>,
    backend_start: Option<String>,
}

/// What Phase A leaves for Phase B.
struct Built {
    scratch: String,
    scratch_bytes: u64,
    dead: DeadTotals,
}

/// What the swap records for the report and Phase C.
struct Swapped {
    physical_before: u64,
    physical_after: u64,
    peak_temp: u64,
    dead: DeadTotals,
}

/// The phase clock and §11 values one measured PostgreSQL compaction observes.
///
/// Phase M times the intent, the mark and the wholly-dead-pack decision, Phase
/// A the scratch copy, Phase B the swap, and Phase C the cleanup — also when a
/// later call resumes an installed `G + 1` leftover. The `pg_` phase values
/// keep these distinct from the SQLite §7 phases in the process totals.
struct RunMetrics {
    started: Instant,
    phase: Option<(CompactPhase, Instant)>,
    phase_nanos: Vec<(CompactPhase, u64)>,
    generation_before: u64,
    generation_after: u64,
    dead_bytes_bound: u64,
    dead_bytes_refined: u64,
    kept_waste: CompactKeptWaste,
    residue_bytes: u64,
    leftover_settled: bool,
}

impl RunMetrics {
    fn start() -> Self {
        Self {
            started: Instant::now(),
            phase: None,
            phase_nanos: Vec::new(),
            generation_before: 0,
            generation_after: 0,
            dead_bytes_bound: 0,
            dead_bytes_refined: 0,
            kept_waste: CompactKeptWaste::default(),
            residue_bytes: 0,
            leftover_settled: false,
        }
    }

    /// Ends the current phase and starts `phase`; a phase is inclusive wall
    /// time, never an exclusive CPU decomposition.
    fn begin_phase(&mut self, phase: CompactPhase) {
        self.end_phase();
        self.phase = Some((phase, Instant::now()));
    }

    fn end_phase(&mut self) {
        if let Some((phase, started)) = self.phase.take() {
            self.phase_nanos.push((
                phase,
                u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            ));
        }
    }

    /// Records this run once, whatever its result. The outcome and refusal
    /// come from the typed error, so a `Busy`/`Unresolved` refusal is counted
    /// without changing the error the caller receives.
    fn finish(mut self, result: &HistoryResult<CompactReceipt>) {
        self.end_phase();
        let total_nanos = u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let (outcome, refusal) = match result {
            Ok(receipt) => {
                // A resumed `G + 1` round reports the original run's
                // generation, so the receipt is the authority here.
                self.generation_before = receipt.generation_before;
                self.generation_after = receipt.generation_after;
                self.dead_bytes_bound = receipt.dead_bytes_bound;
                self.dead_bytes_refined = receipt.dead_bytes_refined;
                self.kept_waste = receipt.kept_waste.clone();
                self.residue_bytes = receipt.residue.bytes;
                let outcome = match receipt.path {
                    CompactPath::Rebuild => CompactOutcome::Rebuild,
                    CompactPath::None => CompactOutcome::None,
                };
                (outcome, None)
            }
            Err(error) => {
                if !error.unknown() {
                    self.generation_after = self.generation_before;
                }
                (
                    if error.unknown() {
                        CompactOutcome::Uncertain
                    } else {
                        CompactOutcome::Failed
                    },
                    match error {
                        HistoryError::Busy | HistoryError::BusyOp(_) => Some(CompactRefusal::Busy),
                        HistoryError::Unresolved(_) => Some(CompactRefusal::Unresolved),
                        _ => None,
                    },
                )
            }
        };
        compact_metrics::record_run(&CompactRunObservation {
            outcome,
            generation_before: self.generation_before,
            generation_after: self.generation_after,
            dead_bytes_bound: self.dead_bytes_bound,
            dead_bytes_refined: self.dead_bytes_refined,
            kept_waste: self.kept_waste,
            residue_bytes: self.residue_bytes,
            phase_nanos: self.phase_nanos,
            total_nanos,
            leftover_settled: self.leftover_settled,
            refusal,
        });
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

fn scratch_name(op_id: &[u8; 16]) -> String {
    format!("{SCRATCH_PREFIX}{}", hex(op_id))
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// One write transaction on the Store session; scratch/DDL work never waits
/// for the publication lane (the session advisory key is the exclusion).
fn stage<T>(
    session: &Arc<Session>,
    body: impl FnOnce(&Transaction<'_>) -> HistoryResult<T>,
) -> HistoryResult<T> {
    session.run_as(Mode::Stage, body)
}

fn scalar(tx: &Transaction<'_>, sql: &str, params: Vec<Param>) -> HistoryResult<i64> {
    let rows = tx.query(sql, params)?;
    if rows.len() != 1 {
        return Err(HistoryError::Integrity("compaction scalar"));
    }
    rows[0].get(0).map_err(Into::into)
}

fn scalar_session(session: &Arc<Session>, sql: &str, params: Vec<Param>) -> HistoryResult<i64> {
    stage(session, |tx| scalar(tx, sql, params))
}

fn read_generation(session: &Arc<Session>, schema: &str) -> HistoryResult<i64> {
    scalar_session(
        session,
        &format!("SELECT catalog_incarnation FROM {schema}.history_meta WHERE id=1"),
        Vec::new(),
    )
}

fn read_op(session: &Arc<Session>, schema: &str) -> HistoryResult<Option<OpRow>> {
    stage(session, |tx| {
        let rows = tx.query(
            &format!(
                "SELECT generation,phase,artifact,dead_canonical,dead_stored,dead_metadata,dead_logical,totals_applied,pid,backend_start FROM {schema}.gc_op WHERE id=1"
            ),
            Vec::new(),
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        Ok(Some(OpRow {
            generation: row.get(0)?,
            phase: row.get(1)?,
            artifact: row.get(2)?,
            dead: DeadTotals {
                canonical: row.get(3)?,
                stored: row.get(4)?,
                metadata: row.get(5)?,
                logical: row.get(6)?,
            },
            totals_applied: row.get::<i64>(7)? != 0,
            pid: row.get(8)?,
            backend_start: row.get(9)?,
        }))
    })
}

fn set_phase(session: &Arc<Session>, schema: &str, phase: &str) -> HistoryResult<()> {
    stage(session, |tx| {
        tx.query(
            &format!("UPDATE {schema}.gc_op SET phase=$1 WHERE id=1"),
            vec![Param::Text(phase.to_owned())],
        )?;
        Ok(())
    })
}

fn stop(
    request: &CompactRequest,
    point: CompactFailpoint,
    session: &Arc<Session>,
    schema: &str,
    phase: &str,
) -> HistoryResult<()> {
    if request.failpoint != Some(point) {
        return Ok(());
    }
    set_phase(session, schema, phase)?;
    Err(HistoryError::UnknownOutcome)
}

/// The set of live object ids and live pack ids, as session temporary tables
/// the Phase A copy and the dead-set query read.
fn load_visited(session: &Arc<Session>, schema: &str, mark: &MarkResult) -> HistoryResult<()> {
    let objects: Vec<Vec<u8>> = mark
        .objects()
        .objects()
        .iter()
        .map(|id| id.as_bytes().to_vec())
        .collect();
    let packs: Vec<i64> = mark.objects().packs().to_vec();
    stage(session, |tx| {
        tx.batch(
            "DROP TABLE IF EXISTS pg_temp.gc_visited;\
             DROP TABLE IF EXISTS pg_temp.gc_live_pack;\
             CREATE TEMP TABLE pg_temp.gc_visited(object_id bytea PRIMARY KEY) ON COMMIT PRESERVE ROWS;\
             CREATE TEMP TABLE pg_temp.gc_live_pack(pack_id bigint PRIMARY KEY) ON COMMIT PRESERVE ROWS;",
        )?;
        let bytes = objects.iter().map(Vec::len).sum::<usize>() as u64;
        tx.typed(
            "INSERT INTO pg_temp.gc_visited(object_id) SELECT unnest($1::bytea[])",
            &[&objects],
            bytes,
        )?;
        tx.typed(
            "INSERT INTO pg_temp.gc_live_pack(pack_id) SELECT unnest($1::bigint[])",
            &[&packs],
            8 * packs.len() as u64,
        )?;
        let _ = schema;
        Ok(())
    })
}

/// Phase M: write `gc_op` (phase mark) and the persisted `gc_dead` set in
/// batches. The set is a membership, not a predicate, so a row created after
/// the mark is never collected.
fn persist_mark(
    session: &Arc<Session>,
    schema: &str,
    mark: &MarkResult,
    tuple: &GcTuple,
    scratch: &str,
    request: &CompactRequest,
) -> HistoryResult<DeadTotals> {
    let digest = mark.live_heads_digest().to_vec();
    stage(session, |tx| {
        tx.query(&format!("DELETE FROM {schema}.gc_dead"), Vec::new())?;
        tx.batch(&format!(
            "INSERT INTO {schema}.gc_dead(kind,id) \
             SELECT 'commit',c.commit_id FROM {schema}.\"commit\" c \
              WHERE NOT EXISTS (SELECT 1 FROM {schema}.kept_node k WHERE k.kind='commit' AND k.node_id=c.commit_id) \
             UNION ALL \
             SELECT 'layer',l.layer_id FROM {schema}.layer l \
              WHERE NOT EXISTS (SELECT 1 FROM {schema}.kept_node k WHERE k.kind='layer' AND k.node_id=l.layer_id) \
             UNION ALL \
             SELECT 'branch',b.branch_id FROM {schema}.branch b \
              WHERE NOT EXISTS (SELECT 1 FROM {schema}.kept_node k WHERE k.kind='branch' AND k.node_id=b.branch_id) \
             UNION ALL \
             SELECT 'stack',s.layer_stack_id FROM {schema}.layer_stack s \
              WHERE NOT EXISTS (SELECT 1 FROM {schema}.kept_node k WHERE k.kind='stack' AND k.node_id=s.layer_stack_id) \
             UNION ALL \
             SELECT 'signature',int8send(cs.slot) FROM {schema}.content_signature cs \
              WHERE NOT EXISTS (SELECT 1 FROM pg_temp.gc_visited v WHERE v.object_id=cs.object_id) \
             UNION ALL \
             SELECT 'digest',fd.digest FROM {schema}.file_digest fd \
              WHERE NOT EXISTS (SELECT 1 FROM pg_temp.gc_visited v WHERE v.object_id=fd.root)"
        ))?;
        let logical = scalar(
            tx,
            &format!(
                "SELECT COALESCE((SELECT SUM(c.logical)::bigint FROM {schema}.\"commit\" c \
                   WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='commit' AND d.id=c.commit_id)),0) \
                  + COALESCE((SELECT SUM(l.logical)::bigint FROM {schema}.layer l \
                   WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='layer' AND d.id=l.layer_id)),0)"
            ),
            Vec::new(),
        )?;
        let started = now_ms();
        tx.query(
            &format!(
                "INSERT INTO {schema}.gc_op \
                 (id,generation,phase,scratch_schema,mark_serial,mark_pack_id,mark_digest,mark_totals,mark_stamp,started_ms,dead_canonical,dead_stored,dead_metadata,dead_logical,totals_applied,artifact,pid,backend_start,usename,incarnation) \
                 VALUES (1,$1,'mark',$2,$3,$4,$5,int8send($6)||int8send($7)||int8send($8)||int8send($9),$10,$11,0,0,0,$12,FALSE,$2,pg_backend_pid(),(SELECT backend_start::text FROM pg_stat_activity WHERE pid=pg_backend_pid()),(SELECT usename::text FROM pg_stat_activity WHERE pid=pg_backend_pid()),$1) \
                 ON CONFLICT (id) DO UPDATE SET generation=EXCLUDED.generation,phase=EXCLUDED.phase,scratch_schema=EXCLUDED.scratch_schema,mark_serial=EXCLUDED.mark_serial,mark_pack_id=EXCLUDED.mark_pack_id,mark_digest=EXCLUDED.mark_digest,mark_totals=EXCLUDED.mark_totals,mark_stamp=EXCLUDED.mark_stamp,started_ms=EXCLUDED.started_ms,dead_canonical=0,dead_stored=0,dead_metadata=0,dead_logical=EXCLUDED.dead_logical,totals_applied=FALSE,artifact=EXCLUDED.artifact,pid=EXCLUDED.pid,backend_start=EXCLUDED.backend_start,usename=EXCLUDED.usename,incarnation=EXCLUDED.incarnation"
            ),
            vec![
                Param::I64(tuple.generation),
                Param::Text(scratch.to_owned()),
                Param::I64(tuple.next_history_serial),
                Param::I64(tuple.next_pack_id),
                Param::Bytes(digest),
                Param::I64(tuple.logical),
                Param::I64(tuple.canonical),
                Param::I64(tuple.stored),
                Param::I64(tuple.metadata),
                Param::I64(tuple.stamp),
                Param::I64(started),
                Param::I64(logical),
            ],
        )?;
        let _ = request;
        Ok(DeadTotals {
            logical,
            ..DeadTotals::default()
        })
    })
}

fn clear_round(session: &Arc<Session>, schema: &str) -> HistoryResult<()> {
    stage(session, |tx| {
        tx.batch(&format!(
            "DELETE FROM {schema}.gc_dead; DELETE FROM {schema}.gc_op;"
        ))?;
        Ok(())
    })
}

/// The stored bytes of every pack the mark retained, in the current Store.
fn live_stored(session: &Arc<Session>, schema: &str) -> HistoryResult<i64> {
    scalar_session(
        session,
        &format!(
            "SELECT COALESCE(SUM(p.length)::bigint,0) FROM {schema}.pack p \
             WHERE EXISTS (SELECT 1 FROM pg_temp.gc_live_pack v WHERE v.pack_id=p.pack_id)"
        ),
        Vec::new(),
    )
}

fn total_stored(session: &Arc<Session>, schema: &str) -> HistoryResult<i64> {
    scalar_session(
        session,
        &format!("SELECT COALESCE(SUM(length)::bigint,0) FROM {schema}.pack"),
        Vec::new(),
    )
}

fn retained_totals(session: &Arc<Session>, schema: &str) -> HistoryResult<(i64, i64, i64)> {
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

fn relation_bytes(session: &Arc<Session>, schema: &str, names: &[&str]) -> HistoryResult<u64> {
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

fn table_storage(
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

fn create_scratch(session: &Arc<Session>, schema: &str, scratch: &str) -> HistoryResult<()> {
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
fn build(
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

/// The §6.4 live-heads digest, computed inside one transaction (the mark's
/// SQL is the same vector).
fn digest(tx: &Transaction<'_>) -> HistoryResult<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/live-heads/v1\0");
    let stacks = tx.query(
        "SELECT s.layer_stack_id, s.head_layer_id FROM layer_stack s \
         WHERE NOT EXISTS (SELECT 1 FROM layer_stack_retired r WHERE r.layer_stack_id=s.layer_stack_id) \
         ORDER BY s.layer_stack_id",
        Vec::new(),
    )?;
    for row in &stacks {
        let id: Vec<u8> = row.get(0)?;
        let head: Vec<u8> = row.get(1)?;
        if id.len() != 17 || head.len() != 33 {
            return Err(HistoryError::Integrity("live-heads stack width"));
        }
        hasher.update(&id);
        hasher.update(&head);
    }
    let branches = tx.query(
        "SELECT b.branch_id, b.head_commit_id, b.base_layer_id FROM branch b \
         WHERE NOT EXISTS (SELECT 1 FROM branch_retired r WHERE r.branch_id=b.branch_id) \
         AND NOT EXISTS (SELECT 1 FROM layer_stack_retired s WHERE s.layer_stack_id=b.layer_stack_id) \
         ORDER BY b.branch_id",
        Vec::new(),
    )?;
    for row in &branches {
        let id: Vec<u8> = row.get(0)?;
        let head: Option<Vec<u8>> = row.get(1)?;
        let base: Vec<u8> = row.get(2)?;
        if id.len() != 17 || base.len() != 33 {
            return Err(HistoryError::Integrity("live-heads branch width"));
        }
        hasher.update(&id);
        match head {
            Some(head) if head.len() == 33 => hasher.update(&head),
            Some(_) => return Err(HistoryError::Integrity("live-heads commit width")),
            None => hasher.update(&[0u8; 33]),
        };
        hasher.update(&base);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// The strength order of §8 step 10: off < local < remote_write < on <
/// remote_apply.
fn sync_rank(setting: &str) -> u8 {
    match setting {
        "local" => 1,
        "remote_write" => 2,
        "on" => 3,
        "remote_apply" => 4,
        _ => 0,
    }
}

fn sync_name(rank: u8) -> &'static str {
    match rank {
        1 => "local",
        2 => "remote_write",
        3 => "on",
        4 => "remote_apply",
        _ => "off",
    }
}

/// The swap's effective strength (§8 step 11): the server's setting or the
/// Store's frozen setting, whichever is stronger, but never below `local`.
fn effective_rank(server: &str, stored: &str) -> u8 {
    sync_rank(server).max(sync_rank(stored)).max(1)
}

/// §8 step 10: the swap waits iff the effective level is at least
/// `remote_write` and `synchronous_standby_names` names a quorum. The setting
/// is parsed as PostgreSQL does: `FIRST n (names)`, `ANY n (names)`, a bare
/// `names` list (which is `FIRST` with `n` equal to the listed names), and the
/// lone `*` that matches any standby. A matching standby must be streaming
/// with the `sync_state` that form assigns it, so one of an `ANY 2` pair is
/// still refused.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct StandbyRequirement {
    /// `FIRST` counts the named standbys in list order; `ANY` counts any
    /// `count` of them. A bare list is `FIRST`.
    first: bool,
    count: usize,
    /// The configured standby names, in priority order.
    members: Vec<String>,
    /// The lone `*`, which matches any standbys.
    any_name: bool,
}

/// The pure parse of `synchronous_standby_names`; `None` when the setting is
/// empty (no standby required).
fn standby_requirement(names: &str) -> Option<StandbyRequirement> {
    let names = names.trim();
    if names.is_empty() {
        return None;
    }
    let lower = names.to_ascii_lowercase();
    let (first, rest) = if lower.starts_with("first") {
        (true, names.get(5..).unwrap_or("").trim())
    } else if lower.starts_with("any") {
        (false, names.get(3..).unwrap_or("").trim())
    } else {
        (true, names)
    };
    // `FIRST n (list)` / `ANY n (list)`: a leading count, then the list.
    let (count, list) = match rest.split_once(char::is_whitespace) {
        Some((head, tail)) if head.bytes().all(|byte| byte.is_ascii_digit()) => {
            match head.parse::<usize>() {
                Ok(count) => (count, tail.trim()),
                Err(_) => return None,
            }
        }
        _ if rest.starts_with('(') => (1, rest),
        _ => (0, rest),
    };
    let members: Vec<String> = list
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split(',')
        .map(str::trim)
        .filter(|member| !member.is_empty())
        .map(|member| member.trim_matches('"').to_owned())
        .collect();
    let any_name = members.len() == 1 && members[0] == "*";
    let count = if count == 0 { members.len() } else { count };
    Some(StandbyRequirement {
        first,
        count,
        members,
        any_name,
    })
}

/// One `pg_stat_replication` row, as the predicate reads it.
struct Replica {
    name: String,
    streaming: bool,
    sync_state: String,
}

/// True when the streaming named standby is one PostgreSQL would count as
/// synchronous for this form: `sync` for `FIRST`, `quorum` (or `sync`) for
/// `ANY`.
fn replica_counts(replica: &Replica, first: bool) -> bool {
    if !replica.streaming {
        return false;
    }
    if first {
        replica.sync_state == "sync"
    } else {
        matches!(replica.sync_state.as_str(), "sync" | "quorum")
    }
}

/// The §8 step 10 predicate, without a server: `rank` is the effective
/// `synchronous_commit` strength, `names` the setting and `replicas` the
/// rows `pg_stat_replication` reports.
fn standby_satisfied(rank: u8, names: &str, replicas: &[Replica]) -> bool {
    if rank < 2 {
        return true;
    }
    let Some(requirement) = standby_requirement(names) else {
        return true;
    };
    if requirement.count == 0 {
        return true;
    }
    if requirement.any_name {
        let matched = replicas
            .iter()
            .filter(|replica| replica_counts(replica, requirement.first))
            .count();
        return matched >= requirement.count;
    }
    let mut matched = 0;
    for member in &requirement.members {
        matched += replicas
            .iter()
            .filter(|replica| replica.name == *member && replica_counts(replica, requirement.first))
            .count();
        if matched >= requirement.count {
            return true;
        }
    }
    false
}

/// The effective `synchronous_commit` and whether the standby predicate
/// applies, read from the server and the Store's frozen setting. The read is
/// a read-only snapshot: a write transaction has already applied the Store
/// setting with `SET LOCAL`, so `current_setting` there would only echo it.
fn effective_sync_rank(session: &Arc<Session>) -> HistoryResult<u8> {
    session.run_as(Mode::Read, |tx| {
        let rows = tx.query("SELECT current_setting('synchronous_commit')", Vec::new())?;
        let server: String = rows[0].get(0)?;
        let stored = session.settings.synchronous_commit.setting();
        Ok(effective_rank(&server, stored))
    })
}

/// §8 step 10: the standby predicate checked before Phase M.
fn standby_precheck(
    session: &Arc<Session>,
    rank: u8,
    op_id: &[u8; 16],
    phase: &str,
) -> HistoryResult<()> {
    stage(session, |tx| {
        let rows = tx.query(
            "SELECT current_setting('synchronous_standby_names')",
            Vec::new(),
        )?;
        let names: String = rows[0].get(0)?;
        standby_check(tx, rank, &names, op_id, phase)
    })
}

fn standby_check(
    tx: &Transaction<'_>,
    rank: u8,
    names: &str,
    op_id: &[u8; 16],
    phase: &str,
) -> HistoryResult<()> {
    if rank < 2 || names.trim().is_empty() {
        return Ok(());
    }
    let rows = tx.query(
        "SELECT application_name::text, state::text, sync_state::text \
         FROM pg_stat_replication",
        Vec::new(),
    )?;
    let replicas = rows
        .iter()
        .map(|row| {
            let state: String = row.get(1)?;
            Ok(Replica {
                name: row.get(0)?,
                streaming: state == "streaming",
                sync_state: row.get(2)?,
            })
        })
        .collect::<HistoryResult<Vec<_>>>()?;
    if !standby_satisfied(rank, names, &replicas) {
        // The quorum is missing, not the Store: name the waiting operation
        // instead of a holderless Busy.
        return Err(HistoryError::BusyOp(Box::new(OpConflict::new(
            GcIntentKind::Compaction,
            GcScopeKind::Store,
            "store",
            Some(phase.to_owned()),
            hex(op_id),
            false,
        ))));
    }
    Ok(())
}

/// The recorded owner backend's `pg_stat_activity` state: `Some` while a
/// backend with the recorded `(pid, backend_start)` is alive, `None` when it
/// has provably ended.
fn current_backend_pid(session: &Arc<Session>) -> HistoryResult<i64> {
    scalar_session(session, "SELECT pg_backend_pid()", Vec::new())
}

fn owner_state(session: &Arc<Session>, op: &OpRow) -> HistoryResult<Option<String>> {
    let (Some(pid), Some(start)) = (op.pid, op.backend_start.as_deref()) else {
        return Ok(None);
    };
    stage(session, |tx| {
        let rows = tx.query(
            "SELECT state::text FROM pg_stat_activity \
              WHERE pid=$1 AND backend_start::text=$2",
            vec![Param::I64(pid), Param::Text(start.to_owned())],
        )?;
        rows.first()
            .map(|row| row.get(0))
            .transpose()
            .map_err(Into::into)
    })
}

/// The recorded owner's client_addr, when the server still reports it.
fn owner_client_addr(session: &Arc<Session>, op: &OpRow) -> HistoryResult<Option<String>> {
    let (Some(pid), Some(start)) = (op.pid, op.backend_start.as_deref()) else {
        return Ok(None);
    };
    stage(session, |tx| {
        let rows = tx.query(
            "SELECT client_addr::text FROM pg_stat_activity \
              WHERE pid=$1 AND backend_start::text=$2",
            vec![Param::I64(pid), Param::Text(start.to_owned())],
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        Ok(row.get::<Option<String>>(0)?)
    })
}

/// §9/§12: bounded poll for a recorded backend that is already ending. A live
/// backend is never terminated. The caller holds the exclusive Store key, so a
/// recorded owner that were still inside this op would hold it and the claim
/// would have failed; an idle backend is a healthy pooled connection, not a
/// dead one, and must be left alone.
fn poll_owner_gone(session: &Arc<Session>, op: &OpRow, polls: u32) -> HistoryResult<bool> {
    for _ in 0..polls {
        if owner_state(session, op)?.is_none() {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Ok(false)
}

fn drop_scratch(session: &Arc<Session>, scratch: &str) -> HistoryResult<()> {
    stage(session, |tx| {
        tx.batch(&format!("DROP SCHEMA IF EXISTS {scratch} CASCADE"))?;
        Ok(())
    })
}

fn compaction_intent(session: &Arc<Session>, schema: &str) -> HistoryResult<Option<[u8; 16]>> {
    stage(session, |tx| {
        let rows = tx.query(
            &format!(
                "SELECT op_id FROM {schema}.gc_intent WHERE kind='compaction' ORDER BY updated_ms LIMIT 1"
            ),
            Vec::new(),
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let bytes: Vec<u8> = row.get(0)?;
        let op: [u8; 16] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| HistoryError::Integrity("compaction intent op"))?;
        Ok(Some(op))
    })
}

/// §9: judge a recorded leftover. `G` removes the artifact and clears the
/// op; `G + 1` finishes Phase C and answers the installed outcome. `None`
/// means there was nothing to resume.
fn settle_leftover(
    handles: &PostgresHandles,
    session: &Arc<Session>,
    schema: &str,
    metrics: &mut RunMetrics,
) -> HistoryResult<Option<CompactReceipt>> {
    let Some(op) = read_op(session, schema)? else {
        return Ok(None);
    };
    let current = read_generation(session, schema)?;
    // The op judged the Store at `current`; after a resume this session must
    // fence its later writes at that generation, not the one it opened at.
    session.incarnation.store(
        u64::try_from(current).unwrap_or(0),
        std::sync::atomic::Ordering::SeqCst,
    );
    if current == op.generation {
        // The caller's own session is the recorded owner when it retries its
        // own uncertain round: adopt it without terminating itself.
        if op.pid != Some(current_backend_pid(session)?) {
            // The caller holds the exclusive Store key: a recorded owner that
            // is still inside this op would hold it, so the claim above proves
            // the op is no longer running. A recorded backend the server still
            // reports is a healthy pooled connection, not a dead owner:
            // §9/§12 do not terminate it. Poll briefly so a backend that is
            // already ending is allowed to finish, then settle under the key.
            if let Some(state) = owner_state(session, &op)? {
                let conflict = || -> HistoryResult<HistoryError> {
                    let op_id = compaction_intent(session, schema)?.unwrap_or([0u8; 16]);
                    let client_addr = owner_client_addr(session, &op)?;
                    Ok(HistoryError::BusyOp(Box::new(
                        OpConflict::new(
                            GcIntentKind::Compaction,
                            GcScopeKind::Store,
                            "store",
                            Some(op.phase.clone()),
                            hex(&op_id),
                            false,
                        )
                        .with_backend(op.pid, client_addr, op.backend_start.clone()),
                    )))
                };
                if state != "idle" {
                    // A live owner stays Busy, naming the op and the backend.
                    return Err(conflict()?);
                }
                // The backend is alive but not executing the op: it is a
                // healthy pooled connection, so poll it until it ends and
                // settle under the exclusive Store key.
                let _ = poll_owner_gone(session, &op, 40)?;
            }
        }
        metrics.begin_phase(CompactPhase::PostgresCleanup);
        if let Some(artifact) = &op.artifact {
            drop_scratch(session, artifact)?;
        }
        clear_round(session, schema)?;
        stage(session, |tx| {
            tx.batch(&format!(
                "DELETE FROM {schema}.gc_intent WHERE kind='compaction'"
            ))?;
            Ok(())
        })?;
        metrics.end_phase();
        metrics.leftover_settled = true;
        return Ok(None);
    }
    if current != op.generation + 1 {
        return Err(HistoryError::Integrity("compaction op generation"));
    }
    metrics.begin_phase(CompactPhase::PostgresCleanup);
    let op_id = compaction_intent(session, schema)?;
    cleanup(handles, session, schema, op.dead, op.totals_applied, op_id)?;
    metrics.end_phase();
    metrics.leftover_settled = true;
    Ok(Some(report(
        CompactPath::Rebuild,
        u64::try_from(op.generation).unwrap_or(0),
        u64::try_from(current).unwrap_or(0),
        0,
        0,
        u64::try_from(op.dead.logical).unwrap_or(0),
        0,
        0,
        CompactKeptWaste::default(),
        0,
        CompactResidue::default(),
        0,
        0,
    )))
}

/// Phase B: lock every base table, re-check the mark tuple, swap the two
/// payload tables, bump the generation and drop the scratch schema in one
/// short transaction. `55P03` is `Busy`; any tuple change is `Conflict` with
/// no writes.
fn swap(
    _handles: &PostgresHandles,
    session: &Arc<Session>,
    schema: &str,
    tuple: &GcTuple,
    mark: &MarkResult,
    built: &Built,
    op_id: &[u8; 16],
) -> HistoryResult<Swapped> {
    let physical_before = relation_bytes(session, schema, &["pack", "object_location"])?;
    let rank = effective_sync_rank(session)?;
    let tables = tables_of(session, schema)?;
    let locked = tables
        .iter()
        .map(|table| format!("{schema}.{}", quote_ident(table)))
        .collect::<Vec<_>>()
        .join(", ");
    let scratch = built.scratch.clone();
    let result = stage(session, |tx| -> HistoryResult<Swapped> {
        tx.batch(&format!(
            "SET LOCAL synchronous_commit TO {}",
            sync_name(rank)
        ))?;
        tx.batch(&format!(
            "LOCK TABLE {locked} IN ACCESS EXCLUSIVE MODE NOWAIT"
        ))?;
        // The tuple the mark recorded, re-read under the lock. Any publication
        // after the mark moved one of these, so nothing is written.
        let now = read_tuple(tx, schema)?;
        if now != *tuple {
            return Err(HistoryError::Conflict("Store moved after the mark"));
        }
        if digest(tx)? != mark.live_heads_digest() {
            return Err(HistoryError::Conflict("live heads moved after the mark"));
        }
        // The ACL lives on the tables this transaction drops, so read it
        // first; the owner and both ACL levels are replayed on the NEW tables
        // after `SET SCHEMA`, when they are the schema's tables again. A
        // `GRANT` before the drop would name the table that is about to go.
        let acls = [
            ("pack", read_acl(tx, schema, "pack")?),
            ("object_location", read_acl(tx, schema, "object_location")?),
        ];
        tx.batch(&format!(
            "DROP TABLE {schema}.object_location;\
             DROP TABLE {schema}.pack;\
             ALTER TABLE {scratch}.object_location SET SCHEMA {schema};\
             ALTER TABLE {scratch}.pack SET SCHEMA {schema};\
             CREATE TRIGGER pack_immutable BEFORE UPDATE OR DELETE ON {schema}.pack \
              FOR EACH ROW EXECUTE FUNCTION {schema}.pack_immutable();"
        ))?;
        // Owner, PUBLIC grants, grant options and column ACLs, on the tables
        // that survive.
        for (table, acl) in &acls {
            apply_acl(tx, schema, table, acl)?;
        }
        verify_swap(tx, schema, mark, tuple, &built.dead)?;
        let applied = scalar(
            tx,
            &format!("SELECT totals_applied FROM {schema}.gc_op WHERE id=1"),
            Vec::new(),
        )?;
        if applied != 0 {
            return Err(HistoryError::Integrity("compaction totals already applied"));
        }
        tx.query(
            &format!(
                "UPDATE {schema}.gc_op SET phase='swap',dead_canonical=$1,dead_stored=$2,dead_metadata=$3 WHERE id=1"
            ),
            vec![
                Param::I64(built.dead.canonical),
                Param::I64(built.dead.stored),
                Param::I64(built.dead.metadata),
            ],
        )?;
        tx.query(
            &format!(
                "UPDATE {schema}.history_meta SET catalog_incarnation={} WHERE id=1",
                tuple.generation + 1
            ),
            Vec::new(),
        )?;
        tx.batch(&format!("DROP SCHEMA {scratch} CASCADE"))?;
        let names: String = tx
            .query(
                "SELECT current_setting('synchronous_standby_names')",
                Vec::new(),
            )?
            .first()
            .ok_or(HistoryError::Integrity("standby names"))?
            .get(0)?;
        standby_check(tx, rank, &names, op_id, "swap")?;
        Ok(Swapped {
            physical_before,
            physical_after: 0,
            peak_temp: built.scratch_bytes,
            dead: built.dead,
        })
    });
    let result = match result {
        Ok(swapped) => swapped,
        Err(HistoryError::Busy) => {
            // §8 step 5: roll back first, then drop the scratch schema in a
            // new transaction, and report Busy with the holder detail.
            let _ = drop_scratch(session, &scratch);
            return Err(swap_busy(session, schema, op_id)?);
        }
        Err(other) => return Err(other),
    };
    // The swap committed at `G + 1`: this session's own fence must follow it,
    // or its next write transaction would (correctly) see a stale generation.
    session.incarnation.store(
        u64::try_from(tuple.generation.saturating_add(1)).unwrap_or(0),
        std::sync::atomic::Ordering::SeqCst,
    );
    let physical_after = relation_bytes(session, schema, &["pack", "object_location"])?;
    Ok(Swapped {
        physical_after,
        ..result
    })
}

/// The Busy a contended swap reports: the compaction identity with the
/// backend holding one of the base tables, read from pg_locks and
/// pg_stat_activity.
fn swap_busy(
    session: &Arc<Session>,
    schema: &str,
    op_id: &[u8; 16],
) -> HistoryResult<HistoryError> {
    let mut conflict = OpConflict::new(
        GcIntentKind::Compaction,
        GcScopeKind::Store,
        "store",
        Some("swap".to_owned()),
        hex(op_id),
        false,
    );
    let holder = stage(session, |tx| {
        let rows = tx.query(
            "SELECT a.pid::bigint, a.client_addr::text, a.backend_start::text \
               FROM pg_locks l JOIN pg_stat_activity a ON a.pid=l.pid \
               JOIN pg_class c ON c.oid=l.relation \
               JOIN pg_namespace n ON n.oid=c.relnamespace \
              WHERE n.nspname=$1 AND c.relkind='r' \
                AND l.mode='AccessExclusiveLock' AND l.granted \
                AND a.pid <> pg_backend_pid() \
              ORDER BY a.backend_start LIMIT 1",
            vec![Param::Text(schema.to_owned())],
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        Ok(Some((
            row.get::<i64>(0)?,
            row.get::<Option<String>>(1)?,
            row.get::<Option<String>>(2)?,
        )))
    })?;
    if let Some((pid, client_addr, backend_start)) = holder {
        conflict = conflict.with_backend(Some(pid), client_addr, backend_start);
    }
    Ok(HistoryError::BusyOp(Box::new(conflict)))
}

fn tables_of(session: &Arc<Session>, schema: &str) -> HistoryResult<Vec<String>> {
    stage(session, |tx| {
        let rows = tx.query(
            "SELECT c.relname::text FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
             WHERE n.nspname=$1 AND c.relkind='r' ORDER BY c.relname",
            vec![Param::Text(schema.to_owned())],
        )?;
        rows.iter()
            .map(|row| row.get::<String>(0).map_err(HistoryError::from))
            .collect()
    })
}

fn read_tuple(tx: &Transaction<'_>, schema: &str) -> HistoryResult<GcTuple> {
    let rows = tx.query(
        &format!(
            "SELECT h.catalog_incarnation,h.next_history_serial,p.next_pack_id,p.next_ordinal,\
             t.logical,t.canonical,t.stored,t.metadata,\
             (SELECT COALESCE(MAX(stamp),0) FROM {schema}.content_signature),\
             (SELECT COUNT(*) FROM {schema}.\"commit\")+(SELECT COUNT(*) FROM {schema}.layer) \
             FROM {schema}.history_meta h, {schema}.store_policy p, {schema}.store_totals t \
             WHERE h.id=1 AND p.id=1 AND t.id=1"
        ),
        Vec::new(),
    )?;
    if rows.len() != 1 {
        return Err(HistoryError::Integrity("Store tuple"));
    }
    let row = &rows[0];
    Ok(GcTuple {
        generation: row.get(0)?,
        next_history_serial: row.get(1)?,
        next_pack_id: row.get(2)?,
        next_ordinal: row.get(3)?,
        logical: row.get(4)?,
        canonical: row.get(5)?,
        stored: row.get(6)?,
        metadata: row.get(7)?,
        stamp: row.get(8)?,
        history_rows: row.get(9)?,
    })
}

/// The owner, table ACL and column ACLs of one payload table, read before the
/// swap drops the table that holds them.
struct TableAcl {
    owner: String,
    table: Vec<AclGrant>,
    columns: Vec<(String, Vec<AclGrant>)>,
}

/// One parsed `aclitem`: the grantee (`PUBLIC` for the empty grantee), the
/// privilege names and whether they carry the grant option.
struct AclGrant {
    grantee: String,
    privileges: Vec<&'static str>,
    grant_option: bool,
}

/// The privilege a PostgreSQL ACL letter names; `MAINTAIN` exists from 17.
fn privilege_name(letter: char) -> Option<&'static str> {
    Some(match letter {
        'r' => "SELECT",
        'w' => "UPDATE",
        'a' => "INSERT",
        'd' => "DELETE",
        'D' => "TRUNCATE",
        'x' => "REFERENCES",
        't' => "TRIGGER",
        'm' => "MAINTAIN",
        _ => return None,
    })
}

/// Parses newline-separated `aclitem`s, dropping the owner's own implicit
/// entry (recreated by `ALTER TABLE ... OWNER TO`) and splitting a privilege
/// list into its plain and grant-option parts.
fn acl_items(acl: &str) -> Vec<AclGrant> {
    let mut grants = Vec::new();
    for line in acl.lines().filter(|line| !line.is_empty()) {
        let Some((grantee, rest)) = line.split_once('=') else {
            continue;
        };
        let Some((letters, grantor)) = rest.split_once('/') else {
            continue;
        };
        if grantee == grantor {
            continue;
        }
        let mut plain = Vec::new();
        let mut optioned = Vec::new();
        let chars: Vec<char> = letters.chars().collect();
        let mut index = 0;
        while index < chars.len() {
            let letter = chars[index];
            let option = chars.get(index + 1) == Some(&'*');
            if let Some(name) = privilege_name(letter) {
                if option {
                    optioned.push(name);
                } else {
                    plain.push(name);
                }
            }
            index += if option { 2 } else { 1 };
        }
        let grantee = if grantee.is_empty() {
            "PUBLIC".to_owned()
        } else {
            quote_ident(grantee)
        };
        if !plain.is_empty() {
            grants.push(AclGrant {
                grantee: grantee.clone(),
                privileges: plain,
                grant_option: false,
            });
        }
        if !optioned.is_empty() {
            grants.push(AclGrant {
                grantee,
                privileges: optioned,
                grant_option: true,
            });
        }
    }
    grants
}

/// Reads one payload table's owner and both ACL levels.
fn read_acl(tx: &Transaction<'_>, schema: &str, table: &str) -> HistoryResult<TableAcl> {
    let rows = tx.query(
        "SELECT pg_get_userbyid(c.relowner)::text, COALESCE(array_to_string(c.relacl,E'\\n'),'') \
         FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
         WHERE n.nspname=$1 AND c.relname=$2",
        vec![
            Param::Text(schema.to_owned()),
            Param::Text(table.to_owned()),
        ],
    )?;
    if rows.len() != 1 {
        return Err(HistoryError::Integrity("compaction owner"));
    }
    let owner: String = rows[0].get(0)?;
    let table_acl: String = rows[0].get(1)?;
    let column_rows = tx.query(
        "SELECT a.attname::text, COALESCE(array_to_string(a.attacl,E'\\n'),'') \
         FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid \
         JOIN pg_namespace n ON n.oid=c.relnamespace \
         WHERE n.nspname=$1 AND c.relname=$2 AND a.attacl IS NOT NULL AND a.attnum>0 \
         ORDER BY a.attnum",
        vec![
            Param::Text(schema.to_owned()),
            Param::Text(table.to_owned()),
        ],
    )?;
    let mut columns = Vec::new();
    for row in &column_rows {
        let name: String = row.get(0)?;
        let acl: String = row.get(1)?;
        columns.push((name, acl_items(&acl)));
    }
    Ok(TableAcl {
        owner,
        table: acl_items(&table_acl),
        columns,
    })
}

/// One `GRANT` statement for an `aclitem`. A column ACL names the column after
/// each privilege (`GRANT SELECT (c), UPDATE (c) ...`); the table ACL has no
/// column list.
fn grant_statement(schema: &str, table: &str, grant: &AclGrant, column: Option<&str>) -> String {
    let privileges = match column {
        Some(column) => grant
            .privileges
            .iter()
            .map(|name| format!("{name} ({})", quote_ident(column)))
            .collect::<Vec<_>>()
            .join(", "),
        None => grant.privileges.join(", "),
    };
    format!(
        "GRANT {privileges} ON TABLE {schema}.{} TO {}{}",
        quote_ident(table),
        grant.grantee,
        if grant.grant_option {
            " WITH GRANT OPTION"
        } else {
            ""
        }
    )
}

/// Replays one table's owner and ACLs on the table that is now in `schema`.
fn apply_acl(tx: &Transaction<'_>, schema: &str, table: &str, acl: &TableAcl) -> HistoryResult<()> {
    tx.batch(&format!(
        "ALTER TABLE {schema}.{} OWNER TO {}",
        quote_ident(table),
        quote_ident(&acl.owner)
    ))?;
    for grant in &acl.table {
        tx.batch(&grant_statement(schema, table, grant, None))?;
    }
    for (column, grants) in &acl.columns {
        for grant in grants {
            tx.batch(&grant_statement(schema, table, grant, Some(column)))?;
        }
    }
    Ok(())
}

fn verify_swap(
    tx: &Transaction<'_>,
    schema: &str,
    mark: &MarkResult,
    tuple: &GcTuple,
    dead: &DeadTotals,
) -> HistoryResult<()> {
    let objects = scalar(
        tx,
        &format!("SELECT count(*) FROM {schema}.object_location"),
        Vec::new(),
    )?;
    let distinct = scalar(
        tx,
        &format!("SELECT count(DISTINCT object_id) FROM {schema}.object_location"),
        Vec::new(),
    )?;
    let packs = scalar(
        tx,
        &format!("SELECT count(*) FROM {schema}.pack"),
        Vec::new(),
    )?;
    if objects != mark.objects().len() as i64
        || distinct != objects
        || packs != mark.objects().packs().len() as i64
    {
        return Err(HistoryError::Integrity("compaction swapped count"));
    }
    let canonical = scalar(
        tx,
        &format!("SELECT COALESCE(SUM(canonical_length)::bigint,0) FROM {schema}.object_location"),
        Vec::new(),
    )?;
    let stored = scalar(
        tx,
        &format!("SELECT COALESCE(SUM(length)::bigint,0) FROM {schema}.pack"),
        Vec::new(),
    )?;
    let metadata = scalar(
        tx,
        &format!("SELECT COALESCE(SUM(length)::bigint,0) FROM {schema}.pack WHERE domain=0"),
        Vec::new(),
    )?;
    if canonical != tuple.canonical - dead.canonical
        || stored != tuple.stored - dead.stored
        || metadata != tuple.metadata - dead.metadata
    {
        return Err(HistoryError::Integrity("compaction swapped totals"));
    }
    let logical = scalar(
        tx,
        &format!("SELECT logical FROM {schema}.store_totals WHERE id=1"),
        Vec::new(),
    )?;
    let sum = scalar(
        tx,
        &format!(
            "SELECT (SELECT COALESCE(SUM(logical)::bigint,0) FROM {schema}.\"commit\")+(SELECT COALESCE(SUM(logical)::bigint,0) FROM {schema}.layer)"
        ),
        Vec::new(),
    )?;
    if logical != sum {
        return Err(HistoryError::Integrity(
            "Store totals disagree with the history rows",
        ));
    }
    if digest(tx)? != mark.live_heads_digest() {
        return Err(HistoryError::Conflict("live heads moved after the mark"));
    }
    Ok(())
}

/// Phase C: delete only the persisted `gc_dead` members, settle the totals
/// once, and clear the op. The Store is usable at `G + 1` throughout.
fn cleanup(
    handles: &PostgresHandles,
    session: &Arc<Session>,
    schema: &str,
    dead: DeadTotals,
    totals_applied: bool,
    op_id: Option<[u8; 16]>,
) -> HistoryResult<()> {
    stage(session, |tx| {
        tx.batch(&format!(
            "DELETE FROM {schema}.\"commit\" c WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='commit' AND d.id=c.commit_id);\
             DELETE FROM {schema}.layer l WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='layer' AND d.id=l.layer_id);\
             DELETE FROM {schema}.branch b WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='branch' AND d.id=b.branch_id);\
             DELETE FROM {schema}.layer_stack s WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='stack' AND d.id=s.layer_stack_id);\
             DELETE FROM {schema}.layer_lineage ll WHERE NOT EXISTS (SELECT 1 FROM {schema}.layer l WHERE l.layer_id=ll.layer_id);\
             DELETE FROM {schema}.commit_lineage cl WHERE NOT EXISTS (SELECT 1 FROM {schema}.\"commit\" c WHERE c.commit_id=cl.commit_id);\
             DELETE FROM {schema}.absorbed a WHERE NOT EXISTS (SELECT 1 FROM {schema}.commit_lineage cl WHERE cl.run=a.run);\
             DELETE FROM {schema}.reach r WHERE r.key NOT IN (SELECT layer_stack_id FROM {schema}.layer_stack UNION SELECT run FROM {schema}.commit_lineage);\
             DELETE FROM {schema}.branch_retired br WHERE NOT EXISTS (SELECT 1 FROM {schema}.branch b WHERE b.branch_id=br.branch_id);\
             DELETE FROM {schema}.layer_stack_retired lr WHERE NOT EXISTS (SELECT 1 FROM {schema}.layer_stack s WHERE s.layer_stack_id=lr.layer_stack_id);\
             DELETE FROM {schema}.path_removed pr WHERE NOT EXISTS (SELECT 1 FROM {schema}.layer_stack s WHERE s.layer_stack_id=pr.layer_stack_id);\
             DELETE FROM {schema}.project_epoch pe WHERE NOT EXISTS (SELECT 1 FROM {schema}.layer_stack s WHERE s.layer_stack_id=pe.layer_stack_id);\
             DELETE FROM {schema}.content_signature cs WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='signature' AND d.id=int8send(cs.slot));\
             DELETE FROM {schema}.file_digest fd WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='digest' AND d.id=fd.digest);\
             DELETE FROM {schema}.binding_hint_block;\
             DELETE FROM {schema}.init_operation;\
             DELETE FROM {schema}.init_entry;\
             DELETE FROM {schema}.init_host_file;"
        ))?;
        if !totals_applied {
            tx.query(
                &format!(
                    "UPDATE {schema}.store_totals SET \
                     canonical=canonical-$1,stored=stored-$2,metadata=metadata-$3,logical=logical-$4 WHERE id=1"
                ),
                vec![
                    Param::I64(dead.canonical),
                    Param::I64(dead.stored),
                    Param::I64(dead.metadata),
                    Param::I64(dead.logical),
                ],
            )?;
            tx.query(
                &format!(
                    "UPDATE {schema}.gc_op SET totals_applied=TRUE,phase='cleanup' WHERE id=1"
                ),
                Vec::new(),
            )?;
        }
        let totals = tx.query(
            &format!(
                "SELECT logical,canonical,stored,metadata FROM {schema}.store_totals WHERE id=1"
            ),
            Vec::new(),
        )?;
        let logical: i64 = totals[0].get(0)?;
        let canonical: i64 = totals[0].get(1)?;
        let stored: i64 = totals[0].get(2)?;
        let metadata: i64 = totals[0].get(3)?;
        let sums = tx.query(
            &format!(
                "SELECT (SELECT COALESCE(SUM(logical)::bigint,0) FROM {schema}.\"commit\")+(SELECT COALESCE(SUM(logical)::bigint,0) FROM {schema}.layer),\
                        (SELECT COALESCE(SUM(canonical_length)::bigint,0) FROM {schema}.object_location),\
                        (SELECT COALESCE(SUM(length)::bigint,0) FROM {schema}.pack),\
                        (SELECT COALESCE(SUM(length)::bigint,0) FROM {schema}.pack WHERE domain=0)"
            ),
            Vec::new(),
        )?;
        let sum_logical: i64 = sums[0].get(0)?;
        let sum_canonical: i64 = sums[0].get(1)?;
        let sum_stored: i64 = sums[0].get(2)?;
        let sum_metadata: i64 = sums[0].get(3)?;
        if logical != sum_logical
            || canonical != sum_canonical
            || stored != sum_stored
            || metadata != sum_metadata
        {
            return Err(HistoryError::Integrity("compaction settled totals"));
        }
        tx.batch(&format!(
            "DELETE FROM {schema}.gc_dead; DELETE FROM {schema}.gc_op;"
        ))?;
        Ok(())
    })?;
    if let Some(op) = op_id {
        let _ = handles.history.gc_release(&op);
    } else {
        stage(session, |tx| {
            tx.batch(&format!(
                "DELETE FROM {schema}.gc_intent WHERE kind='compaction'"
            ))?;
            Ok(())
        })?;
    }
    Ok(())
}

/// One round after admission: mark, decide, build, swap, cleanup.
fn round(
    handles: &PostgresHandles,
    session: &Arc<Session>,
    schema: &str,
    request: &CompactRequest,
    metrics: &mut RunMetrics,
) -> HistoryResult<CompactReceipt> {
    metrics.begin_phase(CompactPhase::PostgresMark);
    let (mark, tuple) = handles.history.mark_gc()?;
    load_visited(session, schema, &mark)?;
    let stored = u64::try_from(total_stored(session, schema)?).unwrap_or(0);
    let live = u64::try_from(live_stored(session, schema)?).unwrap_or(0);
    let dead_bytes = stored.saturating_sub(live);
    let scratch = scratch_name(&request.op_id);
    let mark_dead = persist_mark(session, schema, &mark, &tuple, &scratch, request)?;
    let generation = u64::try_from(tuple.generation).unwrap_or(0);
    let kept = CompactKeptWaste {
        dead_in_retained_packs_bytes: mark.objects().waste().dead_in_retained_packs_bytes(),
        delta_bases_bytes: mark.objects().waste().delta_bases_bytes(),
        delta_base_references: mark
            .objects()
            .waste()
            .delta_base_references()
            .iter()
            .map(|id| *id.as_bytes())
            .collect(),
        inode_blocks_bytes: mark.objects().waste().inode_blocks_bytes(),
    };
    let rebuild = stored > 0 && dead_bytes.saturating_mul(THRESHOLD) >= stored;
    if !rebuild {
        metrics.end_phase();
        clear_round(session, schema)?;
        let _ = handles.history.gc_release(&request.op_id);
        return Ok(report(
            CompactPath::None,
            generation,
            generation,
            dead_bytes,
            dead_bytes,
            0,
            0,
            0,
            kept,
            0,
            CompactResidue::default(),
            stored,
            mark.history().len() as u64,
        ));
    }
    metrics.end_phase();
    stop(
        request,
        CompactFailpoint::PostgresAfterMark,
        session,
        schema,
        "mark",
    )?;
    metrics.begin_phase(CompactPhase::PostgresBuild);
    let mut built = build(session, schema, &scratch, &mark, &tuple)?;
    metrics.end_phase();
    built.dead.logical = mark_dead.logical;
    stop(
        request,
        CompactFailpoint::PostgresAfterBuild,
        session,
        schema,
        "build",
    )?;
    metrics.begin_phase(CompactPhase::PostgresSwap);
    let swapped = swap(
        handles,
        session,
        schema,
        &tuple,
        &mark,
        &built,
        &request.op_id,
    )?;
    metrics.end_phase();
    stop(
        request,
        CompactFailpoint::PostgresAfterSwap,
        session,
        schema,
        "swap",
    )?;
    let logical_discarded = u64::try_from(swapped.dead.logical).unwrap_or(0);
    metrics.begin_phase(CompactPhase::PostgresCleanup);
    cleanup(
        handles,
        session,
        schema,
        swapped.dead,
        false,
        Some(request.op_id),
    )?;
    metrics.end_phase();
    stop(
        request,
        CompactFailpoint::PostgresAfterCleanup,
        session,
        schema,
        "cleanup",
    )?;
    let shrink = swapped
        .physical_before
        .saturating_sub(swapped.physical_after);
    Ok(report(
        CompactPath::Rebuild,
        generation,
        generation + 1,
        dead_bytes,
        dead_bytes,
        logical_discarded,
        0,
        shrink,
        kept,
        swapped.peak_temp,
        CompactResidue::default(),
        stored,
        mark.history().len() as u64,
    ))
}

#[allow(clippy::too_many_arguments)]
fn report(
    path: CompactPath,
    generation_before: u64,
    generation_after: u64,
    dead_bytes: u64,
    dead_refined: u64,
    logical_discarded: u64,
    backend_reusable: u64,
    physical_shrink: u64,
    kept_waste: CompactKeptWaste,
    peak_temp_bytes: u64,
    residue: CompactResidue,
    total_stored: u64,
    history_rows: u64,
) -> CompactReceipt {
    let mut zero_reasons = Vec::new();
    if dead_bytes == 0 {
        let reason = if total_stored == 0 {
            "empty-store"
        } else {
            "no-waste"
        };
        zero_reasons.push(CompactZeroReason {
            field: "deadBytesBound",
            reason,
        });
        zero_reasons.push(CompactZeroReason {
            field: "deadBytesRefined",
            reason,
        });
    }
    if logical_discarded == 0 {
        let reason = if history_rows == 0 {
            "no-history"
        } else if path == CompactPath::None {
            "none-path"
        } else {
            "no-waste"
        };
        zero_reasons.push(CompactZeroReason {
            field: "logicalDiscarded",
            reason,
        });
    }
    if physical_shrink == 0 {
        zero_reasons.push(CompactZeroReason {
            field: "physicalShrink",
            reason: if path == CompactPath::None {
                "none-path"
            } else {
                "no-waste"
            },
        });
    }
    zero_reasons.push(CompactZeroReason {
        field: "backendReusable",
        reason: "postgresql-in-place",
    });
    if kept_waste.is_empty() {
        zero_reasons.push(CompactZeroReason {
            field: "keptWaste",
            reason: "no-waste",
        });
    }
    if peak_temp_bytes == 0 {
        zero_reasons.push(CompactZeroReason {
            field: "peakTempBytes",
            reason: "none-path",
        });
    }
    CompactReceipt {
        path,
        generation_before,
        generation_after,
        dead_bytes_bound: dead_bytes,
        dead_bytes_refined: dead_refined,
        logical_discarded,
        backend_reusable,
        physical_shrink,
        kept_waste,
        peak_temp_bytes,
        residue,
        estimate: true,
        zero_reasons,
    }
}

impl PostgresHandles {
    /// Runs one §8 PostgreSQL Store compaction on this Store's session.
    ///
    /// The call takes the Store's exclusive intent (a session advisory key)
    /// through cleanup, marks at `G`, and on the rebuild path copies the two
    /// payload tables, swaps them in one short transaction at `G + 1`, and
    /// deletes only the persisted dead set. The caller keeps its session; the
    /// Store is usable at `G + 1` from the swap on.
    pub fn compact(&self, request: &CompactRequest) -> HistoryResult<CompactReceipt> {
        let mut metrics = RunMetrics::start();
        let result = self.compact_measured(request, &mut metrics);
        metrics.finish(&result);
        result
    }

    /// The measured body of [`Self::compact`]; `metrics` observes it.
    fn compact_measured(
        &self,
        request: &CompactRequest,
        metrics: &mut RunMetrics,
    ) -> HistoryResult<CompactReceipt> {
        let session = self.storage.session.clone();
        let schema = session.settings.schema.clone();
        // AC-44: a restricted role is refused before the intent, the mark and
        // the scratch schema; the missing privilege is named, never a raw
        // `42501` from whichever statement happened to run first.
        if let Some(missing) = compaction_missing(&session)? {
            return Err(HistoryError::Unsupported(missing));
        }
        // Spec 5.1/§9: the exclusive Store key is taken before the leftover op
        // is judged, so two compactions can never settle the same `gc_op` at
        // once, and a live owner is never terminated off-key.
        let claim = self.history.claim_store_exclusive()?;
        // Read under the key: a swap that committed before the claim is
        // authoritative, and this session's fence must use that generation.
        let generation = read_generation(&session, &schema)?;
        session.incarnation.store(
            u64::try_from(generation).unwrap_or(0),
            std::sync::atomic::Ordering::SeqCst,
        );
        metrics.generation_before = u64::try_from(generation).unwrap_or(0);
        if let Some(receipt) = settle_leftover(self, &session, &schema, metrics)? {
            return Ok(receipt);
        }
        let intent = GcIntent::operation(
            request.op_id,
            GcIntentKind::Compaction,
            GcScopeKind::Store,
            self.history.catalog_id.as_slice().to_vec(),
        )
        .owned(
            request.owner_runtime.as_deref(),
            request.owner_boot.as_deref(),
        )
        .with_phase(Some("mark"));
        // The registration takes its own level of the same key; dropping the
        // claim then leaves exactly that level held until `gc_release`.
        self.history.gc_register(&intent)?;
        drop(claim);
        let result = (|| {
            let rank = effective_sync_rank(&session)?;
            standby_precheck(&session, rank, &request.op_id, "mark")?;
            round(self, &session, &schema, request, metrics)
        })();
        if result.is_err() {
            let _ = self.history.gc_release(&request.op_id);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{standby_satisfied, sync_name, sync_rank, Replica};

    fn replica(name: &str, sync_state: &str) -> Replica {
        Replica {
            name: name.to_owned(),
            streaming: true,
            sync_state: sync_state.to_owned(),
        }
    }

    #[test]
    fn the_standby_setting_parses_first_any_and_a_bare_list() {
        let first = super::standby_requirement("FIRST 1 (s1)").unwrap();
        assert!(first.first);
        assert_eq!(first.count, 1);
        assert_eq!(first.members, ["s1"]);
        let any = super::standby_requirement("ANY 2 (s1, s2)").unwrap();
        assert!(!any.first);
        assert_eq!(any.count, 2);
        assert_eq!(any.members, ["s1", "s2"]);
        let bare = super::standby_requirement("s1, s2").unwrap();
        assert!(bare.first);
        assert_eq!(bare.count, 2, "a bare list is FIRST over every name");
        let star = super::standby_requirement("*").unwrap();
        assert!(star.any_name);
        assert_eq!(star.count, 1);
        assert!(super::standby_requirement("  ").is_none());
    }

    #[test]
    fn the_standby_predicate_counts_matching_sync_states() {
        // Below remote_write, or with no names, no standby is required.
        assert!(standby_satisfied(0, "", &[]));
        assert!(standby_satisfied(1, "", &[]));
        assert!(standby_satisfied(2, "", &[]));
        assert!(standby_satisfied(4, "  ", &[]));
        // At or above remote_write with names, a streaming standby whose
        // `sync_state` is the form's must exist.
        assert!(!standby_satisfied(2, "FIRST 1 (s1)", &[]));
        assert!(!standby_satisfied(4, "ANY 1 (s1)", &[]));
        assert!(standby_satisfied(
            2,
            "FIRST 1 (s1)",
            &[replica("s1", "sync")]
        ));
        assert!(!standby_satisfied(
            2,
            "FIRST 1 (s1)",
            &[replica("s1", "potential")]
        ));
        assert!(
            !standby_satisfied(2, "FIRST 1 (s1)", &[replica("s2", "sync")]),
            "a name the setting does not list never counts"
        );
        // One of ANY 2 is not a quorum: the old count-only test was wrong.
        assert!(!standby_satisfied(
            3,
            "ANY 2 (s1,s2)",
            &[replica("s1", "quorum")]
        ));
        assert!(standby_satisfied(
            3,
            "ANY 2 (s1,s2)",
            &[replica("s1", "quorum"), replica("s2", "quorum")]
        ));
        assert!(
            !standby_satisfied(
                3,
                "ANY 2 (s1,s2)",
                &[replica("s1", "quorum"), replica("s2", "async")]
            ),
            "an async member does not make the quorum"
        );
        // A bare list is FIRST over all listed names.
        assert!(!standby_satisfied(2, "s1,s2", &[replica("s1", "sync")]));
        assert!(standby_satisfied(
            2,
            "s1,s2",
            &[replica("s1", "sync"), replica("s2", "sync")]
        ));
        // `*` matches any streaming standby.
        assert!(standby_satisfied(2, "*", &[replica("anything", "sync")]));
        assert!(!standby_satisfied(2, "*", &[]));
        // An async standby is never counted.
        let mut async_replica = replica("s1", "sync");
        async_replica.streaming = false;
        assert!(!standby_satisfied(2, "FIRST 1 (s1)", &[async_replica]));
    }

    #[test]
    fn synchronous_commit_strength_is_the_spec_order() {
        assert_eq!(sync_rank("off"), 0);
        assert_eq!(sync_rank("local"), 1);
        assert_eq!(sync_rank("remote_write"), 2);
        assert_eq!(sync_rank("on"), 3);
        assert_eq!(sync_rank("remote_apply"), 4);
        assert_eq!(sync_name(4), "remote_apply");
        assert_eq!(sync_name(0), "off");
    }

    #[test]
    fn the_effective_sync_floor_is_local() {
        // Both `off` still commits at `local`: the spec's floor (§8 step 11).
        assert_eq!(super::effective_rank("off", "off"), 1);
        assert_eq!(super::effective_rank("local", "off"), 1);
        assert_eq!(super::effective_rank("off", "remote_apply"), 4);
        assert_eq!(super::effective_rank("on", "off"), 3);
    }
}
