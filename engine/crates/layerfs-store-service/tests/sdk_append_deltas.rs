//! Chunks appended past the old end of a file are all offered the old last
//! chunk as their delta base. An incompressible append stops trying that base
//! after a few lost trials, and a log append that resembles it keeps its
//! deltas: the Save's losing-base memo.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_storage::encoding::delta::select::{DeltaCounters, LOSING_BASE_TRIALS};
use std::fs;
use support::Fixture;

fn noise(seed: u64, length: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

/// Log lines: messages drawn from 48 templates of a few dozen words, with
/// varying fields, so every chunk repeats phrases its base also holds.
fn log(first_line: u64, len: usize) -> Vec<u8> {
    const WORDS: &str = "request worker cache commit branch socket timeout retry \
        payload session upstream accepted rejected pending flushed mount \
        overlay journal segment compacted handshake client server quota \
        latency snapshot replica lease expired renewed index partition";
    let words: Vec<&str> = WORDS.split(' ').collect();
    let template = |k: u64| {
        let mut state = k.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        (0..14)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                words[(state % 32) as usize]
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let templates: Vec<String> = (0..48).map(template).collect();
    let mut out = String::new();
    let mut line = first_line;
    while out.len() < len {
        let r = line.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 7;
        out.push_str(&format!(
            "2026-11-07T{:02}:{:02}:{:02}Z INFO [worker-{:02}] {} id={}\n",
            (line / 3600) % 24,
            (line / 60) % 60,
            line % 60,
            r % 16,
            templates[((r >> 8) % 48) as usize],
            (r >> 16) % 100_000,
        ));
        line += 1;
    }
    out.truncate(len);
    out.into_bytes()
}

/// Writes `/big.bin` on `branch` and returns the Commit's payload selection.
fn write(fixture: &Fixture, branch: &str, bytes: Vec<u8>) -> DeltaCounters {
    let before = fixture.service.store.save_counters().delta;
    let reply = fixture.call(
        "fs.write",
        vec![
            ("branch", SdkValue::text(branch)),
            ("path", SdkValue::text("/big.bin")),
            ("bytes", SdkValue::Bytes(bytes)),
        ],
    );
    assert_eq!(reply.get_text("kind"), Some("Committed"), "{reply:?}");
    let after = fixture.service.store.save_counters().delta;
    let less = |a: u64, b: u64| a - b;
    let mut delta = DeltaCounters {
        trials: less(after.trials, before.trials),
        prefix_selected: less(after.prefix_selected, before.prefix_selected),
        full_losses: less(after.full_losses, before.full_losses),
        losing_skipped: less(after.losing_skipped, before.losing_skipped),
        ..DeltaCounters::default()
    };
    for (slot, (a, b)) in after
        .chunk_depths
        .iter()
        .zip(&before.chunk_depths)
        .enumerate()
    {
        delta.chunk_depths[slot] = a - b;
    }
    delta
}

#[test]
fn an_incompressible_append_stops_trying_and_a_log_append_keeps_its_deltas() {
    let fixture = Fixture::new("sdk-append-deltas", |source| {
        fs::write(source.join("seed.txt"), b"seed\n").unwrap();
        1
    });

    fixture.fork("noise");
    let base = noise(7, 1 << 20);
    write(&fixture, "noise", base.clone());
    let grown = write(&fixture, "noise", [base, noise(8, 3 << 20)].concat());
    let appended: u64 = grown.chunk_depths.iter().sum();
    println!("SDK_APPEND_DELTAS case=noise {grown:?}");
    assert!(appended > 100, "the append is chunked: {grown:?}");
    // A few lost trials, then one after each doubling gap: never one per chunk.
    let bound = u64::from(LOSING_BASE_TRIALS) + 64 - appended.leading_zeros() as u64 + 2;
    assert!(grown.trials <= bound, "{} trials: {grown:?}", grown.trials);
    assert_eq!(grown.prefix_selected, 0, "{grown:?}");
    assert!(
        grown.losing_skipped + grown.trials >= appended - 2,
        "{grown:?}"
    );

    fixture.fork("log");
    let base = log(0, 1 << 20);
    let lines = base.iter().filter(|b| **b == b'\n').count() as u64;
    write(&fixture, "log", base.clone());
    let grown = write(&fixture, "log", [base, log(lines + 1, 1 << 20)].concat());
    let appended: u64 = grown.chunk_depths.iter().sum();
    println!("SDK_APPEND_DELTAS case=log {grown:?}");
    assert!(appended > 30, "the append is chunked: {grown:?}");
    assert_eq!(grown.losing_skipped, 0, "{grown:?}");
    assert!(
        grown.prefix_selected * 10 >= appended * 9,
        "a log append is stored as deltas: {grown:?}"
    );
    fixture.cleanup();
}
