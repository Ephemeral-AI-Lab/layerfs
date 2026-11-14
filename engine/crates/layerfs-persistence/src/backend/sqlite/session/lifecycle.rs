//! Connection custody at the last Session close, including quarantined owners.
use super::connection::State;
use crate::backend::sqlite::lane::writer_admission::Writer;
use rusqlite::config::DbConfig;
use rusqlite::Connection;
use std::ops::Deref;

/// Allows checked seal and last-owner Drop to close before releasing admission.
pub(crate) struct ConnectionSlot(Option<Connection>);

impl ConnectionSlot {
    pub(crate) fn new(connection: Connection) -> Self {
        Self(Some(connection))
    }

    pub(crate) fn take(&mut self) -> Option<Connection> {
        self.0.take()
    }

    pub(crate) fn restore(&mut self, connection: Connection) {
        self.0 = Some(connection);
    }

    fn is_closed(&self) -> bool {
        self.0.is_none()
    }
}

impl Deref for ConnectionSlot {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.0.as_ref().expect("live Session connection")
    }
}

impl Drop for State {
    fn drop(&mut self) {
        if self.connection.is_closed() {
            return;
        }
        let writer = match (self.writer_hold.take(), &self.close_lane) {
            // An uncertain transaction already owns the lane. Taking it again
            // through an independent directory descriptor would deadlock.
            (Some(writer), _) => Some(writer),
            // A read-only or diagnostic connection cannot checkpoint or remove
            // the WAL and SHM files, so its close takes no lane.
            (None, None) => None,
            // A writable close makes one non-blocking attempt: it never waits,
            // not even behind a writer another session keeps after an
            // uncertain write. Without the lane it closes with SQLite's
            // checkpoint-on-close turned off, so it takes no exclusive lock
            // and leaves the WAL and SHM files for the lane's next holder.
            (None, Some(path)) => match Writer::try_acquire(path) {
                Ok(Some(writer)) => Some(writer),
                Ok(None) | Err(_) => {
                    let quiet = self
                        .connection
                        .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true);
                    if quiet.is_err() {
                        // Drop cannot report it: retain the connection until
                        // process exit rather than tear the WAL down unadmitted.
                        std::mem::forget(self.connection.take());
                        return;
                    }
                    None
                }
            },
        };
        // The background connection has already ended; a writable close that
        // holds the lane may be SQLite's last and checkpoint/delete the WAL
        // and SHM files.
        drop(self.connection.take());
        drop(writer);
    }
}
