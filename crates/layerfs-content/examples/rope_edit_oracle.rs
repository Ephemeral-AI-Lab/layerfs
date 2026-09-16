//! Sealed v0.1.6 oracle: exact rope-edit roots and partitions for frozen fixtures.
//!
//! This is an oracle input, not product code and not a candidate dependency. It
//! builds a base file with the reference constructor, applies the reference's
//! localized-edit batch, and prints the resulting root, logical length and the
//! decoded page partition as JSON on stdout. Candidate tests read that JSON and
//! compare against their own root and partition, so "equal final bytes" is never
//! mistaken for reference equivalence.
//!
//! Usage:
//!   cargo +1.85.1 run --locked -p layerfs-content --example rope_edit_oracle -- --case join-80-100

use std::collections::BTreeMap;
use std::io::Cursor;

use layerfs_content::file::extent::{ExtentNodeV3, FileStateV3};
use layerfs_content::file::extent_codec::{decode_file_state, decode_node_with_context};
use layerfs_content::file::rope::{self, FileMutationBatch};
use layerfs_content::object::access::ObjectStore;
use layerfs_content::ObjectId;

/// In-memory object store: the oracle needs no persistence.
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

/// Deterministic incompressible fixture, identical to the candidate generators.
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

/// Deterministic structured fixture, identical to the candidate generators.
fn patterned(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect()
}

/// Chunk-count probe: the smallest `noise` length whose construction has `wanted`
/// extents, found by bisection exactly as the candidate tests do.
fn file_with_extents(wanted: u64) -> Vec<u8> {
    fn extents_at(length: usize) -> (u64, ObjectId) {
        let mut store = Memory::default();
        let (root, _) = rope::build_bytes(&mut store, &noise(length)).expect("build");
        let canonical = store.get(root.0).expect("root");
        let state = decode_file_state(&canonical).expect("state");
        let node = decode_node_with_context(&store.get(state.mapping_root).expect("node"), true)
            .expect("node");
        (node.extent_count(), root.0)
    }
    let mut low = 0_usize;
    let mut high = (wanted as usize) * 32_768 + 65_536;
    while extents_at(high).0 < wanted {
        low = high;
        high *= 2;
    }
    while low + 1 < high {
        let middle = (low + high) / 2;
        if extents_at(middle).0 >= wanted {
            high = middle;
        } else {
            low = middle;
        }
    }
    for candidate in [high, high + 1_024, high + 4_096, high + 16_384] {
        if extents_at(candidate).0 == wanted {
            return noise(candidate);
        }
    }
    let mut candidate = high;
    for _ in 0..512 {
        candidate += 1_024;
        if extents_at(candidate).0 == wanted {
            return noise(candidate);
        }
    }
    panic!("no input produced {wanted} extents");
}

/// Pages of the mapping tree under `root`, in level order.
fn pages(store: &Memory, root: ObjectId) -> Vec<(u8, bool, usize, String)> {
    let canonical = store.get(root).expect("root bytes");
    let state = decode_file_state(&canonical).expect("file state");
    let mut pages = Vec::new();
    let mut level = vec![state.mapping_root];
    let mut depth = state.tree_level;
    let mut is_root = true;
    while !level.is_empty() {
        let mut next = Vec::new();
        for id in &level {
            let node = decode_node_with_context(&store.get(*id).expect("node"), is_root)
                .expect("canonical page");
            pages.push((depth, is_root, node.entry_count(), id.to_string()));
            if let ExtentNodeV3::Branch { children, .. } = node {
                for child in children {
                    next.push(child.child_object_id);
                }
            }
        }
        is_root = false;
        depth = depth.saturating_sub(1);
        level = next;
    }
    pages
}

fn state_len(store: &Memory, root: ObjectId) -> u64 {
    let state: FileStateV3 = decode_file_state(&store.get(root).expect("root")).expect("state");
    state.logical_len
}

fn main() {
    let mut case = String::new();
    let mut arguments = std::env::args().skip(1);
    while let Some(flag) = arguments.next() {
        let value = arguments.next().unwrap_or_default();
        if flag == "--case" {
            case = value;
        }
    }
    let mut store = Memory::default();
    let (base_bytes, edits): (Vec<u8>, Vec<(u64, u64, Vec<u8>)>) = match case.as_str() {
        "join-80-100" => {
            let left = file_with_extents(80);
            let right = file_with_extents(100);
            let mut joined = left.clone();
            joined.extend_from_slice(&right);
            // Insert at the seam: the join boundary is where the two inputs meet.
            let seam = left.len() as u64;
            let replacement = noise(200_000);
            (joined, vec![(seam, 0, replacement)])
        }
        "interior-multi-level" => {
            let base = file_with_extents(400);
            let start = (base.len() / 2) as u64;
            let replacement = noise(40_000);
            (base, vec![(start, 40_000, replacement)])
        }
        "untouched-sibling" => {
            let base = file_with_extents(300);
            let start = (base.len() / 4) as u64;
            let replacement = noise(1_000);
            (base, vec![(start, 1_000, replacement)])
        }
        "height-growth" => {
            let base = file_with_extents(200);
            let end = base.len() as u64;
            let replacement = noise(3_000_000);
            (base, vec![(end, 0, replacement)])
        }
        "root-collapse" => {
            let base = file_with_extents(200);
            let cut = (base.len() - 400_000) as u64;
            (base, vec![(0, cut, Vec::new())])
        }
        "batch-normalized" => {
            let base = patterned(1_500_000);
            let edits = vec![
                (1_000_u64, 0_u64, noise(700)),
                (600_000, 300, noise(300)),
                (1_200_000, 5_000, Vec::new()),
            ];
            (base, edits)
        }
        other => panic!("unsupported case {other}"),
    };
    let (base_root, _) = rope::build_bytes(&mut store, &base_bytes).expect("base construction");
    let base_pages = pages(&store, base_root.0);
    let mut batch = FileMutationBatch::new(&mut store, Some(base_root)).expect("batch");
    for (start, delete, replacement) in &edits {
        batch
            .replace(*start, *delete, Cursor::new(replacement.clone()))
            .expect("replace");
    }
    let (edited_root, counters) = batch.finish().expect("finish");
    let edited_pages = pages(&store, edited_root.0);
    println!("{{");
    println!("  \"case\": \"{case}\",");
    println!("  \"reference\": \"44cf748486863ab7c21ca47e731bd88e2b9a7b4a\",");
    println!("  \"base_root\": \"{}\",", base_root.0);
    println!("  \"base_len\": {},", state_len(&store, base_root.0));
    println!("  \"edited_root\": \"{}\",", edited_root.0);
    println!("  \"edited_len\": {},", state_len(&store, edited_root.0));
    println!("  \"nodes_created\": {},", counters.nodes_created);
    println!("  \"nodes_read\": {},", counters.nodes_read);
    println!("  \"base_pages\": {},", render(&base_pages));
    println!("  \"edited_pages\": {}", render(&edited_pages));
    println!("}}");
}

fn render(pages: &[(u8, bool, usize, String)]) -> String {
    let rows = pages
        .iter()
        .map(|(level, is_root, entries, id)| {
            format!(
                "{{\"level\": {level}, \"root\": {is_root}, \"entries\": {entries}, \"id\": \"{id}\"}}"
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rows}]")
}
