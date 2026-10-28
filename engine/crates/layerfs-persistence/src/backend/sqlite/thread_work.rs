//! SQL work by the thread that ran it (#367). A session's counters sum every
//! thread that used it, so their delta over one call also holds the work of
//! calls running alongside it. Each turn on a session's connection, and each
//! connection opened, also adds its work to its own thread's totals: a call's
//! own work is its thread's totals after it minus before it.
use super::connection::{SqlWork, State};
use std::cell::Cell;

thread_local! {
    static THREAD: Cell<SqlWork> = Cell::new(SqlWork::default());
}

/// The SQL work this thread has run on every session of this process.
pub fn thread_work() -> SqlWork {
    THREAD.get()
}

/// Runs `body` on a locked session state and adds the SQL work it did there
/// to this thread's totals when it returns.
pub(crate) fn attributed<R>(state: &mut State, body: impl FnOnce(&mut State) -> R) -> R {
    let before = *state.work.borrow();
    let result = body(state);
    add(&state.work.borrow().since(&before));
    result
}

/// Adds `done` to this thread's totals.
pub(crate) fn add(done: &SqlWork) {
    THREAD.with(|thread| {
        let mut total = thread.get();
        total.accumulate(done);
        thread.set(total);
    });
}

macro_rules! scalars {
    ($apply:ident) => {
        $apply!(
            statements,
            vm_steps,
            fullscan_steps,
            sorts,
            autoindex_rows,
            reprepares,
            returned_rows,
            bound_bytes,
            statement_ns,
            commit_ns,
            transactions,
            write_transactions,
            write_commits,
            commits,
            read_snapshots,
            rollbacks,
            transaction_ns,
            read_snapshot_ns,
            sealed_inserts,
            sealed_body_bytes,
            blob_open_calls,
            blob_reopen_calls,
            blob_read_calls,
            blob_requested_bytes,
            blob_read_bytes,
            blob_read_ns,
            blob_close_calls
        )
    };
}

impl SqlWork {
    /// Adds `next`'s counters and phase observations to these.
    pub fn accumulate(&mut self, next: &SqlWork) {
        macro_rules! add { ($($f:ident),+) => { $(self.$f = self.$f.saturating_add(next.$f);)+ }; }
        scalars!(add);
        for (to, from) in self
            .statement_phases
            .iter_mut()
            .zip(next.statement_phases)
            .chain(self.commit_phases.iter_mut().zip(next.commit_phases))
        {
            to.calls = to.calls.saturating_add(from.calls);
            to.wall_ns = to.wall_ns.saturating_add(from.wall_ns);
        }
    }

    /// What these counters gained since `before`.
    pub fn since(&self, before: &SqlWork) -> SqlWork {
        let mut out = *self;
        macro_rules! sub { ($($f:ident),+) => { $(out.$f = self.$f.saturating_sub(before.$f);)+ }; }
        scalars!(sub);
        for (to, from) in out
            .statement_phases
            .iter_mut()
            .zip(before.statement_phases)
            .chain(out.commit_phases.iter_mut().zip(before.commit_phases))
        {
            to.calls = to.calls.saturating_sub(from.calls);
            to.wall_ns = to.wall_ns.saturating_sub(from.wall_ns);
        }
        out
    }
}
