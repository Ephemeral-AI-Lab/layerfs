//! Validation topology on the streamed-backed route: the behaviour the namespace
//! producer relies on, and wide, deep, sparse, alias and cycle cases.
//!
//! Every update runs through the public streamed-backed entry over an external
//! record fixture. Where both routes accept, the root is compared with the
//! resident route's; every refusal states what the consumer had been offered.

mod support;

use layerfs_content::filesystem::validate::CLASSIFICATION_WINDOW_ROWS;
use layerfs_content::filesystem::{FilesystemRead, FilesystemResources, FilesystemResult};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ContentError, ObjectRole};
use support::filesystem::{name_of, synthetic};
use support::reference_records::{key, PLACED};
use support::topology::{
    build_backed, build_resident, inode, listing, typed, update_backed, update_resident, Backed,
    Rowset, Shape, Tree, ROOT,
};

const WINDOW: u64 = CLASSIFICATION_WINDOW_ROWS as u64;

fn resources() -> FilesystemResources {
    FilesystemResources::default()
}

/// `/d/e/`, `/d/f`, `/l` and `/h/`: two nested stored directories, a regular
/// file, a symlink and an empty stored directory.
struct Small {
    tree: Tree,
    d: u64,
    e: u64,
    f: u64,
    l: u64,
    h: u64,
}

fn small() -> Small {
    let mut shape = Shape::new();
    let d = shape.dir(ROOT, "d");
    let e = shape.dir(d, "e");
    let f = shape.file(d, "f");
    let l = shape.symlink(ROOT, "l");
    let h = shape.dir(ROOT, "h");
    Small {
        tree: shape.build(),
        d,
        e,
        f,
        l,
        h,
    }
}

/// The accepted result of one backed run.
fn accepted(run: &Backed) -> FilesystemResult {
    assert_eq!(run.records.after_failure, 0);
    *run.result.as_ref().expect("the backed route accepts")
}

/// The run was refused with exactly this record label.
fn refused(run: &Backed, label: &'static str) {
    assert_eq!(
        run.result.as_ref().err(),
        Some(&ContentError::InvalidRecord(label))
    );
    assert!(
        !run.offered_root(),
        "no filesystem root is offered by a refused update"
    );
}

/// Topology point reads, guarded batches and enumerations of one run.
fn traffic(run: &Backed) -> (usize, usize, usize) {
    (
        run.records.topology_reads,
        run.records.topology_batches,
        run.records.topology_enumerations,
    )
}

/// The same update on the resident route with default resources.
fn resident_root(tree: &Tree, rows: &Rowset) -> FilesystemResult {
    update_resident(tree, rows, resources())
        .0
        .expect("the resident route accepts")
}

// ---- 7.1: the behaviour the producer relies on ----

#[test]
fn an_update_with_no_row_returns_the_base_root() {
    let base = small();
    let run = update_backed(&base.tree, &Rowset::new(), resources());
    let result = accepted(&run);
    assert_eq!(result.root, base.tree.root);
    let work = result.counters.validation;
    assert_eq!((work.placements, work.peak_window_rows), (0, 0));
    assert_eq!(traffic(&run), (0, 0, 0));
}

#[test]
fn removing_a_name_the_base_does_not_bind_changes_nothing() {
    let base = small();
    let rows = Rowset::new().unbind(ROOT, "ghost").unbind(base.d, "ghost");
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    assert_eq!(result.root, base.tree.root);
    assert_eq!(
        (
            result.counters.bindings_added,
            result.counters.bindings_removed
        ),
        (0, 0)
    );
    let work = result.counters.validation;
    // Two headers and no bound name: a removal is never a classification row.
    assert_eq!((work.placements, work.peak_window_rows), (0, 2));
    assert_eq!(traffic(&run), (0, 0, 0));
}

#[test]
fn restating_base_bindings_changes_nothing_and_walks_nothing() {
    let base = small();
    let rows = Rowset::new()
        .bind(ROOT, "d", base.d)
        .bind(ROOT, "h", base.h)
        .bind(ROOT, "l", base.l)
        .bind(base.d, "e", base.e)
        .bind(base.d, "f", base.f);
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    assert_eq!(result.root, base.tree.root);
    assert_eq!(
        (
            result.counters.bindings_added,
            result.counters.bindings_removed
        ),
        (0, 0)
    );
    let work = result.counters.validation;
    assert_eq!(
        (
            work.placements,
            work.ancestry_steps,
            work.territory_directories,
            work.territory_entries,
            work.entries_examined
        ),
        (0, 0, 0, 0, 0)
    );
    // One grouped point lookup per stored parent that binds a stored
    // directory: the root for `d` and `h`, and `d` for `e`.
    assert_eq!(work.directory_pages_read, 2);
    assert_eq!(work.peak_window_rows, 7);
    // No placement means no topology record is read, written or enumerated.
    assert_eq!(traffic(&run), (0, 0, 0));
}

#[test]
fn a_fresh_directory_with_an_empty_header_gets_a_real_empty_page() {
    let base = small();
    let fresh = base.tree.next;
    let rows = Rowset::new().mkdir(base.d, "new", fresh);
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    let store = run.folded(&base.tree);
    let value = FilesystemRead::new(&store, result.root)
        .expect("result")
        .resolve_child(base.d, &name_of("new"))
        .expect("the fresh directory is bound")
        .value;
    assert_ne!(
        value.content_root,
        typed(InodeKind::Directory).content_root,
        "the supplied content root of a fresh directory is ignored"
    );
    // The base's own empty directory carries the same canonical empty page.
    assert_eq!(value.content_root, base.tree.stored(base.h).content_root);
    assert_eq!(value.namespace_ref_count, 1);
    assert!(listing(&store, result.root, fresh).is_empty());
    let work = result.counters.validation;
    // One placement, one step up to the stored parent and the same step marked.
    assert_eq!((work.placements, work.ancestry_steps), (1, 2));
    assert_eq!((work.territory_directories, work.entries_examined), (0, 0));
    assert_eq!(resident_root(&base.tree, &rows).root, result.root);
}

#[test]
fn a_header_under_a_removed_base_directory_is_accepted_and_the_directory_released() {
    let base = small();
    // `d` leaves the root and its own header still states a removal.
    let rows = Rowset::new().unbind(ROOT, "d").unbind(base.d, "f");
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    let store = run.folded(&base.tree);
    assert_eq!(
        listing(&store, result.root, ROOT),
        vec![("h".to_owned(), base.h), ("l".to_owned(), base.l)]
    );
    for released in [base.d, base.e, base.f] {
        assert_eq!(
            inode(&store, result.root, released),
            None,
            "inode {released} is released with the removed directory"
        );
    }
    assert_eq!(result.counters.validation.placements, 0);
    assert_eq!(resident_root(&base.tree, &rows).root, result.root);
}

#[test]
fn a_directory_value_keeps_its_supplied_root_without_a_header_and_is_rebuilt_with_one() {
    let base = small();
    let metadata = synthetic("topology/changed-metadata");
    let supplied = InodeValue {
        // Content never opens a directory it has no header for.
        content_root: synthetic("topology/supplied-directory-root"),
        metadata_root: metadata,
        ..base.tree.stored(base.h)
    };
    let rows = Rowset::new().value(base.h, supplied);
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    let store = run.folded(&base.tree);
    let value = inode(&store, result.root, base.h).expect("h");
    assert_eq!(
        (value.content_root, value.metadata_root),
        (supplied.content_root, metadata),
        "without a header the supplied value is the whole statement"
    );
    assert_eq!(value.namespace_ref_count, 1);

    let fresh = base.tree.next;
    let rows = Rowset::new()
        .value(base.h, supplied)
        .fresh(fresh, InodeKind::RegularFile)
        .bind(base.h, "g", fresh);
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    let store = run.folded(&base.tree);
    let value = inode(&store, result.root, base.h).expect("h");
    assert_ne!(
        value.content_root, supplied.content_root,
        "with a header the rebuilt page replaces the supplied content root"
    );
    assert_eq!(value.metadata_root, metadata);
    assert_eq!(
        listing(&store, result.root, base.h),
        vec![("g".to_owned(), fresh)]
    );
    assert_eq!(resident_root(&base.tree, &rows).root, result.root);
}

#[test]
fn supplied_link_counts_are_ignored() {
    let base = small();
    let fresh = base.tree.next;
    let forged = InodeValue {
        namespace_ref_count: 99,
        ..typed(InodeKind::RegularFile)
    };
    let restated = InodeValue {
        namespace_ref_count: 77,
        ..base.tree.stored(base.f)
    };
    let rows = Rowset::new()
        .fresh(fresh, InodeKind::RegularFile)
        .value(fresh, forged)
        .value(base.f, restated)
        .bind(ROOT, "one", fresh)
        .bind(base.d, "two", fresh);
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    let store = run.folded(&base.tree);
    assert_eq!(
        inode(&store, result.root, fresh)
            .expect("fresh file")
            .namespace_ref_count,
        2,
        "two retained bindings, whatever the caller wrote"
    );
    assert_eq!(
        inode(&store, result.root, base.f)
            .expect("base file")
            .namespace_ref_count,
        1
    );
    assert_eq!(resident_root(&base.tree, &rows).root, result.root);
}

#[test]
fn no_backed_refusal_depends_on_the_ordering_budget_and_state_stays_one_window() {
    // `/holder/` and `/m/` with 70 directories of one file each.
    let mut shape = Shape::new();
    let holder = shape.dir(ROOT, "holder");
    let m = shape.dir(ROOT, "m");
    for index in 0..70 {
        let sub = shape.dir(m, &format!("s{index:02}"));
        shape.file(sub, "file");
    }
    let tree = shape.build();
    // One batch moves `m` under `holder` and creates a directory of 200 files
    // and 70 directories: 73 headers and 272 bound names.
    let n = tree.next;
    let mut rows = Rowset::new()
        .unbind(ROOT, "m")
        .bind(holder, "m", m)
        .mkdir(ROOT, "n", n);
    let mut next = n + 1;
    for index in 0..200 {
        rows = rows
            .fresh(next, InodeKind::RegularFile)
            .bind(n, &format!("f{index:03}"), next);
        next += 1;
    }
    for index in 0..70 {
        rows = rows.mkdir(n, &format!("d{index:02}"), next);
        next += 1;
    }
    // The smallest budget `FilesystemResources::check` admits: one ordering row.
    let tiny = FilesystemResources {
        ordering_bytes: 96,
        ..FilesystemResources::default()
    };
    let run = update_backed(&tree, &rows, tiny);
    let result = accepted(&run);
    let work = result.counters.validation;
    assert_eq!(work.peak_window_rows, WINDOW);
    assert_eq!(work.placements, 72);
    // Every placed directory is walked once and marked once.
    assert_eq!(work.ancestry_steps, 144);
    // `m` and its 70 directories are listed once: 70 names and 70 files.
    assert_eq!(
        (work.territory_directories, work.territory_entries),
        (71, 140)
    );
    assert!(run.records.maximum_job <= 65_536);
    assert!(run.records.maximum_keys <= 64);
    assert!(run.records.topology_batches > 0 && run.records.topology_enumerations > 0);
    eprintln!(
        "bounded-state: validation {work:?}; records calls {} maximum_job {} maximum_keys {} maximum_changes {} topology (reads, batches, enumerations) {:?}",
        run.records.calls,
        run.records.maximum_job,
        run.records.maximum_keys,
        run.records.maximum_changes,
        traffic(&run)
    );
    // The same rows under the same budget are refused where state is resident,
    // and accepted there with the default budget, with the same root.
    assert!(matches!(
        update_resident(&tree, &rows, tiny).0,
        Err(ContentError::ObjectLimitExceeded { .. })
    ));
    assert_eq!(resident_root(&tree, &rows).root, result.root);
}

// ---- R4-2: wide, deep and sparse ----

#[test]
fn a_wide_directory_change_is_classified_in_bounded_windows() {
    let mut shape = Shape::new();
    let w = shape.dir(ROOT, "w");
    for index in 0..300 {
        shape.file(w, &format!("old{index:03}"));
    }
    let tree = shape.build();
    let mut rows = Rowset::new();
    for index in (0..300).step_by(2) {
        rows = rows.unbind(w, &format!("old{index:03}"));
    }
    let mut next = tree.next;
    for index in 0..300 {
        rows = rows
            .fresh(next, InodeKind::RegularFile)
            .bind(w, &format!("new{index:03}"), next);
        next += 1;
    }
    for index in 0..50 {
        rows = rows.mkdir(w, &format!("dir{index:02}"), next);
        next += 1;
    }
    // 150 removals, 300 files and 50 directories in one header, and 50 headers.
    assert_eq!(rows.names(), 500);
    let run = update_backed(&tree, &rows, resources());
    let result = accepted(&run);
    let work = result.counters.validation;
    assert_eq!(work.peak_window_rows, WINDOW);
    assert_eq!((work.placements, work.ancestry_steps), (50, 100));
    assert_eq!((work.territory_directories, work.entries_examined), (0, 0));
    let resident = resident_root(&tree, &rows);
    assert_eq!(resident.root, result.root);
    // Without a backing the whole input is one window: 51 headers, 350 names.
    assert_eq!(resident.counters.validation.peak_window_rows, 401);
    assert_eq!(resident.counters.validation.placements, 50);
    let store = run.folded(&tree);
    assert_eq!(listing(&store, result.root, w).len(), 500);
}

#[test]
fn a_deep_chain_of_fresh_directories_is_one_linear_proof_in_either_serial_order() {
    const DEPTH: u64 = 200;
    let base = small();
    for descending in [false, true] {
        let serial =
            |level: u64| base.tree.next + if descending { DEPTH - 1 - level } else { level };
        let mut rows = Rowset::new();
        let mut parent = base.h;
        for level in 0..DEPTH {
            rows = rows.mkdir(parent, "n", serial(level));
            parent = serial(level);
        }
        let run = update_backed(&base.tree, &rows, resources());
        let result = accepted(&run);
        let work = result.counters.validation;
        assert_eq!(work.placements, DEPTH);
        // Ascending serials prove one step per directory; descending serials
        // prove the whole chain from its deepest directory once. Either way each
        // placed directory is stepped over once and marked once.
        assert_eq!(work.ancestry_steps, 2 * DEPTH, "descending {descending}");
        assert_eq!((work.territory_directories, work.entries_examined), (0, 0));
        assert_eq!(work.peak_window_rows, WINDOW);
        assert!(run.records.maximum_keys <= 64);
        assert_eq!(resident_root(&base.tree, &rows).root, result.root);
        let store = run.folded(&base.tree);
        assert_eq!(
            inode(&store, result.root, serial(DEPTH - 1))
                .expect("deepest directory")
                .namespace_ref_count,
            1
        );
    }
}

/// A stored chain `/c/c/.../c` of `depth` directories and one stored `/h/`.
fn chain(depth: usize) -> (Tree, Vec<u64>, u64) {
    let mut shape = Shape::new();
    let mut links = Vec::with_capacity(depth);
    let mut parent = ROOT;
    for _ in 0..depth {
        parent = shape.dir(parent, "c");
        links.push(parent);
    }
    shape.file(parent, "leaf");
    let h = shape.dir(ROOT, "h");
    (shape.build(), links, h)
}

#[test]
fn a_move_deep_inside_a_stored_chain_lists_only_the_moved_subtree() {
    let (tree, links, h) = chain(40);
    // The lower half moves under `/h`: 20 directories, 19 names and one file.
    let rows = Rowset::new().unbind(links[19], "c").bind(h, "c", links[20]);
    let run = update_backed(&tree, &rows, resources());
    let result = accepted(&run);
    let work = result.counters.validation;
    assert_eq!((work.placements, work.ancestry_steps), (1, 2));
    assert_eq!(
        (work.territory_directories, work.territory_entries),
        (20, 20)
    );
    assert_eq!(resident_root(&tree, &rows).root, result.root);

    // Moving an upper directory below its own stored descendant is a cycle that
    // only the territory shows: `links[30]` keeps its base position.
    let rows = Rowset::new()
        .unbind(links[4], "c")
        .bind(links[30], "moved", links[5]);
    let run = update_backed(&tree, &rows, resources());
    refused(&run, "effective tree cycle");
    assert!(run.emitted.is_empty(), "nothing is offered before a cycle");
    assert_eq!(
        update_resident(&tree, &rows, resources()).0.err(),
        Some(ContentError::InvalidRecord("effective tree cycle"))
    );
}

#[test]
fn sparse_changes_across_many_directories_match_the_resident_route() {
    let mut shape = Shape::new();
    let dirs: Vec<u64> = (0..40)
        .map(|index| shape.dir(ROOT, &format!("d{index:02}")))
        .collect();
    let mut files = Vec::new();
    for dir in &dirs {
        for index in 0..5 {
            files.push(shape.file(*dir, &format!("f{index}")));
        }
    }
    let inner = shape.dir(dirs[5], "inner");
    shape.file(inner, "leaf");
    let tree = shape.build();
    let fresh = tree.next;
    // A rename, a create, a removal and a directory move in distant parents.
    let rows = Rowset::new()
        .unbind(dirs[3], "f0")
        .bind(dirs[3], "renamed", files[15])
        .fresh(fresh, InodeKind::RegularFile)
        .bind(dirs[20], "new", fresh)
        .unbind(dirs[37], "f4")
        .unbind(dirs[5], "inner")
        .bind(dirs[30], "inner", inner);
    let run = update_backed(&tree, &rows, resources());
    let result = accepted(&run);
    let work = result.counters.validation;
    // Five headers and three bound names.
    assert_eq!(work.peak_window_rows, 8);
    assert_eq!((work.placements, work.ancestry_steps), (1, 2));
    // The moved directory alone is listed: one directory, one file.
    assert_eq!((work.territory_directories, work.territory_entries), (1, 1));
    assert_eq!(resident_root(&tree, &rows).root, result.root);
    let store = run.folded(&tree);
    let mut read = FilesystemRead::new(&store, result.root).expect("result");
    assert_eq!(
        read.resolve_child(dirs[30], &name_of("inner"))
            .expect("moved")
            .serial,
        inner
    );
    assert_eq!(
        read.resolve_child(dirs[3], &name_of("renamed"))
            .expect("renamed")
            .serial,
        files[15]
    );
    assert_eq!(inode(&store, result.root, files[37 * 5 + 4]), None);
}

// ---- R4-2: alias ----

#[test]
fn a_directory_placed_twice_in_one_batch_is_refused_before_anything_is_offered() {
    let base = small();
    let fresh = base.tree.next;
    // One window: the two placements meet in the window itself.
    let rows = Rowset::new()
        .fresh(fresh, InodeKind::Directory)
        .header(fresh)
        .bind(ROOT, "a", fresh)
        .bind(base.d, "b", fresh);
    let run = update_backed(&base.tree, &rows, resources());
    refused(&run, "multiple parents");
    assert!(run.emitted.is_empty());
    assert_eq!(run.records.topology_batches, 0);

    // Two windows: 70 files sort between the two bindings, so the second
    // placement meets the first one's record.
    let mut rows = rows;
    for index in 0..70 {
        let file = fresh + 1 + index;
        rows = rows
            .fresh(file, InodeKind::RegularFile)
            .bind(ROOT, &format!("f{index:02}"), file);
    }
    let run = update_backed(&base.tree, &rows, resources());
    refused(&run, "multiple parents");
    assert!(run.emitted.is_empty());
    assert_eq!(run.records.topology_batches, 1);
    assert!(run.records.values.contains_key(&key(PLACED, fresh)));
}

#[test]
fn a_stored_directory_bound_again_is_refused_by_its_derived_count_after_directory_pages() {
    let base = small();
    // `e` keeps `/d/e` and gains `/second`: every placement reaches the root,
    // so the proof accepts and the reducer's derived count of two refuses.
    let rows = Rowset::new().bind(ROOT, "second", base.e);
    let run = update_backed(&base.tree, &rows, resources());
    refused(&run, "multiple parents");
    assert!(run.records.values.contains_key(&key(PLACED, base.e)));
    // Exactly the rebuilt root directory page was offered before the refusal.
    assert_eq!(run.offered(), vec![ObjectRole::DirectoryLeaf]);
    assert_eq!(
        update_resident(&base.tree, &rows, resources()).0.err(),
        Some(ContentError::InvalidRecord("multiple parents"))
    );
}

#[test]
fn a_symlink_bound_twice_is_refused_by_its_derived_count() {
    let base = small();
    let fresh = base.tree.next;
    // A fresh symlink with two bindings in one batch.
    let rows = Rowset::new()
        .fresh(fresh, InodeKind::Symlink)
        .bind(ROOT, "s1", fresh)
        .bind(base.h, "s2", fresh);
    let run = update_backed(&base.tree, &rows, resources());
    refused(&run, "multiple parents");
    // The two rebuilt directory pages were offered; no inode page, no root.
    assert_eq!(
        run.offered(),
        vec![ObjectRole::DirectoryLeaf, ObjectRole::DirectoryLeaf]
    );
    assert_eq!(traffic(&run), (0, 0, 0));

    // A stored symlink that keeps its base binding and gains another.
    let rows = Rowset::new().bind(base.h, "again", base.l);
    let run = update_backed(&base.tree, &rows, resources());
    refused(&run, "multiple parents");
    assert_eq!(run.offered(), vec![ObjectRole::DirectoryLeaf]);
}

#[test]
fn a_move_that_unbinds_first_is_accepted_and_a_restated_binding_is_no_placement() {
    let base = small();
    let rows = Rowset::new()
        .unbind(base.d, "e")
        .bind(ROOT, "d", base.d)
        .bind(ROOT, "e", base.e);
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    let work = result.counters.validation;
    assert_eq!((work.placements, work.ancestry_steps), (1, 2));
    assert_eq!(work.territory_directories, 0);
    assert!(!run.records.values.contains_key(&key(PLACED, base.d)));
    let store = run.folded(&base.tree);
    assert_eq!(
        listing(&store, result.root, ROOT),
        vec![
            ("d".to_owned(), base.d),
            ("e".to_owned(), base.e),
            ("h".to_owned(), base.h),
            ("l".to_owned(), base.l)
        ]
    );
    assert_eq!(
        inode(&store, result.root, base.e)
            .expect("moved directory")
            .namespace_ref_count,
        1
    );
    assert_eq!(resident_root(&base.tree, &rows).root, result.root);
}

// ---- R4-2: cycle ----

/// A cycle is refused with its exact label on both routes and before the
/// backed route's consumer is offered anything.
fn cycle(tree: &Tree, rows: &Rowset) {
    let run = update_backed(tree, rows, resources());
    refused(&run, "effective tree cycle");
    assert!(run.emitted.is_empty(), "nothing is offered before a cycle");
    assert_eq!(run.records.after_failure, 0);
    let (resident, emitted) = update_resident(tree, rows, resources());
    assert_eq!(
        resident.err(),
        Some(ContentError::InvalidRecord("effective tree cycle"))
    );
    assert!(emitted.is_empty());
}

#[test]
fn a_directory_bound_inside_itself_is_a_cycle() {
    let base = small();
    cycle(
        &base.tree,
        &Rowset::new().unbind(ROOT, "d").bind(base.d, "self", base.d),
    );
}

#[test]
fn two_stored_directories_swapped_under_each_other_are_a_cycle() {
    let base = small();
    cycle(
        &base.tree,
        &Rowset::new()
            .unbind(ROOT, "d")
            .unbind(ROOT, "h")
            .bind(base.d, "h", base.h)
            .bind(base.h, "d", base.d),
    );
}

#[test]
fn a_directory_moved_below_its_own_stored_child_is_a_cycle() {
    let base = small();
    cycle(
        &base.tree,
        &Rowset::new().unbind(ROOT, "d").bind(base.e, "d", base.d),
    );
}

#[test]
fn a_cycle_through_a_fresh_directory_inside_the_moved_directory_is_a_cycle() {
    // The counter-example of Amendment 1: d -> p -> e -> d with `p` fresh.
    let base = small();
    let p = base.tree.next;
    cycle(
        &base.tree,
        &Rowset::new()
            .unbind(ROOT, "d")
            .mkdir(base.e, "p", p)
            .bind(p, "d", base.d),
    );
}

#[test]
fn a_cycle_of_fresh_directories_is_a_cycle() {
    let base = small();
    let a = base.tree.next;
    let b = a + 1;
    cycle(
        &base.tree,
        &Rowset::new()
            .fresh(a, InodeKind::Directory)
            .fresh(b, InodeKind::Directory)
            .bind(a, "b", b)
            .bind(b, "a", a),
    );
}

#[test]
fn a_fresh_directory_bound_only_inside_a_dropped_directory_is_refused() {
    // `dead` is allocated, carries a header and nothing binds it.
    let base = small();
    let dead = base.tree.next;
    let inside = dead + 1;
    cycle(
        &base.tree,
        &Rowset::new()
            .fresh(dead, InodeKind::Directory)
            .mkdir(dead, "inside", inside),
    );
}

#[test]
fn a_stored_directory_bound_or_moved_inside_a_dropped_directory_is_refused() {
    let base = small();
    let dead = base.tree.next;
    // `e` keeps `/d/e` and is bound again inside the dropped directory.
    cycle(
        &base.tree,
        &Rowset::new()
            .fresh(dead, InodeKind::Directory)
            .bind(dead, "again", base.e),
    );
    // `e` leaves `/d/e` and is bound only inside the dropped directory: its
    // derived count is one, so only the proof refuses the unreachable result.
    cycle(
        &base.tree,
        &Rowset::new()
            .fresh(dead, InodeKind::Directory)
            .unbind(base.d, "e")
            .bind(dead, "e", base.e),
    );
}

#[test]
fn a_stored_symlink_bound_again_inside_a_dropped_directory_has_two_parents() {
    // Symlinks are outside the topology proof: the derived count decides.
    let base = small();
    let dead = base.tree.next;
    let rows = Rowset::new()
        .fresh(dead, InodeKind::Directory)
        .bind(dead, "again", base.l);
    let run = update_backed(&base.tree, &rows, resources());
    refused(&run, "multiple parents");
    // The dropped directory builds no page and no other directory changed.
    assert!(run.emitted.is_empty());
    assert_eq!(traffic(&run), (0, 0, 0));
    assert_eq!(
        update_resident(&base.tree, &rows, resources()).0.err(),
        Some(ContentError::InvalidRecord("multiple parents"))
    );
}

#[test]
fn inverting_a_parent_and_its_child_is_a_legal_move_without_a_base_walk() {
    let base = small();
    // `/d/e` becomes `/e/d`: both directories are placed, so no placement lands
    // in a stored directory that kept its position and no territory is listed.
    let rows = Rowset::new()
        .unbind(ROOT, "d")
        .unbind(base.d, "e")
        .bind(ROOT, "e", base.e)
        .bind(base.e, "d", base.d);
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    let work = result.counters.validation;
    assert_eq!(work.placements, 2);
    // `d` walks through `e` to the root, two steps walked and two marked.
    assert_eq!(work.ancestry_steps, 4);
    assert_eq!((work.territory_directories, work.entries_examined), (0, 0));
    let store = run.folded(&base.tree);
    let mut read = FilesystemRead::new(&store, result.root).expect("result");
    assert_eq!(
        read.resolve_child(ROOT, &name_of("e"))
            .expect("e under the root")
            .serial,
        base.e
    );
    assert_eq!(
        read.resolve_child(base.e, &name_of("d"))
            .expect("d under e")
            .serial,
        base.d
    );
    assert_eq!(resident_root(&base.tree, &rows).root, result.root);
}

// ---- the base-less build route ----

#[test]
fn a_build_refuses_a_fresh_directory_bound_only_inside_a_dropped_directory_on_both_routes() {
    // The root binds nothing; `2` carries a header and is never bound; `3` is
    // bound only inside it.
    let rows = Rowset::new()
        .fresh(ROOT, InodeKind::Directory)
        .header(ROOT)
        .fresh(2, InodeKind::Directory)
        .mkdir(2, "inside", 3);
    let (backed, sink, records) = build_backed(&rows);
    assert_eq!(
        backed.err(),
        Some(ContentError::InvalidRecord("effective tree cycle"))
    );
    assert!(sink.is_empty(), "nothing is offered before the refusal");
    assert_eq!(records.after_failure, 0);
    let (resident, sink) = build_resident(&rows);
    assert_eq!(
        resident.err(),
        Some(ContentError::InvalidRecord("effective tree cycle"))
    );
    assert!(sink.is_empty());
}

#[test]
fn a_build_cycle_is_refused_and_a_deep_build_is_one_linear_proof() {
    let rows = Rowset::new()
        .fresh(ROOT, InodeKind::Directory)
        .header(ROOT)
        .fresh(2, InodeKind::Directory)
        .fresh(3, InodeKind::Directory)
        .bind(2, "b", 3)
        .bind(3, "a", 2);
    let (backed, sink, _) = build_backed(&rows);
    assert_eq!(
        backed.err(),
        Some(ContentError::InvalidRecord("effective tree cycle"))
    );
    assert!(sink.is_empty());

    const DEPTH: u64 = 150;
    let mut rows = Rowset::new().fresh(ROOT, InodeKind::Directory).header(ROOT);
    for level in 0..DEPTH {
        rows = rows.mkdir(ROOT + level, "n", ROOT + level + 1);
    }
    let (backed, _, records) = build_backed(&rows);
    let backed = backed.expect("a deep build");
    let work = backed.counters.validation;
    assert_eq!((work.placements, work.ancestry_steps), (DEPTH, 2 * DEPTH));
    assert_eq!(work.peak_window_rows, WINDOW);
    assert_eq!((work.territory_directories, work.inode_pages_read), (0, 0));
    assert!(records.maximum_keys <= 64 && records.maximum_job <= 65_536);
    let resident = build_resident(&rows).0.expect("the resident build");
    assert_eq!(resident.root, backed.root);
    assert_eq!(resident.counters.validation.placements, DEPTH);
}

// ---- a stored directory renamed inside its own base parent ----

/// The in-place pass's counters: parents scanned, change rows read, renames.
fn in_place(result: &FilesystemResult) -> (u64, u64, u64) {
    let work = result.counters.validation;
    (
        work.in_place_scans,
        work.in_place_rows,
        work.in_place_directories,
    )
}

/// Territory directories and entries, and base entries examined.
fn walked(result: &FilesystemResult) -> (u64, u64, u64) {
    let work = result.counters.validation;
    (
        work.territory_directories,
        work.territory_entries,
        work.entries_examined,
    )
}

#[test]
fn a_deep_directory_renamed_in_place_under_a_stored_parent_walks_nothing() {
    // `links[5]` holds 34 nested directories and stays under `links[4]`.
    let (tree, links, _h) = chain(40);
    let rows = Rowset::new()
        .unbind(links[4], "c")
        .bind(links[4], "renamed", links[5]);
    let run = update_backed(&tree, &rows, resources());
    let result = accepted(&run);
    // One parent scanned, its two change rows read, one rename found.
    assert_eq!(in_place(&result), (1, 2, 1));
    assert_eq!(walked(&result), (0, 0, 0));
    assert_eq!(result.counters.validation.placements, 1);
    assert_eq!(result.counters.validation.ancestry_steps, 0);
    let resident = resident_root(&tree, &rows);
    assert_eq!(resident.root, result.root);
    assert_eq!(in_place(&resident), (1, 2, 1));
    assert_eq!(walked(&resident), (0, 0, 0));
    let store = run.folded(&tree);
    assert_eq!(
        listing(&store, result.root, links[4]),
        vec![("renamed".to_owned(), links[5])]
    );
}

#[test]
fn many_renames_in_one_parent_scan_that_parent_once() {
    // `/p/` holds 64 directories of one file each.
    let mut shape = Shape::new();
    let p = shape.dir(ROOT, "p");
    let children: Vec<u64> = (0..64)
        .map(|index| {
            let child = shape.dir(p, &format!("d{index:02}"));
            shape.file(child, "file");
            child
        })
        .collect();
    let tree = shape.build();
    let mut observed = Vec::new();
    for renames in [8_u64, 64] {
        let mut rows = Rowset::new();
        for (index, child) in children.iter().take(renames as usize).enumerate() {
            rows = rows
                .unbind(p, &format!("d{index:02}"))
                .bind(p, &format!("r{index:02}"), *child);
        }
        let run = update_backed(&tree, &rows, resources());
        let result = accepted(&run);
        // One scan of `p` whatever the number of renames, two rows per rename.
        assert_eq!(in_place(&result), (1, 2 * renames, renames));
        assert_eq!(walked(&result), (0, 0, 0));
        assert_eq!(result.counters.validation.placements, renames);
        assert_eq!(result.counters.validation.ancestry_steps, 0);
        assert!(run.records.maximum_keys <= 64 && run.records.maximum_job <= 65_536);
        assert_eq!(resident_root(&tree, &rows).root, result.root);
        eprintln!(
            "in-place renames {renames}: validation {:?}; topology (reads, batches, enumerations) {:?}",
            result.counters.validation,
            traffic(&run)
        );
        observed.push((renames, traffic(&run).0 as u64));
    }
    // Record point reads grow with the renames, never with their square.
    let (few, few_reads) = observed[0];
    let (many, many_reads) = observed[1];
    assert!(
        many_reads * few <= few_reads * many + few * many,
        "reads per rename must not grow: {observed:?}"
    );
}

/// `/a/m/x/deep/leaf`, `/a/s/` with ten directories, and `/h/`.
struct Mixed {
    tree: Tree,
    a: u64,
    m: u64,
    x: u64,
    deep: u64,
    s: u64,
    h: u64,
}

fn mixed() -> Mixed {
    let mut shape = Shape::new();
    let a = shape.dir(ROOT, "a");
    let m = shape.dir(a, "m");
    let x = shape.dir(m, "x");
    let deep = shape.dir(x, "deep");
    shape.file(deep, "leaf");
    let s = shape.dir(a, "s");
    for index in 0..10 {
        let inner = shape.dir(s, &format!("i{index}"));
        shape.file(inner, "file");
    }
    let h = shape.dir(ROOT, "h");
    Mixed {
        tree: shape.build(),
        a,
        m,
        x,
        deep,
        s,
        h,
    }
}

#[test]
fn a_rename_in_place_beside_a_real_move_walks_only_the_moved_subtree() {
    let base = mixed();
    // `m` moves under `/h`; inside it `x` is renamed in place; beside it `s`,
    // with its ten directories, is renamed in place and must not be listed.
    let fresh = base.tree.next;
    let rows = Rowset::new()
        .unbind(base.a, "m")
        .bind(base.h, "m", base.m)
        .unbind(base.m, "x")
        .bind(base.m, "y", base.x)
        .unbind(base.a, "s")
        .bind(base.a, "t", base.s)
        .mkdir(base.deep, "new", fresh);
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    // `h`, `m` and `a` are scanned once: one, two and three change rows. `x`
    // and `s` are renames; `m` left `a`, so its removed name marks nothing.
    assert_eq!(in_place(&result), (3, 6, 2));
    // The walk lists `m`, descends through the renamed `x` and lists `deep`:
    // the name `y`, the name `deep`, and `leaf` with the fresh `new`.
    assert_eq!(
        (
            result.counters.validation.territory_directories,
            result.counters.validation.territory_entries
        ),
        (3, 4)
    );
    // The fresh directory under `deep` climbs to `m` through the territory.
    assert_eq!(result.counters.validation.placements, 4);
    let resident = resident_root(&base.tree, &rows);
    assert_eq!(resident.root, result.root);
    assert_eq!(in_place(&resident), (3, 6, 2));
    let store = run.folded(&base.tree);
    assert_eq!(
        listing(&store, result.root, base.m),
        vec![("y".to_owned(), base.x)]
    );
    assert_eq!(listing(&store, result.root, base.s).len(), 10);
}

#[test]
fn a_cycle_through_a_directory_renamed_in_place_is_still_a_cycle() {
    let base = mixed();
    // `x` is renamed in place inside `m`, and `m` moves under `x` itself.
    cycle(
        &base.tree,
        &Rowset::new()
            .unbind(base.a, "m")
            .unbind(base.m, "x")
            .bind(base.m, "y", base.x)
            .bind(base.x, "m", base.m),
    );
    // The same rename, and `m` moves under `deep`, a descendant of the renamed
    // directory: the walk has to descend through `x` to find it.
    cycle(
        &base.tree,
        &Rowset::new()
            .unbind(base.a, "m")
            .unbind(base.m, "x")
            .bind(base.m, "y", base.x)
            .bind(base.deep, "m", base.m),
    );
    // The renamed directory's own parent moves under it.
    cycle(
        &base.tree,
        &Rowset::new()
            .unbind(ROOT, "a")
            .unbind(base.a, "s")
            .bind(base.a, "t", base.s)
            .bind(base.s, "a", base.a),
    );
}

#[test]
fn a_rename_in_place_with_a_second_binding_elsewhere_has_two_parents() {
    let base = mixed();
    let rows = Rowset::new()
        .unbind(base.m, "x")
        .bind(base.m, "y", base.x)
        .bind(base.h, "also", base.x);
    let run = update_backed(&base.tree, &rows, resources());
    refused(&run, "multiple parents");
    assert!(
        run.emitted.is_empty(),
        "two placements meet in classification"
    );
    let (resident, emitted) = update_resident(&base.tree, &rows, resources());
    assert_eq!(
        resident.err(),
        Some(ContentError::InvalidRecord("multiple parents"))
    );
    assert!(emitted.is_empty());
}

#[test]
fn a_rename_in_the_root_beside_a_placement_under_a_stored_directory_walks_nothing() {
    let base = mixed();
    // `/a` becomes `/b` in the root while a directory is created under `/h`:
    // the creation lands in a stored directory, and the root's own scan shows
    // that `a` never left it.
    let fresh = base.tree.next;
    let rows = Rowset::new()
        .unbind(ROOT, "a")
        .bind(ROOT, "b", base.a)
        .mkdir(base.h, "new", fresh);
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    assert_eq!(in_place(&result), (1, 2, 1));
    assert_eq!(walked(&result), (0, 0, 0));
    assert_eq!(resident_root(&base.tree, &rows).root, result.root);
}

#[test]
fn a_name_swap_of_two_stored_directories_in_one_parent_is_in_place() {
    let base = mixed();
    // `m` and `s` exchange names inside `a`: no name is removed, each row
    // displaces the other's base binding, and both stay under `a`.
    let rows = Rowset::new()
        .bind(base.a, "m", base.s)
        .bind(base.a, "s", base.m);
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    assert_eq!(in_place(&result), (1, 2, 2));
    assert_eq!(walked(&result), (0, 0, 0));
    assert_eq!(result.counters.validation.ancestry_steps, 0);
    let resident = resident_root(&base.tree, &rows);
    assert_eq!(resident.root, result.root);
    assert_eq!(in_place(&resident), (1, 2, 2));
    let store = run.folded(&base.tree);
    assert_eq!(
        listing(&store, result.root, base.a),
        vec![("m".to_owned(), base.s), ("s".to_owned(), base.m)]
    );
}

#[test]
fn a_rename_with_the_old_name_recreated_walks_nothing() {
    // `mv c c.old; mkdir c` under `links[4]`: the old name is not removed, it
    // is bound to a fresh directory. `links[5]` keeps its 34 nested
    // directories and its base parent.
    let (tree, links, _h) = chain(40);
    let fresh = tree.next;
    let rows = Rowset::new()
        .bind(links[4], "c.old", links[5])
        .mkdir(links[4], "c", fresh);
    let run = update_backed(&tree, &rows, resources());
    let result = accepted(&run);
    assert_eq!(in_place(&result), (1, 2, 1));
    assert_eq!(walked(&result), (0, 0, 0));
    // The renamed directory takes no step; the fresh one climbs one, twice.
    let work = result.counters.validation;
    assert_eq!((work.placements, work.ancestry_steps), (2, 2));
    let resident = resident_root(&tree, &rows);
    assert_eq!(resident.root, result.root);
    assert_eq!(in_place(&resident), (1, 2, 1));
    assert_eq!(walked(&resident), (0, 0, 0));
    let store = run.folded(&tree);
    assert_eq!(
        listing(&store, result.root, links[4]),
        vec![("c".to_owned(), fresh), ("c.old".to_owned(), links[5])]
    );
    assert_eq!(
        inode(&store, result.root, links[5])
            .expect("renamed directory")
            .namespace_ref_count,
        1
    );
}

#[test]
fn a_rotation_of_two_stored_directories_in_one_parent_walks_nothing() {
    // `mv m previous; mv s m` inside `a`: `m`'s old name goes to `s`, and
    // `s`'s old name is removed. Neither directory leaves `a`.
    let base = mixed();
    let rows = Rowset::new()
        .bind(base.a, "previous", base.m)
        .bind(base.a, "m", base.s)
        .unbind(base.a, "s");
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    assert_eq!(in_place(&result), (1, 3, 2));
    assert_eq!(walked(&result), (0, 0, 0));
    let work = result.counters.validation;
    assert_eq!((work.placements, work.ancestry_steps), (2, 0));
    let resident = resident_root(&base.tree, &rows);
    assert_eq!(resident.root, result.root);
    assert_eq!(in_place(&resident), (1, 3, 2));
    assert_eq!(walked(&resident), (0, 0, 0));
    let store = run.folded(&base.tree);
    assert_eq!(
        listing(&store, result.root, base.a),
        vec![("m".to_owned(), base.s), ("previous".to_owned(), base.m)]
    );
}

#[test]
fn a_cycle_through_a_directory_renamed_over_a_recreated_name_is_still_a_cycle() {
    let base = mixed();
    let fresh = base.tree.next;
    // `mv x x.old; mkdir x` inside `m`, and `m` moves under the renamed `x`.
    cycle(
        &base.tree,
        &Rowset::new()
            .bind(base.m, "x.old", base.x)
            .mkdir(base.m, "x", fresh)
            .unbind(base.a, "m")
            .bind(base.x, "m", base.m),
    );
    // The same rename, and `m` moves under `deep`, below the renamed directory.
    cycle(
        &base.tree,
        &Rowset::new()
            .bind(base.m, "x.old", base.x)
            .mkdir(base.m, "x", fresh)
            .unbind(base.a, "m")
            .bind(base.deep, "m", base.m),
    );
    // A swap of `m` and `s` inside `a`, and `a` moves under the swapped `s`.
    cycle(
        &base.tree,
        &Rowset::new()
            .bind(base.a, "m", base.s)
            .bind(base.a, "s", base.m)
            .unbind(ROOT, "a")
            .bind(base.s, "a", base.a),
    );
}

#[test]
fn a_rename_whose_directory_is_also_bound_elsewhere_still_has_two_parents() {
    let base = mixed();
    let fresh = base.tree.next;
    // Renamed over a recreated name inside `m` and bound again under `h`: two
    // placements of one directory meet in classification.
    let rows = Rowset::new()
        .bind(base.m, "x.old", base.x)
        .mkdir(base.m, "x", fresh)
        .bind(base.h, "also", base.x);
    let run = update_backed(&base.tree, &rows, resources());
    refused(&run, "multiple parents");
    assert!(run.emitted.is_empty());
    let (resident, emitted) = update_resident(&base.tree, &rows, resources());
    assert_eq!(
        resident.err(),
        Some(ContentError::InvalidRecord("multiple parents"))
    );
    assert!(emitted.is_empty());

    // The old name restated and a second name in the same parent: the base
    // binding is not displaced, so the directory is not in place, its subtree
    // is listed, and its derived count of two refuses it after `m`'s page.
    let rows = Rowset::new()
        .bind(base.m, "x", base.x)
        .bind(base.m, "x.old", base.x);
    let run = update_backed(&base.tree, &rows, resources());
    refused(&run, "multiple parents");
    assert!(run.records.values.contains_key(&key(PLACED, base.x)));
    assert_eq!(run.offered(), vec![ObjectRole::DirectoryLeaf]);
    assert_eq!(
        update_resident(&base.tree, &rows, resources()).0.err(),
        Some(ContentError::InvalidRecord("multiple parents"))
    );
}

// ---- the memo keeps what a window demands ----

#[test]
fn consecutive_windows_sharing_stored_children_read_them_once() {
    // `/w/` holds 40 files. Six rounds each add one more name to every file
    // and 30 new files, so every window of 64 rows names stored files an
    // earlier window already demanded together with serials no window has
    // seen: 420 bound names over seven windows.
    let mut shape = Shape::new();
    let w = shape.dir(ROOT, "w");
    let files: Vec<u64> = (0..40)
        .map(|index| shape.file(w, &format!("g{index:02}")))
        .collect();
    let tree = shape.build();
    let mut rows = Rowset::new();
    let mut next = tree.next;
    for round in 0..6 {
        for (index, file) in files.iter().enumerate() {
            rows = rows.bind(w, &format!("r{round}a{index:02}"), *file);
        }
        for index in 0..30 {
            rows = rows.fresh(next, InodeKind::RegularFile).bind(
                w,
                &format!("r{round}z{index:02}"),
                next,
            );
            next += 1;
        }
    }
    let run = update_backed(&tree, &rows, resources());
    let result = accepted(&run);
    let work = result.counters.validation;
    assert_eq!(work.peak_window_rows, WINDOW);
    // Every record classification asks for comes from its window's grouped
    // demand: no single-serial descent re-reads a record the memo dropped.
    assert_eq!(work.inode_pages_by_site.bindings, 0, "{work:?}");
    assert!(work.inode_pages_by_site.prefetch > 0);
    assert_eq!(
        work.inode_pages_read,
        work.inode_pages_by_site.allocation + work.inode_pages_by_site.prefetch
    );
    assert_eq!(resident_root(&tree, &rows).root, result.root);
    let store = run.folded(&tree);
    assert_eq!(
        inode(&store, result.root, files[0])
            .expect("linked file")
            .namespace_ref_count,
        7
    );
}

// ---- cycle shapes a random stream does not reach ----

/// `/m1/d1/`, `/top/m2/d2/`: two movers, each with one stored child.
struct Pair {
    tree: Tree,
    m1: u64,
    d1: u64,
    top: u64,
    m2: u64,
    d2: u64,
}

fn pair() -> Pair {
    let mut shape = Shape::new();
    let m1 = shape.dir(ROOT, "m1");
    let d1 = shape.dir(m1, "d1");
    let top = shape.dir(ROOT, "top");
    let m2 = shape.dir(top, "m2");
    let d2 = shape.dir(m2, "d2");
    Pair {
        tree: shape.build(),
        m1,
        d1,
        top,
        m2,
        d2,
    }
}

#[test]
fn two_moved_directories_each_under_the_others_stored_child_are_a_cycle() {
    // m1 -> d2 -> m2 -> d1 -> m1. Each placement lands in a stored directory
    // that kept its base parent, so no placed edge joins two placed
    // directories: only the territory marks on `d1` and `d2` close the walk.
    let base = pair();
    cycle(
        &base.tree,
        &Rowset::new()
            .unbind(ROOT, "m1")
            .unbind(base.top, "m2")
            .bind(base.d2, "m1", base.m1)
            .bind(base.d1, "m2", base.m2),
    );
}

#[test]
fn one_moved_directory_under_another_ones_stored_child_is_accepted() {
    let base = pair();
    // `m1` moves under `d2` while `m2` stays where it is.
    let rows = Rowset::new()
        .unbind(ROOT, "m1")
        .bind(base.d2, "m1", base.m1);
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    let work = result.counters.validation;
    assert_eq!((work.placements, work.ancestry_steps), (1, 2));
    // `m1` and `d1` are listed; `d2` kept its base position and ends the walk.
    assert_eq!((work.territory_directories, work.territory_entries), (2, 1));
    assert_eq!(resident_root(&base.tree, &rows).root, result.root);

    // `m1` moves under `d2` and `m2` itself moves to the root: `d2` now lies
    // in `m2`'s territory and the walk continues through it to the root.
    let rows = Rowset::new()
        .unbind(ROOT, "m1")
        .bind(base.d2, "m1", base.m1)
        .unbind(base.top, "m2")
        .bind(ROOT, "m2", base.m2);
    let run = update_backed(&base.tree, &rows, resources());
    let result = accepted(&run);
    let work = result.counters.validation;
    assert_eq!(work.placements, 2);
    // `m1` climbs `d2`, `m2` and the root: three steps walked and three marked.
    assert_eq!(work.ancestry_steps, 6);
    // `m1`, `d1`, `m2` and `d2` are listed; `d2` now also lists `m1`.
    assert_eq!((work.territory_directories, work.territory_entries), (4, 3));
    assert_eq!(resident_root(&base.tree, &rows).root, result.root);
    let store = run.folded(&base.tree);
    assert_eq!(
        listing(&store, result.root, base.d2),
        vec![("m1".to_owned(), base.m1)]
    );
    assert_eq!(listing(&store, result.root, base.top), Vec::new());
}

#[test]
fn a_ring_of_seventy_fresh_and_stored_directories_is_a_cycle() {
    // s0 -> f0 -> s1 -> f1 -> ... -> s34 -> f34 -> s0: 70 placements, more
    // than one classification window and more than one record window.
    let mut shape = Shape::new();
    let stored: Vec<u64> = (0..35)
        .map(|index| shape.dir(ROOT, &format!("s{index:02}")))
        .collect();
    let tree = shape.build();
    let mut rows = Rowset::new();
    for (index, directory) in stored.iter().enumerate() {
        let fresh = tree.next + index as u64;
        rows = rows
            .unbind(ROOT, &format!("s{index:02}"))
            .fresh(fresh, InodeKind::Directory)
            .bind(fresh, "s", *directory)
            .bind(stored[(index + 1) % 35], "f", fresh);
    }
    cycle(&tree, &rows);
    // One more stored directory cut out of the ring makes it a chain to the
    // root, and the same 70 placements are accepted.
    let rows = rows.unbind(stored[0], "f").bind(ROOT, "f", tree.next + 34);
    let run = update_backed(&tree, &rows, resources());
    let result = accepted(&run);
    let work = result.counters.validation;
    assert_eq!(work.placements, 70);
    assert_eq!(work.ancestry_steps, 140);
    assert_eq!(work.peak_window_rows, WINDOW);
    assert_eq!(resident_root(&tree, &rows).root, result.root);
}

/// `count` stored movers under the root, each with one stored child `c`, and
/// `/h/`. Movers are allocated in ascending or descending ring order.
fn movers(count: usize, descending: bool) -> (Tree, Vec<u64>, Vec<u64>, u64) {
    let mut shape = Shape::new();
    let mut movers = vec![0; count];
    let mut children = vec![0; count];
    for step in 0..count {
        let index = if descending { count - 1 - step } else { step };
        movers[index] = shape.dir(ROOT, &format!("m{index:03}"));
        children[index] = shape.dir(movers[index], "c");
    }
    let h = shape.dir(ROOT, "h");
    (shape.build(), movers, children, h)
}

#[test]
fn a_ring_of_seventy_moves_through_territory_is_a_cycle() {
    // m0 -> c1 -> m1 -> c2 -> ... -> m69 -> c0 -> m0: every mover lands in the
    // stored child of the next, so the ring alternates one placed edge and one
    // territory step, 140 edges for 70 placements.
    let (tree, movers, children, _h) = movers(70, false);
    let mut rows = Rowset::new();
    for (index, mover) in movers.iter().enumerate() {
        rows = rows.unbind(ROOT, &format!("m{index:03}")).bind(
            children[(index + 1) % 70],
            "m",
            *mover,
        );
    }
    cycle(&tree, &rows);
}

#[test]
fn a_legal_walk_as_long_as_the_step_bound_allows_is_accepted_and_linear() {
    // m0 under c1, m1 under c2, ..., and the last mover under `/h`: one upward
    // walk of P placed edges and P - 1 territory steps, against a bound of
    // 2P + 2. Whichever end the serial order starts from, every edge is walked
    // once and marked once.
    for count in [70_usize, 140] {
        for descending in [false, true] {
            let (tree, movers, children, h) = movers(count, descending);
            let mut rows = Rowset::new();
            for (index, mover) in movers.iter().enumerate() {
                let under = if index + 1 == count {
                    h
                } else {
                    children[index + 1]
                };
                rows = rows
                    .unbind(ROOT, &format!("m{index:03}"))
                    .bind(under, "m", *mover);
            }
            let run = update_backed(&tree, &rows, resources());
            let result = accepted(&run);
            let work = result.counters.validation;
            let placed = count as u64;
            assert_eq!(work.placements, placed);
            assert_eq!(
                work.ancestry_steps,
                4 * placed - 2,
                "count {count} descending {descending}"
            );
            // Every mover and its child are listed once: one name in each
            // mover, and the mover placed in each child but the first.
            assert_eq!(
                (work.territory_directories, work.territory_entries),
                (2 * placed, 2 * placed - 1)
            );
            assert_eq!(work.peak_window_rows, WINDOW);
            assert!(run.records.maximum_keys <= 64 && run.records.maximum_job <= 65_536);
            let resident = resident_root(&tree, &rows);
            assert_eq!(resident.root, result.root);
            assert_eq!(resident.counters.validation.ancestry_steps, 4 * placed - 2);
        }
    }
}

#[test]
fn the_refusal_labels_pinned_on_the_resident_route_hold_on_the_backed_route() {
    let base = small();
    let a = base.tree.next;
    let b = a + 1;
    // A stored directory moved below its own child, its base binding restated.
    cycle(
        &base.tree,
        &Rowset::new()
            .bind(ROOT, "d", base.d)
            .bind(base.d, "e", base.e)
            .bind(base.e, "d", base.d),
    );
    // A stored directory bound inside itself, its base binding kept.
    cycle(&base.tree, &Rowset::new().bind(base.d, "self", base.d));
    // Two fresh directories holding each other, with nothing under the root.
    cycle(
        &base.tree,
        &Rowset::new()
            .header(ROOT)
            .fresh(a, InodeKind::Directory)
            .fresh(b, InodeKind::Directory)
            .bind(a, "b", b)
            .bind(b, "a", a),
    );
    // A fresh directory bound under a stored one and again under its own
    // fresh child: two placements, refused in classification.
    for under in [base.d, ROOT] {
        let rows = Rowset::new()
            .fresh(a, InodeKind::Directory)
            .fresh(b, InodeKind::Directory)
            .bind(under, "a", a)
            .bind(a, "b", b)
            .bind(b, "a", a);
        let run = update_backed(&base.tree, &rows, resources());
        refused(&run, "multiple parents");
        assert!(run.emitted.is_empty());
        let (resident, emitted) = update_resident(&base.tree, &rows, resources());
        assert_eq!(
            resident.err(),
            Some(ContentError::InvalidRecord("multiple parents"))
        );
        assert!(emitted.is_empty());
    }
}
