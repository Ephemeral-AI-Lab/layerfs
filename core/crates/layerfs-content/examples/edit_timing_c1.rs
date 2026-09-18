//! Candidate C1 localized-edit timing for the matched pair.
//!
//! Builds the frozen fixture with the candidate constructor, applies the same edit
//! tuple through `apply_edits` with a supplied authenticated provider, and prints the
//! operation's wall time, the resulting root and what the edit demanded and emitted.
//! The reference counterpart
//! (`crates/layerfs-content/examples/rope_edit_timing.rs`) builds the same base
//! bytes, applies the same tuple through `FileMutationBatch` and prints the same
//! fields, so the two arms are comparable as separate processes.
//!
//! Usage: `cargo +1.85.1 run --release --locked -p layerfs-content --example edit_timing_c1`
//!        `… --example edit_timing_c1 -- --case delete|shrink|split`
//!
//! Cases: the no-argument shape is the frozen one (3.3 MB chunked base, 40,000-byte
//! overwrite in the middle; the D27 row). `delete` is the same base with the same
//! range deleted instead of overwritten, and `shrink` deletes everything above
//! 131,000 bytes so the result falls below the 131,072 cutoff: a **chunked base with
//! a whole-file result**, the shape P1-8's ordered cursor is measured on. `split`
//! reaches the same whole-file arm through **three** retained runs — two deletions
//! separated by kept bytes — which is the shape that pays one root-down traversal
//! per retained run unless the assembly keeps one cursor. Every case prints the same
//! fields; the added `case:` line is additive.
//!
//! Cache state: the base is constructed by this process immediately before the timed
//! scope, so no warm-cache credit is claimed for the edit itself.

use std::cell::RefCell;
use std::time::Instant;

use layerfs_telemetry::timer::Timing;

use layerfs_content::file::mapping::{decode_file_state, decode_node_with_context, ExtentNode};
use layerfs_content::{
    apply_edits, construct_bytes, AuthenticatedObjects, ConstructionPolicy, ContentResult, Edit,
    EditRequest, EditStream, FinalizedConsumer, FinalizedObject, ObjectId, Replacements,
};

/// Prepared base objects, served to the edit with demand accounting.
#[derive(Default)]
struct Provider {
    objects: Vec<FinalizedObject>,
    demanded: RefCell<Vec<ObjectId>>,
}

impl Provider {
    fn canonical(&self, id: ObjectId) -> Option<&[u8]> {
        self.objects
            .iter()
            .find(|object| object.id() == id)
            .map(|object| object.canonical())
    }
}

impl FinalizedConsumer for Provider {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.objects.push(object);
        Ok(())
    }
}

impl AuthenticatedObjects for Provider {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demanded.borrow_mut().extend_from_slice(ids);
        ids.iter()
            .map(|id| {
                self.canonical(*id)
                    .map(<[u8]>::to_vec)
                    .ok_or(layerfs_content::ContentError::MissingObject)
            })
            .collect()
    }
}

/// In-memory consumer for what the edit emits.
#[derive(Default)]
struct Collector {
    objects: Vec<FinalizedObject>,
}

impl FinalizedConsumer for Collector {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.objects.push(object);
        Ok(())
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

/// The fixture shapes this vehicle runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Case {
    /// The frozen shape: 40,000 bytes overwritten in the middle of a 3.3 MB base.
    Default,
    /// The same base and range, deleted instead of overwritten: the result stays
    /// chunked (3,260,000 bytes), so the chunked route runs with no replacement.
    Delete,
    /// Deletes everything above 131,000 bytes: the result (131,000 bytes) is below
    /// the 131,072 cutoff, so a chunked base is read and a whole-file object is
    /// assembled. P1-8's positive anchor.
    Shrink,
    /// Two deletions separated by kept bytes: the same whole-file result length as
    /// `shrink` but reached through three retained runs, so an assembly that
    /// re-descends per run pays its mapping pages once per run.
    Split,
}

impl Case {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "default" => Ok(Self::Default),
            "delete" => Ok(Self::Delete),
            "shrink" => Ok(Self::Shrink),
            "split" => Ok(Self::Split),
            other => Err(format!("unsupported case {other}")),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Delete => "delete",
            Self::Shrink => "shrink",
            Self::Split => "split",
        }
    }

    /// Base bytes, the edits, their replacement bytes and the printed edit line.
    fn fixture(self) -> (Vec<u8>, Vec<Edit>, Vec<u8>, String) {
        const BASE: usize = 3_300_000;
        const START: u64 = 1_650_000;
        const END: u64 = START + 40_000;
        match self {
            Self::Default => (
                noise(BASE),
                vec![Edit::overwrite(START, END)],
                noise(40_000),
                format!("replace [{START}, {END})"),
            ),
            Self::Delete => (
                noise(BASE),
                vec![Edit::delete(START, END)],
                Vec::new(),
                format!("delete [{START}, {END})"),
            ),
            Self::Shrink => (
                noise(BASE),
                vec![Edit::delete(131_000, BASE as u64)],
                Vec::new(),
                format!("delete [131000, {BASE})"),
            ),
            Self::Split => split_fixture(),
        }
    }
}

/// The retained-run fixture: three kept runs of a chunked base and a whole-file
/// result.
///
/// The base is long enough for a two-level mapping tree while the kept runs stay
/// short, so the three runs fall on different leaves and the result still falls
/// under the cutoff. The kept ranges are the source of truth; the deletions
/// between them are derived, in current-result coordinates, which is why each
/// range is shifted by what the earlier deletions removed. The result is
/// 120,000 bytes.
fn split_fixture() -> (Vec<u8>, Vec<Edit>, Vec<u8>, String) {
    const BASE: usize = 3_000_000;
    const KEEP: [(u64, u64); 3] = [(0, 40_000), (1_500_000, 1_540_000), (2_960_000, 3_000_000)];
    // Walk the base once: every gap between kept runs is one deletion, already
    // shifted into current-result coordinates by the bytes removed before it.
    let mut edits = Vec::new();
    let mut cursor = 0_u64;
    let mut removed = 0_u64;
    for (start, end) in KEEP.iter().copied().chain([(BASE as u64, BASE as u64)]) {
        if start > cursor {
            edits.push(Edit::delete(cursor - removed, start - removed));
            removed += start - cursor;
        }
        cursor = end;
    }
    (
        noise(BASE),
        edits,
        Vec::new(),
        format!("keep {KEEP:?} of {BASE}; result 120000"),
    )
}

/// Parses `--case <default|delete|shrink|split>`; no argument means the frozen shape.
fn parse_case(args: &[String]) -> Result<Case, String> {
    let mut case = Case::Default;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--case" => {
                index += 1;
                let value = args.get(index).ok_or("--case needs a value")?;
                case = Case::parse(value)?;
            }
            other => return Err(format!("unsupported argument {other}")),
        }
        index += 1;
    }
    Ok(case)
}

fn main() {
    let case = parse_case(&std::env::args().skip(1).collect::<Vec<_>>()).unwrap_or_else(|error| {
        eprintln!("edit_timing_c1: {error}");
        std::process::exit(2);
    });
    let (base, edits, replacement, description) = case.fixture();
    let policy = ConstructionPolicy::frozen_default();
    let mut provider = Provider::default();
    let constructed = Timing::disabled("build", |scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &base,
            &mut provider,
            scope.child("content"),
        )
    })
    .0
    .expect("candidate construction");
    let base_id = constructed.root;
    let canonical = provider.canonical(base_id).expect("base state").to_vec();
    let length = decode_file_state(&canonical).expect("state").logical_len;
    let mapping_pages = pages(&provider, base_id);
    let objects_before = provider.objects.len();

    let mut replacements = Replacements::new();
    if !replacement.is_empty() {
        replacements.push(replacement);
    }
    let stream = EditStream::new(length, edits).expect("valid stream");
    let mut collector = Collector::default();
    let started = Instant::now();
    let edited = Timing::disabled("edit", |scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &provider,
            EditRequest {
                root: base_id,
                edits: &stream,
                source: &replacements,
            },
            &mut collector,
            scope.child("edit"),
        )
    })
    .0
    .expect("candidate edit");
    let elapsed = started.elapsed();

    let demanded = provider.demanded.borrow();
    println!("implementation: candidate stored-tree edit");
    println!("case: {}", case.name());
    println!("base_bytes: {}", base.len());
    println!("logical_length: {length}");
    println!("edit: {description}");
    println!("elapsed_ns: {}", elapsed.as_nanos());
    println!("edited_root: {}", edited.root);
    println!("objects_before: {objects_before}");
    println!("objects_written: {}", collector.objects.len());
    println!(
        "objects_written_bytes: {}",
        collector
            .objects
            .iter()
            .map(|object| object.canonical().len())
            .sum::<usize>()
    );
    println!("nodes_created: {}", collector.objects.len());
    println!("nodes_read: {}", demanded.len());
    // V4 (additive): the operation's own node-load count. `nodes_read` above is
    // the provider-demand count; a load served by a node this operation built is
    // charged here and never demanded, so the two differ and only this one sees
    // the work a split or a rightmost walk does over its own drafts.
    println!("edit_nodes_read: {}", edited.counters.nodes_read);
    println!("mapping_pages: {mapping_pages}");
}

/// Mapping pages of the constructed base, for the record.
fn pages(store: &Provider, root: ObjectId) -> usize {
    let canonical = store.canonical(root).expect("state").to_vec();
    let state = decode_file_state(&canonical).expect("state");
    let mut count = 0;
    let mut level = vec![state.mapping_root];
    let mut is_root = true;
    while !level.is_empty() {
        let mut next = Vec::new();
        for id in &level {
            let node = decode_node_with_context(store.canonical(*id).expect("node"), is_root)
                .expect("page");
            count += 1;
            if let ExtentNode::Branch { children, .. } = node {
                for child in children {
                    next.push(child.child_object_id);
                }
            }
        }
        is_root = false;
        level = next;
    }
    count
}
