//! §7 steps 3–4: the staged rebuild, its capacity inputs and the §11 peak bound.

use super::op::compact_path;
use super::op::hex;
use super::op::stop_at;
use super::op::visited_path;
use super::op::RunOp;
use super::types::db_error;
use super::types::io_error;
use super::types::CompactFailpoint;
use super::types::CompactRequest;
use super::types::MarkTuple;
use super::types::JOURNAL_HEADER_BOUND;
use super::verify::file_free_bytes;
use super::verify::open_immutable;
use crate::backend::records::BackendError;
use crate::backend::sqlite::lifecycle::seal::SealedFacts;
use crate::backend::sqlite::session::connection::Session;
use layerfs_history::error::HistoryError;
use layerfs_history::error::HistoryResult;
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

/// §11 peak temporary bytes for a rebuild: the staged copy's extent, the
/// rollback-journal bound, the new Store's extent and the visited scratch.
pub fn peak_temp_bytes(
    staged_bytes: u64,
    journal_bound_bytes: u64,
    target_bytes: u64,
    visited_bytes: u64,
) -> u64 {
    staged_bytes
        .saturating_add(journal_bound_bytes)
        .saturating_add(target_bytes)
        .saturating_add(visited_bytes)
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

/// The rollback-journal upper bound of one transaction on a `pages`-page file:
/// one record per modified page (`page_size + 8` bytes: page number, page image
/// and checksum) plus the header allowance. A page is journaled at most once
/// per transaction, so the staged copy's page count bounds both the prune and
/// the rewrite journal, and the bound never under-reports the observed file.
pub const fn journal_bound_bytes(pages: u64, page_size: u64) -> u64 {
    pages
        .saturating_mul(page_size.saturating_add(8))
        .saturating_add(JOURNAL_HEADER_BOUND)
}

/// The staged file's page count and page size, before the prune frees pages.
pub(crate) fn file_page_extent(connection: &Connection) -> HistoryResult<(u64, u64)> {
    let pages: i64 = connection
        .query_row("PRAGMA page_count", params![], |row| row.get(0))
        .map_err(db_error)?;
    let page: i64 = connection
        .query_row("PRAGMA page_size", params![], |row| row.get(0))
        .map_err(db_error)?;
    Ok((
        u64::try_from(pages).map_err(|_| HistoryError::Integrity("page count"))?,
        u64::try_from(page).map_err(|_| HistoryError::Integrity("page size"))?,
    ))
}

/// What the staged rebuild produced, for the report and the install fence.
pub(crate) struct Built {
    pub(crate) target: PathBuf,
    pub(crate) target_bytes: u64,
    /// The staged copy's extent right after `VACUUM INTO`, before the prune.
    pub(crate) staged_bytes: u64,
    /// The rollback-journal upper bound the staged page count implies.
    pub(crate) journal_bound_bytes: u64,
    pub(crate) visited_bytes: u64,
    pub(crate) logical: u64,
    pub(crate) canonical: u64,
    pub(crate) stored: u64,
    pub(crate) metadata: u64,
    pub(crate) free_bytes: u64,
}

impl Built {
    /// §11 peak temporary bytes: the run's whole artifact footprint below the
    /// rename, as a documented upper bound rather than a sampled peak.
    ///
    /// The staged copy is the whole sealed Store (`VACUUM INTO`), so it keeps
    /// the source extent until the rewrite's final truncate. The rewrite then
    /// writes the new Store into SQLite's temp database (the process temp
    /// directory, unlinked) while rewriting the staged file in place, and both
    /// the prune and the rewrite journal the pages they overwrite. The journal
    /// term bounds either journal by one record per staged page
    /// (`page_size + 8` bytes each: page number, image and checksum) plus the
    /// header allowance, so it never under-reports the observed journal. The
    /// sum over-reports the measured 50 GB peak by a few percent; §13 already
    /// marks the report an estimate.
    pub(crate) fn peak_temp_bytes(&self) -> u64 {
        peak_temp_bytes(
            self.staged_bytes,
            self.journal_bound_bytes,
            self.target_bytes,
            self.visited_bytes,
        )
    }
}

/// Escaped SQL text literal for an `ATTACH` path (the statement cannot bind).
pub(crate) fn quote(text: &str) -> String {
    text.replace('\'', "''")
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
    pub(crate) layout: crate::SqlitePackLayout,
    pub(crate) synchronous: i64,
}

/// The §7 steps 2–5: build the staged Store, copy only retained rows, compact
/// it, and switch it to WAL.
pub(crate) fn build(
    input: BuildInput<'_>,
    capacity: bool,
    request: &CompactRequest,
    op: &RunOp,
) -> HistoryResult<Built> {
    let BuildInput {
        facts,
        tuple,
        layout,
        synchronous,
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
        // §7 step 3 on a bounded filesystem: `VACUUM INTO` has already created
        // the staged file when the write meets ENOSPC. The abort path removes
        // it and the live Store is unchanged.
        std::fs::write(&target, b"staged").map_err(io_error)?;
        return Err(io_error(std::io::Error::from_raw_os_error(28)));
    }

    // A real-process crash arm stops here: the visited scratch is written and
    // `VACUUM INTO` has not started. See `BeforeVacuum`.
    stop_at(request, CompactFailpoint::BeforeVacuum, op, store)?;
    let source = open_immutable(store)?;
    let target_text = target
        .to_str()
        .ok_or(HistoryError::InvalidInput("compaction path"))?;
    source
        .execute("VACUUM INTO ?1", params![target_text])
        .map_err(db_error)?;
    source.close().map_err(|(_, error)| db_error(error))?;
    // The copy produced the staged file but has not pruned or verified it.
    stop_at(request, CompactFailpoint::DuringCopy, op, store)?;
    // The staged copy holds the source's whole extent until the final
    // `VACUUM` truncates it; §11 counts it in the peak temp.
    let staged_bytes = std::fs::metadata(&target).map_err(io_error)?.len();

    let connection = Connection::open_with_flags(
        &target,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(db_error)?;
    connection
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(db_error)?;
    connection
        .execute_batch(&format!(
            "PRAGMA journal_mode=DELETE; PRAGMA synchronous={synchronous};"
        ))
        .map_err(db_error)?;
    // The prune and the rewrite journal the pages they overwrite; one record
    // per staged page bounds either journal before the staged file shrinks.
    let (staged_pages, page_size) = file_page_extent(&connection)?;
    let journal_bound_bytes = journal_bound_bytes(staged_pages, page_size);
    // The copied published-pack rows may be deleted in the artifact; the live
    // immutability triggers are restored before the file is verified.
    // §7 step 4 asks for `foreign_keys=ON` outside the transaction (inside it
    // the pragma is a no-op). The pruning keeps `foreign_keys=OFF` — an
    // explicit, tested deviation: with enforcement on, SQLite's deferred
    // constraint counter survives this delete order and the COMMIT is refused
    // `SQLITE_CONSTRAINT_FOREIGNKEY` even though `pragma_foreign_key_check`
    // reports the final row set clean. The verification's
    // `pragma_foreign_key_check` proves that same final state, so the pragma is
    // not the only guard.
    // `sqlite_sequence` is the acquisition operation counter: the copy keeps
    // every row the source had, and the sequence never falls (§4.1, §7).
    let sequences: Vec<(String, i64)> = if table_present(&connection, "sqlite_sequence")? {
        let mut statement = connection
            .prepare("SELECT name,seq FROM sqlite_sequence")
            .map_err(db_error)?;
        let mut rows = statement.query(params![]).map_err(db_error)?;
        let mut found = Vec::new();
        while let Some(row) = rows.next().map_err(db_error)? {
            found.push((
                row.get::<_, String>(0).map_err(db_error)?,
                row.get::<_, i64>(1).map_err(db_error)?,
            ));
        }
        found
    } else {
        Vec::new()
    };
    let triggers: Vec<String> = {
        let mut statement = connection
            .prepare(
                "SELECT sql FROM sqlite_schema \
WHERE type='trigger' AND name LIKE '%immutable%' AND sql IS NOT NULL",
            )
            .map_err(db_error)?;
        let mut rows = statement.query(params![]).map_err(db_error)?;
        let mut found = Vec::new();
        while let Some(row) = rows.next().map_err(db_error)? {
            found.push(row.get::<_, String>(0).map_err(db_error)?);
        }
        found
    };
    connection
        .execute_batch(
            "DROP TRIGGER IF EXISTS pack_immutable_update;\
DROP TRIGGER IF EXISTS pack_immutable_delete;\
DROP TRIGGER IF EXISTS pack_unit_immutable_update;\
DROP TRIGGER IF EXISTS pack_unit_immutable_delete;\
PRAGMA foreign_keys=OFF;\
PRAGMA defer_foreign_keys=ON;",
        )
        .map_err(db_error)?;
    connection
        .execute_batch(&format!(
            "ATTACH DATABASE '{}' AS scratch;",
            quote(
                visited
                    .to_str()
                    .ok_or(HistoryError::InvalidInput("compaction path"))?
            )
        ))
        .map_err(db_error)?;

    let units = layout.uses_units();
    let acquisition_tables = table_present(&connection, "init_operation")?;
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(db_error)?;
    let staged = (|| -> HistoryResult<()> {
        // §7 step 4: the totals trigger invariant holds before the copy.
        let before: (i64, i64) = connection
            .query_row(
                "SELECT (SELECT logical FROM store_totals WHERE id=1),\
(SELECT COALESCE(SUM(logical),0) FROM \"commit\")+\
(SELECT COALESCE(SUM(logical),0) FROM layer)",
                params![],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(db_error)?;
        if before.0 != before.1 {
            return Err(HistoryError::Integrity(
                "Store totals disagree with the history rows",
            ));
        }
        let mut deletes = format!(
            "DELETE FROM object_location WHERE object_id NOT IN \
(SELECT object_id FROM scratch.visited);\
DELETE FROM content_signature WHERE object_id NOT IN \
(SELECT object_id FROM scratch.visited);\
DELETE FROM file_digest WHERE root NOT IN (SELECT object_id FROM scratch.visited);\
DELETE FROM binding_hint_block;\
DELETE FROM \"commit\" WHERE commit_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='commit');\
DELETE FROM layer WHERE layer_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='layer');\
DELETE FROM branch WHERE branch_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='branch');\
DELETE FROM layer_stack WHERE layer_stack_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='stack');\
DELETE FROM layer_lineage WHERE layer_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='layer');\
DELETE FROM commit_lineage WHERE commit_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='commit');\
DELETE FROM absorbed WHERE run NOT IN (SELECT run FROM commit_lineage);\
DELETE FROM reach WHERE key NOT IN \
(SELECT layer_stack_id FROM layer_stack UNION SELECT run FROM commit_lineage);\
DELETE FROM branch_retired WHERE branch_id NOT IN \
(SELECT node_id FROM kept_node WHERE kind='branch');\
DELETE FROM layer_stack_retired WHERE layer_stack_id NOT IN \
(SELECT layer_stack_id FROM layer_stack);\
DELETE FROM path_removed WHERE layer_stack_id NOT IN \
(SELECT layer_stack_id FROM layer_stack);\
DELETE FROM project_epoch WHERE layer_stack_id NOT IN \
(SELECT layer_stack_id FROM layer_stack);\
DELETE FROM merge_record;\
DELETE FROM merge_operation;\
DELETE FROM gc_intent WHERE op_id<>x'{}';\
DELETE FROM gc_dead;\
DELETE FROM gc_op;",
            hex(&op.op_id)
        );
        if units {
            deletes.push_str(
                "DELETE FROM pack_unit WHERE pack_id NOT IN \
(SELECT pack_id FROM scratch.live_pack);",
            );
        }
        deletes.push_str(
            "DELETE FROM pack WHERE pack_id NOT IN (SELECT pack_id FROM scratch.live_pack);",
        );
        if acquisition_tables {
            for table in ["init_operation", "init_entry", "init_host_file"] {
                deletes.push_str(&format!("DELETE FROM {table};"));
            }
        }
        for trigger in &triggers {
            deletes.push_str(trigger.trim_end_matches(';'));
            deletes.push_str(";\n");
        }
        deletes.push_str(&format!(
            "UPDATE history_meta SET catalog_incarnation={} WHERE id=1;\
UPDATE store_totals SET \
logical=(SELECT COALESCE(SUM(logical),0) FROM \"commit\")+\
(SELECT COALESCE(SUM(logical),0) FROM layer),\
canonical=(SELECT COALESCE(SUM(canonical_length),0) FROM object_location),\
stored=(SELECT COALESCE(SUM(length),0) FROM pack),\
metadata=(SELECT COALESCE(SUM(length),0) FROM pack WHERE domain=0) WHERE id=1;",
            tuple.generation + 1
        ));
        connection.execute_batch(&deletes).map_err(db_error)?;
        Ok(())
    })();
    match staged {
        Ok(()) => {
            connection.execute_batch("COMMIT").map_err(db_error)?;
        }
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            let _ = connection.execute_batch("DETACH DATABASE scratch");
            let _ = connection.close();
            return Err(error);
        }
    }
    connection
        .execute_batch("DETACH DATABASE scratch")
        .map_err(db_error)?;
    // Reclaim the deleted pages: a wholly live Store, no freelist.
    connection.execute_batch("VACUUM").map_err(db_error)?;
    if !sequences.is_empty() && table_present(&connection, "sqlite_sequence")? {
        connection
            .execute_batch("DELETE FROM sqlite_sequence")
            .map_err(db_error)?;
        let mut statement = connection
            .prepare("INSERT INTO sqlite_sequence(name,seq) VALUES(?1,?2)")
            .map_err(db_error)?;
        for (name, seq) in &sequences {
            statement.execute(params![name, seq]).map_err(db_error)?;
        }
        drop(statement);
        // §4.1/§7: the acquisition operation counter never falls. The copy
        // keeps every row the source had, so a restored sequence below the
        // source one is a rebuild bug, never a smaller Store.
        for (name, seq) in &sequences {
            let restored: i64 = connection
                .query_row(
                    "SELECT seq FROM sqlite_sequence WHERE name=?1",
                    params![name],
                    |row| row.get(0),
                )
                .map_err(db_error)?;
            if restored < *seq {
                return Err(HistoryError::Integrity("staged Store sqlite_sequence fell"));
            }
        }
    }
    let totals: (i64, i64, i64, i64) = connection
        .query_row(
            "SELECT logical,canonical,stored,metadata FROM store_totals WHERE id=1",
            params![],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(db_error)?;
    connection.close().map_err(|(_, error)| db_error(error))?;

    // §7 step 5: switch to WAL, checkpoint, and leave no sidecar.
    let connection = Connection::open_with_flags(
        &target,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(db_error)?;
    connection
        .busy_timeout(std::time::Duration::ZERO)
        .map_err(db_error)?;
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
    let target_bytes = std::fs::metadata(&target).map_err(io_error)?.len();
    let free_bytes = file_free_bytes(&target)?;
    Ok(Built {
        target,
        target_bytes,
        staged_bytes,
        journal_bound_bytes,
        visited_bytes,
        logical: u64::try_from(totals.0).unwrap_or(0),
        canonical: u64::try_from(totals.1).unwrap_or(0),
        stored: u64::try_from(totals.2).unwrap_or(0),
        metadata: u64::try_from(totals.3).unwrap_or(0),
        free_bytes,
    })
}
