//! External proof-helper tests; no product hooks or performance samples.
#[path = "../examples/history_support/canonical_memo.rs"]
mod canonical_memo;
use canonical_memo::{Memo, Reader, BYTE_LIMIT, ROW_LIMIT};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
struct Source {
    bodies: BTreeMap<ObjectId, Vec<u8>>,
    calls: RefCell<Vec<Vec<ObjectId>>>,
    wrong: Cell<bool>,
    short: Cell<bool>,
}
impl AuthenticatedObjects for Source {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.calls.borrow_mut().push(ids.to_vec());
        let ids = if self.short.get() && !ids.is_empty() {
            &ids[..ids.len() - 1]
        } else {
            ids
        };
        ids.iter()
            .map(|id| {
                self.bodies
                    .get(id)
                    .map(|b| if self.wrong.get() { vec![0] } else { b.clone() })
                    .ok_or(ContentError::MissingObject)
            })
            .collect()
    }
}
fn source(n: usize, width: usize) -> (Source, Vec<ObjectId>) {
    let bodies = (0..n)
        .map(|n| {
            let mut b = vec![0; width];
            b[..8].copy_from_slice(&(n as u64).to_be_bytes());
            (ObjectId::for_bytes(&b), b)
        })
        .collect::<BTreeMap<_, _>>();
    let ids = bodies.keys().copied().collect();
    (
        Source {
            bodies,
            calls: RefCell::default(),
            wrong: Cell::new(false),
            short: Cell::new(false),
        },
        ids,
    )
}
#[test]
fn mixed_hits_misses_duplicates_and_order_keep_exact_bytes() {
    let (source, ids) = source(3, 128);
    let memo = RefCell::new(Memo::default());
    let reader = Reader::new(&source, &memo);
    reader.read_canonical(ids[0]).unwrap();
    let wanted = [ids[0], ids[2], ids[1], ids[0]];
    assert_eq!(
        reader.read_canonical_batch(&wanted).unwrap(),
        wanted.map(|id| source.bodies[&id].clone()).to_vec()
    );
    assert_eq!(
        source.calls.borrow().as_slice(),
        &[vec![ids[0]], vec![ids[2], ids[1]]]
    );
    assert_eq!(memo.borrow().counters().hits, 2);
}
#[test]
fn identity_and_missing_failures_are_not_cached() {
    let (source, ids) = source(1, 128);
    let memo = RefCell::new(Memo::default());
    let reader = Reader::new(&source, &memo);
    source.wrong.set(true);
    assert_eq!(
        reader.read_canonical(ids[0]),
        Err(ContentError::IdentityMismatch)
    );
    assert_eq!(memo.borrow().retained(), (0, 0));
    source.wrong.set(false);
    assert_eq!(
        reader.read_canonical(ObjectId::for_bytes(b"absent")),
        Err(ContentError::MissingObject)
    );
    assert_eq!(memo.borrow().retained(), (0, 0));
}
#[test]
fn eviction_respects_both_fixed_bounds_and_fresh_invocation_starts_empty() {
    let (source, ids) = source(700, 8192);
    let memo = RefCell::new(Memo::default());
    let reader = Reader::new(&source, &memo);
    for chunk in ids.chunks(ROW_LIMIT) {
        reader.read_canonical_batch(chunk).unwrap();
    }
    assert!(memo.borrow().retained().0 <= BYTE_LIMIT);
    assert!(memo.borrow().retained().1 <= ROW_LIMIT);
    assert!(memo.borrow().counters().evictions > 0);
    let fresh = RefCell::new(Memo::default());
    let another = Reader::new(&source, &fresh);
    let calls = source.calls.borrow().len();
    another.read_canonical(ids[0]).unwrap();
    assert_eq!(source.calls.borrow().len(), calls + 1);
}

#[test]
fn short_source_batch_is_refused_before_cache_admission() {
    let (source, ids) = source(2, 128);
    let memo = RefCell::new(Memo::default());
    let reader = Reader::new(&source, &memo);
    source.short.set(true);
    assert_eq!(
        reader.read_canonical_batch(&ids),
        Err(ContentError::BatchCardinality {
            requested: 2,
            returned: 1
        })
    );
    assert_eq!(memo.borrow().retained(), (0, 0));
}
