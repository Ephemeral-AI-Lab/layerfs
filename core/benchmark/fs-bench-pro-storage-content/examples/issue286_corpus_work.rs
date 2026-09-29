//! Count-driven corpus-read diagnostic; never a candidate performance arm.
use fs_bench_storage_content::workload::history::{Corpus, Row};
use std::{path::Path, time::Instant};
fn main() {
    let root = Path::new("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data");
    let started = Instant::now();
    let mut corpus = Corpus::open(root, Row::Stride1).unwrap();
    let open_ns = started.elapsed().as_nanos();
    let mut transition_ns = 0;
    let mut changed = 0;
    let mut blob_bytes = 0;
    for position in 0..157 {
        let started = Instant::now();
        let transition = corpus.transition(position).unwrap();
        transition_ns += started.elapsed().as_nanos();
        changed += transition.changed.len();
        blob_bytes += transition.blobs.values().map(Vec::len).sum::<usize>();
    }
    println!(
        "open_ns={open_ns} transition_ns={transition_ns} changed={changed} blob_bytes={blob_bytes}"
    );
}
