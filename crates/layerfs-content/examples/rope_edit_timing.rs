//! Reference-side C1 localized-edit timing for the matched pair.
//!
//! Development tool, not product code and never a candidate dependency. It builds
//! the frozen fixture with the reference constructor, applies the same edit tuple
//! through the reference `FileMutationBatch`, and prints the operation's wall time,
//! the resulting root and the objects the edit wrote. The candidate counterpart
//! (`core/crates/layerfs-content/examples/edit_timing_c1.rs`) builds the same base
//! bytes, applies the same tuple and prints the same fields, so the two arms can be
//! compared as separate processes on identical inputs.
//!
//! Usage: `cargo +1.85.1 run --locked -p layerfs-content --example rope_edit_timing`

use std::collections::BTreeMap;
use std::time::Instant;

use layerfs_content::file::extent_codec::decode_file_state;
use layerfs_content::file::rope::{self, FileMutationBatch};
use layerfs_content::object::access::ObjectStore;
use layerfs_content::ObjectId;

/// In-memory object store: the reference needs no persistence for this arm.
#[derive(Default)]
struct Memory {
    objects: BTreeMap<ObjectId, Vec<u8>>,
}

impl ObjectStore for Memory {
    fn get(&self, id: ObjectId) -> layerfs_content::CoreResult<Vec<u8>> {
        self.objects
            .get(&id)
            .cloned()
            .ok_or(layerfs_content::CoreError::MissingObject)
    }

    fn put(&mut self, canonical: &[u8]) -> layerfs_content::CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        self.objects.insert(id, canonical.to_vec());
        Ok(id)
    }
}

fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    (0..len)
        .map(|_| {
            state ^= state.wrapping_shl(7);
            state ^= state.wrapping_shr(9);
            state ^= state.wrapping_shl(8);
            state as u8
        })
        .collect()
}

fn main() {
    // The frozen fixture: 3 300 000 deterministic bytes and one 40 000-byte
    // overwrite in the middle, identical to the candidate counterpart.
    let base = noise(3_300_000);
    let start = 1_650_000_u64;
    let replacement = noise(40_000);
    let mut store = Memory::default();
    let (root, _) = rope::build_bytes(&mut store, &base).expect("reference construction");
    let base_id = root.0;
    let length = decode_file_state(&store.get(base_id).expect("base state"))
        .expect("state")
        .logical_len;
    let objects_before = store.objects.len();
    let written_before_bytes: usize = store.objects.values().map(|bytes| bytes.len()).sum();

    let mut batch = FileMutationBatch::new(&mut store, Some(root)).expect("batch");
    let started = Instant::now();
    batch
        .replace(
            start,
            replacement.len() as u64,
            std::io::Cursor::new(replacement),
        )
        .expect("reference edit");
    let (edited, counters) = batch.finish().expect("reference finish");
    let elapsed = started.elapsed();
    let written_bytes: usize = store
        .objects
        .values()
        .map(|bytes| bytes.len())
        .sum::<usize>()
        .saturating_sub(written_before_bytes);
    println!("implementation: reference v0.1.6 rope edit");
    println!("base_bytes: {}", base.len());
    println!("logical_length: {length}");
    println!("edit: replace [{start}, {})", start + 40_000);
    println!("elapsed_ns: {}", elapsed.as_nanos());
    println!("edited_root: {}", edited.0);
    println!("objects_before: {objects_before}");
    println!("objects_written: {}", store.objects.len() - objects_before);
    println!("objects_written_bytes: {written_bytes}");
    println!("nodes_created: {}", counters.nodes_created);
    println!("nodes_read: {}", counters.nodes_read);
}
