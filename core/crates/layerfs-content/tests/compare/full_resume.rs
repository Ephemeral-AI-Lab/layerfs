//! A resumed full walk starts each listing at its cursor (#364 item 4).
//!
//! A difference too large to plan takes the full walk of both trees, one page
//! at a time, each page resuming after the last path the previous one listed.
//! Every directory on that path was listed again from its first name, so a
//! page deep into a large directory read every window before its cursor: the
//! relink-over-relink verify of range 16 paid 1.3 ms an entry. A resumed walk
//! lists the same pairs as the uninterrupted one, and the reads a page makes
//! before its first listed pair do not follow how far the cursor is.
use crate::support;

use layerfs_content::compare::{ChangeWalk, Pair, TreeTable};
use layerfs_content::filesystem::name::path::PathName;
use layerfs_content::{AuthenticatedObjects, ContentResult, ObjectId};
use std::cell::Cell;
use support::filesystem::TreeStore;
use support::topology::{Shape, ROOT};

const FILES: usize = 600;

/// Both trees' objects, counting the ids asked for.
struct Counting {
    store: TreeStore,
    reads: Cell<usize>,
}
impl AuthenticatedObjects for Counting {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.reads.set(self.reads.get() + ids.len());
        self.store.read_canonical_batch(ids)
    }
}

/// `d` holds `FILES` files on both sides; the right side allocates one
/// directory first, so every file there has another serial.
fn trees() -> (Counting, TreeTable, TreeTable) {
    let build = |shift: bool| {
        let mut shape = Shape::new();
        if shift {
            shape.dir(ROOT, "a");
        }
        let d = shape.dir(ROOT, "d");
        for file in 0..FILES {
            shape.file(d, &format!("f{file:04}"));
        }
        shape.file(ROOT, "z");
        shape.build()
    };
    let (left, right) = (build(false), build(true));
    let mut store = TreeStore::new();
    store.absorb(&left.store);
    store.absorb(&right.store);
    let reader = Counting {
        store,
        reads: Cell::new(0),
    };
    let left = TreeTable::open(&reader, left.root).unwrap();
    let right = TreeTable::open(&reader, right.root).unwrap();
    (reader, left, right)
}

/// The component of `cursor` right below directory `path`.
fn component(cursor: &str, path: &str) -> Option<PathName> {
    let rest = if path == "/" {
        cursor.strip_prefix('/')
    } else {
        cursor.strip_prefix(path)?.strip_prefix('/')
    }?;
    rest.split('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(|name| PathName::new(name).unwrap())
}

/// Up to `count` pairs after `cursor`, entering every pair as the SDK's
/// difference does, and the reads made before the first of them.
fn page(
    reader: &Counting,
    left: &TreeTable,
    right: &TreeTable,
    cursor: Option<&str>,
    count: usize,
) -> (Vec<String>, usize) {
    reader.reads.set(0);
    let from = cursor.and_then(|cursor| component(cursor, "/"));
    let mut walk = ChangeWalk::full(reader, left, right, from.as_ref()).unwrap();
    let (mut listed, mut before) = (Vec::new(), None);
    while listed.len() < count {
        let Some(pair) = walk.next_pair().unwrap() else {
            break;
        };
        let path: &String = &pair.0;
        if let Some(cursor) = cursor {
            if path.split('/').cmp(cursor.split('/')) != std::cmp::Ordering::Greater {
                let on_path = cursor == path
                    || cursor
                        .strip_prefix(path.as_str())
                        .is_some_and(|rest| rest.starts_with('/'));
                if on_path {
                    let next = component(cursor, path);
                    walk.enter_from(&pair, next.as_ref()).unwrap();
                }
                continue;
            }
        }
        before.get_or_insert(reader.reads.get());
        listed.push(path.clone());
        let pair: Pair = pair;
        walk.enter(&pair).unwrap();
    }
    (listed, before.unwrap_or(reader.reads.get()))
}

#[test]
fn a_resumed_full_walk_lists_the_same_pairs_and_reads_from_its_cursor() {
    let (reader, left, right) = trees();
    let (all, _) = page(&reader, &left, &right, None, usize::MAX);
    // a, d, its files, z.
    assert_eq!(all.len(), FILES + 3);
    let mut costs = Vec::new();
    for at in [2, 100, 300, FILES - 10] {
        let cursor = all[at].as_str();
        let (listed, before) = page(&reader, &left, &right, Some(cursor), 50);
        let expected: Vec<String> = all[at + 1..].iter().take(50).cloned().collect();
        assert_eq!(listed, expected, "resumed after {cursor}");
        costs.push((cursor.to_owned(), before));
    }
    // The reads before a page's first pair do not grow with the cursor's
    // position in the directory.
    let first = costs[0].1;
    for (cursor, before) in &costs {
        assert!(
            *before <= first + 4,
            "resuming after {cursor} read {before} objects first, after {} read {first}",
            costs[0].0
        );
    }
}
