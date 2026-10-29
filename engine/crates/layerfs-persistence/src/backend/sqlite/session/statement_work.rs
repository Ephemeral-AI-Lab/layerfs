//! Fixed statement lifecycle observations, including failed attempts.
use super::connection::SqlWork;
use std::{cell::RefCell, time::Instant};
/// One sequential wrapper phase's calls and wall; nested SQL/transaction spans overlap.
#[derive(Clone, Copy, Debug, Default)]
pub struct StatementPhaseWork {
    /// Phase entries, including failed attempts.
    pub calls: u64,
    /// Phase wall in nanoseconds; includes the operation's library work.
    pub wall_ns: u64,
}
pub(crate) struct Phase<'a> {
    work: &'a RefCell<SqlWork>,
    index: usize,
    commit: bool,
    start: Instant,
}
pub(crate) fn phase(work: &RefCell<SqlWork>, index: usize, commit: bool) -> Phase<'_> {
    Phase {
        work,
        index,
        commit,
        start: Instant::now(),
    }
}
impl Drop for Phase<'_> {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed().as_nanos() as u64;
        let mut work = self.work.borrow_mut();
        work.statement_phases[self.index].calls += 1;
        work.statement_phases[self.index].wall_ns += elapsed;
        if self.commit {
            work.commit_phases[self.index].calls += 1;
            work.commit_phases[self.index].wall_ns += elapsed;
        }
    }
}
