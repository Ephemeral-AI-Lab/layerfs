// Product content signature (encoding/delta/candidates.rs), copied for timing only.
use std::time::Instant;
const WINDOW: usize = 16;
const EMPTY: u64 = u64::MAX;
fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
fn signature(raw: &[u8]) -> [u64; 8] {
    let mut result = [EMPTY; 8];
    if raw.len() < WINDOW { return result; }
    let high = 257_u64.wrapping_pow((WINDOW - 1) as u32);
    let mut rolling = raw[..WINDOW].iter().fold(0_u64, |h, b| h.wrapping_mul(257) + u64::from(*b));
    for start in 0..=raw.len() - WINDOW {
        if start != 0 {
            rolling = rolling.wrapping_sub(u64::from(raw[start - 1]).wrapping_mul(high))
                .wrapping_mul(257).wrapping_add(u64::from(raw[start + WINDOW - 1]));
        }
        let hash = mix(rolling);
        if hash < result[7] && !result.contains(&hash) {
            let index = result.partition_point(|v| *v < hash);
            result.copy_within(index..7, index + 1);
            result[index] = hash;
        }
    }
    result
}
fn main() {
    let path = std::env::args().nth(1).expect("payload file");
    let data = std::fs::read(path).expect("read");
    // payload file: u32 length-prefixed real chunk payloads
    let mut chunks = Vec::new();
    let mut i = 0;
    while i + 4 <= data.len() {
        let n = u32::from_le_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        chunks.push(&data[i + 4..i + 4 + n]);
        i += 4 + n;
    }
    let bytes: usize = chunks.iter().map(|c| c.len()).sum();
    let mut sink = 0u64;
    let start = Instant::now();
    for _ in 0..5 { for c in &chunks { sink ^= signature(c)[0]; } }
    let secs = start.elapsed().as_secs_f64() / 5.0;
    println!("chunks={} bytes={} per_chunk_us={:.2} throughput_MBps={:.0} sink={}", chunks.len(), bytes,
        secs * 1e6 / chunks.len() as f64, bytes as f64 / secs / 1e6, sink & 1);
}
