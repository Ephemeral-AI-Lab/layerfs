//! Reference-equivalence oracle for localized edits (Stage 4 checkpoint D).
//!
//! **Status: this target passes.** It was written as the executable counterexample
//! the stored-node split/concat work had to satisfy, and the work it describes is
//! implemented: every one of the nine sealed cases matches the reference root, page
//! partition and surviving leaf identities. The header said "this target fails
//! today, on purpose" until 2026-09-17, when the independent review recorded that
//! the target passes and that the sentence was stale.
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
    apply_edits, construct_bytes, ConstructionPolicy, Edit, EditRequest, ObjectId,
};
use support::{disabled_scope, edits::Edits, edits::Parts, MemoryStore};

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

/// Directory of one case's fixture. `repartition-80-100` was produced later, into
/// its own generation directory: the earlier receipts are append-only and were not
/// touched, and the reference, the base and the edit tuple are identical between
/// the two generations for every case they share.
fn oracle_path(case: &str) -> PathBuf {
    let generation = if case == "repartition-80-100" {
        "../../../docs/roadmap/0.1/0.1.7/evidence/stages-3-4-oracle-20260917T034500Z"
    } else {
        ORACLE_DIR
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(generation)
        .join(format!("{case}.json"))
}

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

/// Recreate the sealed input bytes without rediscovering their lengths through
/// dozens of candidate constructions. The oracle comparison still checks the
/// exact base root, length, page partition, edits and surviving identities.
fn file_with_extents(wanted: u64) -> Vec<u8> {
    let length = match wanted {
        80 => load("join-80-100").edits[0].0,
        100 => {
            let join = load("join-80-100");
            join.base_len - join.edits[0].0
        }
        140 => load("unequal-height-join").base_len,
        180 => load("half-partition-90-90").base_len,
        200 => load("height-growth").base_len,
        300 => load("untouched-sibling").base_len,
        400 => load("interior-multi-level").base_len,
        _ => panic!("no sealed input with {wanted} extents"),
    };
    noise(usize::try_from(length).expect("fixture length"))
}

/// One raw edit: start, deleted length and replacement bytes.
type RawEdit = (u64, u64, Vec<u8>);

/// Boundaries of the extent run straddling `seam`: from an extent start to an
/// extent end, so the replacement removes and adds whole extents.
fn seam_region(bytes: &[u8], seam: u64) -> (u64, u64) {
    let policy = ConstructionPolicy::frozen_default();
    let mut store = MemoryStore::new();
    let constructed = disabled_scope(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            bytes,
            &mut store,
            scope.child("content"),
        )
    })
    .expect("probe");
    let state =
        decode_file_state(store.canonical(constructed.root).expect("state")).expect("file state");
    let mut extents: Vec<(u64, u64)> = Vec::new();
    let mut cursor = 0_u64;
    let mut level = vec![(state.mapping_root, true)];
    while let Some((id, is_root)) = level.pop() {
        let node = decode_node_with_context(store.canonical(id).expect("node"), is_root)
            .expect("canonical page");
        match node {
            ExtentNode::Leaf { extents: rows, .. } => {
                for row in rows {
                    let length = u64::from(row.logical_length());
                    extents.push((cursor, cursor + length));
                    cursor += length;
                }
            }
            ExtentNode::Branch { children, .. } => {
                for child in children.iter().rev() {
                    level.push((child.child_object_id, false));
                }
            }
        }
    }
    extents.sort_unstable();
    let start = extents
        .iter()
        .rev()
        .find(|(start, _)| *start < seam)
        .expect("an extent before the seam")
        .0;
    let end = extents
        .iter()
        .find(|(_, end)| *end > seam)
        .expect("an extent after the seam")
        .1;
    (start, end)
}

/// Fixture inputs and the edit stream in current-result coordinates.
fn fixture_inputs(case: &str) -> (Vec<u8>, Vec<Edit>, Parts) {
    let (base, raw_edits): (Vec<u8>, Vec<RawEdit>) = match case {
        "join-80-100" => {
            let left = file_with_extents(80);
            let right = file_with_extents(100);
            let seam = left.len() as u64;
            let mut joined = left;
            joined.extend_from_slice(&right);
            (joined, vec![(seam, 0, noise(200_000))])
        }
        "repartition-80-100" => {
            // The literal 80+100 join: two files whose own constructions hold 80 and
            // 100 extents are concatenated, and an edit that replaces the whole
            // extent run straddling the seam keeps the joined extent count, so the
            // reference must repartition the join. The oracle's edited partition is
            // 90 + 90 with the second leaf surviving by identity. The coordinates are
            // computed from the candidate's own probe here; the test asserts the base
            // root matches the oracle's, and identical roots imply the same layout.
            //
            // The name describes the *input*, never the sealed base pages: the
            // canonical construction of this join repartitions it, and the sealed
            // base pages are 89 and 90 (`base_pages` in the fixture). `compare`
            // asserts that partition against the sealed one, so the label and the
            // fixture cannot drift apart unnoticed.
            let left = file_with_extents(80);
            let right = file_with_extents(100);
            let mut joined = left.clone();
            joined.extend_from_slice(&right);
            let seam = left.len() as u64;
            let (start, end) = seam_region(&joined, seam);
            (
                joined,
                vec![(start, end - start, noise((end - start) as usize))],
            )
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
            let state = decode_file_state(canonical).expect("file state");
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
    let mut source = Parts::new();
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
    let path = oracle_path(case);
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
    let stream = Edits::new(base.len() as u64, edits).expect("valid stream");
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
    // The *base* partition is asserted too, not only the edited one. R43 recorded
    // that this case's sealed base pages are 89 and 90 even though its input is the
    // literal 80-extent-plus-100-extent join: the canonical construction rebuilds
    // the join and repartitions it, so the label describes the input, never the
    // sealed pages. Without this check the two could drift apart silently.
    let base_pages = candidate_pages(&base_store, constructed.root);
    report.insert(
        "base_partition".to_string(),
        base_pages
            .iter()
            .map(|page| format!("{}/{}", page.entries, &page.id[..8]))
            .collect::<Vec<_>>()
            .join(" "),
    );
    report.insert(
        "oracle_base_partition".to_string(),
        oracle
            .base_pages
            .iter()
            .map(|page| format!("{}/{}", page.entries, &page.id[..8]))
            .collect::<Vec<_>>()
            .join(" "),
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
        "repartition-80-100",
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
        if report["base_partition"] != report["oracle_base_partition"] {
            failures.push(format!(
                "{case}: base partition {} != oracle {}",
                report["base_partition"], report["oracle_base_partition"]
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
    for case in ["join-80-100", "repartition-80-100", "untouched-sibling"] {
        let path = oracle_path(case);
        let text = std::fs::read_to_string(&path).expect("fixture");
        assert!(
            text.contains("44cf748486863ab7c21ca47e731bd88e2b9a7b4a"),
            "{case}: fixture does not name the pinned reference revision"
        );
    }
}

/// Builds one oracle case's base and applies its edit, reporting the operation's
/// own node-load count beside the root it emitted.
fn edited_nodes(case: &str) -> (u64, String) {
    let policy = ConstructionPolicy::frozen_default();
    let (base, edits, source) = fixture_inputs(case);
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
    let stream = Edits::new(base.len() as u64, edits).expect("valid stream");
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
    (edited.counters.nodes_read, format!("{}", edited.root))
}

#[test]
fn an_interior_join_reads_its_boundary_child_once() {
    // P1-6: the join's taller side is dismantled and its boundary child is checked
    // under the **non-root** context before the join descends into it. That check
    // stays; what used to follow it was a second read and decode of the same node
    // under the weaker root context, which is what this test pins away.
    //
    // The shape is the sealed oracle's `interior-multi-level` case — a 400-extent
    // base with a 40,000-byte replacement at its middle — because it is the case
    // that actually takes the height-mismatched branch: the append-only and
    // root-collapse cases below join equal heights and never load a boundary child.
    // P1-7 tightened the pin: a mapping page the comparison pass acquired is now
    // memo-served to the construction pass, so this shape's `nodes_read` moved
    // 22 -> 20. The bound says what this test protects — the boundary child is
    // decoded once per join, not twice — while allowing a later item to lower the
    // total again; the exact current value is stated here so a rise is visible.
    let (nodes, root) = edited_nodes("interior-multi-level");
    assert_eq!(nodes, 20, "the current count with the shared page memo");
    assert!(
        nodes <= 22,
        "the boundary child is decoded once per join, not twice: {nodes}"
    );
    assert_eq!(
        root, "57e0a51cc3291c890ba1616d3e1669e19554c38ecaad34d89af5a0f6a25493f0",
        "the emitted root is the pre-change value"
    );

    // The negative controls: shapes whose joins are equal-height. P1-7's shared
    // page memo lowered the first two from 4 to 3 (the comparison pass's page is
    // the one the construction pass would have re-demanded); each row states the
    // current count and the bound the case protects, so a rise is still a failure.
    for (case, now, bound) in [
        ("unequal-height-join", 3_u64, 4_u64),
        ("height-growth", 3, 4),
        ("root-collapse", 4, 4),
        ("join-80-100", 9, 9),
    ] {
        let (nodes, _) = edited_nodes(case);
        assert_eq!(nodes, now, "{case}: the current count");
        assert!(
            nodes <= bound,
            "{case} must not rise above {bound}: {nodes}"
        );
    }
}
