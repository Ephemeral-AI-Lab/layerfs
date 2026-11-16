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
    let mut client = connector
        .connect_raw()
        .map_err(|error| match error.code() {
            Some(code) if code.code().starts_with("28") || code.code() == "3D000" => {
                PostgresStatusError::Denied(error.to_string())
            }
            _ => PostgresStatusError::Unreachable(error.to_string()),
        })?;
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
    /// The 16-byte operation identity, from `gc_intent` or the `lfs_gc_`
    /// scratch name `gc_op` records.
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
    /// liveness.
    pub intent_state: Option<String>,
    /// The mark's history serial, when recorded.
    pub mark_serial: Option<i64>,
    /// The mark's next pack id, when recorded.
    pub mark_pack_id: Option<i64>,
    /// The mark's live-heads digest, when recorded.
    pub mark_digest: Option<Vec<u8>>,
    /// True while the row remains: the run did not settle, so its outcome is
    /// unknown. `store.status` reports `busy` while the owner lives and
    /// `unresolved` once it is gone.
    pub uncertain: bool,
    /// True when the recorded owner backend is still alive and not `idle`,
    /// the same §9 test a resolving compaction applies.
    pub alive: bool,
}

impl PostgresCompactOpStatus {
    /// The `store.status` projection of this operation: `busy` while the
    /// owner lives, `unresolved` once it is gone. `uncertain` stays true for
    /// every remaining row, live or leftover; `state` separates the two. The
    /// phases are PostgreSQL's §8 ones: `mark`, `build`, `swap`, `cleanup`.
    #[must_use]
    pub fn status(&self) -> CompactStatus {
        CompactStatus {
            id: format!("op-{}", hex(&self.op_id)),
            name: "store.compact",
            phase: self.phase.clone(),
            started: self.started_ms,
            uncertain: self.uncertain,
            state: if self.alive { "busy" } else { "unresolved" },
            phases: &POSTGRES_COMPACT_PHASES,
        }
    }
}

/// `gc_op` plus its compaction `gc_intent` and the owner's live state, in one
/// query on one status connection. Returns `None` when the Store has no
/// compaction row (idle), or when a Store written before the GC tables has no
/// `gc_op` at all. No Store open, schema mutation, artifact or scratch read
/// happens here.
pub fn compact_status(
    url: &str,
    schema: &str,
) -> Result<Option<PostgresCompactOpStatus>, PostgresStatusError> {
    let connector = Connector::new(&PostgresSelection::new(url, schema))
        .map_err(|_| PostgresStatusError::Unreachable("the URL does not parse".to_owned()))?;
    let mut client = connector
        .connect_raw()
        .map_err(|error| match error.code() {
            Some(code) if code.code().starts_with("28") || code.code() == "3D000" => {
                PostgresStatusError::Denied(error.to_string())
            }
            _ => PostgresStatusError::Unreachable(error.to_string()),
        })?;
    let table = quote_ident(schema);
    let sql = format!(
        "SELECT o.generation,o.phase,o.started_ms,o.pid,o.backend_start,o.artifact,\
                o.mark_serial,o.mark_pack_id,o.mark_digest,\
                i.op_id,i.owner_runtime,i.owner_boot,i.state,o.usename,\
                EXISTS (SELECT 1 FROM pg_stat_activity a \
                         WHERE a.datname=current_database() AND a.pid=o.pid \
                           AND a.backend_start::text=o.backend_start \
                           AND (o.usename IS NULL OR a.usename::text=o.usename) \
                           AND a.state IS DISTINCT FROM 'idle') \
           FROM {table}.gc_op o \
           LEFT JOIN LATERAL (SELECT op_id,owner_runtime,owner_boot,state \
                                FROM {table}.gc_intent WHERE kind='compaction' \
                               ORDER BY updated_ms LIMIT 1) i ON TRUE \
          WHERE o.id=1"
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
    let intent: Option<Vec<u8>> = row.try_get(9).map_err(failed)?;
    let artifact: Option<String> = row.try_get(5).map_err(failed)?;
    let op = PostgresCompactOpStatus {
        op_id: op_identity(intent, artifact.as_deref()),
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
    };
    let _ = client.close();
    Ok(Some(op))
}

/// The `lfs_gc_<hex>` scratch name `gc_op.artifact` records carries the
/// operation identity, so a row whose `gc_intent` was already released still
/// projects a stable `op.id`.
fn op_identity(intent: Option<Vec<u8>>, artifact: Option<&str>) -> [u8; 16] {
    if let Some(bytes) = intent {
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
