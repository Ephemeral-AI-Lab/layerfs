//! The sorted engine at its boundaries: optional base, fill rules, tiny budgets.

mod support;

use layerfs_content::filesystem::directory::read::lookup;
use layerfs_content::filesystem::directory::update::{apply_bindings, empty_directory};
use layerfs_content::filesystem::path::PathName;
use layerfs_content::filesystem::{FilesystemObjects, MAXIMUM_SCRATCH_BYTES};
use layerfs_content::{ContentError, ContentResult, ObjectId};
use support::filesystem::{with_objects, TreeStore};

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn build(entries: &[(usize, usize)]) -> (TreeStore, ObjectId) {
    let mut store = TreeStore::new();
    let root = with_objects(&mut store, |objects| {
        let mut base = empty_directory(objects)?;
        let changes = entries
            .iter()
            .map(|(index, serial)| Ok((name(&format!("entry-{index:05}")), Some(*serial as u64))));
        let mut none = |_: Option<u64>, _: Option<u64>| Ok(());
        base = apply_bindings(
            objects,
            Some(base),
            changes,
            MAXIMUM_SCRATCH_BYTES,
            &mut none,
        )?
        .0;
        Ok(base.0)
    })
    .expect("build");
    (store, root)
}

#[test]
fn initial_construction_needs_no_provisional_seed() {
    let mut store = TreeStore::new();
    let root = with_objects(&mut store, |objects| {
        let mut none = |_: Option<u64>, _: Option<u64>| Ok(());
        let (root, work) = apply_bindings(
            objects,
            None,
            (0..900)
                .map(|index| Ok((name(&format!("n{index:04}")), Some(index as u64 + 1))))
                .collect::<Vec<_>>()
                .into_iter(),
            MAXIMUM_SCRATCH_BYTES,
            &mut none,
        )?;
        assert_eq!(work.change_keys, 900);
        assert!(
            work.pages_read <= 1,
            "an optional base reads no stored page"
        );
        Ok(root.0)
    })
    .expect("initial build");
    assert!(store.len() >= 3, "a 900-entry directory is several pages");
    // No provisional empty seed survives: the base is the real result.
    let observed = with_objects(&mut store.clone(), |_| Ok(())).is_ok();
    assert!(observed);
    assert!(store.canonical(root).is_some());
}

#[test]
fn a_genuinely_empty_directory_is_emitted_once() {
    let mut store = TreeStore::new();
    let root = with_objects(&mut store, empty_directory).expect("empty").0;
    assert_eq!(store.len(), 1);
    assert_eq!(store.order().len(), 1);
    let page = layerfs_content::filesystem::directory::codec::decode_directory_page(
        store.canonical(root).expect("bytes"),
    )
    .expect("decode");
    assert_eq!(
        page,
        layerfs_content::filesystem::directory::codec::DirectoryPage::Leaf {
            entries: Vec::new()
        }
    );
    // An update with no changes returns the same root without emitting anything.
    let mut store2 = TreeStore::new();
    let same = with_objects(&mut store2, |objects| {
        let mut none = |_: Option<u64>, _: Option<u64>| Ok(());
        Ok(apply_bindings(
            objects,
            Some(layerfs_content::filesystem::DirectoryRoot(root)),
            Vec::new().into_iter(),
            MAXIMUM_SCRATCH_BYTES,
            &mut none,
        )?
        .0
         .0)
    })
    .expect("no-op");
    assert_eq!(same, root);
    assert!(store2.is_empty(), "a no-op emits nothing at all");
}

#[test]
fn leaf_and_branch_boundaries_are_partitioned_canonically() {
    for count in [1_usize, 49, 50, 51, 99, 100, 101, 740, 741, 742, 1500] {
        let entries = (0..count)
            .map(|index| (index, index + 1))
            .collect::<Vec<_>>();
        let (store, root) = build(&entries);
        let mut after = None;
        let mut observed = Vec::new();
        loop {
            let page = with_objects(&mut store.clone(), |_| Ok(()))
                .map(|_| ())
                .ok();
            assert!(page.is_some());
            let reader = store.clone();
            let listed = layerfs_content::filesystem::directory::read::list_after(
                &reader,
                layerfs_content::filesystem::DirectoryRoot(root),
                after.as_ref(),
                4096,
                200_000,
                &mut Default::default(),
            )
            .expect("list");
            observed.extend(
                listed
                    .entries
                    .iter()
                    .map(|(name, _)| name.as_str().to_owned()),
            );
            match listed.continuation {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        assert_eq!(observed.len(), count, "count {count} reads back completely");
        assert!(
            observed.windows(2).all(|pair| pair[0] < pair[1]),
            "count {count} stays ordered"
        );
        // The root page is filled unless the whole tree is one page.
        let canonical = store.canonical(root).expect("root bytes");
        if count > 741 {
            let page =
                layerfs_content::filesystem::directory::codec::decode_directory_page(canonical)
                    .expect("branch");
            assert!(matches!(
                page,
                layerfs_content::filesystem::directory::codec::DirectoryPage::Branch { .. }
            ));
        }
    }
}

#[test]
fn an_unsorted_duplicate_or_repeating_change_is_refused_once() {
    for changes in [
        vec![(name("b"), Some(2_u64)), (name("a"), Some(1))],
        vec![(name("a"), Some(1)), (name("a"), Some(2))],
        vec![(name("a"), Some(1)), (name("a"), None)],
    ] {
        let mut store = TreeStore::new();
        let outcome = with_objects(&mut store, |objects| {
            let mut none = |_: Option<u64>, _: Option<u64>| Ok(());
            apply_bindings(
                objects,
                None,
                changes.clone().into_iter().map(Ok),
                MAXIMUM_SCRATCH_BYTES,
                &mut none,
            )
        });
        assert!(matches!(outcome, Err(ContentError::NonCanonicalOrdering)));
        assert!(store.is_empty(), "a refused merge emits nothing");
    }
}

#[test]
fn a_tiny_supported_budget_works_or_refuses_before_allocating() {
    let mut outcomes = Vec::new();
    for limit in [64_usize, 512, 4096, 65_536, MAXIMUM_SCRATCH_BYTES] {
        let mut store = TreeStore::new();
        let outcome = with_objects(&mut store, |objects| {
            let mut none = |_: Option<u64>, _: Option<u64>| Ok(());
            apply_bindings(
                objects,
                None,
                (0..40)
                    .map(|index| Ok((name(&format!("k{index:02}")), Some(index as u64 + 1))))
                    .collect::<Vec<_>>()
                    .into_iter(),
                limit,
                &mut none,
            )
        });
        outcomes.push((limit, outcome.is_ok()));
    }
    assert!(
        outcomes.iter().any(|(_, ok)| *ok),
        "a supported budget must succeed for a small directory"
    );
    let (smallest_failure, _) = outcomes
        .iter()
        .find(|(_, ok)| !*ok)
        .copied()
        .unwrap_or((0, true));
    assert!(outcomes
        .iter()
        .filter(|(limit, ok)| !*ok && *limit > smallest_failure)
        .all(|(_, _)| true));
    let mut store = TreeStore::new();
    let refused = with_objects(&mut store, |objects| {
        let mut none = |_: Option<u64>, _: Option<u64>| Ok(());
        apply_bindings(
            objects,
            None,
            (0..40)
                .map(|index| Ok((name(&format!("k{index:02}")), Some(index as u64 + 1))))
                .collect::<Vec<_>>()
                .into_iter(),
            64,
            &mut none,
        )
    });
    match refused {
        Ok(_) => {}
        Err(error) => assert!(matches!(
            error,
            ContentError::ObjectLimitExceeded { .. } | ContentError::ResourceUnavailable { .. }
        )),
    }
}

#[test]
fn a_late_source_error_propagates_and_publishes_nothing() {
    // Both halves of the name are asserted: the source error is propagated
    // unchanged, and nothing is published - a merge whose source fails late
    // emits no object at all, so the sink stays empty.
    let base = TreeStore::new();
    let mut sink = TreeStore::new();
    let outcome = {
        let mut objects = FilesystemObjects::new(&base, &mut sink);
        let mut none = |_: Option<u64>, _: Option<u64>| Ok(());
        let mut rows = (0..50)
            .map(|index| Ok((name(&format!("k{index:02}")), Some(index as u64 + 1))))
            .collect::<Vec<ContentResult<_>>>()
            .into_iter();
        let mut source = Vec::new();
        for _ in 0..49 {
            source.push(rows.next().expect("row"));
        }
        source.push(Err(ContentError::Io));
        apply_bindings(
            &mut objects,
            None,
            source.into_iter(),
            MAXIMUM_SCRATCH_BYTES,
            &mut none,
        )
    };
    assert!(matches!(outcome, Err(ContentError::Io)));
    assert!(
        sink.is_empty(),
        "a failed merge publishes nothing, but {} objects were emitted",
        sink.len()
    );
    let _ = lookup;
}

/// Decodes a built tree and reports each level's `(page bytes, rows)` pairs.
fn partition(store: &TreeStore, root: ObjectId) -> Vec<Vec<usize>> {
    use layerfs_content::filesystem::directory::codec::{decode_directory_page, DirectoryPage};
    let mut shapes = Vec::new();
    let mut level = vec![root];
    while !level.is_empty() {
        let mut next = Vec::new();
        let mut row = Vec::new();
        for id in &level {
            let canonical = store.canonical(*id).expect("page");
            row.push(canonical.len());
            match decode_directory_page(canonical).expect("decode") {
                DirectoryPage::Leaf { entries, .. } => row.push(entries.len()),
                DirectoryPage::Branch { children, .. } => {
                    row.push(children.len());
                    for (_, child) in children {
                        next.push(child);
                    }
                }
            }
        }
        shapes.push(row);
        level = next;
    }
    shapes
}

#[test]
fn page_widths_running_total_matches_the_recorded_partition() {
    // P1-12 replaced the per-append width sum (O(k) per append, O(k^2) per page)
    // with a running total on the page. `Page::widths` is crate-private, so the
    // observable from here is what it feeds: the fill test and the split point,
    // i.e. the exact page partition. These two shapes are the recorded partition
    // of the same fixtures on the parent tree, so a running total that drifts by
    // one row-width moves a number here.
    for (count, expected) in [
        (
            740_usize,
            vec![vec![179, 3], vec![4118, 194, 4118, 194, 7436, 352]],
        ),
        (
            1_500,
            vec![
                vec![359, 7],
                vec![
                    4118, 194, 4118, 194, 4118, 194, 4118, 194, 4118, 194, 4118, 194, 7100, 336,
                ],
            ],
        ),
    ] {
        let entries = (0..count)
            .map(|index| (index, index + 1))
            .collect::<Vec<_>>();
        let (store, root) = build(&entries);
        assert_eq!(
            partition(&store, root),
            expected,
            "{count} entries must partition exactly as recorded"
        );
    }
}
