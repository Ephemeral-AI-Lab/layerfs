//! Real database-free edit timing: the production scopes, on/off equivalence.
//!
//! The measured operation is the real known-edit path with a supplied bounded
//! authenticated provider and a non-persisting consumer. No store, pack, database
//! or file is opened. Recording enabled and disabled must perform the same work
//! and return the same result, and a failing edit must return its original error
//! unchanged.

mod support;

use layerfs_content::{
    apply_edits, construct_bytes, ConstructionPolicy, ContentError, Edit, EditRequest, EditStream,
    ObjectId, Replacements,
};
use layerfs_telemetry::timer::{Timing, TimingNode, TimingReport};
use support::{noise, patterned, read_back, Counted, CountingProvider, MemoryStore};

fn policy() -> ConstructionPolicy {
    ConstructionPolicy::frozen_default()
}

fn build(bytes: &[u8]) -> (MemoryStore, ObjectId) {
    let mut store = MemoryStore::new();
    let constructed = Timing::disabled("setup", |scope| {
        construct_bytes(
            policy(),
            &policy().capacities(),
            bytes,
            &mut store,
            scope.child("content"),
        )
    })
    .0
    .expect("construction");
    (store, constructed.root)
}

fn child_names(node: &TimingNode) -> Vec<String> {
    node.children()
        .iter()
        .map(|child| child.name().to_string())
        .collect()
}

fn run_edit(
    store: &MemoryStore,
    root: ObjectId,
    stream: &EditStream,
    replacements: &Replacements,
    counts: &CountingProvider,
) -> (Result<ObjectId, ContentError>, TimingReport) {
    let provider = Counted { store, counts };
    let (outcome, report) = Timing::record("file.edit", |edit| {
        let mut result = MemoryStore::new();
        let constructed = apply_edits(
            policy(),
            &policy().capacities(),
            &provider,
            EditRequest {
                root,
                edits: stream,
                source: replacements,
            },
            &mut result,
            edit.child("edit.op"),
        )?;
        Ok(constructed.root)
    });
    (outcome, report)
}

#[test]
fn a_real_edit_reports_its_scopes_without_a_database() {
    let base = noise(400_000);
    let (store, root) = build(&base);
    let mut replacements = Replacements::new();
    replacements.push(noise(30_000));
    let stream =
        EditStream::new(base.len() as u64, vec![Edit::insert(120_000, 30_000)]).expect("valid");
    let counts = CountingProvider::new();
    let (result, report) = run_edit(&store, root, &stream, &replacements, &counts);
    let edited = result.expect("edit succeeds");
    assert_ne!(edited, root);
    let tree = report.root().expect("recorded root");
    assert_eq!(tree.name(), "file.edit");
    assert!(!tree.outcome().is_error());
    let top = child_names(tree);
    assert_eq!(top, vec!["edit.op".to_string()], "scopes: {top:?}");
    let inner = child_names(&tree.children()[0]);
    assert!(
        inner.contains(&"edit.base".to_string()),
        "scopes: {inner:?}"
    );
    assert!(
        inner.contains(&"edit.stream".to_string()),
        "the retained-extent pass is timed: {inner:?}"
    );
    assert!(
        inner.contains(&"edit.finish".to_string()),
        "scopes: {inner:?}"
    );
    assert!(
        counts.batch_calls() >= 1,
        "the base was acquired through the provider"
    );
}

#[test]
fn an_equal_replacement_reports_the_comparison_scope() {
    let base = patterned(5_000);
    let (store, root) = build(&base);
    let mut replacements = Replacements::new();
    replacements.push(base[1_000..2_000].to_vec());
    let stream =
        EditStream::new(base.len() as u64, vec![Edit::overwrite(1_000, 2_000)]).expect("valid");
    let counts = CountingProvider::new();
    let (result, report) = run_edit(&store, root, &stream, &replacements, &counts);
    assert_eq!(result.expect("edit"), root);
    let inner = child_names(&tree_of(&report));
    assert!(
        inner.contains(&"edit.compare".to_string()),
        "scopes: {inner:?}"
    );
    assert!(
        !inner.contains(&"content.chunk".to_string()),
        "no construction scope for a no-op: {inner:?}"
    );
}

fn tree_of(report: &TimingReport) -> TimingNode {
    report.root().expect("recorded root").children()[0].clone()
}

#[test]
fn recording_enabled_and_disabled_produce_the_same_result() {
    let base = noise(300_000);
    let (store, root) = build(&base);
    let mut replacements = Replacements::new();
    replacements.push(noise(500));
    let stream =
        EditStream::new(base.len() as u64, vec![Edit::overwrite(50_000, 50_500)]).expect("valid");

    let counts = CountingProvider::new();
    let (recorded, _) = run_edit(&store, root, &stream, &replacements, &counts);

    let disabled_counts = CountingProvider::new();
    let provider = Counted {
        store: &store,
        counts: &disabled_counts,
    };
    let mut plain_store = store.merged_clone();
    let (plain, plain_report): (Result<ObjectId, ContentError>, TimingReport) =
        Timing::disabled("file.edit", |edit| {
            let constructed = apply_edits(
                policy(),
                &policy().capacities(),
                &provider,
                EditRequest {
                    root,
                    edits: &stream,
                    source: &replacements,
                },
                &mut plain_store,
                edit.child("edit.op"),
            )?;
            Ok(constructed.root)
        });
    let plain_root = plain.expect("edit succeeds");
    assert_eq!(
        recorded.expect("recorded"),
        plain_root,
        "recording state must not change the result"
    );
    assert!(
        plain_report.root().is_none(),
        "disabled recording has no tree"
    );
    let bytes = read_back(&plain_store, plain_root).expect("reads back");
    assert_eq!(bytes.len() as u64, stream.final_len());
    assert_eq!(
        counts.objects(),
        disabled_counts.objects(),
        "the same reads happen with recording on and off"
    );
}

#[test]
fn a_failing_edit_returns_its_original_error_and_an_error_outcome() {
    let base = noise(200_000);
    let (store, root) = build(&base);
    let replacements = Replacements::new();
    let stream = EditStream::new(base.len() as u64, vec![Edit::insert(1_000, 10)]).expect("valid");
    let counts = CountingProvider::new();
    let (result, report) = run_edit(&store, root, &stream, &replacements, &counts);
    let error = result.unwrap_err();
    assert!(
        matches!(error, ContentError::InvalidEdit { .. }),
        "got {error}"
    );
    let tree = report.root().expect("root node still recorded");
    assert!(tree.outcome().is_error(), "the failing node is marked");
}
