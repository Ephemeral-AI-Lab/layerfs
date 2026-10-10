//! Functional checks of the ported Store adapter (`ops::store`), without a row.
//!
//! Nothing here is registered, timed or sampled. These tests exercise the sequence
//! every C2 and pipeline driver now performs through the current public API —
//! create, save, seal, copy, open, presence, read-back and the C1 handoff — and
//! they pin the one decision the port may not get wrong: every Store is
//! Disposable / WAL / `synchronous = OFF`, selected explicitly, never Durable.

use std::path::{Path, PathBuf};

use fs_bench_storage_content::fixture;
use fs_bench_storage_content::ops::store::{
    self, Store, COUNTER_MAPPINGS, UNAVAILABLE_COUNTERS, UNAVAILABLE_TIMER_NODES,
};
use fs_bench_storage_content::workload::edits::{Edits, Parts};
use fs_bench_storage_content::workload::expected;
use fs_bench_storage_content::workload::oracle::{self, Expectation};
use fs_bench_storage_content::workload::providers::TreeStore;
use layerfs_content::{
    apply_edits, construct_bytes, ConstructionPolicy, Edit, EditRequest, ObjectId,
};
use layerfs_persistence::SqlitePersistenceProfile;
use layerfs_storage::{StorageError, StoragePolicy};
use layerfs_telemetry::timer::Timing;

/// A fresh scratch directory under the harness target, owned by one test.
fn scratch(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/store-adapter-checks");
    std::fs::create_dir_all(&root).unwrap();
    let directory = root.join(format!(
        "{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    directory
}

fn sidecars(path: &Path) -> Vec<String> {
    ["-wal", "-shm", "-journal"]
        .iter()
        .filter(|suffix| PathBuf::from(format!("{}{suffix}", path.display())).exists())
        .map(|suffix| (*suffix).to_string())
        .collect()
}

/// One file's canonical objects and the root the constructor reported.
fn objects_of(bytes: &[u8]) -> (TreeStore, ObjectId) {
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let mut objects = TreeStore::new();
    let (file, _) = Timing::disabled("setup.construct", |scope| {
        construct_bytes(
            policy,
            &capacities,
            bytes,
            &mut objects,
            scope.child("content"),
        )
    });
    (objects, file.unwrap().root)
}

fn save(store: &Store, objects: &TreeStore) -> layerfs_storage::WriteOutcome {
    let operation = store.begin_save().unwrap();
    for id in objects.insertion_order() {
        operation
            .accept(objects.cloned_object(*id).unwrap())
            .unwrap();
    }
    operation.finish().unwrap()
}

#[test]
fn every_store_is_disposable_wal_off_and_seals_to_one_file() {
    assert_eq!(store::PROFILE, SqlitePersistenceProfile::Disposable);
    let directory = scratch("profile");
    let path = directory.join("sample.sqlite");
    let created = Store::create(&path, StoragePolicy::frozen_default()).unwrap();
    let profile = created.profile();
    assert_eq!(profile.persistence, SqlitePersistenceProfile::Disposable);
    assert_eq!(profile.journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(profile.synchronous, 0);
    assert_eq!(profile.busy_timeout, 0);
    assert_eq!(created.path(), path.as_path());
    created.seal().unwrap();
    assert_eq!(sidecars(&path), Vec::<String>::new());

    let opened = Store::open(&path).unwrap();
    assert_eq!(
        opened.profile().persistence,
        SqlitePersistenceProfile::Disposable
    );
    assert_eq!(opened.profile().journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(opened.profile().synchronous, 0);
    opened.seal().unwrap();
    assert_eq!(sidecars(&path), Vec::<String>::new());

    // A second creation at the same path is refused, never an overwrite.
    assert!(Store::create(&path, StoragePolicy::frozen_default()).is_err());
    // A Store bound to another history authority is refused at open.
    let mut foreign = store::harness_history();
    foreign.binding_key = b"layerfs/fs-bench-pro-storage-content/other".to_vec();
    assert!(Store::open_with(&path, &foreign).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn saved_objects_survive_seal_byte_copy_and_reopen() {
    let directory = scratch("copy");
    let master = directory.join("base.sqlite");
    let sample = directory.join("sample.sqlite");
    // Above the small-file cutoff, so the file is chunked and has a mapping.
    let bytes = fixture::noise(3 << 20, 0x5eed_0001);
    let (objects, root) = objects_of(&bytes);
    let ids = objects.insertion_order().to_vec();
    assert!(ids.len() > 1);

    let store = Store::create(&master, StoragePolicy::frozen_default()).unwrap();
    let written_before = store.diagnostics().pack_write_bytes;
    let outcome = save(&store, &objects);
    assert_eq!(outcome.inserted, ids.len() as u64);
    assert_eq!(outcome.reused, 0);
    assert!(outcome.packs > 0);
    assert!(store.diagnostics().pack_write_bytes > written_before);
    store.seal().unwrap();
    assert_eq!(sidecars(&master), Vec::<String>::new());

    // The per-sample acquisition: one closed file, copied byte for byte.
    std::fs::write(&sample, std::fs::read(&master).unwrap()).unwrap();
    let store = Store::open(&sample).unwrap();

    // Presence, paged at the Store's declared read ceiling.
    let distinct = expected::distinct(&ids);
    let present = expected::present_all(&store, &distinct).unwrap();
    assert_eq!(expected::distinct(&present), distinct);
    let absent = ObjectId::for_bytes(b"an identity this Store was never offered");
    assert_eq!(store.contains(&[absent]).unwrap(), Vec::<ObjectId>::new());

    // Read-back through the product's authenticated reader.
    let expectation = Expectation::of(&bytes);
    {
        let provider = store.reader().unwrap();
        let (back, _) = Timing::disabled("oracle.readback", |scope| {
            oracle::read_back(&provider, root, &expectation, scope.child("content"))
        });
        assert!(back.unwrap().matches());
    }

    // An exact-hit save reuses every identity and registers no pack.
    let again = save(&store, &objects);
    assert_eq!(again.reused, ids.len() as u64);
    assert_eq!(again.inserted, 0);
    assert_eq!(again.packs, 0);

    store.seal().unwrap();
    assert_eq!(sidecars(&sample), Vec::<String>::new());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn second_begin_is_refused_and_dropping_a_save_is_the_abort() {
    let directory = scratch("ownership");
    let path = directory.join("sample.sqlite");
    let store = Store::create(&path, StoragePolicy::frozen_default()).unwrap();
    let first = store.begin_save().unwrap();
    assert_eq!(first.pending_canonical_bytes(), 0);
    // One save per handle. The refusal's class is reported, not assumed: the
    // lifecycle row's `g2.second-begin-refused` gate still names the removed
    // engine's `OwnershipUnavailable`, and it is the gate, not this test, that
    // decides what that difference means.
    let refusal = match store.begin_save() {
        Ok(_) => panic!("a second begin_save on one handle was acquired"),
        Err(error) => error,
    };
    assert!(
        matches!(refusal, StorageError::Integrity(_)),
        "unexpected refusal class: {refusal:?}"
    );
    // There is no `abort` call: the drop releases the handle for the next save.
    drop(first);
    let third = store.begin_save().unwrap();
    assert_eq!(third.finish().unwrap().inserted, 0);
    store.seal().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn an_edit_crosses_the_save_sink_and_reads_back_through_the_store() {
    let directory = scratch("handoff");
    let master = directory.join("base.sqlite");
    let policy = ConstructionPolicy::frozen_default();
    let capacities = policy.capacities();
    let bytes = fixture::noise(2 << 20, 0x5eed_0002);
    let (base, base_root) = objects_of(&bytes);
    let store = Store::create(&master, StoragePolicy::frozen_default()).unwrap();
    save(&store, &base);

    let (start, end) = (1 << 20, (1 << 20) + 4_096);
    let replacement = fixture::noise(4_096, 0x5eed_0003);
    let stream = Edits::new(
        bytes.len() as u64,
        vec![Edit::new(start, end, replacement.len() as u64)],
    )
    .unwrap();
    let mut source = Parts::new();
    source.push(replacement.clone());
    let expectation = Expectation::spliced(&bytes, start, end, &replacement);

    // The base is read through the Store's reader and the result crosses the
    // product's own C1-to-save adapter, exactly as the pipeline edit rows do.
    let provider = store.reader().unwrap();
    let operation = store.begin_save().unwrap();
    let file = {
        let mut handoff = operation.sink();
        let request = EditRequest {
            root: base_root,
            edits: &stream,
            source: &source,
        };
        let (file, _) = Timing::disabled("edit", |scope| {
            apply_edits(
                policy,
                &capacities,
                &provider,
                request,
                &mut handoff,
                scope.child("edit"),
            )
        });
        file.unwrap()
    };
    assert!(operation.take_failure().is_none());
    let outcome = operation.finish().unwrap();
    assert!(outcome.inserted > 0);
    assert_eq!(file.logical_len, expectation.logical_len);
    drop(provider);
    store.seal().unwrap();

    let store = Store::open(&master).unwrap();
    {
        let provider = store.reader().unwrap();
        let (back, _) = Timing::disabled("oracle.readback", |scope| {
            oracle::read_back(&provider, file.root, &expectation, scope.child("content"))
        });
        assert!(back.unwrap().matches());
    }
    store.seal().unwrap();
    assert_eq!(sidecars(&master), Vec::<String>::new());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn the_port_tables_are_stated_and_do_not_overlap() {
    assert!(!COUNTER_MAPPINGS.is_empty());
    assert!(!UNAVAILABLE_COUNTERS.is_empty());
    assert!(!UNAVAILABLE_TIMER_NODES.is_empty());
    for row in UNAVAILABLE_COUNTERS {
        assert!(!row.names.is_empty() && !row.removed.is_empty() && !row.reason.is_empty());
    }
    for row in COUNTER_MAPPINGS {
        assert!(!row.names.is_empty() && !row.removed.is_empty() && !row.current.is_empty());
    }
    // A removed source is either mapped or unavailable, never both.
    for mapped in COUNTER_MAPPINGS {
        assert!(UNAVAILABLE_COUNTERS
            .iter()
            .all(|unavailable| unavailable.removed != mapped.removed));
    }
    // The four counters the lead's decision names by name are in the list.
    let mut all = String::new();
    for row in UNAVAILABLE_COUNTERS {
        all.push_str(row.names);
        all.push(' ');
        all.push_str(row.removed);
        all.push('\n');
    }
    for name in [
        "pack_appends",
        "presence_queries",
        "connection_opens",
        "pool_index_entries",
        "SaveProfile",
    ] {
        assert!(all.contains(name), "{name} is not in UNAVAILABLE_COUNTERS");
    }
    assert!(store::unavailable_note(&["x.y"]).contains(store::UNAVAILABLE));
    assert!(store::profile_note().contains(store::PROFILE_TOKEN));
}
