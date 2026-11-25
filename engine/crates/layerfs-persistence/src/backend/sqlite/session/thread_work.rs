//! SQL work by the thread that ran it (#367). A session's counters sum every
//! thread that used it, so their delta over one call also holds the work of
//! calls running alongside it. Each turn on a session's connection, and each
//! connection opened, also adds its work to its own thread's totals: a call's
//! own work is its thread's totals after it minus before it.
use super::connection::{SqlWork, State};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

thread_local! {
    static THREAD: Cell<SqlWork> = Cell::new(SqlWork::default());
    /// This thread's per-statement profile; `None` unless one was started.
    static STATEMENTS: RefCell<Option<BTreeMap<String, StatementCost>>> =
        const { RefCell::new(None) };
}

/// What one statement text cost this thread while a profile was running.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StatementCost {
    /// Executions.
    pub calls: u64,
    /// Wall time preparing and executing it, cursor and decode included.
    pub wall_ns: u64,
    /// Rows it returned.
    pub rows: u64,
}

/// The statement text a profile groups by: the first characters, with
/// whitespace collapsed, so a list of bound parameters does not split a group.
const STATEMENT_KEY_CHARS: usize = 110;

/// Starts this thread's per-statement profile, discarding any earlier one.
///
/// A diagnostic for finding which statements an operation spends its time
/// in. Off, it costs one thread-local read per statement.
pub fn profile_statements() {
    STATEMENTS.with(|profile| *profile.borrow_mut() = Some(BTreeMap::new()));
}

/// Ends this thread's per-statement profile and returns it, costliest first.
pub fn take_statement_profile() -> Vec<(String, StatementCost)> {
    let mut rows: Vec<_> = STATEMENTS
        .with(|profile| profile.borrow_mut().take())
        .unwrap_or_default()
        .into_iter()
        .collect();
    rows.sort_by(|left, right| right.1.wall_ns.cmp(&left.1.wall_ns));
    rows
}

/// Adds one preparation or execution of `sql` to a running profile.
pub(crate) fn note_statement(sql: &str, calls: u64, wall_ns: u64, rows: u64) {
    STATEMENTS.with(|profile| {
        let mut profile = profile.borrow_mut();
        let Some(profile) = profile.as_mut() else {
            return;
        };
        let key: String = sql
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(STATEMENT_KEY_CHARS)
            .collect();
        let cost = profile.entry(key).or_default();
        cost.calls += calls;
        cost.wall_ns += wall_ns;
        cost.rows += rows;
    });
}

/// Whether this thread is profiling statements now.
pub(crate) fn profiling_statements() -> bool {
    STATEMENTS.with(|profile| profile.borrow().is_some())
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
            blob_close_calls,
            round_trips,
            prepares,
            connects
        )
    };
}

impl SqlWork {
    /// Adds `next`'s counters and phase observations to these.
    pub fn accumulate(&mut self, next: &SqlWork) {
        macro_rules! add { ($($f:ident),+) => { $(self.$f = self.$f.saturating_add(next.$f);)+ }; }
        scalars!(add);
        self.reopens = self.reopens.saturating_add(next.reopens);
        self.last_reopen_ms = self.last_reopen_ms.max(next.last_reopen_ms);
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
        out.reopens = self.reopens.saturating_sub(before.reopens);
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
