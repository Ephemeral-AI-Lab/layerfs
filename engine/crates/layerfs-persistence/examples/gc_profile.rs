//! Where one GC mark, or one whole compaction, spends its time on a kept
//! `gc_bench` Store: wall time, this thread's SQL work counters, and the
//! statements that cost the most.
//!
//! ```text
//! cargo run -p layerfs-persistence --release --example gc_bench -- --keep --tmpdir DIR
//! cargo run -p layerfs-persistence --release --example gc_profile -- DIR/<run>/store.sqlite
//! ```
//!
//! `--marks N` runs N marks (default 2: the second shows a warm cache).
//! `--compact` then runs one compaction; it changes the Store. `--top N`
//! prints the N costliest statements (default 12). The rebuild's copy runs on
//! its own connection, so its statements are not in the profile; its time is
//! the compaction's wall time less the profiled statements.
#![allow(dead_code)]
#[path = "gc_bench/gc_fixture.rs"]
mod gc_fixture;

use layerfs_persistence::{
    profile_statements, take_statement_profile, thread_work, CompactPath, CompactRequest,
};
use std::time::Instant;

fn counters(label: &str, wall_ms: f64, work: &layerfs_persistence::SqlWork) {
    println!(
        "{label}: wall_ms={wall_ms:.1} statements={} statement_ms={:.1} vm_steps={} \
         fullscan_steps={} sorts={} autoindex_rows={} returned_rows={} blob_opens={} \
         blob_reads={} blob_read_bytes={} blob_read_ms={:.1} transactions={} commit_ms={:.1}",
        work.statements,
        work.statement_ns as f64 / 1e6,
        work.vm_steps,
        work.fullscan_steps,
        work.sorts,
        work.autoindex_rows,
        work.returned_rows,
        work.blob_open_calls,
        work.blob_read_calls,
        work.blob_read_bytes,
        work.blob_read_ns as f64 / 1e6,
        work.transactions,
        work.commit_ns as f64 / 1e6
    );
}

fn statements(top: usize) {
    let profile = take_statement_profile();
    let total: u64 = profile.iter().map(|(_, cost)| cost.wall_ns).sum();
    println!(
        "  {} distinct statements, {:.1} ms; costliest first (ms, calls, rows, text):",
        profile.len(),
        total as f64 / 1e6
    );
    for (sql, cost) in profile.iter().take(top) {
        println!(
            "  {:>9.2} {:>8} {:>9}  {sql}",
            cost.wall_ns as f64 / 1e6,
            cost.calls,
            cost.rows
        );
    }
}

fn main() {
    let mut store = None;
    let (mut marks, mut top, mut compact) = (2usize, 12usize, false);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut number = |name: &str| -> usize {
            args.next()
                .and_then(|value| value.parse().ok())
                .unwrap_or_else(|| panic!("{name} needs a number"))
        };
        match arg.as_str() {
            "--marks" => marks = number("--marks"),
            "--top" => top = number("--top"),
            "--compact" => compact = true,
            _ => store = Some(std::path::PathBuf::from(arg)),
        }
    }
    let store = store.expect("usage: gc_profile STORE [--marks N] [--top N] [--compact]");
    let handles = gc_fixture::reopen(&store);
    for index in 1..=marks {
        let before = thread_work();
        profile_statements();
        let started = Instant::now();
        let mark = handles.history.mark().expect("mark");
        let wall = started.elapsed().as_secs_f64() * 1e3;
        counters(
            &format!("mark {index}"),
            wall,
            &thread_work().since(&before),
        );
        println!(
            "  objects={} packs={} history_nodes={} mark_bytes={}",
            mark.objects().len(),
            mark.objects().packs().len(),
            mark.history().len(),
            mark.mark_bytes()
        );
        statements(top);
    }
    if compact {
        let before = thread_work();
        profile_statements();
        let started = Instant::now();
        let receipt = handles
            .compact(&CompactRequest {
                op_id: [0xC7; 16],
                ..CompactRequest::default()
            })
            .expect("compact");
        let wall = started.elapsed().as_secs_f64() * 1e3;
        counters("compact", wall, &thread_work().since(&before));
        println!(
            "  path={} physical_shrink={} peak_temp_bytes={}",
            match receipt.path {
                CompactPath::None => "none",
                CompactPath::Rebuild => "rebuild",
            },
            receipt.physical_shrink,
            receipt.peak_temp_bytes
        );
        statements(top);
    }
}
