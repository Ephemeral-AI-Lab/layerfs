//! Reference-equivalence oracle for localized edits (Stage 4 checkpoint D).
//!
//! **Status: this target fails today, on purpose.** It is the executable
//! counterexample the stored-node split/concat work must satisfy, not a claim that
//! the work is done.
//!
//! The fixtures under
//! `docs/roadmap/0.1/0.1.7/evidence/stages-3-4-oracle-<UTC>/` were produced by the
//! sealed v0.1.6 reference (`44cf748486863ab7c21ca47e731bd88e2b9a7b4a`) in a
//! separate process: the reference builds the base, applies the same localized
//! edit through `FileMutationBatch`, and prints the resulting root and the decoded
//! page partition. This test builds the same base with the candidate constructor,
//! applies the same edit with `apply_edits`, and compares both the root and the
//! partition, including which leaf identities survive untouched.
//!
//! Equal final bytes or a valid canonical tree are explicitly not the oracle.

mod support;

use std::collections::BTreeMap;
use std::path::PathBuf;

use layerfs_content::file::mapping::{decode_file_state, decode_node_with_context, ExtentNode};
use layerfs_content::{
    apply_edits, construct_bytes, ConstructionPolicy, Edit, EditRequest, EditStream, ObjectId,
    Replacements,
};
use support::{disabled_scope, MemoryStore};

/// Frozen oracle fixtures, produced by the sealed reference.
///
/// This is the corrected generation: both sides execute the same current-result
/// edit tuples and every fixture records those tuples, so the operations are
/// compared before any tree is. The earlier generation
/// (`stages-3-4-oracle-20260916T214846Z`) is retained unchanged, but its
/// comparison claim is superseded: the candidate side translated coordinates
/// wrongly there, which made four of its mismatches look like algorithm gaps.
const ORACLE_DIR: &str =
    "../../../docs/roadmap/0.1/0.1.7/evidence/stages-3-4-oracle-20260916T222738Z-corrected";

#[derive(Debug)]
struct Page {
    root: bool,
    entries: usize,
    id: String,
}

#[derive(Debug)]
struct Fixture {
    base_len: u64,
    base_root: String,
    edited_root: String,
    edited_len: u64,
    base_pages: Vec<Page>,
    edited_pages: Vec<Page>,
    edits: Vec<(u64, u64, u64)>,
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

fn patterned(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect()
}

/// The same extents: a deterministic fixture with exactly `wanted` extents.
fn file_with_extents(wanted: u64) -> Vec<u8> {
    fn extents_at(length: usize) -> u64 {
        let mut store = MemoryStore::new();
        let policy = ConstructionPolicy::frozen_default();
        let constructed = disabled_scope(|scope| {
            construct_bytes(
                policy,
                &policy.capacities(),
                &noise(length),
                &mut store,
                scope.child("content"),
            )
        })
        .expect("build");
        support::extent_count(&store, constructed.root)
    }
    let mut low = 0_usize;
    let mut high = (wanted as usize) * 32_768 + 65_536;
    while extents_at(high) < wanted {
        low = high;
        high *= 2;
    }
    while low + 1 < high {
        let middle = (low + high) / 2;
        if extents_at(middle) >= wanted {
            high = middle;
        } else {
            low = middle;
        }
    }
    for candidate in [high, high + 1_024, high + 4_096, high + 16_384] {
        if extents_at(candidate) == wanted {
            return noise(candidate);
        }
    }
    let mut candidate = high;
    for _ in 0..512 {
        candidate += 1_024;
        if extents_at(candidate) == wanted {
            return noise(candidate);
        }
    }
    panic!("no input produced {wanted} extents");
}

/// One raw edit: start, deleted length and replacement bytes.
type RawEdit = (u64, u64, Vec<u8>);

/// Fixture inputs and the edit stream in current-result coordinates.
fn fixture_inputs(case: &str) -> (Vec<u8>, Vec<Edit>, Replacements) {
    let (base, raw_edits): (Vec<u8>, Vec<RawEdit>) = match case {
        "join-80-100" => {
            let left = file_with_extents(80);
            let right = file_with_extents(100);
            let seam = left.len() as u64;
            let mut joined = left;
            joined.extend_from_slice(&right);
            (joined, vec![(seam, 0, noise(200_000))])
        }
        "half-partition-90-90" => {
            // 180 extents constructed as two 90-entry leaves; the edit sits exactly
            // on the seam so the join must repartition more than 128 entries in half.
            let base = file_with_extents(180);
            let mut store = MemoryStore::new();
            let policy = ConstructionPolicy::frozen_default();
            let probe = disabled_scope(|scope| {
                construct_bytes(
                    policy,
                    &policy.capacities(),
                    &base,
                    &mut store,
                    scope.child("content"),
                )
            })
            .expect("probe");
            let canonical = store.canonical(probe.root).expect("state");
            let state = decode_file_state(&canonical).expect("file state");
            let node =
                decode_node_with_context(store.canonical(state.mapping_root).expect("root"), true)
                    .expect("page");
            let seam = match node {
                ExtentNode::Branch { children, .. } => children[0].cumulative_logical_end,
                ExtentNode::Leaf { .. } => panic!("expected a branch root"),
            };
            (base, vec![(seam.saturating_sub(32), 64, Vec::new())])
        }
        "unequal-height-join" => {
            let base = file_with_extents(140);
            let end = base.len() as u64;
            (base, vec![(end, 0, noise(2_500_000))])
        }
        "untouched-sibling" => {
            let base = file_with_extents(300);
            let start = (base.len() / 4) as u64;
            (base, vec![(start, 1_000, noise(1_000))])
        }
        "interior-multi-level" => {
            let base = file_with_extents(400);
            let start = (base.len() / 2) as u64;
            (base, vec![(start, 40_000, noise(40_000))])
        }
        "height-growth" => {
            let base = file_with_extents(200);
            let end = base.len() as u64;
            (base, vec![(end, 0, noise(3_000_000))])
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
    // `FileMutationBatch::replace` applies each replacement to its **current** root
    // and then advances that root, and the reference's own `extent_model` test
    // proves the batch is equivalent to applying the same tuples sequentially to
    // the running result. The tuples are therefore already in current-result
    // coordinates and are passed through unchanged: no accumulated insertion
    // offset is added.
    let mut edits = Vec::new();
    let mut source = Replacements::new();
    for (start, delete, replacement) in raw_edits {
        let index = source.push(replacement.clone());
        assert_eq!(index, edits.len(), "one replacement per edit");
        edits.push(Edit::new(start, start + delete, replacement.len() as u64));
    }
    (base, edits, source)
}

/// Pages of the candidate mapping tree, in level order, with their identities.
fn candidate_pages(store: &MemoryStore, root: ObjectId) -> Vec<Page> {
    let canonical = store.canonical(root).expect("root bytes");
    if layerfs_content::whole_file_payload(canonical)
        .expect("framing")
        .is_some()
    {
        return vec![Page {
            root: true,
            entries: 1,
            id: root.to_string(),
        }];
    }
    let state = decode_file_state(canonical).expect("file state");
    let mut pages = Vec::new();
    let mut level = vec![state.mapping_root];
    let mut depth = state.tree_level;
    let mut is_root = true;
    while !level.is_empty() {
        let mut next = Vec::new();
        for id in &level {
            let node = decode_node_with_context(store.canonical(*id).expect("node"), is_root)
                .expect("canonical page");
            pages.push(Page {
                root: is_root,
                entries: node.entry_count(),
                id: id.to_string(),
            });
            if let ExtentNode::Branch { children, .. } = node {
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

/// Normalized edit tuples the reference executed: start, deleted, replacement bytes.
fn parse_edits(text: &str) -> Vec<(u64, u64, u64)> {
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with("\"edits\":"))
        .expect("fixture records its edit tuples");
    let body = line.split_once(':').expect("value").1.trim();
    let body = body.trim_end_matches(',').trim();
    let mut edits = Vec::new();
    for entry in body
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split("], [")
    {
        let cleaned = entry.trim_matches(|character: char| {
            character == '[' || character == ']' || character == ' '
        });
        if cleaned.is_empty() {
            continue;
        }
        let numbers = cleaned
            .split(',')
            .map(|value| value.trim().parse::<u64>().expect("tuple field"))
            .collect::<Vec<_>>();
        assert_eq!(numbers.len(), 3, "tuple shape");
        edits.push((numbers[0], numbers[1], numbers[2]));
    }
    edits
}

fn load(case: &str) -> Fixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(ORACLE_DIR)
        .join(format!("{case}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("oracle fixture {}: {error}", path.display()));
    // A tiny hand parser keeps the fixture reading dependency-free: the files are
    // produced by this repository's own oracle printer.
    let scalar = |key: &str| -> String {
        text.lines()
            .find(|line| line.trim_start().starts_with(&format!("\"{key}\":")))
            .map(|line| {
                line.split(':')
                    .nth(1)
                    .expect("value")
                    .trim()
                    .trim_end_matches(',')
                    .trim_matches('"')
                    .to_string()
            })
            .unwrap_or_else(|| panic!("fixture {case} has no {key}"))
    };
    let pages = |key: &str| -> Vec<Page> {
        let line = text
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("\"{key}\":")))
            .unwrap_or_else(|| panic!("fixture {case} has no {key}"));
        let body = line.split_once(':').expect("value").1.trim();
        let body = body.trim_end_matches(',').trim();
        let mut pages = Vec::new();
        for entry in body
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split("}, {")
        {
            if entry.trim().is_empty() {
                continue;
            }
            let field = |name: &str| -> String {
                entry
                    .split(',')
                    .find(|part| part.contains(&format!("\"{name}\"")))
                    .map(|part| {
                        part.split(':')
                            .nth(1)
                            .expect("field")
                            .trim()
                            .trim_matches(|character: char| {
                                character == '"' || character == '}' || character == ']'
                            })
                            .to_string()
                    })
                    .unwrap_or_default()
            };
            pages.push(Page {
                root: field("root") == "true",
                entries: field("entries").parse().expect("entries"),
                id: field("id"),
            });
        }
        pages
    };
    Fixture {
        base_len: scalar("base_len").parse().expect("base_len"),
        base_root: scalar("base_root"),
        edited_root: scalar("edited_root"),
        edited_len: scalar("edited_len").parse().expect("edited_len"),
        base_pages: pages("base_pages"),
        edited_pages: pages("edited_pages"),
        edits: parse_edits(&text),
    }
}

/// One case: build the base, apply the edit and compare everything with the oracle.
fn compare(case: &str) -> BTreeMap<String, String> {
    let oracle = load(case);
    let (base, edits, source) = fixture_inputs(case);
    let policy = ConstructionPolicy::frozen_default();
    let mut base_store = MemoryStore::new();
    let constructed = disabled_scope(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &base,
            &mut base_store,
            scope.child("content"),
        )
    })
    .expect("base construction");
    assert_eq!(
        constructed.logical_len, oracle.base_len,
        "{case}: base length matches the oracle"
    );

    // Operation identity is asserted before any tree is compared: the candidate
    // must execute exactly the tuples the reference executed.
    let recorded: Vec<(u64, u64, u64)> = edits
        .iter()
        .map(|edit| (edit.start(), edit.removed_len(), edit.replacement_len()))
        .collect();
    assert_eq!(
        recorded, oracle.edits,
        "{case}: the candidate edit tuples differ from the reference's"
    );
    let stream = EditStream::new(base.len() as u64, edits).expect("valid stream");
    let mut result_store = base_store.merged_clone();
    let edited = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &base_store,
            EditRequest {
                root: constructed.root,
                edits: &stream,
                source: &source,
            },
            &mut result_store,
            scope.child("edit"),
        )
    })
    .expect("edit succeeds");

    let mut report = BTreeMap::new();
    report.insert("base_root".to_string(), format!("{}", constructed.root));
    report.insert("oracle_base_root".to_string(), oracle.base_root.clone());
    // The base must be the reference's base: it isolates the gap to the edit.
    assert_eq!(
        format!("{}", constructed.root),
        oracle.base_root,
        "{case}: the candidate base differs from the reference base"
    );
    report.insert("edited_root".to_string(), format!("{}", edited.root));
    report.insert("oracle_root".to_string(), oracle.edited_root.clone());
    report.insert(
        "length".to_string(),
        format!("{} / {}", edited.logical_len, oracle.edited_len),
    );
    let pages = candidate_pages(&result_store, edited.root);
    report.insert(
        "partition".to_string(),
        pages
            .iter()
            .map(|page| format!("{}/{}", page.entries, &page.id[..8]))
            .collect::<Vec<_>>()
            .join(" "),
    );
    report.insert(
        "oracle_partition".to_string(),
        oracle
            .edited_pages
            .iter()
            .map(|page| format!("{}/{}", page.entries, &page.id[..8]))
            .collect::<Vec<_>>()
            .join(" "),
    );
    // Unchanged leaves must keep their identities exactly where the reference does.
    let survivor = |oracle: &Fixture| -> Vec<String> {
        oracle
            .edited_pages
            .iter()
            .filter(|page| {
                oracle
                    .base_pages
                    .iter()
                    .any(|base| base.id == page.id && !page.root)
            })
            .map(|page| page.id[..8].to_string())
            .collect()
    };
    report.insert("oracle_survivors".to_string(), survivor(&oracle).join(" "));
    let candidate_survivors = pages
        .iter()
        .filter(|page| {
            oracle
                .base_pages
                .iter()
                .any(|base| base.id == page.id && !page.root)
        })
        .map(|page| page.id[..8].to_string())
        .collect::<Vec<_>>();
    report.insert(
        "candidate_survivors".to_string(),
        candidate_survivors.join(" "),
    );
    report
}

#[test]
fn the_candidate_reproduces_the_reference_root_and_partition() {
    let cases = [
        "join-80-100",
        "half-partition-90-90",
        "unequal-height-join",
        "untouched-sibling",
        "interior-multi-level",
        "height-growth",
        "root-collapse",
        "batch-normalized",
    ];
    let mut failures = Vec::new();
    for case in cases {
        let report = compare(case);
        println!("=== {case}");
        for (key, value) in &report {
            println!("  {key}: {value}");
        }
        if report["edited_root"] != report["oracle_root"] {
            failures.push(format!(
                "{case}: root {} != oracle {}",
                report["edited_root"], report["oracle_root"]
            ));
        }
        if report["partition"] != report["oracle_partition"] {
            failures.push(format!(
                "{case}: partition {} != oracle {}",
                report["partition"], report["oracle_partition"]
            ));
        }
        if report["candidate_survivors"] != report["oracle_survivors"] {
            failures.push(format!(
                "{case}: retained leaves [{}] != oracle [{}]",
                report["candidate_survivors"], report["oracle_survivors"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "localized edits do not reproduce the reference tree:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_oracle_fixtures_are_the_sealed_reference_revision() {
    for case in ["join-80-100", "untouched-sibling"] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(ORACLE_DIR)
            .join(format!("{case}.json"));
        let text = std::fs::read_to_string(&path).expect("fixture");
        assert!(
            text.contains("44cf748486863ab7c21ca47e731bd88e2b9a7b4a"),
            "{case}: fixture does not name the pinned reference revision"
        );
    }
}
