//! Audited macOS seal-only SQLite file control, authorized 2026-10-07.
use super::rows;
use crate::backend::records::BackendError;
use rusqlite::{ffi, Connection};
use std::ffi::c_int;

pub(super) fn disable_persistent_wal(connection: &Connection) -> Result<(), BackendError> {
    control(connection, &mut 0)?;
    let mut observed = -1;
    control(connection, &mut observed)?;
    if observed != 0 {
        return Err(BackendError::Integrity);
    }
    Ok(())
}

fn control(connection: &Connection, value: &mut c_int) -> Result<(), BackendError> {
    // SAFETY: seal consumes the sole Session before reaching this module.
    // The Connection remains live and exclusively owned throughout both calls.
    // `main` is a static NUL-terminated database name. SQLite reads/writes one
    // aligned, initialized c_int synchronously; it retains neither pointer.
    // No callback, reference escape, close or concurrent access occurs here.
    let code = unsafe {
        ffi::sqlite3_file_control(
            connection.handle(),
            c"main".as_ptr(),
            ffi::SQLITE_FCNTL_PERSIST_WAL,
            (value as *mut c_int).cast(),
        )
    };
    if code != ffi::SQLITE_OK {
        return Err(rows::error(rusqlite::Error::SqliteFailure(
            ffi::Error::new(code),
            None,
        )));
    }
    Ok(())
}
