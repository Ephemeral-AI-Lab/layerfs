//! Reply tickets of published jobs, held in the engine's memory. A ticket is
//! issued by the job that publishes and returned by the one reply attempt of
//! its request; a capture and a terminal cleanup wait for a namespace's
//! tickets. The database is created by this process and never reopened, so a
//! stored row would outlive nothing this record does not. The reply attempt
//! itself needs no owner turn: it is recorded here from the replying thread.
use crate::{Generation, OverlayError, OverlayResult, Publication, Route};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Bound,
    sync::{Mutex, MutexGuard},
};

/// What one recorded reply attempt left behind in its namespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Settled {
    /// Other published replies of the namespace are still to be attempted.
    Pending,
    /// The namespace's last ticket, and no owner job waited for it.
    Last,
    /// The namespace's last ticket, and an owner job observed it pending:
    /// one owner turn is owed (`Overlay::reply_settled`).
    Watched,
}
#[derive(Default)]
struct Tickets {
    /// (namespace, revision) to the incarnation and generation published
    /// into. Bounded by the publications whose replies are in flight.
    pending: BTreeMap<(i64, i64), ([u8; 32], i64)>,
    /// Namespaces whose pending tickets an owner job waits for.
    watched: BTreeSet<i64>,
}
/// Shared with the threads that attempt replies; every method is a short
/// critical section and none of them waits for the connection.
pub struct ReplyTickets {
    /// The engine whose publications these are; another engine's is stale.
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
        self.state().pending.insert(
            (publication.route.ns, publication.revision),
            (publication.route.incarnation, publication.generation.0),
        );
    }
    /// Takes back a ticket whose publishing transaction did not commit.
    pub(crate) fn withdraw(&self, ns: i64, revision: i64) {
        let mut state = self.state();
        state.pending.remove(&(ns, revision));
        if !pending(&state, ns) {
            state.watched.remove(&ns);
        }
    }
    pub(crate) fn pending(&self, ns: i64) -> bool {
        pending(&self.state(), ns)
    }
    /// Pending tickets of a namespace an owner job now waits for.
    pub(crate) fn watch(&self, ns: i64) -> bool {
        let mut state = self.state();
        let pending = pending(&state, ns);
        if pending {
            state.watched.insert(ns);
        }
        pending
    }
    pub(crate) fn count(&self, ns: Option<i64>) -> u64 {
        let state = self.state();
        match ns {
            Some(ns) => range(&state, ns, 0).count() as u64,
            None => state.pending.len() as u64,
        }
    }
    pub(crate) fn page(&self, route: Route, after: i64, limit: usize) -> Vec<Publication> {
        range(&self.state(), route.ns, after)
            .take(limit)
            .filter(|(_, (incarnation, _))| *incarnation == route.incarnation)
            .map(|((_, revision), (_, generation))| Publication {
                route,
                revision: *revision,
                generation: Generation(*generation),
            })
            .collect()
    }
    /// Records a send attempt, including a lost reply, from any thread. It
    /// never removes published state and never claims kernel delivery. A
    /// ticket that is not held is `Stale` and changes nothing.
    pub fn attempted(&self, publication: Publication) -> OverlayResult<Settled> {
        let key = (publication.route.ns, publication.revision);
        let held = (publication.route.incarnation, publication.generation.0);
        let mut state = self.state();
        if publication.route.engine != self.engine || state.pending.get(&key) != Some(&held) {
            return Err(OverlayError::Stale);
        }
        state.pending.remove(&key);
        Ok(if pending(&state, key.0) {
            Settled::Pending
        } else if state.watched.remove(&key.0) {
            Settled::Watched
        } else {
            Settled::Last
        })
    }
}
fn range(
    state: &Tickets,
    ns: i64,
    after: i64,
) -> impl Iterator<Item = (&(i64, i64), &([u8; 32], i64))> + '_ {
    state.pending.range((
        Bound::Excluded((ns, after)),
        Bound::Included((ns, i64::MAX)),
    ))
}
fn pending(state: &Tickets, ns: i64) -> bool {
    range(state, ns, 0).next().is_some()
}
