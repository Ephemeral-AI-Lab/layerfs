//! Phase timing for SDK-traced observations.
//!
//! A traced request turns recording on for its own thread through a
//! [`Recording`] guard, which turns it off again on every exit, unwinding
//! included, so no later untraced request on that thread records. Product
//! roots call [`timed`]: untraced it is `Timing::disabled`, with no clocks or
//! nodes; traced it records and keeps the finished report for the reply.
//! Recording never changes an operation's result. A caller of a Store call
//! (the SDK in-process, or the daemon for a Workspace call) starts a
//! [`Recording`] around one operation to get its Commit and read phase reports.
use crate::store::SaveCounters;
use layerfs_bridge::control::SdkValue;
use layerfs_persistence::SqlWork;
use layerfs_storage::encoding::delta::select::DepthHistogram;
use layerfs_telemetry::timer::{
    waits::{self, WAITS, WAIT_NAMES},
    Active, NodeOutcome, Timing, TimingNode, TimingReport, TimingScope,
};
use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    time::Duration,
};

/// Most finished root reports one traced request keeps.
const REPORTS: usize = 16;
/// Most timing nodes one reply carries; the rest are counted as omitted.
const NODES: usize = 256;

/// Shortest wait reported as a `wait.*` node.
const WAIT_NODE: Duration = Duration::from_micros(50);

thread_local! {
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    /// Wait already reported by a node on this thread, by kind.
    static ATTRIBUTED: Cell<[u64; WAITS]> = const { Cell::new([0; WAITS]) };
    static FINISHED: RefCell<Vec<TimingReport>> = const { RefCell::new(Vec::new()) };
}

/// Recording for one traced request on this thread, one at a time; dropping
/// it ends recording and discards what was not finished.
pub struct Recording {
    thread: PhantomData<*const ()>,
}
impl Recording {
    /// Starts recording this thread's timing roots.
    pub fn start() -> Self {
        FINISHED.with_borrow_mut(Vec::clear);
        RECORDING.set(true);
        Self {
            thread: PhantomData,
        }
    }
    /// The root reports finished while recording.
    pub fn finish(self) -> Vec<TimingReport> {
        FINISHED.with_borrow_mut(std::mem::take)
    }
}
impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.set(false);
        FINISHED.with_borrow_mut(Vec::clear);
    }
}

/// This thread's wait totals when a timed span started.
struct WaitMark {
    totals: [u64; WAITS],
    attributed: [u64; WAITS],
}
impl WaitMark {
    fn now() -> Self {
        Self {
            totals: waits::totals(),
            attributed: ATTRIBUTED.get(),
        }
    }
    /// Reports below `scope` each kind of wait this span spent that no inner
    /// span reported, as one `wait.*` node.
    fn attach(self, scope: &TimingScope<'_, Active>) {
        let totals = waits::totals();
        let mut attributed = ATTRIBUTED.get();
        for kind in 0..WAITS {
            let inner = attributed[kind].wrapping_sub(self.attributed[kind]);
            let own = totals[kind]
                .wrapping_sub(self.totals[kind])
                .saturating_sub(inner);
            let own = Duration::from_nanos(own);
            if own >= WAIT_NODE {
                scope.attach(Some(TimingReport::from_root(TimingNode::new(
                    WAIT_NAMES[kind],
                    own,
                ))));
                attributed[kind] = attributed[kind].wrapping_add(own.as_nanos() as u64);
            }
        }
        ATTRIBUTED.set(attributed);
    }
}

/// True while this thread serves a traced request.
pub fn recording() -> bool {
    RECORDING.get()
}

/// One timing root: recorded only while this thread is traced.
pub fn timed<T, E>(
    name: &'static str,
    operation: impl FnOnce(&TimingScope<'_, Active>) -> Result<T, E>,
) -> Result<T, E> {
    if !recording() {
        return Timing::disabled(name, operation).0;
    }
    let (result, report) = Timing::record(name, |scope| {
        let mark = WaitMark::now();
        let result = operation(scope);
        mark.attach(scope);
        result
    });
    FINISHED.with_borrow_mut(|finished| {
        if finished.len() < REPORTS {
            finished.push(report);
        }
    });
    result
}

/// One phase below `scope`. Roots finished inside it are attached beneath it.
pub fn phase<T, E>(
    scope: &TimingScope<'_, Active>,
    name: &'static str,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    scope.child(name).run(|phase| {
        let mark = phase
            .is_recording()
            .then(|| (FINISHED.with_borrow(Vec::len), WaitMark::now()));
        let result = operation();
        if let Some((mark, waited)) = mark {
            let inner =
                FINISHED.with_borrow_mut(|finished| finished.split_off(mark.min(finished.len())));
            inner
                .into_iter()
                .for_each(|report| phase.attach(Some(report)));
            waited.attach(phase);
        }
        result
    })
}

/// Store SQL sessions' totals: the writer's, all readers' summed, and the
/// calling thread's on any session.
pub type SqlTotals = (Option<SqlWork>, Option<SqlWork>, SqlWork);

/// The call's SQL work: each session group's totals after the call minus
/// before it, which include concurrent calls on the same sessions, and
/// `call`, the work of the calling thread alone (#367). A session group
/// whose counters were unavailable at either end is left out.
pub fn sql(before: SqlTotals, after: SqlTotals) -> Option<SdkValue> {
    let difference = |before: Option<SqlWork>, after: Option<SqlWork>| {
        let (before, after) = (sql_values(&before?), sql_values(&after?));
        let delta: Vec<u64> = after
            .iter()
            .zip(before)
            .map(|(after, before)| after.saturating_sub(before))
            .collect();
        Some(named(&SQL_FIELDS, &delta))
    };
    let fields: Vec<_> = [
        ("writer", difference(before.0, after.0)),
        ("readers", difference(before.1, after.1)),
        ("call", difference(Some(before.2), Some(after.2))),
    ]
    .into_iter()
    .filter_map(|(name, value)| Some((name.to_owned(), value?)))
    .collect();
    Some(SdkValue::Map(fields))
}

/// One call's delta selection work: the Store's Save totals after it minus
/// before it, or None when the call finished no Save. Depth lists end at
/// their last nonzero slot; the last of 32 slots counts depth 31 and above.
pub fn delta(before: SaveCounters, after: SaveCounters) -> Option<SdkValue> {
    if before == after {
        return None;
    }
    let less = |after: u64, before: u64| SdkValue::Int(after.saturating_sub(before) as i64);
    let depths = |after: &DepthHistogram, before: &DepthHistogram| {
        let mut slots: Vec<u64> = after
            .iter()
            .zip(before)
            .map(|(after, before)| after.saturating_sub(*before))
            .collect();
        while slots.last() == Some(&0) {
            slots.pop();
        }
        SdkValue::List(slots.into_iter().map(|n| SdkValue::Int(n as i64)).collect())
    };
    let (a, b) = (&after.delta, &before.delta);
    let payload = SdkValue::map([
        ("prepared_full", less(a.prepared_full, b.prepared_full)),
        ("trials", less(a.trials, b.trials)),
        (
            "prefix_selected",
            less(a.prefix_selected, b.prefix_selected),
        ),
        ("full_losses", less(a.full_losses, b.full_losses)),
        ("no_candidate", less(a.no_candidate, b.no_candidate)),
        (
            "absent_candidates",
            less(a.absent_candidates, b.absent_candidates),
        ),
        (
            "ineligible_candidates",
            less(a.ineligible_candidates, b.ineligible_candidates),
        ),
        ("depth_capped", less(a.depth_capped, b.depth_capped)),
        ("work_exceeded", less(a.work_exceeded, b.work_exceeded)),
        (
            "whole_file_depths",
            depths(&a.whole_file_depths, &b.whole_file_depths),
        ),
        ("chunk_depths", depths(&a.chunk_depths, &b.chunk_depths)),
    ]);
    let (a, b) = (&after.pool, &before.pool);
    let metadata = SdkValue::map([
        ("leaves", less(a.leaves, b.leaves)),
        ("delta_leaves", less(a.delta_leaves, b.delta_leaves)),
        ("full_leaves", less(a.full_leaves, b.full_leaves)),
        ("trials", less(a.trials, b.trials)),
        ("depth_capped", less(a.depth_capped, b.depth_capped)),
        ("work_exceeded", less(a.work_exceeded, b.work_exceeded)),
        ("depths", depths(&a.depths, &b.depths)),
    ]);
    let mut fields = vec![
        ("payload".to_owned(), payload),
        ("metadata".to_owned(), metadata),
    ];
    // Merge path naming (#352): merges, walk fallbacks, inodes named, the
    // distinct inodes climbed, recorded-binding reads and object read demands.
    let (a, b) = (&after.merge, &before.merge);
    if a != b {
        fields.push((
            "merge".to_owned(),
            SdkValue::map([
                ("merges", less(a.merges, b.merges)),
                ("fallbacks", less(a.fallbacks, b.fallbacks)),
                ("inodes", less(a.naming.inodes, b.naming.inodes)),
                ("climbed", less(a.naming.climbed, b.naming.climbed)),
                ("bindings", less(a.naming.bindings, b.naming.bindings)),
                ("reads", less(a.naming.reads, b.naming.reads)),
            ]),
        ));
    }
    Some(SdkValue::Map(fields))
}

/// The scalar `SqlWork` counters, in `SQL_FIELDS` order (observation
/// sections 13 and 14 begin with them).
pub fn sql_values(w: &SqlWork) -> [u64; 27] {
    [
        w.statements,
        w.vm_steps,
        w.fullscan_steps,
        w.sorts,
        w.autoindex_rows,
        w.reprepares,
        w.returned_rows,
        w.bound_bytes,
        w.statement_ns,
        w.commit_ns,
        w.transactions,
        w.write_transactions,
        w.write_commits,
        w.commits,
        w.read_snapshots,
        w.rollbacks,
        w.transaction_ns,
        w.read_snapshot_ns,
        w.sealed_inserts,
        w.sealed_body_bytes,
        w.blob_open_calls,
        w.blob_reopen_calls,
        w.blob_read_calls,
        w.blob_requested_bytes,
        w.blob_read_bytes,
        w.blob_read_ns,
        w.blob_close_calls,
    ]
}

pub fn named(names: &[&str], values: &[u64]) -> SdkValue {
    SdkValue::Map(
        names
            .iter()
            .zip(values)
            .map(|(name, value)| ((*name).to_owned(), SdkValue::Int(*value as i64)))
            .collect(),
    )
}

/// Pre-order nodes `[name, parent, microseconds, outcome, incomplete]`;
/// parent is the index of an earlier node, -1 for a root. Over the bound the
/// shallowest levels are kept whole and the next level is filled in
/// pre-order, so top-level phases are never dropped for deep detail; the
/// rest is counted as omitted.
pub fn timing(reports: &[TimingReport]) -> SdkValue {
    let roots: Vec<&TimingNode> = reports.iter().filter_map(TimingReport::root).collect();
    let mut levels = Vec::new();
    roots.iter().for_each(|root| count(root, 0, &mut levels));
    let (mut whole, mut kept) = (0, 0);
    while whole < levels.len() && kept + levels[whole] <= NODES {
        kept += levels[whole];
        whole += 1;
    }
    let mut budget = NODES - kept;
    let mut nodes = Vec::new();
    for root in roots {
        flatten(root, -1, 0, whole, &mut budget, &mut nodes);
    }
    let omitted = levels.iter().sum::<usize>() - nodes.len();
    SdkValue::map([
        ("nodes", SdkValue::List(nodes)),
        ("omitted", SdkValue::Int(omitted as i64)),
    ])
}

fn count(node: &TimingNode, depth: usize, levels: &mut Vec<usize>) {
    if levels.len() == depth {
        levels.push(0);
    }
    levels[depth] += 1;
    node.children()
        .iter()
        .for_each(|child| count(child, depth + 1, levels));
}

/// Emits `node` and its kept descendants: every level above `whole`, and
/// `budget` nodes of level `whole`.
fn flatten(
    node: &TimingNode,
    parent: i64,
    depth: usize,
    whole: usize,
    budget: &mut usize,
    nodes: &mut Vec<SdkValue>,
) {
    if depth > whole || (depth == whole && *budget == 0) {
        return;
    }
    if depth == whole {
        *budget -= 1;
    }
    let index = nodes.len() as i64;
    let outcome = match node.outcome() {
        NodeOutcome::Ok => "ok",
        NodeOutcome::Error => "error",
        NodeOutcome::Unknown => "unknown",
    };
    nodes.push(SdkValue::List(vec![
        SdkValue::text(node.name()),
        SdkValue::Int(parent),
        SdkValue::Int(i64::try_from(node.elapsed().as_micros()).unwrap_or(i64::MAX)),
        SdkValue::text(outcome),
        SdkValue::Bool(node.is_incomplete()),
    ]));
    for child in node.children() {
        flatten(child, index, depth + 1, whole, budget, nodes);
    }
}

/// The scalar Store SQL work counters, `SqlWork` names.
pub const SQL_FIELDS: [&str; 27] = [
    "statements",
    "vm_steps",
    "fullscan_steps",
    "sorts",
    "autoindex_rows",
    "reprepares",
    "returned_rows",
    "bound_bytes",
    "statement_ns",
    "commit_ns",
    "transactions",
    "write_transactions",
    "write_commits",
    "commits",
    "read_snapshots",
    "rollbacks",
    "transaction_ns",
    "read_snapshot_ns",
    "sealed_inserts",
    "sealed_body_bytes",
    "blob_open_calls",
    "blob_reopen_calls",
    "blob_read_calls",
    "blob_requested_bytes",
    "blob_read_bytes",
    "blob_read_ns",
    "blob_close_calls",
];
