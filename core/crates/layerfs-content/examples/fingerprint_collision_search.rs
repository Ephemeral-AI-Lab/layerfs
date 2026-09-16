//! Development search for a real 64-bit fingerprint collision.
//!
//! The Store's pooled candidate filter keeps `(fingerprint, ordinal)` pairs where
//! the fingerprint is the low eight digest bytes of a canonical value, and full
//! 73-byte comparison decides equality. A genuine collision cannot be produced by
//! hand: it needs a birthday search in a 2^64 space. This example runs a
//! distinguished-point (van Oorschot-Wiener) collision search over valid canonical
//! inode values and prints the first collision it finds as JSON.
//!
//! Usage: `cargo run --release --example fingerprint_collision_search [budget_bits]`
//! Output on stdout: one JSON object with the two colliding values in hex, their
//! fingerprints and the work spent. Progress goes to stderr. This is a development
//! tool: it is not part of the product, it is never a test hook, and nothing in the
//! product depends on it.

use std::collections::HashMap;

use layerfs_content::inode_leaf::{decode_inode_value, encode_inode_value, InodeKind, InodeValue};
use layerfs_content::ObjectId;

/// Rebuilds the state a value encodes, so both directions agree by construction.
///
/// Only the content root varies with the state, and it is built from raw bytes, so
/// one search step costs exactly one identity hash over the canonical value.
fn value_of(state: u64) -> [u8; 73] {
    let mut raw = [0_u8; 32];
    raw[..8].copy_from_slice(&state.to_le_bytes());
    raw[8..16].copy_from_slice(&state.rotate_left(29).to_le_bytes());
    raw[16..24].copy_from_slice(&state.rotate_left(43).to_le_bytes());
    raw[24..].copy_from_slice(&state.rotate_left(11).to_le_bytes());
    let content = ObjectId::from_bytes(&raw).expect("32-byte identity");
    encode_inode_value(InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: content,
        metadata_root: ObjectId::from_bytes(&[0xA5; 32]).expect("32-byte identity"),
    })
}

/// The candidate filter of the pooled index, recomputed from the public identity.
fn fingerprint(state: u64) -> u64 {
    let canonical = ObjectId::for_bytes(&value_of(state));
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&canonical.as_bytes()[..8]);
    u64::from_le_bytes(bytes)
}

/// Step of the random walk: the fingerprint is the next state.
fn step(state: u64) -> u64 {
    fingerprint(state)
}

/// Distinguished points: low 20 bits zero, one chain end per 2^20 steps.
fn distinguished(state: u64) -> bool {
    state & 0xF_FFFF == 0
}

/// Walks from `start` to its endpoint, reporting whether it stopped on one.
fn walk(start: u64, limit: u64) -> (u64, u64, bool) {
    let mut state = start;
    let mut steps = 0;
    while steps < limit {
        state = step(state);
        steps += 1;
        if distinguished(state) {
            return (state, steps, true);
        }
    }
    (state, steps, false)
}

fn main() {
    let budget_bits: u32 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(33);
    let budget = 1_u64 << budget_bits;
    let limit = 1_u64 << 24;
    let mut table: HashMap<u64, (u64, u64)> = HashMap::new();
    let mut spent = 0_u64;
    let mut attempts = 0_u64;
    let mut seed = 0x0005_DEEC_E66D_u64;
    while spent < budget {
        // Deterministic pseudo-random starts: a splitmix64 stream, so a run is
        // reproducible from its seed and the printed attempt count.
        seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        let start = z ^ (z >> 31);
        let (end, steps, endpoint) = walk(start, limit);
        spent += steps;
        attempts += 1;
        if !endpoint {
            continue;
        }
        if let Some((other, other_steps)) = table.insert(end, (start, steps)) {
            if other != start {
                if let Some((left, right)) = merge(other, other_steps, start, steps) {
                    let first = value_of(left);
                    let second = value_of(right);
                    if first != second
                        && decode_inode_value(&first).is_ok()
                        && decode_inode_value(&second).is_ok()
                    {
                        println!("{{");
                        println!("  \"work_hashes\": {spent},");
                        println!("  \"attempts\": {attempts},");
                        println!("  \"fingerprint\": \"{:016x}\",", fingerprint(left));
                        println!("  \"first_hex\": \"{}\",", hex(&first));
                        println!("  \"second_hex\": \"{}\",", hex(&second));
                        println!("  \"method\": \"distinguished-point collision search\"");
                        println!("}}");
                        eprintln!("collision after {spent} hashes in {attempts} chains");
                        return;
                    }
                }
            }
        }
        if attempts % 256 == 0 {
            eprintln!("chains {attempts} hashes {spent} endpoints {}", table.len());
        }
    }
    eprintln!("no collision within {spent} hashes in {attempts} chains");
    std::process::exit(2);
}

/// Walks two chains to their merge and returns the colliding predecessors.
fn merge(
    first_start: u64,
    first_steps: u64,
    second_start: u64,
    second_steps: u64,
) -> Option<(u64, u64)> {
    let mut first = first_start;
    let mut second = second_start;
    let mut a = first_steps;
    let mut b = second_steps;
    while a > b {
        first = step(first);
        a -= 1;
    }
    while b > a {
        second = step(second);
        b -= 1;
    }
    if first == second {
        return None;
    }
    loop {
        let previous_first = first;
        let previous_second = second;
        first = step(first);
        second = step(second);
        if first == second {
            return if previous_first == previous_second {
                None
            } else {
                Some((previous_first, previous_second))
            };
        }
        if a == 0 {
            return None;
        }
        a -= 1;
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        use std::fmt::Write;
        let _ = write!(text, "{byte:02x}");
        text
    })
}
