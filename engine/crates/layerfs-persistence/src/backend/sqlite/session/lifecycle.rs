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

impl State {
    /// Ends SQLite's custody of a quarantined connection while the calling
    /// thread holds the Store's writer lane (finding #2). `sqlite3_close`
    /// rolls back any transaction the uncertain attempt left open and frees
    /// SQLite's file locks, so the lane can be released to other processes;
    /// nothing is replayed and the Session stays quarantined, refusing every
    /// later call before it touches the connection. The checkpoint thread's
    /// connection may now be SQLite's last, and it closes without the lane,
    /// so it is told to close without checkpointing or removing `-wal`/`-shm`.
    /// `true`: SQLite holds no transaction for this State. `false`: the close
    /// was refused with a transaction still open, and the caller keeps the
    /// lane. (rusqlite flushes its statement cache first and every statement
    /// or BLOB handle borrows the connection, so an owned connection has
    /// nothing left for SQLite to refuse the close on.)
    pub(crate) fn close_quarantined(
        &mut self,
        background: Option<&super::background::Checkpointer>,
    ) -> bool {
        debug_assert!(self.quarantined);
        let Some(connection) = self.connection.take() else {
            return true;
        };
        if let Some(background) = background {
            background.close_quietly();
        }
        match connection.close() {
            Ok(()) => true,
            Err((connection, _)) => {
                let idle = connection.is_autocommit();
                self.connection.restore(connection);
                idle
            }
        }
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
            // A writable close waits for the lane like any write: its close
            // may be SQLite's last, which checkpoints and removes the WAL and
            // SHM files that a pull's verify and a restore's in-use check
            // rely on. If the wait itself fails (interrupted, or the Store's
            // directory cannot be opened), the close turns SQLite's
            // checkpoint-on-close off first, so it takes no exclusive lock and
            // removes nothing; there is no second attempt at the lane.
            (None, Some(path)) => match Writer::acquire(path) {
                Ok(writer) => Some(writer),
                Err(_) => {
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
