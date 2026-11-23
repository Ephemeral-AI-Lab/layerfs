//! The publication lane: one READ COMMITTED transaction on its own
//! connection that holds the Store's lane row `FOR UPDATE`, with
//! `lock_timeout` bounding the wait. While it is held, this thread's writes
//! run as ordinary transactions on the session connection, each acknowledged
//! at its own COMMIT; every other writer, in any process, waits for the row.
//! Never SERIALIZABLE and never retried: a wait that reaches the timeout is
//! `Busy`. Dropping the hold rolls its transaction back (it wrote nothing).
//! The lane transaction's id is recorded when it begins, so a nested write
//! can prove, before its COMMIT, that the row lock it relies on still exists
//! (finding #10; the barrier is described in `session/profile.rs`).
use crate::backend::postgresql::session::{connection::Session, rows};
use crate::backend::records::BackendError;
use layerfs_telemetry::timer::waits::Wait;
use postgres::{Client, SimpleQueryMessage};
use std::{marker::PhantomData, sync::Arc};

/// The connection that holds the lane, and the id of its open lane
/// transaction while a hold lives.
pub(crate) struct LaneConnection {
    client: Client,
    xid: Option<String>,
}

/// The last row's first column of a simple-query reply.
fn last_value(messages: &[SimpleQueryMessage]) -> Option<String> {
    messages.iter().rev().find_map(|message| match message {
        SimpleQueryMessage::Row(row) => row.get(0).map(str::to_owned),
        _ => None,
    })
}

/// A held lane; dropping it releases the lane row, then the turn.
pub struct LaneHold {
    session: Arc<Session>,
    // The hold belongs to the thread that took it.
    _thread: PhantomData<*const ()>,
}

impl Session {
    pub(crate) fn hold_lane(self: &Arc<Self>) -> Result<LaneHold, BackendError> {
        if !self.writable {
            return Err(BackendError::ReadOnly);
        }
        let turn = self.admission.enter(Wait::Writer)?;
        if turn.nested {
            return Err(BackendError::Integrity);
        }
        let mut lane = self.lane.lock().map_err(|_| BackendError::Unknown)?;
        if lane.is_none() {
            self.lane_work.add(1, 0);
            *lane = Some(LaneConnection {
                client: self.connector.connect()?,
                xid: None,
            });
        }
        let connection = lane.as_mut().ok_or(BackendError::Unknown)?;
        self.lane_work.add(0, 1);
        let begin = format!(
            "{}; SELECT pg_current_xact_id()::text",
            self.settings.lane_begin()
        );
        match connection.client.simple_query(&begin) {
            Ok(messages) => match last_value(&messages) {
                Some(xid) => connection.xid = Some(xid),
                None => {
                    // No id came back: the lane cannot be proven later.
                    *lane = None;
                    return Err(BackendError::Unknown);
                }
            },
            Err(e) => {
                let error = rows::error(&e);
                if error != BackendError::Unknown {
                    self.lane_work.add(0, 1);
                    if connection.client.batch_execute("ROLLBACK").is_ok() {
                        return Err(error);
                    }
                }
                // Closing the connection ends whatever the server still holds.
                *lane = None;
                return Err(BackendError::Unknown);
            }
        }
        drop(lane);
        turn.hold()?;
        Ok(LaneHold {
            session: self.clone(),
            _thread: PhantomData,
        })
    }
}

impl Session {
    /// One round trip on the lane connection, before a nested write's
    /// COMMIT: `Ok` only when the lane transaction recorded at `hold_lane` is
    /// still the open transaction of a live connection, so the lane row is
    /// still locked. Anything else (a closed connection, an error, another or
    /// no transaction) closes the lane connection and is `Closed`: the caller
    /// rolls its write back, a definite failure. The barrier key the write
    /// holds keeps a new lane holder out until then.
    pub(crate) fn lane_still_held(&self) -> Result<(), BackendError> {
        let mut lane = self.lane.lock().map_err(|_| BackendError::Unknown)?;
        let Some(connection) = lane.as_mut() else {
            return Err(BackendError::Closed);
        };
        let Some(expected) = connection.xid.clone() else {
            return Err(BackendError::Closed);
        };
        self.lane_work.add(0, 1);
        let held = connection
            .client
            .simple_query("SELECT pg_current_xact_id_if_assigned()::text")
            .ok()
            .and_then(|messages| last_value(&messages))
            .is_some_and(|xid| xid == expected);
        if !held {
            // Closing it ends whatever that connection still holds.
            *lane = None;
            return Err(BackendError::Closed);
        }
        Ok(())
    }
}

impl Drop for LaneHold {
    fn drop(&mut self) {
        let mut lane = self
            .session
            .lane
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(connection) = lane.as_mut() {
            connection.xid = None;
            self.session.lane_work.add(0, 1);
            if connection.client.batch_execute("ROLLBACK").is_err() {
                // The server ends the transaction, and its row lock, with the connection.
                *lane = None;
            }
        }
        drop(lane);
        self.session.admission.release_hold();
    }
}
