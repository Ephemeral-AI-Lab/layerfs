//! Generated public edit inputs exercise streaming small-result assembly.

mod support;

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use layerfs_content::{
    apply_edits, construct_bytes, AuthenticatedObjects, ConstructionPolicy, ContentError,
    ContentResult, Edit, EditRequest, EditSequence, EditSource, FileContent, ObjectId,
};
use support::{disabled_scope, noise, read_back, MemoryStore};

const RUNS: usize = 4_097;

struct SmallSequence {
    base_len: u64,
    greatest_requested: Cell<usize>,
    calls: Cell<usize>,
}

impl SmallSequence {
    fn new(base_len: u64) -> Self {
        Self {
            base_len,
            greatest_requested: Cell::new(0),
            calls: Cell::new(0),
        }
    }
}

impl EditSequence for SmallSequence {
    fn base_len(&self) -> u64 {
        self.base_len
    }
    fn final_len(&self) -> u64 {
        self.base_len
    }
    fn len(&self) -> usize {
        RUNS
    }
    fn edit_at(&self, index: usize) -> ContentResult<Edit> {
        if index >= RUNS {
            return Err(ContentError::InvalidEdit {
                what: "generated index",
            });
        }
        self.calls.set(self.calls.get() + 1);
        self.greatest_requested
            .set(self.greatest_requested.get().max(index));
        Ok(if index == 0 {
            Edit::overwrite(0, 1)
        } else {
            // A zero-length insertion at the preceding replacement's end is
            // ordered, changes no length and never addresses earlier new bytes.
            Edit::insert(1, 0)
        })
    }
}

struct Replacement<'a> {
    sequence: &'a SmallSequence,
    calls: Cell<usize>,
    assembly_index: Cell<Option<usize>>,
    refuse_assembly: bool,
}

impl EditSource for Replacement<'_> {
    fn replacement_len(&self, index: usize) -> u64 {
        u64::from(index == 0)
    }
    fn read_at(&self, index: usize, offset: u64, output: &mut [u8]) -> ContentResult<usize> {
        assert_eq!(index, 0);
        assert_eq!(offset, 0);
        let call = self.calls.get() + 1;
        self.calls.set(call);
        if call == 2 {
            self.assembly_index
                .set(Some(self.sequence.greatest_requested.get()));
            if self.refuse_assembly {
                return Err(ContentError::Io);
            }
        }
        if output.is_empty() {
            return Ok(0);
        }
        output[0] = 0xa7;
        Ok(1)
    }
}

fn build(bytes: &[u8]) -> (MemoryStore, ObjectId) {
    let policy = ConstructionPolicy::default();
    let mut store = MemoryStore::new();
    let file = disabled_scope(|scope| {
        construct_bytes(
            policy,
            &policy.capacities(),
            bytes,
            &mut store,
            scope.child("build"),
        )
    })
    .unwrap();
    (store, file.root)
}

#[test]
fn assembly_reads_its_first_replacement_before_requesting_later_generated_rows() {
    let base = b"abcdefgh";
    let (store, root) = build(base);
    let sequence = SmallSequence::new(base.len() as u64);
    let source = Replacement {
        sequence: &sequence,
        calls: Cell::new(0),
        assembly_index: Cell::new(None),
        refuse_assembly: false,
    };
    let policy = ConstructionPolicy::default();
    let mut emitted = MemoryStore::new();
    let file = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &store,
            EditRequest {
                root,
                edits: &sequence,
                source: &source,
            },
            &mut emitted,
            scope.child("edit"),
        )
    })
    .unwrap();
    assert_eq!(source.calls.get(), 2, "comparison then assembly");
    assert_eq!(source.assembly_index.get(), Some(0));
    assert_eq!(sequence.greatest_requested.get(), RUNS - 1);
    assert_eq!(sequence.calls.get(), RUNS + 1);
    let mut expected = *base;
    expected[0] = 0xa7;
    let (_, fresh_root) = build(&expected);
    assert_eq!(file.root, fresh_root);
    assert_eq!(file.logical_len, expected.len() as u64);
    assert_eq!(read_back(&emitted, file.root).unwrap(), expected);
}

#[test]
fn first_assembly_failure_stops_before_later_rows_or_final_emission() {
    let base = b"abcdefgh";
    let (store, root) = build(base);
    let sequence = SmallSequence::new(base.len() as u64);
    let source = Replacement {
        sequence: &sequence,
        calls: Cell::new(0),
        assembly_index: Cell::new(None),
        refuse_assembly: true,
    };
    let policy = ConstructionPolicy::default();
    let mut emitted = MemoryStore::new();
    let result = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &store,
            EditRequest {
                root,
                edits: &sequence,
                source: &source,
            },
            &mut emitted,
            scope.child("edit"),
        )
    });
    assert_eq!(result, Err(ContentError::Io));
    assert_eq!(source.calls.get(), 2);
    assert_eq!(source.assembly_index.get(), Some(0));
    assert_eq!(sequence.greatest_requested.get(), 0);
    assert_eq!(sequence.calls.get(), 2);
    assert!(emitted.is_empty());
}

struct Deletions {
    base_len: u64,
    tail_deletes: usize,
    greatest_requested: Cell<usize>,
}

impl EditSequence for Deletions {
    fn base_len(&self) -> u64 {
        self.base_len
    }
    fn final_len(&self) -> u64 {
        2
    }
    fn len(&self) -> usize {
        self.tail_deletes + 1
    }
    fn edit_at(&self, index: usize) -> ContentResult<Edit> {
        if index > self.tail_deletes {
            return Err(ContentError::InvalidEdit {
                what: "generated deletion index",
            });
        }
        self.greatest_requested
            .set(self.greatest_requested.get().max(index));
        Ok(if index == 0 {
            Edit::delete(1, self.base_len - self.tail_deletes as u64 - 1)
        } else {
            Edit::delete(1, 2)
        })
    }
}

struct EmptyReplacements;

impl EditSource for EmptyReplacements {
    fn replacement_len(&self, _: usize) -> u64 {
        0
    }
    fn read_at(&self, _: usize, _: u64, _: &mut [u8]) -> ContentResult<usize> {
        panic!("a generated deletion has no replacement bytes")
    }
}

struct Observed<'a> {
    store: &'a MemoryStore,
    demands: RefCell<BTreeMap<ObjectId, usize>>,
}

impl AuthenticatedObjects for Observed<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        for id in ids {
            *self.demands.borrow_mut().entry(*id).or_default() += 1;
        }
        self.store.read_canonical_batch(ids)
    }
}

#[test]
fn many_generated_deletions_retain_one_cursor_for_a_tiny_result() {
    let base = noise(3_000_000);
    let (store, root) = build(&base);
    let FileContent::Chunked(state) =
        layerfs_content::file::classify(store.canonical(root).unwrap()).unwrap()
    else {
        panic!("large fixture must be chunked")
    };
    let reader = Observed {
        store: &store,
        demands: RefCell::new(BTreeMap::new()),
    };
    let sequence = Deletions {
        base_len: base.len() as u64,
        tail_deletes: RUNS - 1,
        greatest_requested: Cell::new(0),
    };
    let policy = ConstructionPolicy::default();
    let mut emitted = MemoryStore::new();
    let file = disabled_scope(|scope| {
        apply_edits(
            policy,
            &policy.capacities(),
            &reader,
            EditRequest {
                root,
                edits: &sequence,
                source: &EmptyReplacements,
            },
            &mut emitted,
            scope.child("edit"),
        )
    })
    .unwrap();
    let expected = [base[0], base[base.len() - 1]];
    let (_, fresh_root) = build(&expected);
    assert_eq!(file.root, fresh_root);
    assert_eq!(file.logical_len, 2);
    assert_eq!(read_back(&emitted, file.root).unwrap(), expected);
    assert_eq!(sequence.greatest_requested.get(), RUNS - 1);
    assert_eq!(reader.demands.borrow()[&state.mapping_root], 1);
}
