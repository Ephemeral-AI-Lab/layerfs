// Standalone diagnostic: rustc -C opt-level=3 -o /tmp/lfs237-signature-microbench signature_microbench.rs
// Uses synthetic in-memory bytes. Results describe this pure function only,
// never the cold-source public Init timer.
use std::{hint::black_box, time::Instant};

const EMPTY: u64 = u64::MAX;
const WINDOW: usize = 16;

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

// Copied from core/crates/layerfs-storage/src/encoding/delta/candidates.rs.
fn reference(raw: &[u8]) -> [u64; 8] {
    let mut result = [EMPTY; 8];
    if raw.len() < WINDOW {
        return result;
    }
    let high = 257_u64.wrapping_pow((WINDOW - 1) as u32);
    let mut rolling = raw[..WINDOW].iter().fold(0_u64, |hash, byte| {
        hash.wrapping_mul(257) + u64::from(*byte)
    });
    for start in 0..=raw.len() - WINDOW {
        if start != 0 {
            rolling = rolling
                .wrapping_sub(u64::from(raw[start - 1]).wrapping_mul(high))
                .wrapping_mul(257)
                .wrapping_add(u64::from(raw[start + WINDOW - 1]));
        }
        let hash = mix(rolling);
        if hash < result[7] && !result.contains(&hash) {
            let index = result.partition_point(|value| *value < hash);
            result.copy_within(index..7, index + 1);
            result[index] = hash;
        }
    }
    result
}

#[inline(always)]
fn keep(result: &mut [u64; 8], hash: u64) {
    if hash < result[7] && !result.contains(&hash) {
        let index = result.partition_point(|value| *value < hash);
        result.copy_within(index..7, index + 1);
        result[index] = hash;
    }
}

#[inline(always)]
fn advance(rolling: u64, old: u8, new: u8, kick: u64) -> u64 {
    // Same polynomial mod 2^64; independent rolling*257 and old*kick terms.
    rolling
        .wrapping_mul(257)
        .wrapping_sub(u64::from(old).wrapping_mul(kick))
        .wrapping_add(u64::from(new))
}

fn recurrence(raw: &[u8]) -> [u64; 8] {
    let mut result = [EMPTY; 8];
    if raw.len() < WINDOW {
        return result;
    }
    let kick = 257_u64.wrapping_pow(WINDOW as u32);
    let mut rolling = raw[..WINDOW].iter().fold(0_u64, |hash, byte| {
        hash.wrapping_mul(257).wrapping_add(u64::from(*byte))
    });
    keep(&mut result, mix(rolling));
    for start in 1..=raw.len() - WINDOW {
        rolling = advance(rolling, raw[start - 1], raw[start + WINDOW - 1], kick);
        keep(&mut result, mix(rolling));
    }
    result
}

fn pipelined(raw: &[u8]) -> [u64; 8] {
    let mut result = [EMPTY; 8];
    if raw.len() < WINDOW {
        return result;
    }
    let kick = 257_u64.wrapping_pow(WINDOW as u32);
    let mut rolling = raw[..WINDOW].iter().fold(0_u64, |hash, byte| {
        hash.wrapping_mul(257).wrapping_add(u64::from(*byte))
    });
    let windows = raw.len() - WINDOW + 1;
    let mut start = 0;
    while start + 4 <= windows {
        let h0 = rolling;
        let h1 = advance(h0, raw[start], raw[start + WINDOW], kick);
        let h2 = advance(h1, raw[start + 1], raw[start + WINDOW + 1], kick);
        let h3 = advance(h2, raw[start + 2], raw[start + WINDOW + 2], kick);
        keep(&mut result, mix(h0));
        keep(&mut result, mix(h1));
        keep(&mut result, mix(h2));
        keep(&mut result, mix(h3));
        start += 4;
        if start < windows {
            rolling = advance(h3, raw[start - 1], raw[start + WINDOW - 1], kick);
        }
    }
    while start < windows {
        keep(&mut result, mix(rolling));
        start += 1;
        if start < windows {
            rolling = advance(rolling, raw[start - 1], raw[start + WINDOW - 1], kick);
        }
    }
    result
}

fn bytes(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

fn check() {
    for len in (0..80).chain([325, 326, 20_782, 131_072, 1_048_576]) {
        for seed in [0, 1, 0xdead_beef_u64] {
            let raw = bytes(len, seed);
            let expected = reference(&raw);
            assert_eq!(recurrence(&raw), expected, "recurrence len={len} seed={seed}");
            assert_eq!(pipelined(&raw), expected, "pipelined len={len} seed={seed}");
            let repeated = vec![(seed & 255) as u8; len];
            let expected = reference(&repeated);
            assert_eq!(recurrence(&repeated), expected);
            assert_eq!(pipelined(&repeated), expected);
        }
    }
}

fn measure(label: &str, files: &[Vec<u8>], f: fn(&[u8]) -> [u64; 8]) {
    let started = Instant::now();
    let mut checksum = 0_u64;
    for file in files {
        for hash in f(black_box(file)).into_iter().filter(|hash| *hash != EMPTY) {
            checksum ^= hash;
        }
    }
    println!("{label} ns={} checksum={checksum:016x}", started.elapsed().as_nanos());
}

fn main() {
    check();
    let mut files = Vec::with_capacity(9_399);
    for (count, len) in [(6_825, 326), (1_074, 325), (1_500, 20_782)] {
        for index in 0..count {
            files.push(bytes(len, (index + len) as u64));
        }
    }
    println!("files={} bytes={}", files.len(), files.iter().map(Vec::len).sum::<usize>());
    measure("reference", &files, reference);
    measure("recurrence", &files, recurrence);
    measure("pipelined", &files, pipelined);
}
