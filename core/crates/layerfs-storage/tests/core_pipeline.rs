//! The integrated pipeline: real C1 construction handed to real C2 storage.
//!
//! Nothing here mocks storage: the same `SaveHandoff` adapter that any integrated
//! caller uses feeds the real save operation, and readback goes through the real
//! independent read path.

mod support;

use layerfs_content::{
    construct_bytes, construct_stream, read_all, ConstructionPolicy, ContentError, ObjectId,
};
use layerfs_storage::{SaveHandoff, StorageError, Store};
use support::{create_store, disabled, noise, open_store, patterned, repeat, TempDir};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileResult {
    root: ObjectId,
    logical_len: u64,
}

fn pipeline(store: &Store, bytes: &[u8]) -> Result<FileResult, StorageError> {
    let policy = ConstructionPolicy::frozen_default();
    disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        let mut handoff = SaveHandoff::new(&mut operation);
        let constructed = construct_stream(
            policy,
            &policy.capacities(),
            bytes,
            &mut handoff,
            scope.child("content"),
        );
        if let Some(failure) = handoff.take_failure() {
            return Err(failure);
        }
        let constructed = constructed?;
        operation.finish(scope.child("storage.finish"))?;
        Ok(FileResult {
            root: constructed.root,
            logical_len: constructed.logical_len,
        })
    })
}

fn read_back(store: &Store, root: ObjectId) -> Vec<u8> {
    let (values, _) = disabled(|scope| store.read_batch(&[root], scope.child("storage.read")))
        .expect("root read");
    let canonical = values.into_iter().next().expect("one root");
    let mut out = Vec::new();
    disabled(|scope| {
        read_all(
            &StoreProvider::new(store),
            root,
            &mut out,
            scope.child("content.read"),
        )
    })
    .expect("logical read");
    let _ = canonical;
    out
}

/// Reads intermediate objects through the product Store bridge.
use layerfs_storage::StoreProvider;

#[test]
fn a_streamed_file_survives_construction_storage_and_readback() {
    let dir = TempDir::new("pipeline");
    let path = dir.store_path("pipeline");
    let store = create_store(&path);
    for bytes in [
        Vec::new(),
        vec![0x5a],
        patterned(70_000),
        noise(131_072 * 3 + 7),
        repeat(131_072 * 2, 0x21),
    ] {
        let result = pipeline(&store, &bytes).expect("pipeline succeeds");
        assert_eq!(result.logical_len, bytes.len() as u64);
        assert_eq!(read_back(&store, result.root), bytes);
    }
}

#[test]
fn many_distinct_compressible_records_share_groups_and_read_back_identically() {
    let dir = TempDir::new("compressible");
    let path = dir.store_path("compressible");
    let store = create_store(&path);
    // Distinct chunks that each compress to a few dozen bytes: a group is bounded by
    // its framed bytes, so many records share one group. Every chunk must still read
    // back as its own bytes, and the whole file must survive construction, storage
    // and an independent read.
    let mut bytes = repeat(4 * 1024 * 1024, 0x00);
    for (index, stamp) in bytes.chunks_mut(64 * 1024).enumerate() {
        stamp[..4].copy_from_slice(&(index as u32).to_be_bytes());
    }
    let result = pipeline(&store, &bytes).expect("pipeline succeeds");
    assert_eq!(result.logical_len, bytes.len() as u64);
    assert_eq!(read_back(&store, result.root), bytes);

    // The same file again: every identity is already stored, so it is reused.
    let repeated = pipeline(&store, &bytes).expect("second pipeline succeeds");
    assert_eq!(repeated.root, result.root);
}

#[test]
fn integrated_and_independent_runs_produce_the_same_roots() {
    let dir = TempDir::new("equivalence");
    let store = create_store(&dir.store_path("equivalence"));
    let bytes = noise(131_072 * 2 + 33);

    // Integrated: real C1 construction feeding real C2 storage.
    let integrated = pipeline(&store, &bytes).expect("integrated run");

    // Independent C1-only run over the same stable input.
    let mut consumer = support::Collected::new();
    let policy = ConstructionPolicy::frozen_default();
    let constructed = disabled(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            &bytes,
            &mut consumer,
            scope.child("content"),
        )
    })
    .unwrap();
    assert_eq!(integrated.root, constructed.root);
    assert_eq!(integrated.logical_len, constructed.logical_len);

    // Independent C2-only run of the very same supplied objects.
    let independent_store = create_store(&dir.store_path("independent"));
    let outcome = disabled(|scope| {
        let mut operation = independent_store.begin_save(scope.child("storage.begin"))?;
        for object in consumer.finalized() {
            operation.accept(object, scope.child("storage.accept"))?;
        }
        operation.finish(scope.child("storage.finish"))
    })
    .unwrap();
    assert_eq!(outcome.inserted, consumer.objects().len() as u64);

    let from_integrated = read_back(&store, integrated.root);
    let from_independent = {
        let mut out = Vec::new();
        disabled(|scope| {
            read_all(
                &StoreProvider::new(&independent_store),
                integrated.root,
                &mut out,
                scope.child("content.read"),
            )
        })
        .unwrap();
        out
    };
    assert_eq!(from_integrated, bytes);
    assert_eq!(from_independent, bytes);
}

#[test]
fn many_files_stream_through_one_save_with_bounded_batches() {
    let dir = TempDir::new("many");
    let store = create_store(&dir.store_path("many"));
    let policy = ConstructionPolicy::frozen_default();
    let limits = store.capacities();

    let (results, outcome) = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        let mut results = Vec::new();
        for index in 0..16u8 {
            let body = repeat(70_000 + usize::from(index) * 37, index);
            let mut handoff = SaveHandoff::new(&mut operation);
            let constructed = construct_stream(
                policy,
                &policy.capacities(),
                body.as_slice(),
                &mut handoff,
                scope.child("content"),
            );
            if let Some(failure) = handoff.take_failure() {
                return Err(failure);
            }
            let constructed = constructed?;
            results.push(FileResult {
                root: constructed.root,
                logical_len: constructed.logical_len,
            });
            let (objects, bytes) = operation.pending();
            assert!(objects <= limits.batch_objects);
            assert!(bytes <= limits.batch_bytes);
            assert!(operation.retained_tail_bytes()? <= 3 * limits.pack_limit);
        }
        let outcome = operation.finish(scope.child("storage.finish"))?;
        Ok((results, outcome))
    })
    .unwrap();

    assert_eq!(results.len(), 16);
    assert!(outcome.inserted >= 16);
    for (index, result) in results.iter().enumerate() {
        let expected = repeat(70_000 + index * 37, index as u8);
        assert_eq!(result.logical_len, expected.len() as u64);
        assert_eq!(read_back(&store, result.root), expected);
    }
}

#[test]
fn a_pipeline_input_failure_keeps_the_original_error_and_cleans_up() {
    let dir = TempDir::new("pipeline_failure");
    let path = dir.store_path("pipeline_failure");
    let store = create_store(&path);
    let retained = patterned(1_500);
    let retained_result = pipeline(&store, &retained).expect("retained file");
    let before = std::fs::metadata(&path).unwrap().len();

    let body = noise(131_072 * 4);
    let failing = support::FailingAfter::new(body, 131_072 * 3 + 100);
    let policy = ConstructionPolicy::frozen_default();
    let error = disabled(|scope| {
        let mut operation = store.begin_save(scope.child("storage.begin"))?;
        let mut handoff = SaveHandoff::new(&mut operation);
        let constructed = construct_stream(
            policy,
            &policy.capacities(),
            failing,
            &mut handoff,
            scope.child("content"),
        );
        if let Some(failure) = handoff.take_failure() {
            return Err(failure);
        }
        constructed?;
        operation.finish(scope.child("storage.finish"))?;
        Ok(())
    })
    .unwrap_err();
    assert!(
        matches!(error, StorageError::Content(ContentError::Io)),
        "the input failure is reported once: {error}"
    );

    let reopened = open_store(&path);
    assert_eq!(read_back(&reopened, retained_result.root), retained);
    let after = std::fs::metadata(&path).unwrap().len();
    assert!(
        after <= before + 4_096,
        "the failed attempt added no retained pack: {before} -> {after}"
    );
}

/// A file larger than 5 MiB survives the whole real pipeline, including a reopen.
///
/// This is the C2 side of the size boundary: 6 MiB + 1 is about 200 chunk payloads
/// spread over several packs and preparation waves, and every byte of it is
/// constructed, stored and read back through a real `Store` and a fresh handle on
/// the same file - no mock, no in-memory shortcut. The C1 side of the same
/// boundary (24 MiB + 1 in one logical read) is
/// `file_read::a_large_read_acquires_payloads_in_bounded_batches`.
#[test]
fn a_file_larger_than_five_mebibytes_survives_a_real_store() {
    let dir = TempDir::new("large-pipeline");
    let path = dir.store_path("large");
    let store = create_store(&path);
    let bytes = noise(6 * 1024 * 1024 + 1);
    let result = pipeline(&store, &bytes).expect("large pipeline succeeds");
    assert_eq!(result.logical_len, bytes.len() as u64);
    assert_eq!(read_back(&store, result.root), bytes);

    drop(store);
    let reopened = open_store(&path);
    let (values, counters) =
        disabled(|scope| reopened.read_batch(&[result.root], scope.child("storage.read")))
            .expect("root read after reopen");
    assert_eq!(values.len(), 1);
    // The root read is one object: the payloads are demanded by the logical read,
    // not by this call. The publication ceiling it captured shows the file spans
    // many packs rather than one.
    assert_eq!(counters.objects, 1);
    println!("MEASURED 6 MiB root read: {counters:?}");
    assert!(
        counters.ceiling > 1,
        "a file this size must span several packs: {counters:?}"
    );
    assert_eq!(read_back(&reopened, result.root), bytes);
}
