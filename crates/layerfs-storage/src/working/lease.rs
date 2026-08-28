//! Working operation lease cleanup and expiry time.

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::record_id::OperationId;
use rusqlite::params;
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn release_operation_lease(
    connection: &Connection,
    operation_id: OperationId,
) -> EngineResult<()> {
    connection
        .execute(
            "DELETE FROM layerfs_version_leases
             WHERE owner_kind = 'operation_workspace' AND owner_id = ?1",
            params![operation_id.as_bytes()],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub(crate) fn unix_seconds() -> EngineResult<i64> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| EngineError::InvalidRecord("system clock"))?
        .as_secs();
    i64::try_from(seconds).map_err(|_| EngineError::CounterOverflow)
}
