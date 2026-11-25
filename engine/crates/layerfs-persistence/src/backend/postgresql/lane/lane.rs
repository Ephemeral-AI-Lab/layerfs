//! The publication lane: one READ COMMITTED transaction on its own
//! connection that holds the Store's lane row `FOR UPDATE`, with
//! `lock_timeout` bounding the wait. While it is held, this thread's writes
//! run as ordinary transactions on the session connection, each acknowledged
//! at its own COMMIT; every other writer, in any process, waits for the row.
//! Never SERIALIZABLE and never retried: a wait that reaches the timeout is
//! `Busy`. Dropping the hold rolls its transaction back (it wrote nothing).
//! The lane transaction's id is recorded when it begins, so a nested write
//! can prove, in its COMMIT round trip, that the row lock it relies on still
//! exists (finding #10; the barrier is described in `session/profile.rs`).
use crate::backend::postgresql::session::{connection::Session, rows};
use crate::backend::records::BackendError;
use layerfs_telemetry::timer::waits::Wait;
use postgres::{Client, SimpleQueryMessage};
use std::{fmt::Write, marker::PhantomData, sync::Arc};

/// The connection that holds the lane, and the id of its open lane
/// transaction while a hold lives.
pub(crate) struct LaneConnection {
    client: Client,
    xid: Option<String>,
}

/// Column `column` of a simple-query reply's last row.
fn last_value(messages: &[SimpleQueryMessage], column: usize) -> Option<String> {
    messages.iter().rev().find_map(|message| match message {
        SimpleQueryMessage::Row(row) => row.get(column).map(str::to_owned),
        _ => None,
    })
}

/// A Branch, its expected head Commit (if any) and base Layer, by identity.
pub(crate) type HeadProbe<'a> = (&'a [u8], Option<&'a [u8]>, &'a [u8]);

/// A `bytea` literal of `bytes`.
fn bytea(bytes: &[u8]) -> String {
    let mut literal = String::from("decode('");
    for byte in bytes {
        let _ = write!(literal, "{byte:02x}");
    }
    literal.push_str("','hex')");
    literal
}

/// The trailing column of the lane's `BEGIN` batch that checks a Branch's
/// head after the lane row and the barrier: `t` when the Branch is visible
/// (the `branch_snapshot` filters) at exactly that head Commit and base.
fn head_check((branch, head, base): HeadProbe<'_>) -> String {
    let head = head.map_or("b.head_commit_id IS NULL".to_owned(), |head| {
        format!("b.head_commit_id = {}", bytea(head))
    });
    format!(
        ", EXISTS (SELECT 1 FROM branch b WHERE b.branch_id = {} AND b.base_layer_id = {} \
         AND {head} AND NOT EXISTS (SELECT 1 FROM branch_retired r WHERE r.branch_id = b.branch_id) \
         AND NOT EXISTS (SELECT 1 FROM layer_stack_retired d WHERE d.layer_stack_id = b.layer_stack_id))",
        bytea(branch),
        bytea(base),
    )
}

/// A held lane; dropping it releases the lane row, then the turn.
pub struct LaneHold {
    session: Arc<Session>,
    // The hold belongs to the thread that took it.
    _thread: PhantomData<*const ()>,
}

impl Session {
    pub(crate) fn hold_lane(self: &Arc<Self>) -> Result<LaneHold, BackendError> {
        self.hold_lane_unmoved(None).map(|(hold, _)| hold)
    }

    /// [`Self::hold_lane`], and with `probe`, in the same round trip, whether
    /// the Branch is visible at that head once the lane is held (#768): the
    /// check runs after the lane row and the barrier, as a read issued after
    /// `hold_lane` returned would. It is not generation-fenced; the
    /// publication's own transaction is.
    pub(crate) fn hold_lane_unmoved(
        self: &Arc<Self>,
        probe: Option<HeadProbe<'_>>,
    ) -> Result<(LaneHold, bool), BackendError> {
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
            "{}; SELECT pg_current_xact_id()::text{}",
            self.settings.lane_begin(),
            probe.map(head_check).unwrap_or_default()
        );
        let unmoved = match connection.client.simple_query(&begin) {
            Ok(messages) => match last_value(&messages, 0) {
                Some(xid) => {
                    connection.xid = Some(xid);
                    probe.is_some() && last_value(&messages, 1).as_deref() == Some("t")
                }
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
                    let rollback = format!(
                        "ROLLBACK{}",
                        crate::backend::postgresql::session::profile::barrier_leftover(
                            &self.settings.schema
                        )
                    );
                    if connection.client.batch_execute(&rollback).is_ok() {
                        return Err(error);
                    }
                }
                // Closing the connection ends whatever the server still holds.
                *lane = None;
                return Err(BackendError::Unknown);
            }
        };
        drop(lane);
        turn.hold();
        let hold = LaneHold {
            session: self.clone(),
            _thread: PhantomData,
        };
        Ok((hold, unmoved))
    }
}

impl Session {
    /// Waits until every publication that held the lane when it began has
    /// ended (#776 C2). On a connection of its own it takes the lane as a
    /// holder does: the lane row `FOR UPDATE`, which a non-nested publication
    /// keeps until its transaction ends, then the barrier key exclusively,
    /// which a nested write keeps shared until its own `COMMIT` or abort, both
    /// under `lock_timeout`. Then it rolls back and closes the connection: it
    /// writes and keeps nothing, needs no turn and works on a session an
    /// unknown outcome quarantined. A wait that reaches the timeout is `Busy`.
    ///
    /// On a quarantined session the lane alone proves nothing: a fused
    /// Commit sends its packs before it takes the lane row, so a backend
    /// still inserting them when the reply was lost would take the lane and
    /// commit after this pass. So first its backend is ended
    /// ([`Self::end_lost_backend`]); a `COMMIT` it already made is then
    /// found by the caller's read, and one it had not made is aborted.
    pub(crate) fn pass_lane(&self) -> Result<(), BackendError> {
        let lost = self
            .state
            .lock()
            .map_err(|_| BackendError::Unknown)?
            .quarantined;
        let mut client = self.connector.connect()?;
        self.lane_work.add(1, 1);
        if lost {
            self.end_lost_backend(&mut client)?;
        }
        let passed = client
            .simple_query(&self.settings.lane_begin())
            .map(|_| ())
            .map_err(|error| rows::error(&error));
        // Closing the connection ends the transaction even if this fails.
        let _ = client.batch_execute("ROLLBACK");
        passed
    }

    /// Terminates the backend of this quarantined session's connection, if
    /// it is still there, and waits until it is gone, bounded by
    /// `lock_timeout` (30 s when that is 0): `pg_terminate_backend` with a
    /// timeout answers true only once the process has exited, after its
    /// transaction ended. Waiting for it is not a retry. One round trip on
    /// `client`. Not proven gone within the bound is `Busy`; a backend the
    /// open did not name, or one a session lock showed on another backend
    /// (a transaction-pooling proxy), is `Unknown`: the outcome stays
    /// unknown either way.
    fn end_lost_backend(&self, client: &mut Client) -> Result<(), BackendError> {
        let (pid, start) = *self.opened_backend.get().ok_or(BackendError::Unknown)?;
        let noted = self.backend_pid.load(std::sync::atomic::Ordering::SeqCst);
        let pid = i32::try_from(pid).map_err(|_| BackendError::Unknown)?;
        if noted != 0 && noted != i64::from(pid) {
            return Err(BackendError::Unknown);
        }
        let bound = match self.settings.lock_timeout_ms {
            0 => 30_000,
            ms => i64::from(ms),
        };
        self.lane_work.add(0, 1);
        let rows = client
            .query(
                "SELECT pg_terminate_backend(pid, $3) FROM pg_catalog.pg_stat_activity \
                  WHERE pid = $1 AND ($2::bigint IS NULL \
                    OR (extract(epoch FROM backend_start) * 1000000)::bigint = $2)",
                &[&pid, &start, &bound],
            )
            .map_err(|error| rows::error(&error))?;
        match rows.first().map(|row| row.get::<_, bool>(0)) {
            None | Some(true) => Ok(()),
            Some(false) => Err(BackendError::Busy),
        }
    }
}

impl Session {
    /// A nested write's `COMMIT`, preceded in the same round trip by its lane
    /// check on the writer (finding #10; #768 moved it off the lane
    /// connection): the statement divides by zero, so the server skips the
    /// `COMMIT`, unless the lane transaction recorded at `hold_lane` is still
    /// in progress, which it is exactly while it holds the lane row. No
    /// recorded lane transaction is `Closed` before anything is sent. The
    /// barrier key the write holds keeps a new lane holder out until its
    /// `COMMIT` ends.
    pub(crate) fn lane_commit(&self) -> Result<String, BackendError> {
        let lane = self.lane.lock().map_err(|_| BackendError::Unknown)?;
        let xid = lane
            .as_ref()
            .and_then(|connection| connection.xid.as_deref())
            .filter(|xid| !xid.is_empty() && xid.bytes().all(|b| b.is_ascii_digit()))
            .ok_or(BackendError::Closed)?;
        Ok(format!(
            "SELECT 1/(pg_xact_status('{xid}'::xid8) IS NOT DISTINCT FROM 'in progress')::int; COMMIT"
        ))
    }

    /// After a failed lane check: closing the lane connection ends whatever
    /// it still holds.
    pub(crate) fn close_lane(&self) {
        let mut lane = self
            .lane
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *lane = None;
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
