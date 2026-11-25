//! Reopening a quarantined writer session: one new connection, opened by
//! the next top-level call before its own work, then what the closed one
//! left settled before the new one serves anything.
//!
//! A quarantine closes the connection after an uncertain outcome
//! (`State::quarantine`). On a session that reopens (the Store service's
//! writer and readers, [`Session::enable_reopen`]) the next call that takes
//! its turn outside a held lane opens exactly one new connection: a new session
//! for a new operation, never a resend of the one whose outcome stays
//! unknown. A connection the server already closed (a terminated backend, a
//! restart, a dropped route), seen with no round trip before anything is
//! sent, is quarantined the same way first, since nothing of the call has
//! gone out. Then, in order:
//!
//! 1. The closed connections' backends are ended, as a lost reply's
//!    resolution ends them (`Session::pass_lane`): by pid and start, never
//!    one whose start is unknown, waiting (bounded by `lock_timeout`) until
//!    each has exited. One not proven gone stays recorded, so a retained
//!    Commit's resolution still ends it before concluding "not committed";
//!    the reopen proceeds.
//! 2. The new backend is read with the Store's generation: a moved
//!    generation is `Stale`, as the fence of every write would answer.
//! 3. The keys of every intent this session still holds (`held_ops`) went
//!    with the old connection: they are taken again, all or none, in the
//!    statement a registration uses (`gc_lock_session`), which checks the
//!    generation and names the backend. A refused key is `Busy`, a moved
//!    generation `Stale`, a key on another backend (a transaction-pooling
//!    proxy) `Integrity`: the new connection is closed and the writer stays
//!    quarantined. The intent rows are not touched, so they refuse removals
//!    and compaction throughout.
//! 4. Each orphaned intent (`Session::orphans`) has its row deleted by the
//!    owner release's generation-checked statement, by op id: a mount whose
//!    admission ended with an unknown outcome (it never reached its caller),
//!    and an op whose release could not be sent because its turn found the
//!    writer quarantined (its owner has closed). Nothing else would release
//!    them. This runs only when step 1 proved every closed connection's
//!    backend gone, so no admission `COMMIT` can still land after the
//!    delete; otherwise every orphan waits for a later reopen. An orphan is
//!    forgotten once its delete was answered (a definite refusal leaves the
//!    row, as before reopening existed); one whose delete may not have run
//!    (a lost reply, its prepare's included) stays for the next reopen. The
//!    delete names one op and is a no-op once the row is gone, so sending
//!    it at a later reopen never touches another op's row.
//! 5. The mount statements prepared at open are prepared again.
//!
//! Every change to `held_ops` and `orphans` happens inside a writer turn,
//! with the statement that changes what the connection holds, and a reopen
//! reads them inside its turn: a reopen never sees an op half registered
//! or half released (`history/admission`, `history/release.rs`). A turn
//! pinned to the connection an intent's keys were taken on
//! ([`Session::ready_pinned`]) never reopens: once that connection closed,
//! the keys are gone and the turn is refused `Unknown`, as before.
//!
//! One attempt per public call ([`one_call`]): each session reopens at most
//! once in it, and a reopen that fails answers its error, the writer stays
//! quarantined, and a later call may try once more. Outside a call scope the
//! budget is one per writer turn. Calls queued for the writer's turn see the
//! new connection in turn. A call inside a held lane never reopens: its lane
//! relies on the closed connection.
use super::connection::{LostBackend, Session, SessionKey, State};
use super::driver::Driver;
use crate::backend::postgresql::history::{mount, statements};
use crate::backend::records::BackendError;
use postgres::SimpleQueryMessage;
use std::{
    cell::{Cell, RefCell},
    sync::atomic::Ordering,
};

thread_local! {
    /// The sessions that already attempted a reopen in this thread's
    /// current call ([`one_call`]); `None` outside a call.
    static CALL: RefCell<Option<Vec<usize>>> = const { RefCell::new(None) };
}

/// Runs `call` as one public call (a Commit, a mount, an unmount, a Store
/// call): every session it uses attempts at most one reopen in it, so a
/// reopen that fails, or a connection that closes again later in the call,
/// is refused (`Unknown`) rather than opened again. A call inside another
/// shares the outer one's budget.
pub fn one_call<T>(call: impl FnOnce() -> T) -> T {
    struct Scope(bool);
    impl Drop for Scope {
        fn drop(&mut self) {
            if self.0 {
                CALL.with(|tried| *tried.borrow_mut() = None);
            }
        }
    }
    let outer = CALL.with(|tried| {
        let mut tried = tried.borrow_mut();
        let fresh = tried.is_none();
        if fresh {
            *tried = Some(Vec::new());
        }
        fresh
    });
    let _scope = Scope(outer);
    call()
}

/// Whether `session` may attempt a reopen in this thread's call, noting it.
fn first_attempt(session: &Session) -> bool {
    let id = session as *const Session as usize;
    CALL.with(|tried| match tried.borrow_mut().as_mut() {
        Some(tried) if tried.contains(&id) => false,
        Some(tried) => {
            tried.push(id);
            true
        }
        None => true,
    })
}

/// The token the orphan release records in `layerfs.released`; no unlock
/// follows it.
const ORPHAN_TOKEN: &str = "reopen";

/// The pid and start that end `lost`, or `Unknown` with nothing to send: a
/// backend never read, one whose start the role could not see (the pid
/// alone may since name another client's backend), or one a session lock
/// showed on another backend (a transaction-pooling proxy). `noted` is the
/// session's backend for an entry not yet stamped.
pub(crate) fn endable(lost: &LostBackend, noted: i64) -> Result<(i32, i64), BackendError> {
    let (pid, start) = lost.backend;
    let start = start.ok_or(BackendError::Unknown)?;
    let pid = (i32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .ok_or(BackendError::Unknown)?;
    let noted = lost.noted.unwrap_or(noted);
    if noted != 0 && noted != i64::from(pid) {
        return Err(BackendError::Unknown);
    }
    Ok((pid, start))
}

/// One statement that terminates each of `targets` still listed with that
/// start, each `pg_terminate_backend` waiting up to `bound` ms until the
/// process has exited: per target `t` (gone), `f` (not gone in time) or
/// NULL (not listed: already gone). Only integers are written into it.
pub(crate) fn end_sql(targets: &[(i32, i64)], bound: i64) -> String {
    let values: Vec<String> = (targets.iter())
        .map(|(pid, start)| format!("({pid}::int,{start}::bigint)"))
        .collect();
    format!(
        "SELECT (SELECT pg_terminate_backend(a.pid, {bound}) FROM pg_catalog.pg_stat_activity a \
          WHERE a.pid = v.pid AND (extract(epoch FROM a.backend_start) * 1000000)::bigint = v.start) \
         FROM (VALUES {}) v(pid, start)",
        values.join(",")
    )
}

/// Per target of [`end_sql`], whether its backend is proven gone; `None`
/// when the reply does not have one row per target.
pub(crate) fn ended(messages: &[SimpleQueryMessage], targets: usize) -> Option<Vec<bool>> {
    let gone: Vec<bool> = (messages.iter())
        .filter_map(|message| match message {
            SimpleQueryMessage::Row(row) => Some(row.get(0).is_none_or(|seen| seen == "t")),
            _ => None,
        })
        .collect();
    (gone.len() == targets).then_some(gone)
}

impl Session {
    /// Makes this session reopen after a quarantine (module docs).
    pub(crate) fn enable_reopen(&self) {
        self.reopens.store(true, Ordering::SeqCst);
    }

    /// The bound a lost backend's termination waits: `lock_timeout`, 30 s
    /// when that is 0.
    pub(crate) fn end_bound(&self) -> i64 {
        match self.settings.lock_timeout_ms {
            0 => 30_000,
            ms => i64::from(ms),
        }
    }

    /// Whether `state` may serve a call that holds this session's turn
    /// (`nested`: inside this thread's held lane). A quarantined session
    /// is refused `Unknown`, unless it reopens: then a top-level call opens
    /// one new connection first ([`Self::reopen`]), once per call.
    pub(crate) fn ready(&self, state: &mut State, nested: bool) -> Result<(), BackendError> {
        self.ready_pinned(state, nested, None)
    }

    /// [`Self::ready`] for a turn that relies on keys taken on the
    /// connection of epoch `pin` (`State::closed` when they were taken):
    /// it runs only on that connection and never reopens, since a new one
    /// holds none of them. `Unknown` once that connection has closed.
    pub(crate) fn ready_pinned(
        &self,
        state: &mut State,
        nested: bool,
        pin: Option<u64>,
    ) -> Result<(), BackendError> {
        let reopens = self.reopens.load(Ordering::Relaxed);
        if reopens && !state.quarantined && !state.client.alive() {
            // The server closed it before this call sent anything.
            state.quarantine();
        }
        if let Some(epoch) = pin {
            return match state.quarantined || state.closed != epoch {
                true => Err(BackendError::Unknown),
                false => Ok(()),
            };
        }
        if !state.quarantined {
            return Ok(());
        }
        if !reopens || nested || !first_attempt(self) {
            return Err(BackendError::Unknown);
        }
        self.reopen(state)
    }

    /// One new connection for this quarantined session, settled before it
    /// serves (module docs). On failure the new connection is closed again
    /// and the session stays quarantined.
    fn reopen(&self, state: &mut State) -> Result<(), BackendError> {
        let noted = self.backend_pid.load(Ordering::SeqCst);
        for lost in state.lost.iter_mut().filter(|lost| lost.noted.is_none()) {
            lost.noted = Some(noted);
        }
        let client = self.connector.connect_ordinary()?;
        state.work.connects += 1;
        state.client = Driver::Ordinary(client);
        self.backend_pid.store(0, Ordering::SeqCst);
        match self.settle(state) {
            Ok(()) => {
                state.quarantined = false;
                state.work.reopens += 1;
                state.work.last_reopen_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |since| since.as_millis() as u64);
                Ok(())
            }
            Err(error) => {
                state.quarantine();
                Err(error)
            }
        }
    }

    fn settle(&self, state: &mut State) -> Result<(), BackendError> {
        self.end_lost(state)?;
        self.read_backend(state)?;
        self.retake_keys(state)?;
        if state.lost.is_empty() {
            self.release_orphans(state)?;
        }
        self.prepare_again(state)
    }

    /// Step 1: ends every recorded backend that can be ended, on the new
    /// connection; those proven gone are forgotten. Only a lost reply fails.
    fn end_lost(&self, state: &mut State) -> Result<(), BackendError> {
        let targets: Vec<(LostBackend, (i32, i64))> = (state.lost.iter())
            .filter_map(|lost| endable(lost, 0).ok().map(|target| (*lost, target)))
            .collect();
        if targets.is_empty() {
            return Ok(());
        }
        let ends: Vec<(i32, i64)> = targets.iter().map(|(_, target)| *target).collect();
        state.work.round_trips += 1;
        state.work.statements += 1;
        let messages = match state.client.simple_query(&end_sql(&ends, self.end_bound())) {
            Ok(messages) => messages,
            Err(error) => {
                return match error.backend() {
                    BackendError::Unknown => Err(BackendError::Unknown),
                    // Refused: nothing ended, every record stays.
                    _ => Ok(()),
                };
            }
        };
        if let Some(gone) = ended(&messages, targets.len()) {
            for ((lost, _), gone) in targets.iter().zip(gone) {
                if gone {
                    state.lost.retain(|kept| kept != lost);
                }
            }
        }
        Ok(())
    }

    /// Step 2: the new backend and the Store's generation, in one statement.
    fn read_backend(&self, state: &mut State) -> Result<(), BackendError> {
        let sql = format!(
            "SELECT h.catalog_incarnation, pg_backend_pid(), \
             (SELECT (extract(epoch FROM a.backend_start) * 1000000)::bigint \
                FROM pg_catalog.pg_stat_activity a WHERE a.pid = pg_backend_pid()) \
             FROM {}.history_meta h WHERE h.id = 1",
            self.settings.schema
        );
        state.work.round_trips += 1;
        state.work.statements += 1;
        let messages = state
            .client
            .simple_query(&sql)
            .map_err(|error| error.backend())?;
        state.work.read_snapshots += 1;
        let row = (messages.iter())
            .find_map(|message| match message {
                SimpleQueryMessage::Row(row) => Some(row),
                _ => None,
            })
            .ok_or(BackendError::Integrity)?;
        let int = |index| -> Option<i64> { row.get(index)?.parse().ok() };
        let generation = int(0).ok_or(BackendError::Integrity)?;
        let pid = int(1).ok_or(BackendError::Integrity)?;
        let expected = self.incarnation.load(Ordering::SeqCst);
        if expected != 0 && u64::try_from(generation).ok() != Some(expected) {
            return Err(BackendError::Stale);
        }
        state.backend = Some((pid, int(2)));
        Ok(())
    }

    /// Step 3: every held intent's keys again, all or none.
    fn retake_keys(&self, state: &mut State) -> Result<(), BackendError> {
        let keys: Vec<SessionKey> = (self.held_ops.lock())
            .map_err(|_| BackendError::Unknown)?
            .values()
            .flatten()
            .copied()
            .collect();
        if keys.is_empty() {
            return Ok(());
        }
        let (ids, modes): (Vec<i64>, Vec<bool>) = keys.iter().copied().unzip();
        let expected = i64::try_from(self.incarnation.load(Ordering::SeqCst))
            .map_err(|_| BackendError::Integrity)?;
        let schema = self.settings.schema.as_str();
        let sql = statements::bare("gc_lock_session")
            .ok_or(BackendError::Integrity)?
            .replace("{s}", schema);
        let rows = super::transaction::immutable(
            state,
            schema,
            Some(Cell::new(false)),
            None,
            |tx| -> Result<Vec<postgres::Row>, BackendError> {
                tx.typed(&sql, &[&ids, &modes, &expected], 9 * ids.len() as u64 + 8)
            },
        )?;
        if rows.len() != keys.len() {
            return Err(BackendError::Unknown);
        }
        for row in &rows {
            let read = || -> Result<(Option<bool>, i64, i64), postgres::Error> {
                Ok((row.try_get(1)?, row.try_get(2)?, row.try_get(3)?))
            };
            let (granted, pid, generation) = read().map_err(|_| BackendError::Unknown)?;
            if expected != 0 && generation != expected {
                return Err(BackendError::Stale);
            }
            if !self.note_backend(pid) {
                return Err(BackendError::Integrity);
            }
            if granted != Some(true) {
                // Closing the new connection gives back what it was granted.
                return Err(BackendError::Busy);
            }
        }
        Ok(())
    }

    /// Step 4: each orphaned intent's row, deleted by op id. Only an
    /// orphan whose delete was answered is forgotten.
    fn release_orphans(&self, state: &mut State) -> Result<(), BackendError> {
        let orphans = std::mem::take(&mut *self.orphans.lock().map_err(|_| BackendError::Unknown)?);
        let expected = i64::try_from(self.incarnation.load(Ordering::SeqCst))
            .map_err(|_| BackendError::Integrity)?;
        let schema = self.settings.schema.as_str();
        let sql = statements::bare("gc_intent_release_owner")
            .ok_or(BackendError::Integrity)?
            .replace("{s}", schema);
        for (index, op) in orphans.iter().enumerate() {
            let op_id = op.as_slice();
            let released = super::transaction::immutable(
                state,
                schema,
                Some(Cell::new(false)),
                None,
                |tx| -> Result<Vec<postgres::Row>, BackendError> {
                    tx.typed(&sql, &[&op_id, &expected, &ORPHAN_TOKEN], 30)
                },
            );
            if let Err(BackendError::Unknown) = released {
                // This delete may not have run (its prepare's reply lost,
                // among others): it stays, with those not yet attempted.
                if let Ok(mut kept) = self.orphans.lock() {
                    kept.extend_from_slice(&orphans[index..]);
                }
                return Err(BackendError::Unknown);
            }
        }
        Ok(())
    }

    /// Step 5: the mount statements the open prepared, prepared again.
    fn prepare_again(&self, state: &mut State) -> Result<(), BackendError> {
        if !self.mount_writes.load(Ordering::SeqCst) {
            return Ok(());
        }
        let texts = mount::writer_texts(&self.settings.schema)?;
        let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
        let begin = self.settings.read_begin();
        match state.statements_in(&texts, Some(&begin)) {
            Err(error) if error.backend() == BackendError::Unknown => Err(BackendError::Unknown),
            // A refused prepare is prepared again at its first use.
            _ => Ok(()),
        }
    }

    /// Records an intent no caller will release (step 4), inside the
    /// writer turn that found it so: a mount whose admission ended with an
    /// unknown outcome, or an op whose release could not be sent.
    pub(crate) fn note_orphan(&self, op: [u8; 16]) {
        if self.reopens.load(Ordering::Relaxed) {
            if let Ok(mut orphans) = self.orphans.lock() {
                if !orphans.contains(&op) {
                    orphans.push(op);
                }
            }
        }
    }
}
