//! §7 steps 2–5: the fresh-copy rebuild, its capacity inputs and the §11 peak
//! bound.
//!
//! The rebuild writes a new file beside the Store: the sealed Store's schema
//! (as `sqlite_master` holds it, so `schema_source`/`schema_definition` stay
//! identical), then only the rows the mark retained, and only then the
//! secondary indexes and triggers. There is no staged whole-Store copy and no
//! in-place rewrite: `VACUUM INTO` + delete + `VACUUM` would write the retained
//! bytes about four times over (measured), while this path writes them once.

use super::op::compact_path;
use super::op::hex;
use super::op::stop_at;
use super::op::visited_path;
use super::op::RunOp;
use super::repack::Repacked;
use super::space;
use super::types::db_error;
use super::types::io_error;
use super::types::CompactFailpoint;
use super::types::CompactRequest;
use super::types::MarkTuple;
use super::verify::immutable_uri;
use super::window::windows;
use super::window::ReadAhead;
use super::window::Window;
use crate::backend::records::BackendError;
use crate::backend::sqlite::lifecycle::seal::SealedFacts;
use crate::backend::sqlite::session::connection::Session;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

/// The bulk load's page cache: 256 MiB, so the retained packs stream through
/// memory instead of the default 2 MiB cache.
const CACHE_KIB: i64 = 262_144;

/// §7 step 2's large window for the attached sealed source. The session default
/// is no mmap (`session/profile.rs:86-87`), so the bulk copy would page the old
/// file through the 2 MiB pager cache; a 256 MiB window maps the ordered pack
/// and location reads instead.
pub const SRC_MMAP_BYTES: i64 = 268_435_456;

/// Apply §7 step 2's source window and return the size SQLite applied (a build
/// may cap it). The caller attaches the sealed file as `src` first.
pub fn source_mmap(connection: &Connection) -> HistoryResult<i64> {
    connection
        .execute_batch(&format!("PRAGMA src.mmap_size={SRC_MMAP_BYTES};"))
        .map_err(db_error)?;
    connection
        .query_row("PRAGMA src.mmap_size", params![], |row| row.get(0))
        .map_err(db_error)
}

/// §11 peak temporary bytes for a rebuild: the fresh Store and the `visited`
/// scratch. The fresh path stages no whole-Store copy, writes no prune
/// journal and opens no rewrite temp database, so it has no other artifact to
/// count.
pub fn peak_temp_bytes(target_bytes: u64, visited_bytes: u64) -> u64 {
    target_bytes.saturating_add(visited_bytes)
}

pub(crate) fn pack_lengths(session: &Arc<Session>) -> HistoryResult<BTreeMap<i64, u64>> {
    session
        .run::<_, BackendError>(false, |tx| {
            let rows = tx.query("SELECT pack_id,length FROM pack", vec![])?;
            let mut lengths = BTreeMap::new();
            for row in &rows {
                let id: i64 = row.get(0)?;
                let length: i64 = row.get(1)?;
                lengths.insert(
                    id,
                    u64::try_from(length).map_err(|_| BackendError::Integrity)?,
                );
            }
            Ok(lengths)
        })
        .map_err(Into::into)
}

pub(crate) fn live_page_metrics(session: &Arc<Session>) -> HistoryResult<(u64, u64)> {
    session
        .run::<_, BackendError>(false, |tx| {
            let free = tx.query("PRAGMA freelist_count", vec![])?;
            let page = tx.query("PRAGMA page_size", vec![])?;
            let free = free.first().ok_or(BackendError::Integrity)?.get::<i64>(0)?;
            let page = page.first().ok_or(BackendError::Integrity)?.get::<i64>(0)?;
            Ok((
                u64::try_from(free).map_err(|_| BackendError::Integrity)?,
                u64::try_from(page).map_err(|_| BackendError::Integrity)?,
            ))
        })
        .map_err(Into::into)
}

/// What the fresh-copy rebuild produced, for the report and the install fence.
pub(crate) struct Built {
    pub(crate) target: PathBuf,
    pub(crate) target_bytes: u64,
    pub(crate) visited_bytes: u64,
    pub(crate) logical: u64,
    pub(crate) canonical: u64,
    pub(crate) stored: u64,
    pub(crate) metadata: u64,
    /// The retained packs this build wrote again without their dead records.
    pub(crate) repacked: Repacked,
}

impl Built {
    /// §11 peak temporary bytes: the fresh Store plus the `visited` scratch,
    /// as a documented upper bound rather than a sampled peak. The staged
    /// fresh file grows to exactly `target_bytes`, and the mark's scratch is
    /// deleted at the settle, so the run's whole artifact footprint never
    /// exceeds their sum. §13 already marks the report an estimate.
    pub(crate) fn peak_temp_bytes(&self) -> u64 {
        peak_temp_bytes(self.target_bytes, self.visited_bytes)
    }
}

pub(crate) fn table_present(connection: &Connection, name: &str) -> HistoryResult<bool> {
    let found: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name=?1",
            params![name],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    Ok(found == 1)
}

/// The sealed Store and the mark a staged build is measured against.
pub(crate) struct BuildInput<'a> {
    pub(crate) facts: &'a SealedFacts,
    pub(crate) tuple: &'a MarkTuple,
    pub(crate) synchronous: i64,
    /// The stored bytes of the packs the mark retains: what the bulk load is
    /// about to write, known before the fresh file holds a row.
    pub(crate) payload_bytes: u64,
    /// Every pack of the sealed Store and its stored length.
    pub(crate) lengths: &'a BTreeMap<i64, u64>,
    /// The packs the mark retains, ascending.
    pub(crate) retained: &'a [i64],
    /// The mark's retained packs that hold dead records, for [`super::repack`].
    pub(crate) partly_dead: &'a [(i64, u64)],
}

/// How one table's rows enter the fresh file.
enum Copy {
    /// Every row (catalog, allocators, markers the closure keeps).
    All,
    /// The rows a filter keeps; the filter reads `src` and the mark scratch.
    Where(&'static str),
    /// The table's DDL is created, the table itself stays empty: transient
    /// working rows and the dead set are never carried across a rebuild.
    Empty,
}

/// The retained-row filter per table, in the order the copy runs. Every
/// referenced table is copied before the tables whose foreign keys name it, so
/// the immediate constraints hold; the deferred history constraints are
/// checked at COMMIT.
///
/// `history_meta` and `store_totals` are copied whole and then brought to the
/// rebuild's values in the same transaction. `gc_intent` keeps only this run's
/// admission intent, so the installed Store carries the row the settle clears.
fn copy_rule(table: &str) -> Copy {
    match table {
        // A pack with any live object is copied whole, unless `repack::plan`
        // took it out of `live_pack` to write it again without its dead
        // records.
        "pack" | "pack_unit" => Copy::Where(RETAINED_PACK),
        "object_location" | "content_signature" => {
            Copy::Where("object_id IN (SELECT object_id FROM scratch.visited)")
        }
        "file_digest" => Copy::Where("root IN (SELECT object_id FROM scratch.visited)"),
        "layer_stack" => {
            Copy::Where("layer_stack_id IN (SELECT node_id FROM src.kept_node WHERE kind='stack')")
        }
        "layer" => {
            Copy::Where("layer_id IN (SELECT node_id FROM src.kept_node WHERE kind='layer')")
        }
        "commit" => {
            Copy::Where("commit_id IN (SELECT node_id FROM src.kept_node WHERE kind='commit')")
        }
        "branch" => {
            Copy::Where("branch_id IN (SELECT node_id FROM src.kept_node WHERE kind='branch')")
        }
        "layer_lineage" => {
            Copy::Where("layer_id IN (SELECT node_id FROM src.kept_node WHERE kind='layer')")
        }
        "commit_lineage" => {
            Copy::Where("commit_id IN (SELECT node_id FROM src.kept_node WHERE kind='commit')")
        }
        "absorbed" => Copy::Where(
            "run IN (SELECT run FROM src.commit_lineage WHERE commit_id IN \
(SELECT node_id FROM src.kept_node WHERE kind='commit'))",
        ),
        "reach" => Copy::Where(
            "key IN (SELECT node_id FROM src.kept_node WHERE kind='stack' \
UNION SELECT run FROM src.commit_lineage WHERE commit_id IN \
(SELECT node_id FROM src.kept_node WHERE kind='commit'))",
        ),
        "branch_retired" => {
            Copy::Where("branch_id IN (SELECT node_id FROM src.kept_node WHERE kind='branch')")
        }
        "layer_stack_retired" | "path_removed" | "project_epoch" => {
            Copy::Where("layer_stack_id IN (SELECT node_id FROM src.kept_node WHERE kind='stack')")
        }
        "inode_value_block" | "kept_node" | "scope_allocator" | "store_policy" | "store_totals"
        | "history_meta" => Copy::All,
        "gc_intent" => Copy::Empty,
        "gc_op" | "gc_dead" | "merge_operation" | "merge_record" | "init_operation"
        | "init_entry" | "init_host_file" | "binding_hint_block" => Copy::Empty,
        _ => Copy::All,
    }
}

/// The filter that keeps a retained pack's rows.
const RETAINED_PACK: &str = "pack_id IN (SELECT pack_id FROM scratch.live_pack)";

/// The pack copy: the rows of the table that holds the pack payload, one
/// window of retained packs per statement. Before each window the sealed file
/// is asked for the part the window reads, so a source that is not in memory
/// arrives in sequential requests instead of one pack per round trip.
fn copy_packs(
    connection: &Connection,
    table: &str,
    plan: &[Window],
    sealed: &mut ReadAhead,
) -> HistoryResult<()> {
    let name = identifier(table);
    let mut statement = connection
        .prepare(&format!(
            // The range goes inside the subquery: SQLite drives this loop
            // from `live_pack`, so a range on the outer table alone would
            // seek every retained pack of the Store once per window.
            "INSERT INTO main.{name} SELECT * FROM src.{name} WHERE pack_id IN \
(SELECT pack_id FROM scratch.live_pack WHERE pack_id BETWEEN ?1 AND ?2)"
        ))
        .map_err(db_error)?;
    for window in plan {
        sealed.ask(window);
        statement
            .execute(params![window.first, window.last])
            .map_err(db_error)?;
    }
    Ok(())
}

/// The `store_policy` columns the rebuild copies, named explicitly: the fresh
/// file's schema is a copy of the source's `sqlite_master`, so `SELECT *` would
/// follow whatever column order that schema happens to have. The named list
/// keeps the one policy row bound to the columns already validated at open.
const STORE_POLICY_COLUMNS: &str = "id,schema_version,format_profile,small_file_threshold_bytes,\
whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth,\
next_pack_id,next_ordinal,trace_salt";

/// The retained-row copy, in foreign-key order. Tables the sealed Store does
/// not have (an older layout) are absent; [`copy_rows`] skips them.
const COPY_ORDER: [&str; 30] = [
    "pack",
    "pack_unit",
    "object_location",
    "content_signature",
    "file_digest",
    "inode_value_block",
    "layer_stack",
    "layer",
    "commit",
    "branch",
    "layer_lineage",
    "commit_lineage",
    "absorbed",
    "reach",
    "branch_retired",
    "layer_stack_retired",
    "path_removed",
    "project_epoch",
    "kept_node",
    "scope_allocator",
    "store_policy",
    "store_totals",
    "history_meta",
    "gc_intent",
    "merge_operation",
    "merge_record",
    "init_operation",
    "init_entry",
    "init_host_file",
    "binding_hint_block",
];

/// Quotes a table name for the generated `INSERT ... SELECT`.
fn identifier(table: &str) -> String {
    format!("\"{}\"", table.replace('"', "\"\""))
}

/// The schema objects one rebuild copies from `sqlite_master`, in creation
/// order: the tables the bulk load fills, then the indexes and triggers built
/// after it.
struct SchemaObjects {
    tables: Vec<String>,
    secondary: Vec<String>,
}

fn schema_objects(connection: &Connection) -> HistoryResult<SchemaObjects> {
    let mut statement = connection
        .prepare(
            "SELECT type,sql FROM src.sqlite_master \
WHERE sql IS NOT NULL AND substr(name,1,7)!='sqlite_' ORDER BY rowid",
        )
        .map_err(db_error)?;
    let mut rows = statement.query(params![]).map_err(db_error)?;
    let mut tables = Vec::new();
    let mut secondary = Vec::new();
    while let Some(row) = rows.next().map_err(db_error)? {
        let kind: String = row.get(0).map_err(db_error)?;
        let sql: String = row.get(1).map_err(db_error)?;
        match kind.as_str() {
            "table" => tables.push(sql),
            "index" | "trigger" | "view" => secondary.push(sql),
            _ => return Err(HistoryError::Integrity("staged Store schema object")),
        }
    }
    Ok(SchemaObjects { tables, secondary })
}

/// Copies the retained rows of every table the sealed Store has, in
/// [`COPY_ORDER`], then recomputes the totals and advances the generation.
fn copy_rows(
    connection: &Connection,
    op: &RunOp,
    generation: i64,
    target_tables: &BTreeSet<String>,
    plan: &[Window],
    sealed: &mut ReadAhead,
    partly_dead: &[(i64, u64)],
) -> HistoryResult<Repacked> {
    // The pack payload is `pack.body` in a Monolithic Store and the
    // `pack_unit` rows otherwise; that table is copied window by window.
    let payload = if target_tables.contains("pack_unit") {
        "pack_unit"
    } else {
        "pack"
    };
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(db_error)?;
    let copied = (|| -> HistoryResult<Repacked> {
        // Packs that hold dead records leave the whole-pack copy here and are
        // written again, without those records, once the tables are copied.
        let repack = super::repack::plan(connection, partly_dead)?;
        // §7 step 4: the totals trigger invariant holds before the copy. The
        // rewrite of `store_totals` below trusts it.
        let before: (i64, i64) = connection
            .query_row(
                "SELECT (SELECT logical FROM src.store_totals WHERE id=1),\
(SELECT COALESCE(SUM(logical),0) FROM src.\"commit\")+\
(SELECT COALESCE(SUM(logical),0) FROM src.layer)",
                params![],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(db_error)?;
        if before.0 != before.1 {
            return Err(HistoryError::Integrity(
                "Store totals disagree with the history rows",
            ));
        }
        for table in COPY_ORDER {
            if !target_tables.contains(table) {
                continue;
            }
            if table == payload {
                copy_packs(connection, table, plan, sealed)?;
                continue;
            }
            let name = identifier(table);
            let statement = if table == "gc_intent" {
                format!(
                    "INSERT INTO main.{name} SELECT * FROM src.{name} WHERE op_id=x'{}'",
                    hex(&op.op_id)
                )
            } else if table == "store_policy" {
                format!(
                    "INSERT INTO main.{name} ({STORE_POLICY_COLUMNS}) SELECT {STORE_POLICY_COLUMNS} FROM src.{name}"
                )
            } else {
                match copy_rule(table) {
                    Copy::All => format!("INSERT INTO main.{name} SELECT * FROM src.{name}"),
                    Copy::Where(filter) => {
                        format!("INSERT INTO main.{name} SELECT * FROM src.{name} WHERE {filter}")
                    }
                    Copy::Empty => continue,
                }
            };
            connection.execute_batch(&statement).map_err(db_error)?;
        }
        // `sqlite_sequence` is the acquisition operation counter: the copy
        // keeps every row the source had, so the counter never falls (§4.1).
        if target_tables.contains("sqlite_sequence") {
            let source_sequence: i64 = connection
                .query_row(
                    "SELECT count(*) FROM src.sqlite_master \
WHERE type='table' AND name='sqlite_sequence'",
                    params![],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            if source_sequence == 1 {
                connection
                    .execute_batch(
                        "INSERT INTO main.sqlite_sequence(name,seq) \
SELECT name,seq FROM src.sqlite_sequence",
                    )
                    .map_err(db_error)?;
            }
        }
        let repacked =
            super::repack::apply(connection, &repack, target_tables.contains("pack_unit"))?;
        // §7 step 4: `store_totals` is a whole row, so the `init_*` columns
        // keep the source values; only the four derived sums are rewritten.
        connection
            .execute_batch(&format!(
                "UPDATE main.store_totals SET \
logical=(SELECT COALESCE(SUM(logical),0) FROM main.\"commit\")+\
(SELECT COALESCE(SUM(logical),0) FROM main.layer),\
canonical=(SELECT COALESCE(SUM(canonical_length),0) FROM main.object_location),\
(stored,metadata)=(SELECT COALESCE(SUM(length),0),\
COALESCE(SUM(CASE WHEN domain=0 THEN length ELSE 0 END),0) FROM main.pack) WHERE id=1;\
UPDATE main.history_meta SET catalog_incarnation={generation} WHERE id=1;"
            ))
            .map_err(db_error)?;
        Ok(repacked)
    })();
    match copied {
        Ok(repacked) => {
            connection.execute_batch("COMMIT").map_err(db_error)?;
            Ok(repacked)
        }
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

/// The tables the fresh file has, after the schema copy created them.
fn main_tables(connection: &Connection) -> HistoryResult<BTreeSet<String>> {
    let mut statement = connection
        .prepare("SELECT name FROM main.sqlite_master WHERE type='table'")
        .map_err(db_error)?;
    let mut rows = statement.query(params![]).map_err(db_error)?;
    let mut found = BTreeSet::new();
    while let Some(row) = rows.next().map_err(db_error)? {
        found.insert(row.get::<_, String>(0).map_err(db_error)?);
    }
    Ok(found)
}

/// The source page settings the fresh file must preserve (§7 step 2).
fn page_settings(connection: &Connection) -> HistoryResult<(i64, i64, i64, i64)> {
    let read = |pragma: &str| -> HistoryResult<i64> {
        connection
            .query_row(pragma, params![], |row| row.get(0))
            .map_err(db_error)
    };
    Ok((
        read("PRAGMA src.page_size")?,
        read("PRAGMA src.auto_vacuum")?,
        read("PRAGMA src.user_version")?,
        read("PRAGMA src.application_id")?,
    ))
}

/// §7 steps 2–5: create the fresh Store, copy only retained rows, then build
/// the secondary indexes and triggers, recompute the totals, and switch the
/// file to WAL.
pub(crate) fn build(
    input: BuildInput<'_>,
    capacity: bool,
    request: &CompactRequest,
    op: &RunOp,
) -> HistoryResult<Built> {
    let BuildInput {
        facts,
        tuple,
        synchronous,
        payload_bytes,
        lengths,
        retained,
        partly_dead,
    } = input;
    let store = &facts.path;
    let target = compact_path(store);
    let visited = visited_path(store);
    // The mark already streamed the §6.3 visited set into `visited`; only the
    // staged Store is judged here. A missing scratch means the mark's table
    // did not survive its own transaction.
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err(HistoryError::Exists("staged compaction artifact"));
    }
    let visited_bytes = std::fs::metadata(&visited)
        .map_err(|_| HistoryError::Integrity("visited scratch"))?
        .len();
    stop_at(request, CompactFailpoint::BeforeCopy, op, store)?;
    if capacity {
        // §7 step 3 on a bounded filesystem: the fresh file has already been
        // created when the write meets ENOSPC. The abort path removes it and
        // the live Store is unchanged.
        std::fs::write(&target, b"staged").map_err(io_error)?;
        return Err(io_error(std::io::Error::from_raw_os_error(28)));
    }

    // A real-process crash arm stops here: the visited scratch is written and
    // the fresh file has not been created. `BeforeLoad` is the build's first
    // write, so the arm proves the crash before the staged name exists.
    stop_at(request, CompactFailpoint::BeforeLoad, op, store)?;

    // §7 step 2: a raw connection on a new file. The source is attached
    // `immutable=1` so the sealed file stays byte-identical and no sidecar
    // appears beside it; the mark's scratch is attached from the same file the
    // mark wrote.
    let connection = Connection::open_with_flags(
        &target,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(db_error)?;
    connection
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(db_error)?;
    connection
        .execute("ATTACH DATABASE ?1 AS src", params![immutable_uri(store)])
        .map_err(db_error)?;
    // §7 step 2: the sealed source is mapped, not paged through the default
    // no-mmap profile.
    source_mmap(&connection)?;
    connection
        .execute(
            "ATTACH DATABASE ?1 AS scratch",
            params![visited
                .to_str()
                .ok_or(HistoryError::InvalidInput("compaction path"))?],
        )
        .map_err(db_error)?;
    let (page_size, auto_vacuum, user_version, application_id) = page_settings(&connection)?;
    connection
        .execute_batch(&format!(
            "PRAGMA page_size={page_size};PRAGMA auto_vacuum={auto_vacuum};\
PRAGMA journal_mode=OFF;PRAGMA synchronous=OFF;PRAGMA foreign_keys=OFF;\
PRAGMA temp_store=MEMORY;PRAGMA cache_size=-{CACHE_KIB};\
PRAGMA user_version={user_version};PRAGMA application_id={application_id};"
        ))
        .map_err(db_error)?;

    // §7 step 2: the schema as the sealed Store records it, so the installed
    // file's `schema_definition` stays byte-identical. Only the tables are
    // created now; indexes and triggers wait for the bulk load.
    let schema = schema_objects(&connection)?;
    for sql in &schema.tables {
        connection.execute_batch(sql).map_err(db_error)?;
    }
    let (restored_page, restored_auto_vacuum): (i64, i64) = match (
        connection.query_row("PRAGMA main.page_size", params![], |row| row.get(0)),
        connection.query_row("PRAGMA main.auto_vacuum", params![], |row| row.get(0)),
    ) {
        (Ok(page), Ok(auto)) => (page, auto),
        (Err(error), _) | (_, Err(error)) => return Err(db_error(error)),
    };
    if restored_page != page_size || restored_auto_vacuum != auto_vacuum {
        return Err(HistoryError::Integrity("staged Store page settings"));
    }

    // The fresh file now carries the schema and no rows: the state a crash
    // inside the bulk load leaves (the in-process arm tears this file, and the
    // next run removes the whole recorded set instead of trusting it).
    stop_at(request, CompactFailpoint::DuringCopy, op, store)?;
    // The load appends the retained payload: size the file for it once, so
    // its page writes land inside the file instead of each moving its end.
    let page_bytes = u64::try_from(page_size).unwrap_or(0);
    space::reserve(&target, payload_bytes, page_bytes)?;
    let target_tables = main_tables(&connection)?;
    let plan = windows(lengths, retained, facts.bytes)?;
    let mut sealed = ReadAhead::open(store)?;
    let repacked = copy_rows(
        &connection,
        op,
        tuple.generation + 1,
        &target_tables,
        &plan,
        &mut sealed,
        partly_dead,
    )?;
    drop(sealed);

    // §7 step 4: the secondary indexes and the immutability/logical triggers
    // are built after the bulk load, so the load writes no index page and fires
    // no trigger.
    for sql in &schema.secondary {
        connection.execute_batch(sql).map_err(db_error)?;
    }
    // The installed Store's schema definition must match the recorded one, or
    // its next open refuses; the DDL text is copied verbatim, so a mismatch is
    // a rebuild bug.
    let definition: String = {
        let mut statement = connection
            .prepare(
                "SELECT type,name,sql FROM main.sqlite_master \
WHERE sql IS NOT NULL AND substr(name,1,7)!='sqlite_' ORDER BY type,name",
            )
            .map_err(db_error)?;
        let mut rows = statement.query(params![]).map_err(db_error)?;
        let mut text = String::new();
        while let Some(row) = rows.next().map_err(db_error)? {
            let kind: String = row.get(0).map_err(db_error)?;
            let name: String = row.get(1).map_err(db_error)?;
            let sql: String = row.get(2).map_err(db_error)?;
            text.push_str(&kind);
            text.push('\n');
            text.push_str(&name);
            text.push('\n');
            text.push_str(&sql);
            text.push('\n');
        }
        text
    };
    let recorded: String = connection
        .query_row(
            "SELECT schema_definition FROM main.history_meta WHERE id=1",
            params![],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if recorded != definition {
        return Err(HistoryError::Integrity(
            "staged Store schema definition differs from the recorded one",
        ));
    }
    let totals: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT logical,canonical,stored,metadata FROM main.store_totals WHERE id=1",
            params![],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(db_error)?;
    // The schema is complete, so the header's page count is final: the file
    // ends there, whatever the load was sized for.
    let pages: i64 = connection
        .query_row("PRAGMA main.page_count", params![], |row| row.get(0))
        .map_err(db_error)?;
    // §7 step 5: the same raw connection switches the fresh file to WAL, so the
    // mode switch runs under `journal_mode=OFF` and writes no rollback journal
    // beside the staged name; then checkpoint and leave no sidecar.
    connection
        .execute_batch("DETACH DATABASE src;DETACH DATABASE scratch;")
        .map_err(db_error)?;
    // Before the staged file enters WAL mode: Apple's SQLite turns the
    // persistent-WAL flag on by default, which would keep the staged `-wal`
    // and `-shm` beside the fresh file after close and trip the sidecar fence
    // at the end of the build. The seal and a snapshot clear it the same way.
    #[cfg(target_os = "macos")]
    crate::backend::sqlite::lifecycle::file_control::disable_persistent_wal(&connection)?;
    let mode: String = connection
        .query_row("PRAGMA journal_mode=wal", params![], |row| row.get(0))
        .map_err(db_error)?;
    if mode != "wal" {
        return Err(HistoryError::Integrity("staged Store is not in WAL mode"));
    }
    connection
        .execute_batch(&format!("PRAGMA synchronous={synchronous}"))
        .map_err(db_error)?;
    let busy: i64 = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", params![], |row| {
            row.get(0)
        })
        .map_err(db_error)?;
    if busy != 0 {
        return Err(HistoryError::Busy);
    }
    connection.close().map_err(|(_, error)| db_error(error))?;
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = target.as_os_str().to_os_string();
        sidecar.push(suffix);
        if std::fs::symlink_metadata(sidecar).is_ok() {
            return Err(HistoryError::Integrity("staged Store sidecar"));
        }
    }
    let target_bytes = space::trim(&target, u64::try_from(pages).unwrap_or(0), page_bytes)?;
    Ok(Built {
        target,
        target_bytes,
        visited_bytes,
        logical: u64::try_from(totals.0).unwrap_or(0),
        canonical: u64::try_from(totals.1).unwrap_or(0),
        stored: u64::try_from(totals.2).unwrap_or(0),
        metadata: u64::try_from(totals.3).unwrap_or(0),
        repacked,
    })
}
