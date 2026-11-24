//! A PostgreSQL server's status for a home's connection page (#506 E5/E7):
//! its version, one round trip's latency and the size of each named schema
//! (tables and materialized views with their indexes and TOAST), on one
//! connection opened for the call and closed after it, with no retry.
//!
//! It also reads a Store's compaction op — the `gc_op` row with its
//! `gc_intent` — for `store.status` (T10.5): one query, with no Store open,
//! schema change, artifact read or scratch-schema read.
use crate::backend::postgresql::session::connection::Connector;
use crate::report::{CompactStatus, POSTGRES_COMPACT_PHASES};
use crate::PostgresSelection;
use std::time::{Duration, Instant};

/// What a server answered.
#[derive(Debug)]
pub struct PostgresStatus {
    /// `SHOW server_version`.
    pub server_version: String,
    /// The `SHOW server_version` round trip.
    pub latency: Duration,
    /// Each schema asked for, with its size in bytes; `None` when the
    /// server has no such schema.
    pub schemas: Vec<(String, Option<u64>)>,
}

/// Why a status read did not complete.
#[derive(Debug)]
pub enum PostgresStatusError {
    /// The server was not reached, or the connection failed before a query.
    Unreachable(String),
    /// The server refused the credentials or the database.
    Denied(String),
    /// A query failed after the connection was made.
    Failed(String),
}

impl std::fmt::Display for PostgresStatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(why) => write!(f, "PostgreSQL server unreachable: {why}"),
            Self::Denied(why) => write!(f, "PostgreSQL server refused the connection: {why}"),
            Self::Failed(why) => write!(f, "PostgreSQL status query failed: {why}"),
        }
    }
}

const SIZES: &str = "SELECT s.name, CASE WHEN n.oid IS NULL THEN NULL ELSE \
     COALESCE(sum(pg_total_relation_size(c.oid)), 0)::bigint END \
     FROM unnest($1::text[]) AS s(name) \
     LEFT JOIN pg_namespace n ON n.nspname = s.name \
     LEFT JOIN pg_class c ON c.relnamespace = n.oid AND c.relkind IN ('r', 'm') \
     GROUP BY s.name, n.oid ORDER BY s.name";

/// Reads the status of the server at `url` and the sizes of `schemas`.
pub fn status(url: &str, schemas: &[String]) -> Result<PostgresStatus, PostgresStatusError> {
    let connector = Connector::new(&PostgresSelection::new(url, "public"))
        .map_err(|_| PostgresStatusError::Unreachable("the URL does not parse".to_owned()))?;
    let mut client = connector.connect_status()?;
    let failed = |error: postgres::Error| PostgresStatusError::Failed(error.to_string());
    let started = Instant::now();
    let server_version: String = client
        .query_one("SHOW server_version", &[])
        .map_err(failed)?
        .try_get(0)
        .map_err(failed)?;
    let latency = started.elapsed();
    let names: Vec<String> = schemas.to_vec();
    let mut sizes = Vec::new();
    for row in client.query(SIZES, &[&names]).map_err(failed)? {
        let name: String = row.try_get(0).map_err(failed)?;
        let bytes: Option<i64> = row.try_get(1).map_err(failed)?;
        sizes.push((name, bytes.map(|b| u64::try_from(b).unwrap_or(0))));
    }
    let _ = client.close();
    Ok(PostgresStatus {
        server_version,
        latency,
        schemas: sizes,
    })
}

/// The Store-side compaction row (`gc_op`) and the compaction `gc_intent`,
/// as `store.status` (T10.5) projects them: the operation identity, the
/// generation the run started at, its phase, its owner and whether its
/// outcome is known. A settled run removes the row, so [`compact_status`]
/// returns `None`.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostgresCompactOpStatus {
    /// The 16-byte operation identity: the `gc_op` row's own `op_id`, then a
    /// remaining `gc_intent`, then (for a row written before `op_id` was
    /// recorded) the `lfs_gc_` scratch name.
    pub op_id: [u8; 16],
    /// The generation the run started at.
    pub generation: i64,
    /// The recorded phase: `mark`, `build`, `swap` or `cleanup`.
    pub phase: String,
    /// When the run started, in Unix milliseconds.
    pub started_ms: i64,
    /// The owning backend's pid, when the row records one.
    pub owner_pid: Option<i64>,
    /// The owning backend's `backend_start`, when the row records one.
    pub owner_backend_start: Option<String>,
    /// The owning role's `usename`, when the row records one.
    pub owner_usename: Option<String>,
    /// The owning process runtime `gc_intent` recorded, when it left one.
    pub owner_runtime: Option<String>,
    /// The owning process boot id `gc_intent` recorded, when it left one.
    pub owner_boot: Option<String>,
    /// The `gc_intent` row's own state (`live`, `uncertain` or `done`), when
    /// it is still there; a crash leaves `live`, so this is evidence, not
    /// liveness. A released intent is absent: the run returned to its caller,
    /// which is PostgreSQL's form of SQLite's `returned` record.
    pub intent_state: Option<String>,
    /// The mark's history serial, when recorded.
    pub mark_serial: Option<i64>,
    /// The mark's next pack id, when recorded.
    pub mark_pack_id: Option<i64>,
    /// The mark's live-heads digest, when recorded.
    pub mark_digest: Option<Vec<u8>>,
    /// True while the row remains: the run did not settle, so its outcome is
    /// unknown. `store.status` reports `busy` while the run is in flight and
    /// `unresolved` once it returned or its owner is gone.
    pub uncertain: bool,
    /// True when the recorded owner backend is still alive (its recorded
    /// `(pid, backend_start, usename)` is in `pg_stat_activity`), the same §9
    /// test a resolving compaction applies. Its `idle` state does not make an
    /// in-flight run a leftover: the state below also needs the intent.
    pub alive: bool,
    /// True when the projection comes from the registered compaction
    /// `gc_intent` alone: the run is still in Phase M and has not written its
    /// `gc_op` row, so it records no owner `(pid, backend_start)`. The live
    /// intent is then the in-flight witness, as SQLite's admission record is.
    pub intent_only: bool,
}

impl PostgresCompactOpStatus {
    /// The `store.status` projection of this operation: `busy` while the run
    /// is in flight — its compaction `gc_intent` is still `live` and its
    /// owner is not provably gone — and `unresolved` once the run returned
    /// (the intent is released), was marked `uncertain`, or its owner is
    /// dead. A released intent is the PostgreSQL form of SQLite's `returned`
    /// record: the owner may still be alive, but it is no longer running the
    /// op. `uncertain` stays true for every remaining row, live or leftover;
    /// `state` separates the two. The phases are PostgreSQL's §8 ones:
    /// `mark`, `build`, `swap`, `cleanup`.
    #[must_use]
    pub fn status(&self) -> CompactStatus {
        // SQLite marks a record `returned` before replying; PostgreSQL
        // releases the intent. Either way the op is in flight only while the
        // intent is still *live* and its owner still runs it. Before
        // `persist_mark` there is no `gc_op` row naming a backend owner yet:
        // the registered Phase M intent is then the in-flight witness, but
        // only while it is live and its process owner is not provably gone.
        // An uncertain intent says the outcome is unknown, not that the run
        // continues; a dead process owner cannot be running it at all.
        let dead_owner = crate::history::admission::OwnerView::now().owner_dead(
            self.owner_runtime.as_deref(),
            self.owner_boot.as_deref(),
            true,
        );
        let in_flight = self.intent_state.as_deref() == Some("live")
            && (self.alive || self.intent_only)
            && !dead_owner;
        CompactStatus {
            id: format!("op-{}", hex(&self.op_id)),
            name: "store.compact",
            phase: self.phase.clone(),
            started: self.started_ms,
            uncertain: self.uncertain,
            state: if in_flight { "busy" } else { "unresolved" },
            phases: &POSTGRES_COMPACT_PHASES,
        }
    }
}

/// `gc_op` plus its compaction `gc_intent` and the owner's live state, in one
/// query on one status connection. Returns `None` when the Store has neither a
/// compaction row nor a registered intent (idle), or when a Store written
/// before the GC tables has no `gc_op` at all. During Phase M the run has
/// registered its intent but `persist_mark` has not written `gc_op` yet; that
/// window projects the intent as an owner-less `mark` op instead of idle. No
/// Store open, schema mutation, artifact or scratch read happens here.
pub fn compact_status(
    url: &str,
    schema: &str,
) -> Result<Option<PostgresCompactOpStatus>, PostgresStatusError> {
    let connector = Connector::new(&PostgresSelection::new(url, schema))
        .map_err(|_| PostgresStatusError::Unreachable("the URL does not parse".to_owned()))?;
    let mut client = connector.connect_status()?;
    let table = quote_ident(schema);
    let sql = format!(
        "SELECT COALESCE(o.generation,i.generation,0),\
                COALESCE(o.phase,i.phase,'mark'),\
                COALESCE(o.started_ms,i.updated_ms,0),\
                o.pid,o.backend_start,COALESCE(o.artifact,i.artifact),\
                o.mark_serial,o.mark_pack_id,o.mark_digest,\
                i.op_id,i.owner_runtime,i.owner_boot,i.state,o.usename,\
                CASE WHEN o.id IS NULL THEN FALSE ELSE EXISTS \
                  (SELECT 1 FROM pg_stat_activity a \
                    WHERE a.datname=current_database() AND a.pid=o.pid \
                      AND a.backend_start::text=o.backend_start \
                      AND (o.usename IS NULL OR a.usename::text=o.usename)) END, \
                COALESCE(o.op_id,i.op_id), \
                o.id IS NOT NULL, i.op_id IS NOT NULL \
           FROM (SELECT 1 AS id) anchor \
           LEFT JOIN {table}.gc_op o ON o.id=1 \
           LEFT JOIN LATERAL (SELECT op_id,owner_runtime,owner_boot,state,\
                                     phase,generation,artifact,updated_ms \
                                FROM {table}.gc_intent WHERE kind='compaction' AND state <> 'done' \
                               ORDER BY updated_ms LIMIT 1) i ON TRUE"
    );
    let rows = match client.query(&sql, &[]) {
        Ok(rows) => rows,
        // A Store opened before the GC tables has no row to read: idle.
        Err(error)
            if matches!(
                error.code().map(postgres::error::SqlState::code),
                Some("42P01" | "3F000")
            ) =>
        {
            let _ = client.close();
            return Ok(None);
        }
        Err(error) => return Err(PostgresStatusError::Failed(error.to_string())),
    };
    let Some(row) = rows.first() else {
        let _ = client.close();
        return Ok(None);
    };
    let failed = |error: postgres::Error| PostgresStatusError::Failed(error.to_string());
    let has_op: bool = row.try_get(16).map_err(failed)?;
    let has_intent: bool = row.try_get(17).map_err(failed)?;
    if !has_op && !has_intent {
        let _ = client.close();
        return Ok(None);
    }
    let intent: Option<Vec<u8>> = row.try_get(9).map_err(failed)?;
    let artifact: Option<String> = row.try_get(5).map_err(failed)?;
    let recorded: Option<Vec<u8>> = row.try_get(15).map_err(failed)?;
    let op = PostgresCompactOpStatus {
        op_id: op_identity(recorded, intent, artifact.as_deref()),
        generation: row.try_get(0).map_err(failed)?,
        phase: row.try_get(1).map_err(failed)?,
        started_ms: row.try_get(2).map_err(failed)?,
        owner_pid: row.try_get(3).map_err(failed)?,
        owner_backend_start: row.try_get(4).map_err(failed)?,
        owner_usename: row.try_get(13).map_err(failed)?,
        owner_runtime: row.try_get(10).map_err(failed)?,
        owner_boot: row.try_get(11).map_err(failed)?,
        intent_state: row.try_get(12).map_err(failed)?,
        mark_serial: row.try_get(6).map_err(failed)?,
        mark_pack_id: row.try_get(7).map_err(failed)?,
        mark_digest: row.try_get(8).map_err(failed)?,
        // A remaining row is an unsettled run: the record's own fact. It is
        // never inferred from the owner's liveness.
        uncertain: true,
        alive: row.try_get(14).map_err(failed)?,
        intent_only: !has_op,
    };
    let _ = client.close();
    Ok(Some(op))
}

/// The operation identity: the `gc_op` row records it at the mark, so a row
/// whose `gc_intent` was already released still projects a stable `op.id`.
/// The intent is the next fallback and the old `lfs_gc_<hex(op_id)>` artifact
/// the last, for a row written before `gc_op.op_id` existed.
fn op_identity(
    recorded: Option<Vec<u8>>,
    intent: Option<Vec<u8>>,
    artifact: Option<&str>,
) -> [u8; 16] {
    for bytes in [recorded, intent].into_iter().flatten() {
        if let Ok(op) = <[u8; 16]>::try_from(bytes.as_slice()) {
            return op;
        }
    }
    artifact
        .and_then(|name| name.strip_prefix("lfs_gc_"))
        .and_then(unhex)
        .unwrap_or([0; 16])
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn unhex(text: &str) -> Option<[u8; 16]> {
    if text.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}
