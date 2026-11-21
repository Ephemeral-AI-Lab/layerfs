//! The publication lane: one READ COMMITTED transaction on its own
//! connection that holds the Store's lane row `FOR NO KEY UPDATE`, with
//! `lock_timeout` bounding the wait. While it is held, this thread's writes
//! run as ordinary transactions on the session connection, each acknowledged
//! at its own COMMIT; every other writer, in any process, waits for the row.
//! Never SERIALIZABLE and never retried: a wait that reaches the timeout is
//! `Busy`. Dropping the hold rolls its transaction back (it wrote nothing).
use crate::backend::postgresql::session::{connection::Session, driver::Error};
use crate::backend::records::BackendError;
use layerfs_telemetry::timer::waits::Wait;
use std::{marker::PhantomData, sync::Arc};

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
            *lane = Some(self.connector.connect()?);
        }
        let client = lane.as_mut().ok_or(BackendError::Unknown)?;
        self.lane_work.add(0, 1);
        // Drain any nested KEY SHARE writers left by a disconnected holder before
        // acknowledging the new hold. Rolling back the savepoint downgrades the
        // lock so our own nested writes can pin the row through their COMMIT.
        let xid = client
            .simple_query(&self.settings.lane_begin())
            .map_err(|e| {
                let disconnected = Error::from(e);
                if disconnected.disconnected() {
                    BackendError::Closed
                } else {
                    disconnected.backend()
                }
            })
            .and_then(|messages| {
                messages
                    .iter()
                    .rev()
                    .find_map(|message| match message {
                        postgres::SimpleQueryMessage::Row(row)
                            if row.columns()[0].name() == "lane_xid" =>
                        {
                            row.get(0)
                        }
                        _ => None,
                    })
                    .and_then(|id| id.parse::<u64>().ok())
                    .ok_or(BackendError::Integrity)
            });
        let xid = match xid {
            Ok(xid) => xid,
            Err(error) => {
                self.lane_work.add(0, 1);
                if client.batch_execute("ROLLBACK").is_err() {
                    *lane = None;
                }
                return Err(error);
            }
        };
        self.lane_xid
            .store(xid, std::sync::atomic::Ordering::Release);
        drop(lane);
        turn.hold()?;
        Ok(LaneHold {
            session: self.clone(),
            _thread: PhantomData,
        })
    }
}

impl Drop for LaneHold {
    fn drop(&mut self) {
        let mut lane = self
            .session
            .lane
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(client) = lane.as_mut() {
            self.session.lane_work.add(0, 1);
            if client.batch_execute("ROLLBACK").is_err() {
                // The server ends the transaction, and its row lock, with the connection.
                *lane = None;
            }
        }
        drop(lane);
        self.session.admission.release_hold();
    }
}
