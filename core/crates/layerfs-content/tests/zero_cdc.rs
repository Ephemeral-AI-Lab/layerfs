use layerfs_content::file::cdc::{FastCdc, MAXIMUM_CHUNK_BYTES};
use std::io::Read;

#[test]
fn frozen_zero_chunk_period_is_the_maximum() {
    let mut chunks = Vec::new();
    let zeros = std::io::repeat(0).take((MAXIMUM_CHUNK_BYTES * 4 + 17) as u64);
    FastCdc::new()
        .scan(zeros, |chunk| {
            assert!(chunk.iter().all(|byte| *byte == 0));
            chunks.push(chunk.len());
            Ok(())
        })
        .unwrap();
    println!("ZERO_CDC chunks={chunks:?} period={MAXIMUM_CHUNK_BYTES}");
    assert_eq!(
        chunks,
        [
            MAXIMUM_CHUNK_BYTES,
            MAXIMUM_CHUNK_BYTES,
            MAXIMUM_CHUNK_BYTES,
            MAXIMUM_CHUNK_BYTES,
            17
        ]
    );
}
