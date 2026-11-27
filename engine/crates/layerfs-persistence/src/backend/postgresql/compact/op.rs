//! §8 Phase M: the `gc_op` row, the mark tuple, the scratch helpers and the run metrics.

use super::types::takes_rebuild_path;
use super::types::DeadTotals;
use super::types::OpRow;
use super::types::SCRATCH_PREFIX;
use crate::backend::postgresql::session::connection::Session;
use crate::backend::postgresql::session::transaction::Mode;
use crate::backend::postgresql::session::transaction::Transaction;
use crate::backend::seam::Param;
use crate::gc::compact::CompactFailpoint;
use crate::gc::compact::CompactKeptWaste;
use crate::gc::compact::CompactRequest;
use crate::history::GcTuple;
use crate::history::Reach;
use crate::MarkResult;
use layerfs_content::ObjectId;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The deterministic, length-safe scratch schema name of one Store: the
/// catalog identity, never the caller's operation id, so a crash on another
/// machine can still name the artifact from the Store. `lfs_gc_` plus the
/// truncated hex identity stays within PostgreSQL's 63-byte identifier.
pub(crate) fn scratch_name(catalog_id: &[u8]) -> String {
    let mut name = format!("{SCRATCH_PREFIX}{}", hex(catalog_id));
    name.truncate(63);
    name
}

pub(crate) fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// The `pg_attribute`/`attstorage`/`reloptions`/`tablespace` fingerprint of
/// one table, with its TOAST relation's reloptions. The mark records both payload tables' fingerprints in `gc_op`;
/// the swap compares them with the tables it moves back into the Store, so a
/// scratch table that lost a storage class, option or tablespace is an
/// `Integrity` failure rather than a silent layout change.
pub(crate) fn table_fingerprint(
    tx: &Transaction<'_>,
    schema: &str,
    table: &str,
) -> HistoryResult<String> {
    let rows = tx.query(
        "SELECT COALESCE((SELECT string_agg(a.attname::text || ':' || format_type(a.atttypid,a.atttypmod) || ':' || a.attnotnull::text || ':' || a.attstorage::text || ':' || COALESCE(a.attcompression::text,'') || ':' || a.attnum::text, ',' ORDER BY a.attnum) \
           FROM pg_attribute a WHERE a.attrelid=c.oid AND a.attnum>0 AND NOT a.attisdropped),'') || '|' || \
             COALESCE((SELECT t.spcname::text FROM pg_tablespace t WHERE t.oid=c.reltablespace),'') || '|' || \
             COALESCE(array_to_string(c.reloptions,','),'') || '|' || \
             COALESCE((SELECT array_to_string(t.reloptions,',') FROM pg_class t WHERE t.oid=c.reltoastrelid),'') \
         FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
         WHERE n.nspname=$1 AND c.relname=$2",
        vec![Param::Text(schema.to_owned()), Param::Text(table.to_owned())],
    )?;
    if rows.len() != 1 {
        return Err(HistoryError::Integrity("compaction table fingerprint"));
    }
    rows[0].get(0).map_err(Into::into)
}

/// One write transaction on the Store session; scratch/DDL work never waits
/// for the publication lane (the session advisory key is the exclusion).
pub(crate) fn stage<T>(
    session: &Arc<Session>,
    body: impl FnOnce(&Transaction<'_>) -> HistoryResult<T>,
) -> HistoryResult<T> {
    session.run_as(Mode::Stage, body)
}

pub(crate) fn scalar(tx: &Transaction<'_>, sql: &str, params: Vec<Param>) -> HistoryResult<i64> {
    let rows = tx.query(sql, params)?;
    if rows.len() != 1 {
        return Err(HistoryError::Integrity("compaction scalar"));
    }
    rows[0].get(0).map_err(Into::into)
}

pub(crate) fn scalar_session(
    session: &Arc<Session>,
    sql: &str,
    params: Vec<Param>,
) -> HistoryResult<i64> {
    stage(session, |tx| scalar(tx, sql, params))
}

pub(crate) fn read_generation(session: &Arc<Session>, schema: &str) -> HistoryResult<i64> {
    scalar_session(
        session,
        &format!("SELECT catalog_incarnation FROM {schema}.history_meta WHERE id=1"),
        Vec::new(),
    )
}

pub(crate) fn read_op(session: &Arc<Session>, schema: &str) -> HistoryResult<Option<OpRow>> {
    stage(session, |tx| {
        let rows = tx.query(
            &format!(
                "SELECT generation,phase,artifact,dead_canonical,dead_stored,dead_metadata,dead_logical,totals_applied,pid,backend_start,physical_before,physical_after,peak_temp,kept_waste,mark_totals,usename FROM {schema}.gc_op WHERE id=1"
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
            physical_before: u64::try_from(row.get::<i64>(10)?).unwrap_or(0),
            physical_after: u64::try_from(row.get::<i64>(11)?).unwrap_or(0),
            peak_temp: u64::try_from(row.get::<i64>(12)?).unwrap_or(0),
            kept_waste: match row.get::<Option<Vec<u8>>>(13)? {
                Some(bytes) => decode_kept_waste(&bytes)?,
                None => CompactKeptWaste::default(),
            },
            total_stored: match row.get::<Option<Vec<u8>>>(14)? {
                Some(bytes) => stored_at_mark(&bytes)?,
                None => 0,
            },
            usename: row.get::<Option<String>>(15)?,
        }))
    })
}

/// `store_totals.stored` from the `mark_totals` blob (`int8send` of logical,
/// canonical, stored and metadata, in that order).
fn stored_at_mark(blob: &[u8]) -> HistoryResult<i64> {
    let bytes = blob
        .get(16..24)
        .ok_or(HistoryError::Integrity("compaction mark totals"))?;
    let mut value = [0u8; 8];
    value.copy_from_slice(bytes);
    Ok(i64::from_be_bytes(value))
}

pub(crate) fn set_phase(session: &Arc<Session>, schema: &str, phase: &str) -> HistoryResult<()> {
    stage(session, |tx| {
        tx.query(
            &format!("UPDATE {schema}.gc_op SET phase=$1 WHERE id=1"),
            vec![Param::Text(phase.to_owned())],
        )?;
        Ok(())
    })
}

pub(crate) fn stop(
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

/// Object ids one statement of [`load_visited`] carries: about 300 KiB, and
/// the most reached identities a compaction holds at a time.
const VISITED_IDS: usize = 8192;
/// Pack ids one statement of [`load_live_packs`] carries.
const VISITED_PACKS: usize = 32_768;

/// What Phase M's mark transaction hands the round.
pub(crate) struct Marked {
    pub(crate) mark: MarkResult,
    pub(crate) tuple: GcTuple,
    /// The stored bytes of the packs the mark retained.
    pub(crate) live: i64,
    /// The §3 decision; the visited ids are loaded only when it is true.
    pub(crate) rebuild: bool,
}

/// Phase M's mark: the Store tuple, the closure, the live packs, the §3
/// decision and, for a rebuild, the visited ids, all in the mark's one
/// repeatable-read transaction.
///
/// The reached identities are never listed in this process: the mark keeps a
/// bit per object over the Store's locator order, and the visited table is
/// filled by reading the locators again a block at a time and sending back
/// the reached ones. A locator's rank is its position in this snapshot, so
/// the fill reads the snapshot the walk read; a publication that lands while
/// the mark runs is in neither, and moves the tuple Phase B checks.
pub(crate) fn mark(session: &Arc<Session>, schema: &str) -> HistoryResult<Marked> {
    session.run_snapshot(|tx| {
        let (mark, tuple, mut reach) = crate::history::mark_gc_reached(tx)?;
        load_live_packs(tx, &mark)?;
        // §3 decides over the mark's own snapshot, never a later Store
        // total: a publication after the mark raises `pack` bytes, and
        // reading them later would flip a `none` decision into a rebuild
        // the mark never judged.
        let live = live_stored(tx, schema)?;
        let rebuild = takes_rebuild_path(tuple.stored, live);
        // The visited ids and the dead set are a rebuild's inputs alone: a
        // `none` round writes its op row and clears it, and loads neither.
        if rebuild {
            load_visited(tx, &mut reach)?;
        }
        Ok(Marked {
            mark,
            tuple,
            live,
            rebuild,
        })
    })
}

/// The live pack ids, as a session temporary table: what the §3 decision
/// sums, and what the Phase A copy joins. They come from the mark's own
/// bounded list in ascending order, a piece per statement, and are never
/// held together here.
fn load_live_packs(tx: &Transaction<'_>, mark: &MarkResult) -> HistoryResult<()> {
    tx.batch(
        "DROP TABLE IF EXISTS pg_temp.gc_live_pack;\
         CREATE TEMP TABLE pg_temp.gc_live_pack(pack_id bigint PRIMARY KEY) ON COMMIT PRESERVE ROWS;",
    )?;
    let send = |packs: &[i64]| -> HistoryResult<()> {
        if packs.is_empty() {
            return Ok(());
        }
        tx.typed(
            "INSERT INTO pg_temp.gc_live_pack(pack_id) SELECT unnest($1::bigint[])",
            &[&packs],
            packs.len() as u64 * 8,
        )?;
        Ok(())
    };
    let mut packs: Vec<i64> = Vec::with_capacity(VISITED_PACKS);
    mark.objects().retained_packs().each(&mut |pack| {
        packs.push(pack as i64);
        if packs.len() == VISITED_PACKS {
            send(&packs)?;
            packs.clear();
        }
        Ok(())
    })?;
    send(&packs)?;
    // The decision's sum and the Phase A copy read the set; analyze it
    // after the load so the planner sees the real cardinality.
    tx.batch("ANALYZE pg_temp.gc_live_pack;")?;
    Ok(())
}

/// The live object ids, as a session temporary table the dead-set query and
/// the Phase A copy read. Only a round that rebuilds loads it: a `none`
/// decision needs the live packs alone.
///
/// The ids come from the mark's reached set in ascending identity order, a
/// bounded piece per statement, so the key's index is built by appending. A
/// binary COPY was tried first and spent about four microseconds per row
/// handing rows to the driver one at a time; an array is encoded once and
/// the server unnests it.
fn load_visited(tx: &Transaction<'_>, reach: &mut Reach) -> HistoryResult<()> {
    tx.batch(
        "DROP TABLE IF EXISTS pg_temp.gc_visited;\
         CREATE TEMP TABLE pg_temp.gc_visited(object_id bytea PRIMARY KEY) ON COMMIT PRESERVE ROWS;",
    )?;
    let send = |ids: &[ObjectId]| -> HistoryResult<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let ids: Vec<&[u8]> = ids.iter().map(|id| id.as_bytes().as_slice()).collect();
        tx.typed(
            "INSERT INTO pg_temp.gc_visited(object_id) SELECT unnest($1::bytea[])",
            &[&ids],
            ids.len() as u64 * 36,
        )?;
        Ok(())
    };
    let mut ids: Vec<ObjectId> = Vec::with_capacity(VISITED_IDS);
    reach.each(tx, &mut |row, _| {
        ids.push(row.object_id);
        if ids.len() == VISITED_IDS {
            send(&ids)?;
            ids.clear();
        }
        Ok(())
    })?;
    send(&ids)?;
    tx.batch("ANALYZE pg_temp.gc_visited;")?;
    Ok(())
}

/// The `gc_dead` set of one mark, and the `logical` bytes of its dead Commit
/// and Layer rows.
fn persist_dead(tx: &Transaction<'_>, schema: &str) -> HistoryResult<i64> {
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
         SELECT 'signature',int8send(cs.slot)||int8send(cs.stamp) FROM {schema}.content_signature cs \
          WHERE NOT EXISTS (SELECT 1 FROM pg_temp.gc_visited v WHERE v.object_id=cs.object_id) \
         UNION ALL \
         SELECT DISTINCT 'digest',fd.root FROM {schema}.file_digest fd \
          WHERE NOT EXISTS (SELECT 1 FROM pg_temp.gc_visited v WHERE v.object_id=fd.root) \
         UNION ALL \
         SELECT 'layer_lineage',ll.layer_id FROM {schema}.layer_lineage ll \
          WHERE NOT EXISTS (SELECT 1 FROM {schema}.kept_node k WHERE k.kind='layer' AND k.node_id=ll.layer_id) \
         UNION ALL \
         SELECT 'commit_lineage',cl.commit_id FROM {schema}.commit_lineage cl \
          WHERE NOT EXISTS (SELECT 1 FROM {schema}.kept_node k WHERE k.kind='commit' AND k.node_id=cl.commit_id) \
         UNION ALL \
         SELECT 'absorbed',a.run||int8send(a.depth) FROM {schema}.absorbed a \
          WHERE NOT EXISTS (SELECT 1 FROM {schema}.commit_lineage cl JOIN {schema}.kept_node k ON k.kind='commit' AND k.node_id=cl.commit_id WHERE cl.run=a.run) \
         UNION ALL \
         SELECT 'reach',r.key||int8send(r.x) FROM {schema}.reach r \
          WHERE NOT EXISTS (SELECT 1 FROM {schema}.kept_node k WHERE k.kind='stack' AND k.node_id=r.key) \
            AND NOT EXISTS (SELECT 1 FROM {schema}.commit_lineage cl JOIN {schema}.kept_node k ON k.kind='commit' AND k.node_id=cl.commit_id WHERE cl.run=r.key)"
    ))?;
    scalar(
        tx,
        &format!(
            "SELECT COALESCE((SELECT SUM(c.logical)::bigint FROM {schema}.\"commit\" c \
               WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='commit' AND d.id=c.commit_id)),0) \
              + COALESCE((SELECT SUM(l.logical)::bigint FROM {schema}.layer l \
               WHERE EXISTS (SELECT 1 FROM {schema}.gc_dead d WHERE d.kind='layer' AND d.id=l.layer_id)),0)"
        ),
        Vec::new(),
    )
}

/// Phase M: write `gc_op` (phase mark) and the persisted `gc_dead` set in
/// batches. The set is a membership, not a predicate, so a row created after
/// the mark is never collected. It carries the history rows (commit, layer,
/// branch, stack), the signature and digest rows, and the derived lineage
/// closure (layer_lineage, commit_lineage, absorbed, reach), so Phase C can
/// delete each project closure in its per-project transaction instead of
/// scanning the derived tables for orphans in the final settle. The
/// run/depth and key/x primary keys are persisted as the concatenated bytes
/// run || int8send(depth) and key || int8send(x). A digest root is recorded
/// once however many `file_digest` rows name it; every other arm selects its
/// table's own key.
///
/// With `rebuild` false (a `none` decision, §3) only the op row is written
/// and `gc_dead` is left empty: nothing reads the set of a round that
/// installs nothing, and the caller clears the op row before it replies.
pub(crate) fn persist_mark(
    session: &Arc<Session>,
    schema: &str,
    mark: &MarkResult,
    tuple: &GcTuple,
    scratch: &str,
    request: &CompactRequest,
    rebuild: bool,
) -> HistoryResult<DeadTotals> {
    let digest = mark.live_heads_digest().to_vec();
    stage(session, |tx| {
        tx.query(&format!("DELETE FROM {schema}.gc_dead"), Vec::new())?;
        let logical = if rebuild {
            persist_dead(tx, schema)?
        } else {
            0
        };
        let started = now_ms();
        let kept_waste = encode_kept_waste(&CompactKeptWaste::from_waste(mark.objects().waste()));
        let fingerprint = format!(
            "{}|{}",
            table_fingerprint(tx, schema, "pack")?,
            table_fingerprint(tx, schema, "object_location")?
        );
        tx.query(
            &format!(
                "INSERT INTO {schema}.gc_op \
                 (id,generation,phase,scratch_schema,mark_serial,mark_pack_id,mark_digest,mark_totals,mark_stamp,started_ms,dead_canonical,dead_stored,dead_metadata,dead_logical,physical_before,peak_temp,kept_waste,totals_applied,artifact,pid,backend_start,usename,incarnation,op_id,mark_fingerprint,mark_visited) \
                 VALUES (1,$1,'mark',$2,$3,$4,$5,int8send($6)||int8send($7)||int8send($8)||int8send($9),$10,$11,0,0,0,$12,0,0,$13,FALSE,$2,pg_backend_pid(),(SELECT backend_start::text FROM pg_stat_activity WHERE pid=pg_backend_pid()),(SELECT usename::text FROM pg_stat_activity WHERE pid=pg_backend_pid()),$1,$14,$15,$16) \
                 ON CONFLICT (id) DO UPDATE SET generation=EXCLUDED.generation,phase=EXCLUDED.phase,scratch_schema=EXCLUDED.scratch_schema,mark_serial=EXCLUDED.mark_serial,mark_pack_id=EXCLUDED.mark_pack_id,mark_digest=EXCLUDED.mark_digest,mark_totals=EXCLUDED.mark_totals,mark_stamp=EXCLUDED.mark_stamp,started_ms=EXCLUDED.started_ms,dead_canonical=0,dead_stored=0,dead_metadata=0,dead_logical=EXCLUDED.dead_logical,physical_before=0,physical_after=0,peak_temp=0,kept_waste=EXCLUDED.kept_waste,totals_applied=FALSE,artifact=EXCLUDED.artifact,pid=EXCLUDED.pid,backend_start=EXCLUDED.backend_start,usename=EXCLUDED.usename,incarnation=EXCLUDED.incarnation,op_id=EXCLUDED.op_id,mark_fingerprint=EXCLUDED.mark_fingerprint,mark_visited=EXCLUDED.mark_visited"
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
                Param::Bytes(kept_waste),
                Param::Bytes(request.op_id.to_vec()),
                Param::Text(fingerprint),
                Param::I64(mark.objects().len() as i64),
            ],
        )?;
        Ok(DeadTotals {
            logical,
            ..DeadTotals::default()
        })
    })
}

/// Clears the transient mark state of a round that installs nothing: the dead
/// set, the op row and the §6.1 `kept_node` closure the mark wrote. A
/// Store carries no mark state after the call, as its none path promises.
pub(crate) fn clear_round(session: &Arc<Session>, schema: &str) -> HistoryResult<()> {
    stage(session, |tx| {
        tx.batch(&format!(
            "DELETE FROM {schema}.gc_dead; DELETE FROM {schema}.gc_op; DELETE FROM {schema}.kept_node;"
        ))?;
        Ok(())
    })
}

/// The stored bytes of every pack the mark retained.
fn live_stored(tx: &Transaction<'_>, schema: &str) -> HistoryResult<i64> {
    scalar(
        tx,
        &format!(
            "SELECT COALESCE(SUM(p.length)::bigint,0) FROM {schema}.pack p \
             WHERE EXISTS (SELECT 1 FROM pg_temp.gc_live_pack v WHERE v.pack_id=p.pack_id)"
        ),
        Vec::new(),
    )
}

/// The Store's retained-plus-collected history row count. Before Phase C it is
/// the mark's `history_rows`; the resumed report uses it for its zero reasons.
pub(crate) fn history_rows(session: &Arc<Session>, schema: &str) -> HistoryResult<i64> {
    scalar_session(
        session,
        &format!(
            "SELECT (SELECT COUNT(*) FROM {schema}.\"commit\")+(SELECT COUNT(*) FROM {schema}.layer)"
        ),
        Vec::new(),
    )
}

/// The wire version of [`encode_kept_waste`].
const KEPT_WASTE_VERSION: u8 = 1;

/// The fixed header of the encoded `keptWaste`: version, three `u64` byte
/// totals, the capped flag and the reference count.
const KEPT_WASTE_HEADER: usize = 1 + 8 + 8 + 8 + 1 + 4;

/// Encodes the §11 `keptWaste` into the `gc_op.kept_waste` bytea so a resumed
/// `G + 1` run reports the mark's accounting instead of a zero placeholder.
///
/// The layout is versioned and fixed: one version byte, the three big-endian
/// `u64` byte totals, the capped flag, a big-endian `u32` reference count and
/// the identity bytes. The caller's references are already capped by
/// [`CompactKeptWaste::from_waste`].
pub fn encode_kept_waste(waste: &CompactKeptWaste) -> Vec<u8> {
    let mut out = Vec::with_capacity(KEPT_WASTE_HEADER + waste.delta_base_references.len() * 32);
    out.push(KEPT_WASTE_VERSION);
    out.extend_from_slice(&waste.dead_in_retained_packs_bytes.to_be_bytes());
    out.extend_from_slice(&waste.delta_bases_bytes.to_be_bytes());
    out.extend_from_slice(&waste.inode_blocks_bytes.to_be_bytes());
    out.push(u8::from(waste.delta_base_references_capped));
    let count = u32::try_from(waste.delta_base_references.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&count.to_be_bytes());
    for reference in &waste.delta_base_references {
        out.extend_from_slice(reference);
    }
    out
}

/// Decodes [`encode_kept_waste`]; a malformed blob is `Integrity`, never a
/// silently zero report.
pub fn decode_kept_waste(bytes: &[u8]) -> layerfs_history::error::HistoryResult<CompactKeptWaste> {
    use layerfs_history::error::HistoryError;
    let header = bytes
        .get(..KEPT_WASTE_HEADER)
        .ok_or(HistoryError::Integrity("compaction kept waste"))?;
    if header[0] != KEPT_WASTE_VERSION {
        return Err(HistoryError::Integrity("compaction kept waste version"));
    }
    let u64_at = |start: usize| -> u64 {
        let mut value = [0u8; 8];
        value.copy_from_slice(&header[start..start + 8]);
        u64::from_be_bytes(value)
    };
    let dead_in_retained_packs_bytes = u64_at(1);
    let delta_bases_bytes = u64_at(9);
    let inode_blocks_bytes = u64_at(17);
    let delta_base_references_capped = header[25] != 0;
    let mut count_bytes = [0u8; 4];
    count_bytes.copy_from_slice(&header[26..30]);
    let count = usize::try_from(u32::from_be_bytes(count_bytes))
        .map_err(|_| HistoryError::Integrity("compaction kept waste count"))?;
    let references = bytes
        .get(KEPT_WASTE_HEADER..)
        .ok_or(HistoryError::Integrity("compaction kept waste references"))?;
    if references.len() != count.saturating_mul(32) {
        return Err(HistoryError::Integrity("compaction kept waste references"));
    }
    let delta_base_references = references
        .chunks_exact(32)
        .map(|chunk| {
            let mut id = [0u8; 32];
            id.copy_from_slice(chunk);
            id
        })
        .collect();
    Ok(CompactKeptWaste {
        dead_in_retained_packs_bytes,
        delta_bases_bytes,
        delta_base_references,
        delta_base_references_capped,
        inode_blocks_bytes,
    })
}
