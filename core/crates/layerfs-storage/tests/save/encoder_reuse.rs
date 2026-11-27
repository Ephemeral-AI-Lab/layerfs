//! F-043: Saves made in turn on a pool of long-lived threads hold one encode
//! workspace between them, not one per thread's allocator arena.
#![cfg(all(target_os = "linux", target_env = "gnu"))]
#[path = "../support/memory_metadata.rs"]
mod metadata;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::Storage;
use std::sync::{mpsc, Arc};
use std::time::Duration;

const THREADS: usize = 8;

fn anonymous_mib() -> u64 {
    let rollup = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap();
    let line = rollup
        .lines()
        .find(|line| line.starts_with("Anonymous:"))
        .unwrap();
    line.split_whitespace()
        .nth(1)
        .unwrap()
        .parse::<u64>()
        .unwrap()
        / 1024
}

#[test]
fn saves_in_turn_on_a_thread_pool_do_not_keep_a_workspace_per_thread() {
    let metadata = Arc::new(metadata::MemoryMetadata::default());
    let mut turns = Vec::new();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let mut workers = Vec::new();
    for thread in 0..THREADS {
        let (turn_tx, turn_rx) = mpsc::channel::<u64>();
        turns.push(turn_tx);
        let (metadata, done) = (metadata.clone(), done_tx.clone());
        workers.push(std::thread::spawn(move || {
            // Each worker lives across turns, like a runtime's pool thread.
            while let Ok(turn) = turn_rx.recv_timeout(Duration::from_secs(20)) {
                let storage = Storage::new(metadata.clone()).unwrap();
                let save = storage.begin_save().unwrap();
                let value = format!("thread {thread} turn {turn}");
                let canonical =
                    layerfs_content::object::codec::encode_bytes_object(value.as_bytes()).unwrap();
                save.accept(FinalizedObject::new(ObjectRole::FileState, canonical).unwrap())
                    .unwrap();
                save.finish().unwrap();
                done.send(()).unwrap();
            }
        }));
    }
    let round = |turn: u64| {
        for sender in &turns {
            sender.send(turn).unwrap();
            done_rx.recv_timeout(Duration::from_secs(20)).unwrap();
        }
    };
    // Warm every thread's arena once, then measure further rounds.
    round(0);
    let before = anonymous_mib();
    for turn in 1..=4 {
        round(turn);
    }
    let after = anonymous_mib();
    drop(turns);
    for worker in workers {
        worker.join().unwrap();
    }
    eprintln!("anonymous before {before} MiB, after {after} MiB");
    // One 16 MiB workspace per thread would be 128 MiB at `before` already.
    assert!(
        before < 64,
        "{THREADS} threads hold {before} MiB after one Save each"
    );
    assert!(after <= before + 8, "{before} MiB grew to {after} MiB");
}
