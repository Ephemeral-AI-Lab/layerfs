//! Reply tickets of published jobs in one Workspace's engine. A publishing job
//! issues a ticket; its request's one reply attempt settles it without an owner
//! turn. Capture and terminal cleanup wait for this engine's pending tickets.
use crate::{Generation, OverlayError, OverlayResult, Publication};
use std::{
    collections::BTreeMap,
    ops::Bound,
    sync::{Mutex, MutexGuard},
};

/// What one recorded reply attempt left behind in its Workspace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Settled {
    /// Other published replies are still to be attempted.
    Pending,
    /// The last ticket, and no owner job waited for it.
    Last,
    /// The last ticket, and an owner job observed it pending.
    /// The serving owner wakes before-effect capture readiness.
    Watched,
}
#[derive(Default)]
struct Tickets {
    /// Revision to the generation published into, bounded by
    /// publications whose replies are in flight.
    pending: BTreeMap<i64, i64>,
    watched: bool,
}
/// Shared with replying threads; short critical sections never wait for SQL.
pub struct ReplyTickets {
    engine: u64,
    state: Mutex<Tickets>,
}
impl ReplyTickets {
    pub(crate) fn new(engine: u64) -> Self {
        Self {
            engine,
            state: Mutex::default(),
        }
    }
    fn state(&self) -> MutexGuard<'_, Tickets> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
    pub(crate) fn issue(&self, publication: Publication) {
        self.state()
            .pending
            .insert(publication.revision, publication.generation.0);
    }
    /// Takes back a ticket whose publishing transaction did not commit.
    pub(crate) fn withdraw(&self, revision: i64) {
        let mut state = self.state();
        state.pending.remove(&revision);
        if state.pending.is_empty() {
            state.watched = false;
        }
    }
    pub(crate) fn pending(&self) -> bool {
        !self.state().pending.is_empty()
    }
    /// Records that an owner job waits for the pending frontier.
    pub(crate) fn watch(&self) -> bool {
        let mut state = self.state();
        let pending = !state.pending.is_empty();
        state.watched |= pending;
        pending
    }
    pub(crate) fn count(&self) -> u64 {
        self.state().pending.len() as u64
    }
    pub(crate) fn page(&self, after: i64, limit: usize) -> Vec<Publication> {
        self.state()
            .pending
            .range((Bound::Excluded(after), Bound::Unbounded))
            .take(limit)
            .map(|(revision, generation)| Publication {
                engine: self.engine,
                revision: *revision,
                generation: Generation(*generation),
            })
            .collect()
    }
    /// Records a send attempt, including a lost reply, from any thread. Never
    /// removes published state or claims delivery; an unheld ticket is Stale.
    pub fn attempted(&self, publication: Publication) -> OverlayResult<Settled> {
        let held = publication.generation.0;
        let mut state = self.state();
        if publication.engine != self.engine
            || state.pending.get(&publication.revision) != Some(&held)
        {
            return Err(OverlayError::Stale);
        }
        state.pending.remove(&publication.revision);
        Ok(if !state.pending.is_empty() {
            Settled::Pending
        } else if std::mem::take(&mut state.watched) {
            Settled::Watched
        } else {
            Settled::Last
        })
    }
}
