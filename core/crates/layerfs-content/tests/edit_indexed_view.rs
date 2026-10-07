//! Fallible indexed replacements over a retained authenticated public FileView.
mod support;
use layerfs_content::file::{
    apply_indexed_edits_view_backed, IndexedEditRequest, IndexedEditSource,
};
use layerfs_content::{
    apply_edits, apply_edits_backed, AuthenticatedObjects, ConstructedFile, ConstructionCapacities,
    ConstructionPolicy, ContentError, ContentResult, Edit, EditRecordApply, EditRecordChange,
    EditRecordKey, EditRequest, EditSequence, EditSource, FileRun, FileView, FinalizedConsumer,
    FinalizedObject, IndexedEditBacking, ObjectId, ObjectRole,
};
use std::cell::{Cell, RefCell};
use support::edit_runs::{Backing, Counted, Piece, Sparse};
use support::{build_file, disabled_scope, edits::Edits, noise, read_back, MemoryStore};

struct Indexed {
    sparse: Sparse,
    lengths: RefCell<Vec<usize>>,
    reads: RefCell<Vec<(usize, u64, usize)>>,
    length_error: RefCell<Option<ContentError>>,
    read_error: RefCell<Option<ContentError>>,
    length_fail_at: usize,
    fail_at: u64,
    failed: Cell<bool>,
}
impl Indexed {
    fn new(parts: Vec<Vec<Piece>>) -> Self {
        Self {
            sparse: Sparse {
                parts,
                ..Sparse::default()
            },
            lengths: RefCell::new(Vec::new()),
            reads: RefCell::new(Vec::new()),
            length_error: RefCell::new(None),
            read_error: RefCell::new(None),
            length_fail_at: 1,
            fail_at: 0,
            failed: Cell::new(false),
        }
    }
}
impl IndexedEditSource for Indexed {
    fn replacement_len(&self, index: usize) -> ContentResult<u64> {
        assert!(!self.failed.get(), "length replay after original failure");
        self.lengths.borrow_mut().push(index);
        if self.lengths.borrow().len() == self.length_fail_at {
            if let Some(original) = self.length_error.borrow_mut().take() {
                self.failed.set(true);
                return Err(original);
            }
        }
        Ok(self.sparse.replacement_len(index))
    }
    fn read_run_at(&self, index: usize, offset: u64, output: &mut [u8]) -> ContentResult<FileRun> {
        assert!(!self.failed.get(), "read replay after original failure");
        self.reads.borrow_mut().push((index, offset, output.len()));
        if offset >= self.fail_at {
            if let Some(original) = self.read_error.borrow_mut().take() {
                self.failed.set(true);
                return Err(original);
            }
        }
        self.sparse.read_run_at(index, offset, output)
    }
}

fn open(reader: &dyn AuthenticatedObjects, root: ObjectId) -> FileView {
    disabled_scope(|scope| FileView::open(reader, root, scope.child("retained-view"))).unwrap()
}
fn run(
    reader: &dyn AuthenticatedObjects,
    view: &FileView,
    edits: &dyn EditSequence,
    source: &dyn IndexedEditSource,
    consumer: &mut dyn FinalizedConsumer,
    backing: &mut dyn IndexedEditBacking,
) -> ContentResult<ConstructedFile> {
    let policy = ConstructionPolicy::frozen_default();
    configured(
        policy,
        &policy.capacities(),
        reader,
        view,
        edits,
        source,
        consumer,
        backing,
    )
}
#[allow(clippy::too_many_arguments)]
fn configured(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    reader: &dyn AuthenticatedObjects,
    view: &FileView,
    edits: &dyn EditSequence,
    source: &dyn IndexedEditSource,
    consumer: &mut dyn FinalizedConsumer,
    backing: &mut dyn IndexedEditBacking,
) -> ContentResult<ConstructedFile> {
    disabled_scope(|scope| {
        apply_indexed_edits_view_backed(
            policy,
            capacities,
            reader,
            IndexedEditRequest {
                view,
                edits,
                source,
            },
            consumer,
            backing,
            scope.child("indexed-edit"),
        )
    })
}
fn legacy(
    store: &MemoryStore,
    root: ObjectId,
    edits: &Edits,
    source: &dyn EditSource,
    backed: bool,
) -> ConstructedFile {
    let policy = ConstructionPolicy::frozen_default();
    let mut output = MemoryStore::new();
    let mut backing = Backing::default();
    disabled_scope(|scope| {
        let request = EditRequest {
            root,
            edits,
            source,
        };
        if backed {
            apply_edits_backed(
                policy,
                &policy.capacities(),
                store,
                request,
                &mut output,
                &mut backing,
                scope.child("legacy"),
            )
        } else {
            apply_edits(
                policy,
                &policy.capacities(),
                store,
                request,
                &mut output,
                scope.child("legacy"),
            )
        }
    })
    .unwrap()
}

#[test]
fn retained_view_and_fallible_runs_keep_memory_backed_and_transition_roots() {
    for (base_len, start, end, replacement) in [
        (31_337, 11, 29, 211_117),
        (2 * 1024 * 1024 + 19, 11, 29, 211_117),
        (512 * 1024, 0, 512 * 1024, 31),
        (31, 0, 31, 0),
        (0, 0, 0, 31),
    ] {
        let base = noise(base_len);
        let (store, built) = build_file(&base);
        let source = Indexed::new(vec![vec![Piece::Zero(replacement)]]);
        let edits = Edits::new(base_len as u64, vec![Edit::new(start, end, replacement)]).unwrap();
        let expected = legacy(&store, built.root, &edits, &source.sparse, false);
        assert_eq!(
            legacy(&store, built.root, &edits, &source.sparse, true).root,
            expected.root
        );
        let reader = Counted::new(&store);
        let view = open(&reader, built.root);
        // A reacquisition would receive this original refusal. The view still
        // supplies the previously authenticated classification and root bytes.
        *reader.deny.borrow_mut() = Some((
            built.root,
            ContentError::ProviderFailure {
                what: "root must not be reacquired",
            },
        ));
        let mut output = MemoryStore::new();
        let mut backing = Backing::default();
        let actual = run(&reader, &view, &edits, &source, &mut output, &mut backing).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            reader
                .requests
                .borrow()
                .iter()
                .filter(|id| **id == built.root)
                .count(),
            1
        );
        assert_eq!(reader.denied.get(), 0);
        assert_eq!(view.root(), built.root);
        assert_eq!(view.logical_len(), base_len as u64);
        let mut merged = store.merged_clone();
        merged.absorb(&output);
        let mut bytes = base[..start as usize].to_vec();
        bytes.resize(bytes.len() + replacement as usize, 0);
        bytes.extend_from_slice(&base[end as usize..]);
        assert_eq!(read_back(&merged, actual.root).unwrap(), bytes);
        if actual.logical_len < 131_072 {
            assert_eq!(backing.calls, 0);
        } else if base_len >= 131_072 {
            assert!(backing.calls > 0);
            assert_eq!(
                backing.values[&EditRecordKey {
                    kind: 10,
                    key: [0; 32]
                }],
                vec![1]
            );
        } else {
            assert_eq!(backing.calls, 0);
        }
    }
}

#[test]
fn original_indexed_length_refusals_stop_before_reads_backing_or_output() {
    for base_len in [31_337, 512 * 1024] {
        let (store, built) = build_file(&noise(base_len));
        for edit in [
            Edit::overwrite(11, 29),
            Edit::insert(11, 18),
            Edit::delete(0, base_len as u64),
        ] {
            let reader = Counted::new(&store);
            let view = open(&reader, built.root);
            let original = ContentError::BoundedCapacityExceeded {
                what: "original indexed length refusal",
                limit: 97,
                actual: 101,
            };
            let source = Indexed::new(vec![vec![Piece::Zero(edit.replacement_len())]]);
            *source.length_error.borrow_mut() = Some(original.clone());
            let edits = Edits::new(base_len as u64, vec![edit]).unwrap();
            let mut output = MemoryStore::new();
            let mut backing = Backing {
                forbidden: true,
                ..Backing::default()
            };
            assert_eq!(
                run(&reader, &view, &edits, &source, &mut output, &mut backing).unwrap_err(),
                original
            );
            assert_eq!(*source.lengths.borrow(), vec![0]);
            assert!(source.reads.borrow().is_empty());
            assert_eq!(*reader.requests.borrow(), vec![built.root]);
            assert_eq!(backing.calls, 0);
            assert!(output.is_empty());
            assert!(source.length_error.borrow().is_none());
        }
    }
    // A successful comparison metadata wave does not make a later original
    // metadata failure infallible. No split/backing/provider work precedes this
    // second planned request for a length-changing chunked edit.
    let (store, built) = build_file(&noise(512 * 1024));
    let reader = Counted::new(&store);
    let view = open(&reader, built.root);
    let mut source = Indexed::new(vec![vec![Piece::Zero(18)]]);
    source.length_fail_at = 2;
    let original = ContentError::ProviderFailure {
        what: "original construction length refusal",
    };
    *source.length_error.borrow_mut() = Some(original.clone());
    let edits = Edits::new(view.logical_len(), vec![Edit::insert(11, 18)]).unwrap();
    let mut output = MemoryStore::new();
    let mut backing = Backing {
        forbidden: true,
        ..Backing::default()
    };
    assert_eq!(
        run(&reader, &view, &edits, &source, &mut output, &mut backing).unwrap_err(),
        original
    );
    assert_eq!(*source.lengths.borrow(), vec![0, 0]);
    assert!(source.reads.borrow().is_empty());
    assert_eq!(*reader.requests.borrow(), vec![built.root]);
    assert_eq!(backing.calls, 0);
    assert!(output.is_empty());
}

#[test]
fn original_indexed_comparison_read_refusal_is_not_flattened_or_replayed() {
    for base_len in [31_337, 512 * 1024] {
        let (store, built) = build_file(&noise(base_len));
        let reader = Counted::new(&store);
        let view = open(&reader, built.root);
        let original = ContentError::ProviderFailure {
            what: "original indexed read refusal",
        };
        let source = Indexed::new(vec![vec![Piece::Zero(18)]]);
        *source.read_error.borrow_mut() = Some(original.clone());
        let edits = Edits::new(base_len as u64, vec![Edit::overwrite(11, 29)]).unwrap();
        let mut output = MemoryStore::new();
        let mut backing = Backing {
            forbidden: true,
            ..Backing::default()
        };
        assert_eq!(
            run(&reader, &view, &edits, &source, &mut output, &mut backing).unwrap_err(),
            original
        );
        assert_eq!(*source.lengths.borrow(), vec![0]);
        assert_eq!(*source.reads.borrow(), vec![(0, 0, 18)]);
        assert_eq!(*reader.requests.borrow(), vec![built.root]);
        assert_eq!(backing.calls, 0);
        assert!(output.is_empty());
        assert!(source.read_error.borrow().is_none());
    }
}

#[test]
fn retained_view_no_ops_precede_occupied_backing_and_empty_edits_request_no_source() {
    for base_len in [31, 512 * 1024] {
        let (store, built) = build_file(&vec![0; base_len]);
        let reader = Counted::new(&store);
        let view = open(&reader, built.root);
        let source = Indexed::new(vec![vec![Piece::Zero(base_len as u64)]]);
        let edits = Edits::new(base_len as u64, vec![Edit::overwrite(0, base_len as u64)]).unwrap();
        let mut backing = Backing {
            forbidden: true,
            ..Backing::default()
        };
        backing.values.insert(
            EditRecordKey {
                kind: 10,
                key: [0; 32],
            },
            vec![2],
        );
        let mut output = MemoryStore::new();
        let result = run(&reader, &view, &edits, &source, &mut output, &mut backing).unwrap();
        assert_eq!(result.root, built.root);
        assert_eq!(
            result.counters,
            layerfs_content::file::EditCounters::default()
        );
        assert_eq!(backing.calls, 0);
        assert!(output.is_empty());
        assert_eq!(source.reads.borrow().len(), 1);
        let before = reader.requests.borrow().len();
        let empty = Edits::new(base_len as u64, Vec::new()).unwrap();
        let unused = Indexed::new(Vec::new());
        *unused.length_error.borrow_mut() = Some(ContentError::IncompleteOperation);
        *unused.read_error.borrow_mut() = Some(ContentError::IncompleteOperation);
        assert_eq!(
            run(&reader, &view, &empty, &unused, &mut output, &mut backing)
                .unwrap()
                .root,
            built.root
        );
        assert!(unused.lengths.borrow().is_empty());
        assert!(unused.reads.borrow().is_empty());
        assert_eq!(reader.requests.borrow().len(), before);
        assert_eq!(backing.calls, 0);
        assert!(output.is_empty());
    }
}

#[test]
fn borrowed_view_configuration_and_base_length_refuse_before_effects() {
    let (store, built) = build_file(&noise(131_073));
    let reader = Counted::new(&store);
    let view = open(&reader, built.root);
    let source = Indexed::new(Vec::new());
    *source.length_error.borrow_mut() = Some(ContentError::IncompleteOperation);
    *source.read_error.borrow_mut() = Some(ContentError::IncompleteOperation);
    let empty = Edits::new(view.logical_len(), Vec::new()).unwrap();
    let policy = ConstructionPolicy::frozen_default();
    let mut output = MemoryStore::new();
    let mut backing = Backing {
        forbidden: true,
        ..Backing::default()
    };
    let invalid = ConstructionPolicy::new(131_073, 8, 4);
    assert_eq!(
        configured(
            invalid,
            &policy.capacities(),
            &reader,
            &view,
            &empty,
            &source,
            &mut output,
            &mut backing
        )
        .unwrap_err(),
        ContentError::UnsupportedPolicy {
            field: "small_file_threshold_bytes"
        }
    );
    for field in 0..3 {
        let mut capacities = policy.capacities();
        match field {
            0 => capacities.whole_file_raw_limit += 1,
            1 => capacities.chunk_raw_limit += 1,
            _ => capacities.chunk_delta_max_depth += 1,
        }
        assert_eq!(
            configured(
                policy,
                &capacities,
                &reader,
                &view,
                &empty,
                &source,
                &mut output,
                &mut backing
            )
            .unwrap_err(),
            ContentError::InvalidRecord("edit policy capacities")
        );
    }
    let wrong = Edits::new(view.logical_len() + 1, Vec::new()).unwrap();
    assert_eq!(
        run(&reader, &view, &wrong, &source, &mut output, &mut backing).unwrap_err(),
        ContentError::InvalidEdit {
            what: "declared base length"
        }
    );
    assert_eq!(*reader.requests.borrow(), vec![built.root]);
    assert!(source.lengths.borrow().is_empty());
    assert!(source.reads.borrow().is_empty());
    assert_eq!(backing.calls, 0);
    assert!(output.is_empty());
}

struct WatchedReader<'a> {
    reader: Counted<'a>,
    stopped: &'a Cell<bool>,
}
impl AuthenticatedObjects for WatchedReader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        assert!(
            !self.stopped.get(),
            "provider demand after original failure"
        );
        self.reader.read_canonical_batch(ids)
    }
}
struct WatchedBacking<'a> {
    backing: Backing,
    stopped: &'a Cell<bool>,
}
impl IndexedEditBacking for WatchedBacking<'_> {
    fn contains(&mut self, key: EditRecordKey) -> ContentResult<bool> {
        assert!(
            !self.stopped.get(),
            "backing membership after original failure"
        );
        self.backing.contains(key)
    }
    fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        assert!(!self.stopped.get(), "backing read after original failure");
        self.backing.get(key)
    }
    fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<EditRecordApply> {
        assert!(
            !self.stopped.get(),
            "backing mutation after original failure"
        );
        self.backing.apply(changes)
    }
    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        assert!(!self.stopped.get(), "backing window after original failure");
        self.backing.first_keys(kind, excluded)
    }
}

#[test]
fn late_indexed_run_failure_retains_accepted_chunks_and_does_no_cleanup_job() {
    let (store, built) = build_file(&noise(512 * 1024));
    let mut source = Indexed::new(vec![vec![Piece::Data(noise(600_000))]]);
    let original = ContentError::ProviderFailure {
        what: "original late indexed run",
    };
    *source.read_error.borrow_mut() = Some(original.clone());
    source.fail_at = 32_768;
    let reader = WatchedReader {
        reader: Counted::new(&store),
        stopped: &source.failed,
    };
    let view = open(&reader, built.root);
    let edits = Edits::new(view.logical_len(), vec![Edit::insert(0, 600_000)]).unwrap();
    let mut backing = WatchedBacking {
        backing: Backing::default(),
        stopped: &source.failed,
    };
    let mut output = MemoryStore::new();
    assert_eq!(
        run(&reader, &view, &edits, &source, &mut output, &mut backing).unwrap_err(),
        original
    );
    assert_eq!(source.reads.borrow().len(), 2);
    assert!(output.roles().contains(&ObjectRole::Chunk));
    assert!(!output.roles().contains(&ObjectRole::FileState));
    assert!(backing.backing.calls > 0);
    assert_eq!(
        backing.backing.values[&EditRecordKey {
            kind: 10,
            key: [0; 32]
        }],
        vec![0]
    );
    assert!(source.read_error.borrow().is_none());
}

#[test]
fn borrowed_view_consumer_refusal_keeps_original_object_and_hint_without_later_work() {
    struct Refusing<'a> {
        stopped: &'a Cell<bool>,
        original: Option<FinalizedObject>,
        attempts: usize,
        error: Option<ContentError>,
    }
    impl FinalizedConsumer for Refusing<'_> {
        fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
            assert!(!self.stopped.get(), "consumer replay");
            self.attempts += 1;
            self.original = Some(object);
            self.stopped.set(true);
            Err(self.error.take().unwrap())
        }
    }
    let (store, built) = build_file(&noise(31_337));
    let source = Indexed::new(vec![vec![Piece::Data(vec![1, 2, 3])]]);
    let stopped = Cell::new(false);
    let reader = WatchedReader {
        reader: Counted::new(&store),
        stopped: &stopped,
    };
    let view = open(&reader, built.root);
    let edits = Edits::new(view.logical_len(), vec![Edit::insert(17, 3)]).unwrap();
    let original = ContentError::ProviderFailure {
        what: "original consumer custody",
    };
    let mut consumer = Refusing {
        stopped: &stopped,
        original: None,
        attempts: 0,
        error: Some(original.clone()),
    };
    let mut backing = WatchedBacking {
        backing: Backing {
            forbidden: true,
            ..Backing::default()
        },
        stopped: &stopped,
    };
    assert_eq!(
        run(&reader, &view, &edits, &source, &mut consumer, &mut backing).unwrap_err(),
        original
    );
    assert_eq!(consumer.attempts, 1);
    let object = consumer.original.as_ref().unwrap();
    assert_eq!(object.role(), ObjectRole::WholeFile);
    assert_eq!(
        object.predecessors().ids().collect::<Vec<_>>(),
        vec![built.root]
    );
    assert_eq!(
        object.predecessors().entries()[0].provenance(),
        layerfs_content::PredecessorProvenance::OriginalBase
    );
    assert_eq!(source.reads.borrow().len(), 1);
    assert_eq!(backing.backing.calls, 0);
    assert_eq!(*reader.reader.requests.borrow(), vec![built.root]);
    assert_eq!(view.root(), built.root);
}
