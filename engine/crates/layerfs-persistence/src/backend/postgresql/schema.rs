//! The PostgreSQL Store schema in one named schema, and its validation.
//!
//! Tables mirror the SQLite Store's logical rows (same names and columns)
//! with these physical differences: integers are `bigint`, BLOBs `bytea`;
//! history names collate `"C"` so ordering and paging match SQLite's byte
//! order; pack bodies are whole `bytea` values stored `EXTERNAL`
//! (uncompressed, out of line) for ranged `substring` reads; `store_lane`
//! holds the one row writers lock for the publication lane. The pack-unit
//! tables are absent: group-row layouts are SQLite physical choices. A
//! schema made by [`create`] also has Init's acquisition tables (#533 step
//! 10b, `acquisition.sql`), recorded in its schema source; one an earlier
//! release imported from SQLite has none and refuses acquisition. A Store
//! written before the GC tables gains them at its first writable open
//! ([`add_gc`]), and one written before the position columns gains them,
//! with the T1.4 serial backfill, at that same open
//! ([`add_position_columns`]); any other schema mismatch is still refused.
//! A merge's records live in the writer connection's temporary tables
//! (`merge_records.rs`), not here.
use super::session::{
    connection::Session,
    transaction::{Mode, Transaction},
};
use crate::backend::records::{BackendError, Param};
use crate::PostgresCheck;
use layerfs_history::{CatalogId, HistoryCatalogConfig};
use layerfs_storage::{port::PersistenceError, StoragePolicy};
use postgres::SimpleQueryRow;

#[path = "schema_diagnostic.rs"]
mod diagnostic;

pub(crate) const SCHEMA: &str = concat!(
    include_str!("../../../sql/postgresql/schema.sql"),
    include_str!("../../../sql/postgresql/gc.sql")
);
/// The Store tables a schema had before the GC tables ([`GC`]) were appended.
const PRE_GC_SCHEMA: &str = include_str!("../../../sql/postgresql/schema.sql");
/// The `history_meta` position column as `schema.sql` writes it; it sits on
/// its own line, so removing the line leaves the text an older Store recorded.
const NEXT_SERIAL: &str = " next_history_serial bigint NOT NULL DEFAULT 1,\n";
/// The `commit` and `layer` position column as `schema.sql` writes it; it
/// sits on its own line in each of the two tables.
const HISTORY_SERIAL: &str = " history_serial bigint NOT NULL DEFAULT 0,\n";
/// The `store_policy` position salt column, with the comma the column before
/// it gains, as `schema.sql` writes them; it ends that table's column list.
const TRACE_SALT: &str = ",\n trace_salt bytea NOT NULL DEFAULT decode(repeat('00',32),'hex') CHECK (length(trace_salt)=32)";
/// The GC marker, position and admission tables; a Store written before them
/// gains them at open ([`add_gc`]).
const GC: &str = include_str!("../../../sql/postgresql/gc.sql");
/// Init's acquisition tables, created after [`SCHEMA`] in a new schema.
const ACQUISITION: &str = include_str!("../../../sql/postgresql/acquisition.sql");
/// The acquisition tables, in name order.
const ACQUISITION_TABLES: [&str; 3] = ["init_entry", "init_host_file", "init_operation"];
/// The engine-scoped schema version in `store_policy` and `history_meta`.
/// Version 2 keeps inode values and binding hints in append-only blocks
/// (#91) and object locations in narrow rows under an 8-byte-prefix index
/// (#90); a version 1 schema is refused at open, never converted.
pub(crate) const SCHEMA_VERSION: i64 = 2;
/// Every table of one Store schema, in name order.
pub(crate) const TABLES: [&str; 26] = [
    "absorbed",
    "binding_hint_block",
    "branch",
    "branch_retired",
    "commit",
    "commit_lineage",
    "content_signature",
    "file_digest",
    "gc_dead",
    "gc_intent",
    "gc_op",
    "history_meta",
    "inode_value_block",
    "layer",
    "layer_lineage",
    "layer_stack",
    "layer_stack_retired",
    "object_location",
    "pack",
    "path_removed",
    "project_epoch",
    "reach",
    "scope_allocator",
    "store_lane",
    "store_policy",
    "store_totals",
];
/// The tables a Store schema had before the GC tables were appended.
const PRE_GC_TABLES: [&str; 19] = [
    "absorbed",
    "binding_hint_block",
    "branch",
    "commit",
    "commit_lineage",
    "content_signature",
    "file_digest",
    "history_meta",
    "inode_value_block",
    "layer",
    "layer_lineage",
    "layer_stack",
    "object_location",
    "pack",
    "reach",
    "scope_allocator",
    "store_lane",
    "store_policy",
    "store_totals",
];

/// The live column definition of the current schema, in one deterministic text.
pub(crate) fn definition(tx: &Transaction<'_>) -> Result<String, BackendError> {
    let rows = tx.query(COLUMNS, vec![])?;
    let mut text = String::new();
    for row in rows {
        for index in 0..4 {
            text.push_str(&row.get::<String>(index)?);
            text.push(if index == 3 { '\n' } else { ' ' });
        }
    }
    Ok(text)
}

/// The recorded schema source: the Store tables, then the acquisition tables
/// when the schema has them.
fn source(acquisition: bool) -> String {
    match acquisition {
        true => format!("{SCHEMA}{ACQUISITION}"),
        false => SCHEMA.to_owned(),
    }
}

/// The recorded schema source of a Store written before the GC tables.
fn pre_gc_source(acquisition: bool) -> String {
    match acquisition {
        true => format!("{PRE_GC_SCHEMA}{ACQUISITION}"),
        false => PRE_GC_SCHEMA.to_owned(),
    }
}

/// `text` as a Store wrote it before the position columns ([`HISTORY_SERIAL`]
/// in `commit` and `layer`, [`TRACE_SALT`] in `store_policy`) existed.
fn without_position_columns(text: String) -> String {
    text.replacen(HISTORY_SERIAL, "", 2)
        .replacen(TRACE_SALT, "", 1)
}

/// Whether `recorded` is `current` or an earlier revision of it a Store may
/// have written: before [`NEXT_SERIAL`], before the position columns, or
/// before both. An additive migration matches one of those and refuses any
/// text no release wrote.
fn is_earlier_source(recorded: &str, current: String) -> bool {
    let without_columns = without_position_columns(current.clone());
    [
        current.clone(),
        current.replacen(NEXT_SERIAL, "", 1),
        without_columns.clone(),
        without_columns.replacen(NEXT_SERIAL, "", 1),
    ]
    .iter()
    .any(|revision| revision == recorded)
}

/// Creates the new schema `tx.schema` with the Store and acquisition tables,
/// its policy row with one random trace salt and its catalog row, inside the
/// caller's transaction. A schema of that name that already exists is
/// refused before anything is written; nothing is merged into it.
pub(crate) fn create(
    tx: &Transaction<'_>,
    policy: StoragePolicy,
    history: &HistoryCatalogConfig,
    catalog_id: CatalogId,
) -> Result<(), PersistenceError> {
    let name = tx.schema;
    let found = tx.query(
        "SELECT 1 FROM pg_namespace WHERE nspname=$1",
        vec![Param::Text(name.to_owned())],
    )?;
    if !found.is_empty() {
        return Err(PersistenceError::Refused {
            status: "schema exists".to_owned(),
        });
    }
    tx.batch(&format!(
        "CREATE SCHEMA {name}; SET LOCAL search_path TO {name}; {SCHEMA}{ACQUISITION}"
    ))?;
    tx.query(
        "INSERT INTO store_policy(id,schema_version,format_profile,small_file_threshold_bytes,whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth,trace_salt) VALUES(1,$1,$2,$3,$4,$5,$6,decode(md5(random()::text||clock_timestamp()::text)||md5(clock_timestamp()::text||random()::text),'hex'))",
        vec![
            Param::I64(SCHEMA_VERSION),
            Param::I64(i64::from(policy.format_profile())),
            Param::I64(policy.small_file_threshold_bytes() as i64),
            Param::I64(i64::from(policy.whole_file_delta_max_depth())),
            Param::I64(i64::from(policy.chunk_delta_max_depth())),
            Param::I64(i64::from(policy.metadata_delta_max_depth())),
        ],
    )?;
    let definition = definition(tx)?;
    tx.query(
        "INSERT INTO history_meta(id,catalog_id,catalog_incarnation,binding_key,identity_format,schema_version,schema_source,schema_definition) VALUES(1,$1,$2,$3,3,$4,$5,$6)",
        vec![
            Param::Bytes(catalog_id.as_slice().to_vec()),
            Param::I64(history.incarnation as i64),
            Param::Bytes(history.binding_key.clone()),
            Param::I64(SCHEMA_VERSION),
            Param::Text(source(true)),
            Param::Text(definition),
        ],
    )?;
    Ok(())
}

/// Adds the GC marker, position and admission tables to a Store written
/// before them, then records the current schema: an additive migration under
/// the same conditions as the SQLite Store's `add_gc`. A schema that already
/// has `gc_intent` is left alone; a recorded source, table set or definition
/// that is not the pre-GC schema is refused, never migrated.
pub(crate) fn add_gc(session: &Session) -> Result<(), PersistenceError> {
    // A Store that already has the GC tables is left alone: one statement,
    // no transaction and no lane wait.
    let present = session.run_as::<_, BackendError>(Mode::Statement, |tx| {
        let sql = format!(
            "SELECT 1 FROM information_schema.tables WHERE table_schema='{}' AND table_name='gc_intent' AND table_type='BASE TABLE'",
            tx.schema
        );
        Ok(!tx.query(&sql, vec![])?.is_empty())
    })?;
    if present {
        return Ok(());
    }
    session.run_as(Mode::Write, |tx| -> Result<(), PersistenceError> {
        // Re-check under the lane: another process may have migrated first.
        let present = tx.query(
            "SELECT 1 FROM information_schema.tables WHERE table_schema=current_schema() AND table_name='gc_intent' AND table_type='BASE TABLE'",
            vec![],
        )?;
        if !present.is_empty() {
            return Ok(());
        }
        let rows = tx.query(
            "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
            vec![],
        )?;
        let [row] = rows.as_slice() else {
            return Err(BackendError::Integrity.into());
        };
        let recorded_source: String = row.get(0)?;
        let acquisition = if is_earlier_source(&recorded_source, pre_gc_source(false)) {
            false
        } else if is_earlier_source(&recorded_source, pre_gc_source(true)) {
            true
        } else {
            return Err(BackendError::Integrity.into());
        };
        let tables = tx.query(TABLE_NAMES, vec![])?;
        let actual: Vec<String> = tables.iter().map(|r| r.get(0)).collect::<Result<_, _>>()?;
        let mut expected = PRE_GC_TABLES.to_vec();
        if acquisition {
            expected.extend(ACQUISITION_TABLES);
        }
        expected.sort_unstable();
        if !actual.iter().map(String::as_str).eq(expected) {
            return Err(BackendError::Integrity.into());
        }
        if row.get::<String>(1)? != definition(tx)? {
            return Err(BackendError::Integrity.into());
        }
        tx.batch(GC)?;
        let definition = definition(tx)?;
        tx.query(
            "UPDATE history_meta SET schema_source=$1,schema_definition=$2 WHERE id=1",
            vec![Param::Text(source(acquisition)), Param::Text(definition)],
        )?;
        Ok(())
    })
}

/// Adds the position column `history_meta.next_history_serial` (T1.4) to a
/// Store written before it, then records the current schema: an additive
/// migration under the same conditions as [`add_gc`]. A schema that already
/// has the column is left alone; a recorded source or definition that is not
/// an earlier revision of the current one is refused, never migrated. The
/// `commit`/`layer` serial columns and their backfill follow in
/// [`add_position_columns`], the next step of the same open; only the column
/// and one `project_epoch` row per project are recorded here.
pub(crate) fn add_position_state(session: &Session) -> Result<(), PersistenceError> {
    // A Store that already has the column is left alone: one statement, no
    // transaction and no lane wait.
    let present = session.run_as::<_, BackendError>(Mode::Statement, |tx| {
        let sql = format!(
            "SELECT 1 FROM information_schema.columns WHERE table_schema='{}' AND table_name='history_meta' AND column_name='next_history_serial'",
            tx.schema
        );
        Ok(!tx.query(&sql, vec![])?.is_empty())
    })?;
    if present {
        return Ok(());
    }
    session.run_as(Mode::Write, |tx| -> Result<(), PersistenceError> {
        // Re-check under the lane: another process may have migrated first.
        let present = tx.query(
            "SELECT 1 FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='history_meta' AND column_name='next_history_serial'",
            vec![],
        )?;
        if !present.is_empty() {
            return Ok(());
        }
        let rows = tx.query(
            "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
            vec![],
        )?;
        let [row] = rows.as_slice() else {
            return Err(BackendError::Integrity.into());
        };
        let recorded_source: String = row.get(0)?;
        let acquisition = if is_earlier_source(&recorded_source, source(false)) {
            false
        } else if is_earlier_source(&recorded_source, source(true)) {
            true
        } else {
            return Err(BackendError::Integrity.into());
        };
        if row.get::<String>(1)? != definition(tx)? {
            return Err(BackendError::Integrity.into());
        }
        tx.batch("ALTER TABLE history_meta ADD COLUMN next_history_serial bigint NOT NULL DEFAULT 1;")?;
        tx.batch(
            "INSERT INTO project_epoch(layer_stack_id, epoch) SELECT s.layer_stack_id, 0 FROM layer_stack s WHERE NOT EXISTS (SELECT 1 FROM project_epoch e WHERE e.layer_stack_id = s.layer_stack_id);",
        )?;
        let definition = definition(tx)?;
        tx.query(
            "UPDATE history_meta SET schema_source=$1,schema_definition=$2 WHERE id=1",
            vec![Param::Text(source(acquisition)), Param::Text(definition)],
        )?;
        Ok(())
    })
}

/// The ALTERs an old Store needs for the position columns, the one random
/// trace salt, and the T1.4 serial backfill. `gen_random_bytes` is used when
/// the server can reach it; without pgcrypto two md5 digests of the clock and
/// `random()` stand in for the same 32 bytes.
const POSITION_COLUMNS: &str = concat!(
    "ALTER TABLE \"commit\" ADD COLUMN history_serial bigint NOT NULL DEFAULT 0;",
    "ALTER TABLE layer ADD COLUMN history_serial bigint NOT NULL DEFAULT 0;",
    "ALTER TABLE store_policy ADD COLUMN trace_salt bytea NOT NULL DEFAULT decode(repeat('00',32),'hex') CHECK (length(trace_salt)=32);",
    "DO $$ BEGIN ",
    "UPDATE store_policy SET trace_salt=gen_random_bytes(32) WHERE id=1; ",
    "EXCEPTION WHEN undefined_function THEN ",
    "UPDATE store_policy SET trace_salt=decode(md5(random()::text||clock_timestamp()::text)||md5(clock_timestamp()::text||random()::text),'hex') WHERE id=1; ",
    "END $$;"
);

/// Adds the position columns T1.2c introduced to a Store written before them:
/// `commit.history_serial` and `layer.history_serial` (default 0) and
/// `store_policy.trace_salt` (32 random bytes), then backfills the serials
/// with T1.4's semantics in the same transaction. An additive migration under
/// the same conditions as [`add_gc`]: a schema that already has
/// `commit.history_serial` is left alone, and a recorded source or definition
/// that is not an earlier revision of the current one is refused, never
/// migrated.
pub(crate) fn add_position_columns(session: &Session) -> Result<(), PersistenceError> {
    // A Store that already has the columns is left alone: one statement, no
    // transaction and no lane wait.
    let present = session.run_as::<_, BackendError>(Mode::Statement, |tx| {
        let sql = format!(
            "SELECT 1 FROM information_schema.columns WHERE table_schema='{}' AND table_name='commit' AND column_name='history_serial'",
            tx.schema
        );
        Ok(!tx.query(&sql, vec![])?.is_empty())
    })?;
    if present {
        return Ok(());
    }
    session.run_as(Mode::Write, |tx| -> Result<(), PersistenceError> {
        // Re-check under the lane: another process may have migrated first.
        let present = tx.query(
            "SELECT 1 FROM information_schema.columns WHERE table_schema=current_schema() AND table_name='commit' AND column_name='history_serial'",
            vec![],
        )?;
        if !present.is_empty() {
            return Ok(());
        }
        let rows = tx.query(
            "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
            vec![],
        )?;
        let [row] = rows.as_slice() else {
            return Err(BackendError::Integrity.into());
        };
        let recorded_source: String = row.get(0)?;
        let acquisition = if is_earlier_source(&recorded_source, source(false)) {
            false
        } else if is_earlier_source(&recorded_source, source(true)) {
            true
        } else {
            return Err(BackendError::Integrity.into());
        };
        if row.get::<String>(1)? != definition(tx)? {
            return Err(BackendError::Integrity.into());
        }
        tx.batch(POSITION_COLUMNS)?;
        backfill_position_serials(tx)?;
        let definition = definition(tx)?;
        tx.query(
            "UPDATE history_meta SET schema_source=$1,schema_definition=$2 WHERE id=1",
            vec![Param::Text(source(acquisition)), Param::Text(definition)],
        )?;
        Ok(())
    })
}

/// The T1.4 serial backfill: Commits take the next Store serials in
/// `(created_ms, commit_id)` order, then each project's genesis Layer takes
/// the next one in stack-id order, every other Layer republishes its source
/// Commit's serial, existing serials are kept and `next_history_serial` never
/// falls below `MAX(serial) + 1`.
fn backfill_position_serials(tx: &Transaction<'_>) -> Result<(), BackendError> {
    let next: i64 = tx
        .query(
            "SELECT next_history_serial FROM history_meta WHERE id=1",
            vec![],
        )?
        .first()
        .ok_or(BackendError::Integrity)?
        .get(0)?;
    let commit_max: i64 = tx
        .query(
            "SELECT COALESCE(MAX(history_serial),0) FROM \"commit\"",
            vec![],
        )?
        .first()
        .ok_or(BackendError::Integrity)?
        .get(0)?;
    let layer_max: i64 = tx
        .query("SELECT COALESCE(MAX(history_serial),0) FROM layer", vec![])?
        .first()
        .ok_or(BackendError::Integrity)?
        .get(0)?;
    let base = next.max(commit_max.max(layer_max) + 1);
    tx.query(
        &format!(
            "WITH ordered AS (SELECT commit_id,{base}+ROW_NUMBER() OVER (ORDER BY created_ms,commit_id)-1 AS serial FROM \"commit\" WHERE history_serial=0) UPDATE \"commit\" AS c SET history_serial=ordered.serial FROM ordered WHERE ordered.commit_id=c.commit_id AND c.history_serial=0"
        ),
        vec![],
    )?;
    let after_commits: i64 = tx
        .query(
            "SELECT COALESCE(MAX(history_serial),0)+1 FROM \"commit\"",
            vec![],
        )?
        .first()
        .ok_or(BackendError::Integrity)?
        .get(0)?;
    tx.query(
        &format!(
            "WITH ordered AS (SELECT layer_id,{after_commits}+ROW_NUMBER() OVER (ORDER BY layer_stack_id,layer_id)-1 AS serial FROM layer WHERE history_serial=0 AND parent_layer_id IS NULL) UPDATE layer AS l SET history_serial=ordered.serial FROM ordered WHERE ordered.layer_id=l.layer_id AND l.history_serial=0 AND l.parent_layer_id IS NULL"
        ),
        vec![],
    )?;
    tx.query(
        "UPDATE layer AS l SET history_serial=(SELECT c.history_serial FROM \"commit\" AS c WHERE c.layer_stack_id=l.layer_stack_id AND c.commit_id=l.source_commit_id) WHERE l.history_serial=0 AND l.source_commit_id IS NOT NULL",
        vec![],
    )?;
    tx.query(
        "UPDATE history_meta SET next_history_serial=GREATEST(next_history_serial,(SELECT COALESCE(MAX(history_serial),0)+1 FROM (SELECT history_serial FROM \"commit\" UNION ALL SELECT history_serial FROM layer) AS assigned)) WHERE id=1",
        vec![],
    )?;
    Ok(())
}

/// What an open's check read: the catalog's incarnation, whether the schema
/// has the acquisition tables, the recorded binding, the frozen policy and
/// the server's version.
pub(crate) struct Checked {
    pub(crate) incarnation: u64,
    pub(crate) acquisition: bool,
    pub(crate) binding: Vec<u8>,
    pub(crate) policy: StoragePolicy,
    pub(crate) server_version: String,
}

/// The `history_meta` columns both checks read, then the policy row and the
/// server's version; `{s}.` names the schema for a statement outside the
/// search path.
fn meta_columns(s: &str) -> String {
    format!(
        "encode(h.catalog_id,'hex'),h.catalog_incarnation,encode(h.binding_key,'hex'),\
         h.identity_format,h.schema_version,p.format_profile,p.small_file_threshold_bytes,\
         p.whole_file_delta_max_depth,p.chunk_delta_max_depth,p.metadata_delta_max_depth,\
         p.schema_version,current_setting('server_version') \
         FROM {s}history_meta h, {s}store_policy p WHERE h.id=1 AND p.id=1"
    )
}

/// Checks the schema `session` names in one unprepared round trip (#63), as
/// `check` selects ([`PostgresCheck`]). `binding`, when given, must be the
/// recorded one; with none (a reach check) the recorded binding is returned
/// for the caller to compare. Every mismatch is `Integrity`, or `Malformed`
/// for a policy row that does not validate, as the separate reads were.
pub(crate) fn check(
    session: &Session,
    check: PostgresCheck,
    binding: Option<&[u8]>,
) -> Result<Checked, PersistenceError> {
    let s = &session.settings.schema;
    let observational = session
        .state
        .lock()
        .map_err(|_| BackendError::Unknown)
        .map(|state| matches!(state.client, super::session::driver::Driver::Diagnostic(_)))?;
    let (row, acquisition) = match check {
        PostgresCheck::Full if observational => diagnostic::row(session)?,
        PostgresCheck::Full => {
            let sql = format!(
                "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY; SET LOCAL search_path TO {s}; \
                 {TABLE_NAMES}; {COLUMNS}; \
                 SELECT h.schema_source,h.schema_definition,{} ; COMMIT",
                meta_columns("")
            );
            let mut groups = session.simple(&sql, true)?;
            if groups.len() != 6 {
                return Err(BackendError::Integrity.into());
            }
            let rows = groups.swap_remove(4);
            let columns = &groups[3];
            let actual = groups[2]
                .iter()
                .map(|r| text(r, 0).map(str::to_owned))
                .collect::<Result<Vec<_>, _>>()?;
            let acquisition = table_set(&actual)?;
            let row = single(rows)?;
            let mut definition = String::new();
            for column in columns {
                for index in 0..4 {
                    definition.push_str(text(column, index)?);
                    definition.push(if index == 3 { '\n' } else { ' ' });
                }
            }
            if text(&row, 0)? != source(acquisition) || text(&row, 1)? != definition {
                return Err(BackendError::Integrity.into());
            }
            (row, acquisition)
        }
        PostgresCheck::Light => {
            let present = ACQUISITION_TABLES
                .iter()
                .map(|t| format!("(to_regclass('{s}.{t}') IS NOT NULL)::int"))
                .collect::<Vec<_>>()
                .join("+");
            let sql = format!(
                "SELECT octet_length(h.schema_source),{present},{}",
                meta_columns(&format!("{s}."))
            );
            let groups = session.simple(&sql, false)?;
            let row = single(groups.into_iter().next().unwrap_or_default())?;
            let acquisition = match int(&row, 1)? {
                0 => false,
                3 => true,
                _ => return Err(BackendError::Integrity.into()),
            };
            if int(&row, 0)? != source(acquisition).len() as i64 {
                return Err(BackendError::Integrity.into());
            }
            (row, acquisition)
        }
    };
    let at = 2; // the meta columns follow the two check columns
    let recorded = unhex(text(&row, at + 2)?)?;
    let catalog_id = CatalogId::derive(&recorded).map_err(|_| PersistenceError::Malformed)?;
    let incarnation = int(&row, at + 1)?;
    if unhex(text(&row, at)?)? != catalog_id.as_slice()
        || binding.is_some_and(|b| b != recorded)
        || incarnation <= 0
        || int(&row, at + 3)? != 3
        || int(&row, at + 4)? != SCHEMA_VERSION
    {
        return Err(BackendError::Integrity.into());
    }
    let byte = |i| u8::try_from(int(&row, at + i)?).map_err(|_| BackendError::Integrity);
    let threshold = u64::try_from(int(&row, at + 6)?).map_err(|_| BackendError::Integrity)?;
    if int(&row, at + 10)? != SCHEMA_VERSION {
        return Err(PersistenceError::Malformed);
    }
    let policy = StoragePolicy::new(byte(5)?, threshold, byte(7)?, byte(8)?)
        .with_metadata_depth(byte(9)?)
        .validated()
        .map_err(|_| PersistenceError::Malformed)?;
    Ok(Checked {
        incarnation: incarnation as u64,
        acquisition,
        binding: recorded,
        policy,
        server_version: text(&row, at + 11)?.to_owned(),
    })
}

const TABLE_NAMES: &str = "SELECT table_name::text FROM information_schema.tables \
     WHERE table_schema = current_schema() AND table_type = 'BASE TABLE' ORDER BY table_name";
const COLUMNS: &str =
    "SELECT table_name::text, column_name::text, data_type::text, is_nullable::text \
     FROM information_schema.columns WHERE table_schema = current_schema() \
     ORDER BY table_name, ordinal_position";

fn single(rows: Vec<SimpleQueryRow>) -> Result<SimpleQueryRow, BackendError> {
    let mut rows = rows.into_iter();
    match (rows.next(), rows.next()) {
        (Some(row), None) => Ok(row),
        _ => Err(BackendError::Integrity),
    }
}
fn text(row: &SimpleQueryRow, index: usize) -> Result<&str, BackendError> {
    row.try_get(index)
        .ok()
        .flatten()
        .ok_or(BackendError::Integrity)
}
fn int(row: &SimpleQueryRow, index: usize) -> Result<i64, BackendError> {
    text(row, index)?
        .parse()
        .map_err(|_| BackendError::Integrity)
}
fn unhex(text: &str) -> Result<Vec<u8>, BackendError> {
    if text.len() % 2 != 0 {
        return Err(BackendError::Integrity);
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| BackendError::Integrity))
        .collect()
}

/// Refuses a schema whose tables are not exactly a LayerFS Store's, with or
/// without the acquisition tables; returns whether it has them.
fn tables(tx: &Transaction<'_>) -> Result<bool, BackendError> {
    let tables = tx.query(TABLE_NAMES, vec![])?;
    let actual: Vec<String> = tables.iter().map(|r| r.get(0)).collect::<Result<_, _>>()?;
    table_set(&actual)
}

/// Whether `actual`, the schema's table names in order, is a Store's with
/// the acquisition tables; refused when it is neither set.
fn table_set(actual: &[String]) -> Result<bool, BackendError> {
    if actual == TABLES {
        return Ok(false);
    }
    let mut with = [TABLES.as_slice(), ACQUISITION_TABLES.as_slice()].concat();
    with.sort_unstable();
    match actual == with {
        true => Ok(true),
        false => Err(BackendError::Integrity),
    }
}

/// The binding key recorded in a LayerFS schema; opening checks the rest.
pub(crate) fn binding(tx: &Transaction<'_>) -> Result<Vec<u8>, BackendError> {
    tables(tx)?;
    let rows = tx.query("SELECT binding_key FROM history_meta WHERE id=1", vec![])?;
    match rows.as_slice() {
        [row] => row.get(0),
        _ => Err(BackendError::Integrity),
    }
}

/// Revalidates bounded trusted schema projections in the caller's pinned view.
pub(crate) fn diagnostic_guard(tx: &Transaction<'_>) -> Result<(), BackendError> {
    diagnostic::guard(tx)
}
