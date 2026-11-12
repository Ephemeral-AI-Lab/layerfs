//! Connection custody at the last Session close, including quarantined owners.
use super::connection::State;
use crate::backend::sqlite::lane::writer_admission::Writer;
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
        // An uncertain transaction already owns the lane. Taking it again
        // through an independent directory descriptor would deadlock.
        let writer = match self.writer_hold.take() {
            Some(writer) => writer,
            None => match Writer::acquire(&self.path) {
                Ok(writer) => writer,
                Err(_) => {
                    // Drop cannot report a cancelled/failed admission. Retain
                    // the SQLite connection until process exit instead of
                    // attempting unadmitted WAL teardown or retrying the wait.
                    std::mem::forget(self.connection.take());
                    return;
                }
            },
        };
        // The background connection has already ended; this may be SQLite's
        // last connection and can checkpoint/delete the WAL and SHM files.
        drop(self.connection.take());
        drop(writer);
    }
}
