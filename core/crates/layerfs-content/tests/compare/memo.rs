//! A comparison's memo keeps the pages every lookup shares when more than
//! `MEMO_PAGES` distinct objects pass through it (#352 014.a1). It emptied
//! wholesale when full, so a range-14 checkpoint diff that touched more objects
//! than that re-read the B-trees' upper pages after every reset: 1,506 more
//! reader statements than with no bound (5,754 against 4,248).
use layerfs_content::compare::{Memo, MEMO_PAGES};
use layerfs_content::{AuthenticatedObjects, ContentResult, ObjectId};
use std::cell::RefCell;
use std::collections::HashMap;

/// A provider that serves any id and counts every id it is asked for.
#[derive(Default)]
struct Counting {
    reads: RefCell<HashMap<ObjectId, usize>>,
}

impl AuthenticatedObjects for Counting {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let mut reads = self.reads.borrow_mut();
        Ok(ids
            .iter()
            .map(|id| {
                *reads.entry(*id).or_default() += 1;
                id.as_bytes().to_vec()
            })
            .collect())
    }
}

fn id(n: u64) -> ObjectId {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&n.to_le_bytes());
    bytes[31] = 1;
    ObjectId::from_bytes(&bytes).unwrap()
}

#[test]
fn shared_pages_are_read_once_past_the_bound_and_evicted_pages_are_read_again() {
    let provider = Counting::default();
    let memo = Memo::new(&provider);
    let (root, inner) = (id(u64::MAX), id(u64::MAX - 1));
    let leaves = 4 * MEMO_PAGES as u64;
    // Every lookup descends root -> inner -> its own leaf, one level per demand.
    for leaf in 0..leaves {
        for page in [root, inner, id(leaf)] {
            assert_eq!(memo.read_canonical(page).unwrap(), page.as_bytes().to_vec());
        }
    }
    let reads = provider.reads.borrow();
    assert_eq!(
        (reads[&root], reads[&inner]),
        (1, 1),
        "the upper pages were re-read past {MEMO_PAGES} objects"
    );
    assert!((0..leaves).all(|leaf| reads[&id(leaf)] == 1));
    drop(reads);
    // The first leaves were evicted to stay within the bound: served again,
    // from the provider, with the same bytes.
    assert_eq!(
        memo.read_canonical(id(0)).unwrap(),
        id(0).as_bytes().to_vec()
    );
    assert_eq!(provider.reads.borrow()[&id(0)], 2);
    // The most recent ones are still held.
    memo.read_canonical(id(leaves - 1)).unwrap();
    assert_eq!(provider.reads.borrow()[&id(leaves - 1)], 1);
}
