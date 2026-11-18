//! A lifecycle call's progress (call surface 2.0): `{ phase, done, total }`
//! events on the caller's stream, where `done` and `total` count the
//! call's phases.
use layerfs_home::events::Events;

/// The phases of one call, in order, and how many are done.
pub struct Phases<'e, 'a> {
    events: &'e mut Events<'a>,
    names: &'static [&'static str],
}

impl<'e, 'a> Phases<'e, 'a> {
    /// A call that goes through `names`, reporting on `events`.
    pub fn new(events: &'e mut Events<'a>, names: &'static [&'static str]) -> Self {
        Phases { events, names }
    }

    fn send(&mut self, phase: &str, done: usize) {
        let count = |n: usize| i64::try_from(n).ok();
        self.events
            .phase(phase, count(done), count(self.names.len()), Vec::new());
    }

    /// Phase `phase` begins: every phase before it is done.
    pub fn begin(&mut self, phase: &str) {
        let done = self.names.iter().position(|name| *name == phase);
        self.send(phase, done.unwrap_or(0));
    }

    /// The last phase ended: all are done.
    pub fn end(&mut self) {
        let last = self.names.last().copied().unwrap_or_default();
        self.send(last, self.names.len());
    }
}
