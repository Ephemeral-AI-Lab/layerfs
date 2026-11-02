//! Changed chunks of a rewritten chunked file are stored as deltas against
//! the old chunks they correspond to by byte range (#352 F-005): an append,
//! an insert near the start and an edit in the middle of a 1 MiB file.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::Fixture;

/// Pseudo-random bytes: a full chunk stores about its own size.
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

struct Stored {
    /// Chunk objects this write stored, FULL and as deltas.
    full: u64,
    deltas: u64,
    /// Content bytes the Commit stored.
    bytes: i64,
}

fn write(fixture: &Fixture, branch: &str, bytes: Vec<u8>) -> Stored {
    let before = fixture.service.store.save_counters().delta.chunk_depths;
    let reply = fixture.call(
        "fs.write",
        vec![
            ("branch", SdkValue::text(branch)),
            ("path", SdkValue::text("/big.bin")),
            ("bytes", SdkValue::Bytes(bytes)),
        ],
    );
    assert_eq!(reply.get_text("kind"), Some("Committed"), "{reply:?}");
    let after = fixture.service.store.save_counters().delta.chunk_depths;
    let added: Vec<u64> = after
        .iter()
        .zip(before.iter())
        .map(|(a, b)| a - b)
        .collect();
    Stored {
        full: added[0],
        deltas: added[1..].iter().sum(),
        bytes: reply
            .get("commit")
            .and_then(|c| c.get_int("newStored"))
            .unwrap(),
    }
}

#[test]
fn changed_chunks_of_a_rewritten_file_are_deltas() {
    let fixture = Fixture::new("sdk-chunk-deltas", |source| {
        fs::write(source.join("seed.txt"), b"seed\n").unwrap();
        1
    });
    let mut cases = Vec::new();
    // Each case has its own base, so no case reuses another's chunks.
    for (name, seed) in [("append", 7), ("insert", 17), ("edit", 27)] {
        let base = noise(seed, 1 << 20);
        let changed = match name {
            // Appended text resembles the end of the file, as a log's does.
            "append" => {
                let mut tail = base[base.len() - 3000..].to_vec();
                tail[1500..1510].copy_from_slice(&noise(seed + 1, 10));
                [base.clone(), tail].concat()
            }
            "insert" => [&base[..1000], &noise(seed + 1, 100), &base[1000..]].concat(),
            _ => {
                let mut edited = base.clone();
                edited[512 * 1024..512 * 1024 + 50].copy_from_slice(&noise(seed + 1, 50));
                edited
            }
        };
        fixture.fork(name);
        let first = write(&fixture, name, base);
        assert!(first.full > 50, "{name}: the base is chunked");
        let stored = write(&fixture, name, changed);
        println!(
            "SDK_CHUNK_DELTAS case={name} chunks_full={} chunks_delta={} new_stored={}",
            stored.full, stored.deltas, stored.bytes
        );
        cases.push((name, stored));
    }
    for (name, stored) in &cases {
        assert!(stored.deltas >= 1, "{name}: a changed chunk is a delta");
        // Every fs.write Commit also stores one 12-byte chunk outside
        // /big.bin; no chunk of the file itself is stored FULL.
        assert!(stored.full <= 1, "{name}: a changed chunk is stored FULL");
        assert!(
            stored.bytes < 8 * 1024,
            "{name}: stored {} bytes, half a target chunk is 8 KiB",
            stored.bytes
        );
    }
    fixture.cleanup();
}
