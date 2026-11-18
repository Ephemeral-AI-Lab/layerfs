//! Where one GC mark, or one whole compaction, spends its time: wall time,
//! this thread's SQL work counters, and the statements that cost the most.
//!
//! ```text
//! cargo run -p layerfs-persistence --release --example gc_bench -- --keep --tmpdir DIR
//! cargo run -p layerfs-persistence --release --example gc_profile -- DIR/<run>/store.sqlite
//! ```
//!
//! `--marks N` runs N marks (default 2: the second shows a warm cache; 0 runs
//! none). `--compact` then runs one compaction and prints a line for each of
//! its phases as well; it changes the Store. `--top N` prints the N costliest
//! statements (default 12). The rebuild's copy and the visited scratch run on
//! their own connections, so their statements are not in the profile; their
//! time is the phase's wall time less its profiled statements.
//!
//! The Store is a kept `gc_bench` Store unless `GC_BINDING_HEX` and
//! `GC_CURSOR_HEX` name another Store's binding and its 32-byte cursor key, in
//! hex. A mark writes `kept_node` and a compaction can rebuild the file, so
//! profile a copy of a real Store, never the Store itself.
#![allow(dead_code)]
#[path = "gc_bench/gc_fixture.rs"]
mod gc_fixture;

use layerfs_persistence::{
    profile_statements, take_statement_profile, thread_work, CompactPath, CompactRequest,
    PersistenceConfig, SqlWork, SqliteHandles, SqlitePersistenceProfile,
};
use std::path::Path;
use std::time::Instant;

fn unhex(name: &str, text: &str) -> Vec<u8> {
    assert!(text.len() % 2 == 0, "{name} is not hex");
    (0..text.len() / 2)
        .map(|at| u8::from_str_radix(&text[2 * at..2 * at + 2], 16))
        .collect::<Result<_, _>>()
        .unwrap_or_else(|_| panic!("{name} is not hex"))
}

/// Opens a `gc_bench` Store, or the Store whose keys the environment names.
fn open(store: &Path) -> SqliteHandles {
    let (Ok(binding), Ok(cursor)) = (
        std::env::var("GC_BINDING_HEX"),
        std::env::var("GC_CURSOR_HEX"),
    ) else {
        return gc_fixture::reopen(store);
    };
    let cursor: [u8; 32] = unhex("GC_CURSOR_HEX", &cursor)
        .try_into()
        .expect("GC_CURSOR_HEX is 32 bytes");
    SqliteHandles::open_writable(
        PersistenceConfig::sqlite(store.to_path_buf())
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        &unhex("GC_BINDING_HEX", &binding),
        cursor,
    )
    .expect("Store open")
}

fn counters(label: &str, wall_ms: f64, work: &SqlWork) {
    println!(
        "{label}: wall_ms={wall_ms:.1} statements={} statement_ms={:.1} vm_steps={} \
         fullscan_steps={} sorts={} autoindex_rows={} returned_rows={} blob_opens={} \
         blob_reads={} blob_read_bytes={} blob_read_ms={:.1} transactions={} \
         write_transactions={} commits={} rollbacks={} commit_ms={:.1}",
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
        work.write_transactions,
        work.commits,
        work.rollbacks,
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
    let handles = open(&store);
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
        // Each phase as it starts: its name, the time and the work so far.
        let mut phases: Vec<(&'static str, f64, SqlWork)> = Vec::new();
        let receipt = handles
            .compact_observed(
                &CompactRequest {
                    op_id: [0xC7; 16],
                    ..CompactRequest::default()
                },
                &mut |phase| {
                    phases.push((phase, started.elapsed().as_secs_f64() * 1e3, thread_work()));
                },
            )
            .expect("compact");
        let wall = started.elapsed().as_secs_f64() * 1e3;
        let done = thread_work();
        counters("compact", wall, &done.since(&before));
        // What runs before the first phase is the admission: the leftover
        // check, the Store tuple and the op record.
        let mut current = ("admission", 0.0, before);
        for next in phases.into_iter().chain([("", wall, done)]) {
            counters(
                &format!("  phase {}", current.0),
                next.1 - current.1,
                &next.2.since(&current.2),
            );
            current = next;
        }
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
