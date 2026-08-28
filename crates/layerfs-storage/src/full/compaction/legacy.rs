//! Transitional legacy_full compaction verification helpers.

use crate::integrity;
use crate::{
    add_verification_progress_counters, authenticate_borrowed_unaccounted, checked_add,
    map_sqlite_error, EngineCounters, EngineError, EngineResult,
};
use layerfs_core::{validate_object_from, ObjectId};
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

pub(super) fn candidate_auxiliary_bytes(path: &Path) -> u64 {
    ["-journal", "-wal", "-shm"]
        .into_iter()
        .filter_map(|suffix| {
            let mut name = path.as_os_str().to_os_string();
            name.push(suffix);
            fs::metadata(PathBuf::from(name))
                .ok()
                .map(|value| value.len())
        })
        .sum()
}

pub(super) fn add_compaction_verification_counters(
    counters: &mut EngineCounters,
    observation: integrity::VerificationObservation,
) -> EngineResult<()> {
    add_verification_progress_counters(counters, observation)?;
    checked_add(
        &mut counters.namespace_graph_verification_passes,
        observation.namespace_graphs,
    )?;
    checked_add(
        &mut counters.retained_roots_validated,
        observation.retained_roots_validated,
    )
}

pub(super) fn reject_legacy_compaction_state(connection: &Connection) -> EngineResult<()> {
    let present = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM layerfs_roots)
                 OR EXISTS(SELECT 1 FROM layerfs_deltas WHERE format_version = 0)
                 OR EXISTS(SELECT 1 FROM layerfs_store_meta WHERE visible_root IS NOT NULL)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)?;
    if present {
        Err(EngineError::InvalidRecord("legacy compaction state"))
    } else {
        Ok(())
    }
}

pub(super) fn authenticate_complete_object_index(connection: &Connection) -> EngineResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT object_id, kind, canonical_length, canonical_bytes
             FROM layerfs_objects ORDER BY rowid",
        )
        .map_err(map_sqlite_error)?;
    let mut rows = statement.query([]).map_err(map_sqlite_error)?;
    while let Some(row) = rows.next().map_err(map_sqlite_error)? {
        let id = ObjectId::from_bytes(&row.get::<_, Vec<u8>>(0).map_err(map_sqlite_error)?)?;
        let kind = row.get::<_, i64>(1).map_err(map_sqlite_error)?;
        let length = row.get::<_, i64>(2).map_err(map_sqlite_error)?;
        let bytes = match row.get_ref(3).map_err(map_sqlite_error)? {
            ValueRef::Blob(bytes) => bytes,
            _ => return Err(EngineError::InvalidRecord("object bytes")),
        };
        let (summary, _) = authenticate_borrowed_unaccounted(id, kind, length, bytes)?;
        let decoded = validate_object_from(Cursor::new(bytes))
            .map_err(|cause| EngineError::MalformedObject { id, cause })?;
        if decoded != summary {
            return Err(EngineError::InvalidRecord("object summary"));
        }
    }
    Ok(())
}
