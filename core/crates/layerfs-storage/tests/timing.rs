//! Storage timing: supplied objects, real SQLite, on/off equivalence and errors.

mod support;

use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_storage::{StorageError, Store};
use layerfs_telemetry::timer::{Timing, TimingNode, TimingReport};
use support::{construct_file, create_store, noise, open_store, patterned, TempDir};

fn disabled(store: &Store, root: layerfs_content::ObjectId) -> Vec<u8> {
    let (values, _) = Timing::disabled("read", |scope| {
        store.read_batch(&[root], scope.child("read"))
    })
    .0
    .unwrap();
    values.into_iter().next().expect("one value")
}

fn child_names(node: &TimingNode) -> Vec<String> {
    node.children()
        .iter()
        .map(|child| child.name().to_string())
        .collect()
}

fn save_recorded(
    store: &Store,
    objects: Vec<FinalizedObject>,
    name: &'static str,
) -> (
    Result<layerfs_storage::SaveOutcome, StorageError>,
    TimingReport,
) {
    Timing::record(name, |root| {
        let mut operation = store.begin_save(root.child("storage.begin"))?;
        for object in objects {
            operation.accept(object, root.child("storage.accept"))?;
        }
        operation.finish(root.child("storage.finish"))
    })
}

/// R39: a provider's work is a named child of the caller's span, not its duration.
///
/// A content read that resolves mapping pages and payloads through the product
/// bridge records those waves inside the operation tree the caller started. The
/// Store's own connection, ceiling read and decode nest under the navigation wave
/// the reader named, so an integrated row can tell C1's traversal from C2's read
/// instead of charging both to one span.
#[test]
fn a_content_read_attributes_its_mapping_waves_inside_the_callers_span() {
    let dir = TempDir::new("timing_read");
    let path = dir.store_path("timing_read");
    let store = create_store(&path);
    let bytes = noise(1024 * 1024 + 17);
    let (collected, root, _) = construct_file(&bytes);
    save_recorded(&store, collected.finalized(), "c2.save")
        .0
        .expect("save succeeds");
    drop(store);

    let reopened = open_store(&path);
    let provider = layerfs_storage::StoreProvider::new(&reopened);
    let mut out = Vec::new();
    let (result, report) = Timing::record("c1.read", |read| {
        layerfs_content::read_all(&provider, root, &mut out, read.child("content.read"))
    });
    result.expect("read succeeds");
    assert_eq!(out, bytes);

    let tree = report.root().expect("a recorded root");
    assert_eq!(child_names(tree), vec!["content.read".to_string()]);
    let content = &tree.children()[0];
    let inner = child_names(content);
    assert!(inner.contains(&"content.acquire".to_string()), "{inner:?}");
    assert!(inner.contains(&"content.traverse".to_string()), "{inner:?}");
    let traverse = content
        .children()
        .iter()
        .find(|child| child.name() == "content.traverse")
        .expect("the traversal node");
    let waves = child_names(traverse);
    assert!(waves.contains(&"mapping.navigate".to_string()), "{waves:?}");
    assert!(waves.contains(&"mapping.payload".to_string()), "{waves:?}");
    let navigate = traverse
        .children()
        .iter()
        .find(|child| child.name() == "mapping.navigate")
        .expect("a navigation wave");
    let inside = child_names(navigate);
    assert!(
        inside.contains(&"storage.read".to_string()),
        "the Store's own read nests under the wave the reader named: {inside:?}"
    );
}

#[test]
fn an_independent_save_reports_its_real_scopes() {
    let dir = TempDir::new("timing_save");
    let path = dir.store_path("timing_save");
    let store = create_store(&path);
    let bytes = noise(131_072 * 2);
    let (collected, root, _) = construct_file(&bytes);
    let objects = collected.finalized();

    let (result, report) = save_recorded(&store, objects, "c2.save");
    let outcome = result.unwrap();
    assert!(outcome.inserted > 0);
    let tree = report.root().expect("a recorded root");
    assert_eq!(tree.name(), "c2.save");
    assert!(!tree.outcome().is_error());
    let scopes = child_names(tree);
    assert_eq!(scopes.first().map(String::as_str), Some("storage.begin"));
    assert_eq!(scopes.last().map(String::as_str), Some("storage.finish"));
    assert_eq!(
        scopes
            .iter()
            .filter(|name| *name == "storage.accept")
            .count(),
        collected.objects().len(),
        "one accept per supplied object"
    );
    assert!(scopes[1..scopes.len() - 1]
        .iter()
        .all(|name| name == "storage.accept"));
    let inner = child_names(&tree.children()[1]);
    assert!(inner.contains(&"storage.batch".to_string()), "{inner:?}");

    let (read_result, read_report) = Timing::record("c2.read", |read| {
        store.read_batch(&[root], read.child("storage.read"))
    });
    let (values, counters) = read_result.unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(counters.objects, 1);
    let read_tree = read_report.root().expect("a recorded read root");
    assert_eq!(child_names(read_tree), vec!["storage.read".to_string()]);
}

#[test]
fn recording_and_disabled_storage_execution_agree_exactly() {
    let dir = TempDir::new("equivalence");
    let bytes = patterned(80_000);

    let recorded_path = dir.store_path("recorded");
    let recorded_store = create_store(&recorded_path);
    let (collected, root, _) = construct_file(&bytes);
    let objects = collected.finalized();
    let (recorded_result, report) = save_recorded(&recorded_store, objects, "c2.save");
    let recorded_outcome = recorded_result.unwrap();
    assert!(report.root().is_some());

    let disabled_path = dir.store_path("disabled");
    let disabled_store = create_store(&disabled_path);
    let (collected, _second_root, _) = construct_file(&bytes);
    let objects = collected.finalized();
    let (disabled_result, disabled_report) = Timing::disabled("c2.save", |root_scope| {
        let mut operation = disabled_store.begin_save(root_scope.child("storage.begin"))?;
        for object in objects {
            operation.accept(object, root_scope.child("storage.accept"))?;
        }
        operation.finish(root_scope.child("storage.finish"))
    });
    let disabled_outcome = disabled_result.unwrap();
    assert!(!disabled_report.has_root());
    assert_eq!(recorded_outcome.inserted, disabled_outcome.inserted);
    assert_eq!(recorded_outcome.reused, disabled_outcome.reused);
    assert_eq!(
        recorded_outcome.packs_created,
        disabled_outcome.packs_created
    );

    let recorded_bytes = disabled(&recorded_store, root);
    let disabled_bytes = disabled(&disabled_store, root);
    assert_eq!(recorded_bytes, disabled_bytes);
}

#[test]
fn a_failed_save_returns_the_original_error_with_a_recorded_tree() {
    let dir = TempDir::new("recorded_failure");
    let path = dir.store_path("recorded_failure");
    let store = create_store(&path);
    let (collected, _, _) = construct_file(&noise(131_072 * 2));
    let leaf = collected
        .objects()
        .iter()
        .find(|(_, role, _, _)| *role == ObjectRole::ExtentLeaf)
        .map(|(_, role, raw, references)| {
            FinalizedObject::new(*role, raw.clone())
                .unwrap()
                .with_references(references.clone())
        })
        .unwrap();

    let (result, report) = Timing::record("c2.save", |root| {
        let mut operation = store.begin_save(root.child("storage.begin"))?;
        operation.accept(leaf, root.child("storage.accept"))?;
        operation.finish(root.child("storage.finish"))
    });
    let error = result.unwrap_err();
    assert!(
        matches!(error, StorageError::MissingDependency { .. }),
        "{error}"
    );
    let tree = report.root().expect("a recorded root");
    assert_eq!(tree.name(), "c2.save");
    assert_eq!(tree.outcome(), layerfs_telemetry::timer::NodeOutcome::Error);
    assert_eq!(
        child_names(tree),
        vec![
            "storage.begin".to_string(),
            "storage.accept".to_string(),
            "storage.finish".to_string()
        ]
    );
}

#[test]
fn a_bounded_report_stays_inside_the_recorder_limits() {
    let dir = TempDir::new("bounded");
    let path = dir.store_path("bounded");
    let store = create_store(&path);
    let (collected, _, _) = construct_file(&noise(131_072 * 8));
    let objects = collected.finalized();
    let (result, report) = save_recorded(&store, objects, "c2.save");
    result.unwrap();
    assert!(report.node_count() <= layerfs_telemetry::timer::MAX_NODES);
    assert!(report.levels() <= usize::from(layerfs_telemetry::timer::MAX_DEPTH));
    assert!(!report.is_incomplete());
}
