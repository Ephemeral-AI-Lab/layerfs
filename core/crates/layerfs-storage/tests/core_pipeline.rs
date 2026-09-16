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
            &StoreProvider { store, root },
            root,
            &mut out,
            scope.child("content.read"),
        )
    })
    .expect("logical read");
    let _ = canonical;
    out
}

/// Reads intermediate objects through a single independent Store connection.
struct StoreProvider<'a> {
    store: &'a Store,
    root: ObjectId,
}

impl layerfs_content::AuthenticatedObjects for StoreProvider<'_> {
    fn read_canonical_batch(
        &self,
        ids: &[ObjectId],
    ) -> layerfs_content::ContentResult<Vec<Vec<u8>>> {
        let (values, _) = disabled(|scope| self.store.read_batch(ids, scope.child("storage.read")))
            .map_err(|_| ContentError::MissingObject)?;
        let _ = self.root;
        Ok(values)
    }
}

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
                &StoreProvider {
                    store: &independent_store,
                    root: integrated.root,
                },
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
