//! Construction timing: database-free real work, on/off equivalence and errors.
//!
//! This target uses only the C1 public API and an in-memory consumer. No
//! database, pack, file or store is opened, so the measured operations are the
//! real complete-file constructor and nothing else.

mod support;

use layerfs_content::{
    construct_bytes, construct_stream, read_all, ConstructionPolicy, ContentError,
};
use support::{noise, patterned, MemoryStore};

fn record<T>(
    name: &'static str,
    body: impl FnOnce(
        &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>,
    ) -> Result<T, ContentError>,
) -> (
    Result<T, ContentError>,
    layerfs_telemetry::timer::TimingReport,
) {
    layerfs_telemetry::timer::Timing::record(name, body)
}

fn child_names(node: &layerfs_telemetry::timer::TimingNode) -> Vec<String> {
    node.children()
        .iter()
        .map(|child| child.name().to_string())
        .collect()
}

#[test]
fn small_construction_reports_its_real_scopes() {
    let bytes = patterned(4_000);
    let (result, report) = record("c1.construct", |root| {
        let mut consumer = MemoryStore::new();
        construct_bytes(
            ConstructionPolicy::frozen_default(),
            &ConstructionPolicy::frozen_default().capacities(),
            &bytes,
            &mut consumer,
            root.child("content"),
        )
    });
    let constructed = result.unwrap();
    assert_eq!(constructed.logical_len, 4_000);
    let tree = report.root().expect("a recorded root");
    assert_eq!(tree.name(), "c1.construct");
    assert!(!tree.outcome().is_error());
    let scopes = child_names(tree);
    assert_eq!(scopes, vec!["content".to_string()]);
    let inner = child_names(&tree.children()[0]);
    assert!(
        inner.contains(&"content.encode".to_string()),
        "scopes: {inner:?}"
    );
    assert!(
        inner.contains(&"content.emit".to_string()),
        "scopes: {inner:?}"
    );
    assert!(!tree.is_incomplete());
}

#[test]
fn chunked_construction_reports_the_chunk_scope() {
    let bytes = noise(131_072 * 2);
    let (result, report) = record("c1.construct", |root| {
        let mut consumer = MemoryStore::new();
        construct_bytes(
            ConstructionPolicy::frozen_default(),
            &ConstructionPolicy::frozen_default().capacities(),
            &bytes,
            &mut consumer,
            root.child("content"),
        )
    });
    result.unwrap();
    let tree = report.root().expect("a recorded root");
    let inner = child_names(&tree.children()[0]);
    assert!(
        inner.contains(&"content.chunk".to_string()),
        "scopes: {inner:?}"
    );
    assert!(
        inner.contains(&"content.emit".to_string()),
        "scopes: {inner:?}"
    );
}

#[test]
fn recording_and_disabled_execution_produce_identical_results() {
    let bytes = noise(131_072 * 3 + 5);
    let policy = ConstructionPolicy::frozen_default();

    let mut recorded = MemoryStore::new();
    let (recorded_result, report) = record("c1.construct", |root| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &bytes,
            &mut recorded,
            root.child("content"),
        )
    });
    let recorded_root = recorded_result.unwrap();
    assert!(report.root().is_some());

    let mut disabled = MemoryStore::new();
    let (disabled_result, disabled_report) =
        layerfs_telemetry::timer::Timing::disabled("c1.construct", |root| {
            construct_bytes(
                policy,
                &policy.capacities(),
                &bytes,
                &mut disabled,
                root.child("content"),
            )
        });
    let disabled_root = disabled_result.unwrap();

    assert_eq!(recorded_root, disabled_root);
    assert_eq!(recorded.order(), disabled.order());
    assert_eq!(recorded.canonical_bytes(), disabled.canonical_bytes());
    assert!(disabled_report.root().is_none());
    assert!(!disabled_report.has_root());
    assert_eq!(disabled_report.node_count(), 0);
}

#[test]
fn a_construction_failure_keeps_its_completed_timing() {
    let bytes = noise(131_072 * 2);
    let source = support::FailingAfter::new(bytes, 131_072 + 1_024);
    let (result, report) = record("c1.construct", |root| {
        let mut consumer = MemoryStore::new();
        construct_stream(
            ConstructionPolicy::frozen_default(),
            &ConstructionPolicy::frozen_default().capacities(),
            source,
            &mut consumer,
            root.child("content"),
        )
    });
    assert_eq!(result.unwrap_err(), ContentError::Io);
    let tree = report.root().expect("a recorded root");
    assert!(tree.outcome().is_error());
    assert_eq!(tree.name(), "c1.construct");
}

#[test]
fn a_read_reports_its_own_scopes_without_storage() {
    let bytes = noise(131_072 * 2);
    let policy = ConstructionPolicy::frozen_default();
    let mut store = MemoryStore::new();
    let constructed = support::disabled_scope(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &bytes,
            &mut store,
            scope.child("content"),
        )
    })
    .unwrap();

    let mut out = Vec::new();
    let (result, report) = record("c1.read", |root| {
        read_all(
            &store,
            constructed.root,
            &mut out,
            root.child("content.read"),
        )
    });
    result.unwrap();
    assert_eq!(out, bytes);
    let tree = report.root().expect("a recorded root");
    assert_eq!(child_names(tree), vec!["content.read".to_string()]);
}

#[test]
fn a_discarding_consumer_still_runs_the_real_constructor() {
    let bytes = noise(131_072 + 9);
    let policy = ConstructionPolicy::frozen_default();
    let mut consumer = layerfs_content::DiscardingConsumer::new();
    let (result, report) = record("c1.construct", |root| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &bytes,
            &mut consumer,
            root.child("content"),
        )
    });
    let constructed = result.unwrap();
    assert_eq!(constructed.logical_len, bytes.len() as u64);
    assert!(consumer.objects() > 0);
    assert!(report.root().is_some());
    assert!(report.node_count() > 1);
}

#[test]
fn repeated_labels_stay_distinct_and_ordered() {
    let policy = ConstructionPolicy::frozen_default();
    let small = patterned(1_000);
    let large = noise(131_072 * 2);
    let (result, report) = record("c1.construct", |root| {
        let mut first = MemoryStore::new();
        let small_root = construct_bytes(
            policy,
            &policy.capacities(),
            &small,
            &mut first,
            root.child("content"),
        )?;
        let mut second = MemoryStore::new();
        let large_root = construct_bytes(
            policy,
            &policy.capacities(),
            &large,
            &mut second,
            root.child("content"),
        )?;
        Ok((small_root, large_root))
    });
    let (small_root, large_root) = result.unwrap();
    assert_ne!(small_root.root, large_root.root);
    let tree = report.root().expect("a recorded root");
    let children = child_names(tree);
    assert_eq!(
        children,
        vec!["content".to_string(), "content".to_string()],
        "repeated labels are two distinct invocations in start order"
    );
    let scopes = child_names(&tree.children()[0]);
    assert!(scopes.contains(&"content.encode".to_string()));
    let scopes = child_names(&tree.children()[1]);
    assert!(scopes.contains(&"content.chunk".to_string()));
}
