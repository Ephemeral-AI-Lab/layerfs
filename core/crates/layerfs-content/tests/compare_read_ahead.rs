//! A difference walk reads the pages it will expand in batched demands
//! (layerfs-project #95). A Commit's logical count and binding hints walk the
//! inode and directory trees; one demand per page cost a Store one round trip
//! per page (C16, 10,000 new files: 149 and 239). The walk now reads the
//! pages after the one it opens, on the same side and level, that it will
//! certainly expand, in the same demand: no page is read that the walk does
//! not expand, none twice, and at most `READ_AHEAD_PAGES` are held.
mod support;

use layerfs_content::compare::sorted::READ_AHEAD_PAGES;
use layerfs_content::compare::{binding_changes, inode_differences, DifferenceWork, TreeTable};
use layerfs_content::{AuthenticatedObjects, ContentResult, ObjectId};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use support::filesystem::TreeStore;
use support::topology::{Shape, ROOT};

/// Both trees' objects, counting demands and the ids each asks for.
struct Counting {
    store: TreeStore,
    demands: Cell<usize>,
    largest: Cell<usize>,
    reads: RefCell<HashMap<ObjectId, usize>>,
}
impl AuthenticatedObjects for Counting {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demands.set(self.demands.get() + 1);
        self.largest.set(self.largest.get().max(ids.len()));
        let mut reads = self.reads.borrow_mut();
        for id in ids {
            *reads.entry(*id).or_default() += 1;
        }
        self.store.read_canonical_batch(ids)
    }
}
impl Counting {
    fn reset(&self) {
        self.demands.set(0);
        self.largest.set(0);
        self.reads.borrow_mut().clear();
    }
}

/// `d` holds `before` files on the left and `after` on the right, the first
/// `before` the same on both sides.
fn trees(before: usize, after: usize) -> (Counting, TreeTable, TreeTable, u64) {
    let build = |files: usize| {
        let mut shape = Shape::new();
        let d = shape.dir(ROOT, "d");
        for file in 0..files {
            shape.file(d, &format!("f{file:05}"));
        }
        (shape.build(), d)
    };
    let ((left, d), (right, _)) = (build(before), build(after));
    let mut store = TreeStore::new();
    store.absorb(&left.store);
    store.absorb(&right.store);
    let reader = Counting {
        store,
        demands: Cell::new(0),
        largest: Cell::new(0),
        reads: RefCell::new(HashMap::new()),
    };
    let left = TreeTable::open(&reader, left.root).unwrap();
    let right = TreeTable::open(&reader, right.root).unwrap();
    reader.reset();
    (reader, left, right, d)
}

/// Every page asked for was expanded, and once.
fn read_exactly_expanded(reader: &Counting, work: &DifferenceWork) {
    let reads = reader.reads.borrow();
    assert!(reads.values().all(|count| *count == 1), "a page read twice");
    assert_eq!(
        reads.len() as u64,
        work.pages_read,
        "a page read ahead was not expanded"
    );
    assert!(reader.largest.get() <= READ_AHEAD_PAGES);
}

#[test]
fn many_new_rows_are_read_in_a_few_demands_and_nothing_more() {
    let (reader, left, right, d) = trees(1_000, 10_000);
    let mut work = DifferenceWork::default();
    let mut added = 0;
    inode_differences(
        &reader,
        Some(&left),
        &right,
        &mut work,
        |_, before, after| {
            added += usize::from(before.is_none() && after.is_some());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(added, 9_000);
    read_exactly_expanded(&reader, &work);
    let (inode_pages, inode_demands) = (work.pages_read, reader.demands.get());
    // About 90 new inode leaves and their branches.
    assert!(inode_pages >= 90, "{work:?}");
    assert!(
        inode_demands * 10 <= inode_pages as usize,
        "{inode_demands} demands, {work:?}"
    );

    let directory = |tree: &TreeTable| {
        let mut root = None;
        inode_differences(
            &reader,
            None,
            tree,
            &mut DifferenceWork::default(),
            |serial, _, value| {
                if serial == d {
                    root = value.map(|value| value.content_root);
                }
                Ok(())
            },
        )
        .unwrap();
        root.unwrap()
    };
    let (was, now) = (directory(&left), directory(&right));
    reader.reset();
    let mut work = DifferenceWork::default();
    let mut bound = 0;
    binding_changes(
        &reader,
        Some(was),
        Some(now),
        &mut work,
        |_, before, after| {
            bound += usize::from(before.is_none() && after.is_some());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(bound, 9_000);
    read_exactly_expanded(&reader, &work);
    let demands = reader.demands.get();
    assert!(
        demands * 10 <= work.pages_read as usize,
        "{demands} demands, {work:?}"
    );
    println!(
        "READ_AHEAD inode pages={inode_pages} demands={inode_demands}; directory pages={} \
         demands={demands}",
        work.pages_read
    );
}

#[test]
fn one_new_row_reads_only_its_path() {
    let (reader, left, right, _) = trees(10_000, 10_001);
    let mut work = DifferenceWork::default();
    let mut changed = 0;
    inode_differences(&reader, Some(&left), &right, &mut work, |_, _, _| {
        changed += 1;
        Ok(())
    })
    .unwrap();
    assert!(changed >= 1);
    read_exactly_expanded(&reader, &work);
    // Shared pages are skipped, not read ahead: the walk reads the changed
    // path on each side, a few pages per level.
    assert!(work.pages_read <= 12, "{work:?}");
}
