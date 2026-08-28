use std::io::Cursor;

use blake3::Hasher;
use layerfs_core::cdc::FastCdc;
use layerfs_core::ObjectId;

const SEED: u64 = 0x4c41594552534653;
const K: usize = 64;
const F: usize = 64;
const CONTEXT: &str = "layerfs/canonical-v2/ordered-occurrence/v1";

#[derive(Clone, Copy)]
struct Reference {
    offset: usize,
    length: u32,
    id: ObjectId,
}

#[derive(Clone)]
struct Node {
    id: ObjectId,
    canonical: Vec<u8>,
    total: u64,
    refs: std::ops::Range<usize>,
    children: Vec<Node>,
    level: u8,
}

struct Oracle {
    label: &'static str,
    source_fingerprint: String,
    raw_sequence: String,
    commitment: String,
    corpus: String,
    reconstruction: String,
    range: String,
    references: usize,
    leaves: usize,
    branches: usize,
    level: u8,
    mapping_bytes: usize,
    file_root: ObjectId,
    workspace_root: ObjectId,
    transition: ObjectId,
    closure: String,
}
