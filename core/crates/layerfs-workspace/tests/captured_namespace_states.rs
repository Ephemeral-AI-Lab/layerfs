//! R4-3: every captured state of the R3 table, each as its own case. The
//! recording provider shows which rows the capture really held; the result is
//! always the complete walk against the independent model.
mod common;
mod harness;
mod oracle;
mod producer;
use harness::{Bench, T1};
use layerfs_overlay::{Inode, InodeKind, OpenFile};
use layerfs_workspace::{Operation, Outcome, Position};
use oracle::{absent, value};
use producer::{
    build_over, drain, evidence, record, Built, Call, Drive, Log, Recording, FRESH, HEADER, VALUE,
};

fn number(value: u64) -> Option<Vec<u8>> {
    Some(value.to_be_bytes().to_vec())
}
/// Every name row the whole-capture sequence returned, in order.
fn names(log: &Log) -> Vec<(u64, Vec<u8>, Option<u64>)> {
    log.name_pages
        .iter()
        .flat_map(|page| &page.rows)
        .map(|row| (row.parent, row.name.clone(), row.serial))
        .collect()
}
fn bound(log: &Log, parent: u64, name: &str) -> Option<Option<u64>> {
    names(log)
        .into_iter()
        .find(|row| row.0 == parent && row.1 == name.as_bytes())
        .map(|row| row.2)
}
/// Every inode row the capture returned, in order.
fn inodes(log: &Log) -> Vec<Inode> {
    log.inode_pages
        .iter()
        .flat_map(|(_, rows)| rows.iter().cloned())
        .collect()
}
fn inode(log: &Log, serial: u64) -> Option<Inode> {
    inodes(log).into_iter().find(|row| row.serial == serial)
}
/// A writable descriptor of a file, opened before any name is removed.
fn hold(b: &Bench, serial: u64) -> OpenFile {
    b.window(|view| {
        let fact = match b.overlay.inode(b.route(), serial).unwrap() {
            Some(local) => local,
            None => {
                let stat = view.stat(&b.overlay, serial).unwrap();
                Inode {
                    serial,
                    kind: InodeKind::File,
                    mode: u16::try_from(stat.metadata.mode).unwrap(),
                    mtime_seconds: stat.metadata.mtime_seconds,
                    mtime_nanoseconds: stat.metadata.mtime_nanoseconds,
                    nlink: stat.namespace_refs,
                    size: stat.logical_len,
                    inherited_cutoff: stat.logical_len,
                    born: 0,
                    entries: 0,
                }
            }
        };
        b.overlay
            .open_file(view.source(), 9_000 + serial, &fact, true)
            .unwrap()
    })
}
/// Content's validation work of one moved-directory case, for the receipt.
fn walked(built: &Built, what: &str) {
    evidence(format_args!(
        "R4-3 {what}: validation {:?}",
        built.content.validation
    ));
}
fn file_calls(log: &Log) -> u64 {
    log.count(Call::RunStep) + log.count(Call::FileApply) + log.count(Call::FileRead)
}

// Non-root inode with no link: nothing for the inode, no content construction.

#[test]
fn a_base_file_unlinked_while_held_is_absent_and_never_constructed() {
    let b = Bench::new("cs-held");
    let mut d = Drive::new(&b);
    let held = hold(&b, 2);
    d.write("file", 0, b"written before the unlink");
    d.remove("file");
    d.remove("alias");
    // A descriptor write after the last name is gone.
    let later = Operation::WriteOpen {
        file: held,
        position: Position::At(4),
        data: b"held".as_slice().into(),
    };
    assert!(matches!(b.run(later, T1).unwrap(), Outcome::Applied { .. }));

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "held");
    let log = recording.log.borrow();
    let row = inode(&log, 2).expect("the held file keeps its captured row");
    assert_eq!((row.nlink, row.born), (0, 0));
    assert!(absent(&b.fixture.store, built.root, 2));
    assert_eq!(built.work.files_constructed, 0);
    assert_eq!(built.work.tombstones_skipped, 1);
    assert_eq!(file_calls(&log), 0, "no file work for a held orphan");
    assert_eq!(record(&b, &built.taken, VALUE, 2), None);
    drop(log);
    built.release(&b);
    b.overlay.close_file(held).unwrap();
}

#[test]
fn a_removed_base_directory_keeps_its_whiteouts_and_has_no_value() {
    let b = Bench::new("cs-rmdir");
    let mut d = Drive::new(&b);
    d.remove("cache/state");
    d.remove("cache");

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "rmdir");
    let log = recording.log.borrow();
    // The capture holds a removal under the removed base parent itself.
    assert_eq!(bound(&log, 6, "state"), Some(None));
    assert_eq!(bound(&log, 1, "cache"), Some(None));
    assert_eq!(inode(&log, 6).map(|row| row.nlink), Some(0));
    // Its header is kept, with exactly that one change; it has no value.
    assert_eq!(record(&b, &built.taken, HEADER, 6), number(1));
    assert_eq!(record(&b, &built.taken, VALUE, 6), None);
    assert_eq!(built.work.headers_dropped, 0);
    assert!(absent(&b.fixture.store, built.root, 6));
    assert_eq!(
        value(&b.fixture.store, built.root, 8).namespace_ref_count,
        3
    );
    drop(log);
    built.release(&b);
}

#[test]
fn inodes_created_and_removed_inside_the_capture_are_never_constructed() {
    let b = Bench::new("cs-transient");
    let mut d = Drive::new(&b);
    let file = d.create("temp", 0o644);
    d.write("temp", 0, &[3; 90_000]);
    let link = d.symlink("temp-link", b"temp");
    let directory = d.mkdir("temp-dir", 0o755);
    d.remove("temp");
    d.remove("temp-link");
    d.remove("temp-dir");

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "transient");
    let log = recording.log.borrow();
    let work = built.work;
    assert_eq!((work.fresh_serials, work.files_constructed), (0, 0));
    assert_eq!(work.symlinks, 0);
    assert_eq!(file_calls(&log) + log.count(Call::Symlink), 0);
    for serial in [file, link, directory] {
        assert!(absent(&b.fixture.store, built.root, serial));
        assert_eq!(record(&b, &built.taken, VALUE, serial), None);
        assert_eq!(record(&b, &built.taken, FRESH, serial), None);
        assert_eq!(record(&b, &built.taken, HEADER, serial), None);
        // A retained row of a transient inode can only be a tombstone.
        if let Some(row) = inode(&log, serial) {
            assert_eq!(row.nlink, 0, "serial {serial}");
        }
    }
    // Only the touched root has a value: its time changed, its names did not.
    assert_eq!(work.values_written, 1);
    drop(log);
    built.release(&b);
}

// Names under a parent created and removed inside the capture: header dropped.

#[test]
fn names_under_a_fresh_removed_directory_never_reach_the_update() {
    let b = Bench::new("cs-dropped");
    let mut d = Drive::new(&b);
    let gone = d.mkdir("gone", 0o755);
    let sub = d.mkdir("gone/sub", 0o755);
    d.create("gone/a", 0o644);
    // A base file passes through the transient directory and leaves again.
    d.rename("file", "gone/visitor");
    d.rename("gone/visitor", "gone/sub/visitor");
    d.rename("gone/sub/visitor", "file");
    d.remove("gone/a");
    d.remove("gone/sub");
    d.remove("gone");

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "dropped");
    let log = recording.log.borrow();
    // Whatever rows the capture still holds under the two tombstones, each
    // such parent is dropped exactly once and is never opened by the update.
    let mut parents: Vec<u64> = names(&log)
        .into_iter()
        .map(|row| row.0)
        .filter(|parent| [gone, sub].contains(parent))
        .collect();
    parents.dedup();
    assert_eq!(built.work.headers_dropped, parents.len() as u64);
    for serial in [gone, sub] {
        assert_eq!(record(&b, &built.taken, HEADER, serial), None);
        assert_eq!(record(&b, &built.taken, VALUE, serial), None);
        assert!(absent(&b.fixture.store, built.root, serial));
        assert!(
            log.parent_pages.iter().all(|page| page.parent != serial),
            "a dropped parent's names were read by the update"
        );
        assert!(log.points.iter().all(|point| point.parent != serial));
    }
    assert_eq!(built.walked.serial("file"), 2);
    assert_eq!(built.work.fresh_serials, 0);
    drop(log);
    built.release(&b);
}

// The root: a metadata value, never fresh.

#[test]
fn the_root_is_a_patched_metadata_value_and_never_fresh() {
    let b = Bench::new("cs-root");
    let mut d = Drive::new(&b);
    d.chmod("", 0o750);
    d.utimens("", (-1, 1));
    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "root");
    let store = &b.fixture.store;
    assert!(record(&b, &built.taken, VALUE, 1).is_some());
    assert_eq!(record(&b, &built.taken, FRESH, 1), None);
    // No name changed, so the root has no header and keeps its directory.
    assert_eq!(record(&b, &built.taken, HEADER, 1), None);
    let (old, new) = (value(store, b.fixture.root, 1), value(store, built.root, 1));
    assert_eq!(new.content_root, old.content_root);
    assert_ne!(new.metadata_root, old.metadata_root);
    assert_eq!(new.namespace_ref_count, 0);
    let work = built.work;
    assert_eq!((work.values_written, work.metadata_patched), (1, 1));
    assert_eq!((work.fresh_serials, work.tombstones_skipped), (0, 0));
    assert_eq!(recording.log.borrow().count(Call::ParentPage), 0);
    built.release(&b);
}

// Live inode born above the floor: fresh serial and typed value.

#[test]
fn a_fresh_file_is_constructed_from_its_captured_runs() {
    let b = Bench::new("cs-fresh-file");
    let mut d = Drive::new(&b);
    let empty = d.create("empty", 0o600);
    let sparse = d.create("sparse", 0o644);
    d.write("sparse", 70_000, b"after a hole");
    d.resize("sparse", 200_000);
    let dense = d.create("dense", 0o755);
    let bytes: Vec<u8> = (0..120_000_u32).map(|i| (i % 249) as u8).collect();
    d.write("dense", 0, &bytes);

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "fresh files");
    let work = built.work;
    assert_eq!((work.fresh_serials, work.files_constructed), (3, 3));
    assert_eq!(work.files_unchanged, 0);
    // Dense ranks in serial order.
    let mut fresh = [empty, sparse, dense];
    fresh.sort_unstable();
    for (rank, serial) in fresh.into_iter().enumerate() {
        assert_eq!(record(&b, &built.taken, FRESH, serial), number(rank as u64));
        assert!(record(&b, &built.taken, VALUE, serial).is_some());
    }
    // Every file's own records live under its serial as file scope.
    let log = recording.log.borrow();
    for serial in fresh {
        assert!(
            log.applies.iter().any(|job| job.file_scope == serial),
            "serial {serial} has no file-scoped record job"
        );
    }
    drop(log);
    built.release(&b);
}

#[test]
fn a_fresh_symlink_takes_exactly_its_captured_target() {
    let b = Bench::new("cs-fresh-link");
    let mut d = Drive::new(&b);
    let long = vec![b'/'; 4096];
    let first = d.symlink("first", b"relative/../target \xfe");
    let second = d.symlink("output/second", &long);

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "fresh symlinks");
    let mut asked = recording.log.borrow().symlinks.clone();
    asked.sort_unstable();
    let mut expected = vec![first, second];
    expected.sort_unstable();
    assert_eq!(asked, expected, "one target read per fresh symlink");
    assert_eq!((built.work.symlinks, built.work.fresh_serials), (2, 2));
    assert_eq!(built.work.files_constructed, 0);
    built.release(&b);
}

#[test]
fn fresh_directories_get_headers_even_with_no_name() {
    let b = Bench::new("cs-fresh-dir");
    let mut d = Drive::new(&b);
    let bare = d.mkdir("bare", 0o700);
    let full = d.mkdir("full", 0o755);
    let nested = d.mkdir("full/nested", 0o755);
    d.create("full/a", 0o644);
    d.create("full/b", 0o644);
    d.symlink("full/nested/link", b"../a");

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "fresh directories");
    assert_eq!(record(&b, &built.taken, HEADER, bare), number(0));
    assert_eq!(record(&b, &built.taken, HEADER, full), number(3));
    assert_eq!(record(&b, &built.taken, HEADER, nested), number(1));
    assert_eq!(record(&b, &built.taken, HEADER, 1), number(2));
    assert_eq!(built.work.headers_written, 4);
    assert_eq!(built.work.fresh_serials, 6);
    let store = &b.fixture.store;
    assert_eq!(value(store, built.root, bare).namespace_ref_count, 1);
    built.release(&b);
}

// Live base inode: base roots, metadata patched with mode and mtime only.

#[test]
fn a_base_file_with_only_metadata_changes_keeps_its_content_root() {
    let b = Bench::new("cs-base-meta");
    let mut d = Drive::new(&b);
    d.chmod("file", 0o400);
    d.utimens(".git/index", (5, 6));
    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "base metadata");
    let store = &b.fixture.store;
    for serial in [2, 8] {
        let (old, new) = (
            value(store, b.fixture.root, serial),
            value(store, built.root, serial),
        );
        assert_eq!(new.content_root, old.content_root, "serial {serial}");
        assert_ne!(new.metadata_root, old.metadata_root, "serial {serial}");
        assert_eq!(new.namespace_ref_count, old.namespace_ref_count);
    }
    let work = built.work;
    assert_eq!((work.files_constructed, work.files_unchanged), (2, 2));
    assert_eq!((work.metadata_patched, work.metadata_built), (2, 0));
    // No name changed anywhere: no header, no name cursor, no name point.
    assert_eq!(work.headers_written, 0);
    let log = recording.log.borrow();
    assert_eq!(log.count(Call::ParentPage) + log.count(Call::NamePoint), 0);
    drop(log);
    built.release(&b);
}

#[test]
fn a_base_file_with_changed_content_gets_a_new_content_root() {
    let b = Bench::new("cs-base-content");
    let mut d = Drive::new(&b);
    d.write(".git/index", 399_990, b"crosses the old end of file");
    d.resize("file", 4);
    let built = d.build("base content");
    let store = &b.fixture.store;
    for serial in [2, 8] {
        assert_ne!(
            value(store, built.root, serial).content_root,
            value(store, b.fixture.root, serial).content_root
        );
    }
    assert_eq!(
        (built.work.files_constructed, built.work.files_unchanged),
        (2, 0)
    );
    built.release(&b);
}

#[test]
fn a_base_directory_and_symlink_keep_their_content_roots_under_new_metadata() {
    let b = Bench::new("cs-base-nonfile");
    let mut d = Drive::new(&b);
    d.chmod("output", 0o1700);
    d.utimens("symlink", (0, 0));
    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "base non-files");
    let store = &b.fixture.store;
    for serial in [3, 7] {
        let (old, new) = (
            value(store, b.fixture.root, serial),
            value(store, built.root, serial),
        );
        assert_eq!(new.content_root, old.content_root, "serial {serial}");
        assert_ne!(new.metadata_root, old.metadata_root, "serial {serial}");
    }
    // A stored symlink's target is never read from the capture.
    assert_eq!(recording.log.borrow().count(Call::Symlink), 0);
    assert_eq!(built.work.symlinks, 0);
    assert_eq!(built.work.metadata_patched, 2);
    built.release(&b);
}

#[test]
fn link_count_changes_alone_keep_content_and_metadata_roots() {
    let b = Bench::new("cs-links");
    let mut d = Drive::new(&b);
    d.link("file", "output/extra");
    d.link(".git/index", "top");
    d.remove("cache/state");
    let fresh = d.create("fresh", 0o644);
    d.write("fresh", 0, b"three names, then two");
    d.link("fresh", "output/fresh-2");
    d.link("fresh", ".git/fresh-3");
    d.remove("fresh");

    let built = d.build("links");
    let store = &b.fixture.store;
    for (serial, count) in [(2, 3), (8, 4), (fresh, 2)] {
        assert_eq!(
            value(store, built.root, serial).namespace_ref_count,
            count,
            "serial {serial}"
        );
    }
    for serial in [2, 8] {
        let (old, new) = (
            value(store, b.fixture.root, serial),
            value(store, built.root, serial),
        );
        // The same mode and time patched again are the same canonical tree.
        assert_eq!(new.content_root, old.content_root);
        assert_eq!(new.metadata_root, old.metadata_root);
    }
    assert_eq!(
        (built.work.files_constructed, built.work.files_unchanged),
        (3, 2)
    );
    built.release(&b);
}

// Entry rows: bound, restated, removed and removed over nothing.

#[test]
fn restated_bindings_and_removals_over_nothing_change_no_name() {
    let b = Bench::new("cs-restated");
    let mut d = Drive::new(&b);
    d.rename("file", "elsewhere");
    d.rename("elsewhere", "file");
    d.link("alias", "extra");
    d.remove("extra");
    d.rename("cache/state", "cache/other");
    d.rename("cache/other", "cache/state");

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "restated");
    let log = recording.log.borrow();
    // The capture restates what the base already binds.
    assert_eq!(bound(&log, 1, "file"), Some(Some(2)));
    assert_eq!(bound(&log, 6, "state"), Some(Some(8)));
    // A name the base never bound is a removal over nothing, if it is a row.
    for (parent, name) in [(1, "elsewhere"), (1, "extra"), (6, "other")] {
        assert!(matches!(bound(&log, parent, name), None | Some(None)));
    }
    let store = &b.fixture.store;
    // Same names, same counts: both directories are the stored directories.
    for serial in [1, 6] {
        assert_eq!(
            value(store, built.root, serial).content_root,
            value(store, b.fixture.root, serial).content_root,
            "serial {serial}"
        );
    }
    for serial in [2, 8] {
        assert_eq!(
            value(store, built.root, serial),
            value(store, b.fixture.root, serial)
        );
    }
    drop(log);
    built.release(&b);
}

#[test]
fn a_removed_name_and_a_rebound_name_are_exact_changes() {
    let b = Bench::new("cs-entries");
    let mut d = Drive::new(&b);
    d.remove("alias");
    d.remove("symlink");
    let again = d.symlink("symlink", b"rebound");
    d.rename("output/result", "alias");

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "entries");
    let log = recording.log.borrow();
    assert_eq!(bound(&log, 1, "alias"), Some(Some(8)));
    assert_eq!(bound(&log, 1, "symlink"), Some(Some(again)));
    assert_eq!(bound(&log, 7, "result"), Some(None));
    let store = &b.fixture.store;
    assert!(
        absent(store, built.root, 3),
        "the base symlink was replaced"
    );
    assert_eq!(value(store, built.root, 2).namespace_ref_count, 1);
    assert_eq!(built.walked.serial("alias"), 8);
    drop(log);
    built.release(&b);
}

// Moved directory: two name changes, no row and no value for it or below it.

#[test]
fn a_moved_base_directory_has_no_row_and_no_value() {
    let b = Bench::new("cs-moved-base");
    let mut d = Drive::new(&b);
    d.rename("cache", "output/c");

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "moved base directory");
    let log = recording.log.borrow();
    let rows: Vec<u64> = inodes(&log).iter().map(|row| row.serial).collect();
    assert_eq!(rows, vec![1, 7], "only the two touched parents have rows");
    assert_eq!(bound(&log, 1, "cache"), Some(None));
    assert_eq!(bound(&log, 7, "c"), Some(Some(6)));
    for serial in [6, 8] {
        assert_eq!(record(&b, &built.taken, VALUE, serial), None);
        assert_eq!(
            value(&b.fixture.store, built.root, serial),
            value(&b.fixture.store, b.fixture.root, serial)
        );
    }
    assert_eq!(built.walked.serial("output/c/state"), 8);
    assert_eq!(built.work.values_written, 2);
    assert_eq!(built.work.files_constructed, 0);
    // A move under a stored non-root directory lists the moved subtree once:
    // the one moved directory, which holds one name.
    walked(&built, "moved base directory");
    let validation = built.content.validation;
    assert_eq!(
        (
            validation.territory_directories,
            validation.territory_entries
        ),
        (1, 1)
    );
    drop(log);
    built.release(&b);
}

#[test]
fn a_moved_fresh_directory_keeps_its_serial_and_children() {
    let b = Bench::new("cs-moved-fresh");
    let mut d = Drive::new(&b);
    let tree = d.mkdir("tree", 0o755);
    let leaf = d.create("tree/leaf", 0o644);
    d.write("tree/leaf", 0, b"leaf");
    d.mkdir("tree/branch", 0o755);
    d.rename("tree", ".git/tree");
    d.rename(".git/tree/branch", "branch");

    let built = d.build("moved fresh directory");
    assert_eq!(built.walked.serial(".git/tree"), tree);
    assert_eq!(built.walked.serial(".git/tree/leaf"), leaf);
    assert!(built.walked.has("branch") && !built.walked.has("tree"));
    assert_eq!(built.work.fresh_serials, 3);
    // No stored directory moved: nothing of the base is listed.
    walked(&built, "moved fresh directory");
    assert_eq!(built.content.validation.territory_directories, 0);
    built.release(&b);
}

#[test]
fn a_base_directory_moves_under_a_fresh_directory_inside_a_base_directory() {
    let b = Bench::new("cs-moved-under-fresh");
    let mut d = Drive::new(&b);
    let fresh = d.mkdir("output/new", 0o755);
    d.rename("cache", "output/new/cache");
    d.rename("node_modules", "output/new/cache/modules");

    let built = d.build("moved under fresh");
    assert_eq!(built.walked.serial("output/new"), fresh);
    assert_eq!(built.walked.serial("output/new/cache"), 6);
    assert_eq!(built.walked.serial("output/new/cache/modules"), 5);
    assert_eq!(built.walked.serial("output/new/cache/modules/pkg"), 8);
    assert_eq!(built.work.fresh_serials, 1);
    walked(&built, "moved under fresh");
    built.release(&b);
}

#[test]
fn two_base_directories_exchange_their_names() {
    let b = Bench::new("cs-swap");
    let mut d = Drive::new(&b);
    d.rename("cache", "swap");
    d.rename("output", "cache");
    d.rename("swap", "output");

    let built = d.build("swap");
    assert_eq!(built.walked.serial("cache"), 7);
    assert_eq!(built.walked.serial("output"), 6);
    assert_eq!(built.walked.serial("cache/result"), 8);
    assert_eq!(built.walked.serial("output/state"), 8);
    // Only the root's names changed.
    assert_eq!(built.work.values_written, 1);
    // Stored directories renamed inside their own base parent list nothing.
    walked(&built, "swap");
    let validation = built.content.validation;
    assert_eq!(
        (
            validation.territory_directories,
            validation.territory_entries
        ),
        (0, 0)
    );
    built.release(&b);
}

#[test]
fn a_stored_parent_and_child_directory_are_inverted() {
    let b = Bench::new("cs-invert");
    let mut d = Drive::new(&b);
    let outer = d.mkdir("a", 0o755);
    let inner = d.mkdir("a/b", 0o755);
    let leaf = d.create("a/b/leaf", 0o644);
    d.write("a/b/leaf", 0, b"below both");
    d.build("nested").install(&b);
    drain(&b);

    // Both directories are stored now. The child leaves, the parent follows.
    d.rename("a/b", "b");
    d.rename("a", "b/a");
    let built = d.build("inverted");
    assert_eq!(built.walked.serial("b"), inner);
    assert_eq!(built.walked.serial("b/a"), outer);
    assert_eq!(built.walked.serial("b/leaf"), leaf);
    let work = built.work;
    assert_eq!((work.fresh_serials, work.files_constructed), (0, 0));
    // The root and both moved directories were touched as parents.
    assert_eq!(work.values_written, 3);
    for serial in [outer, inner, leaf] {
        assert_eq!(record(&b, &built.taken, FRESH, serial), None);
    }
    walked(&built, "inverted");
    built.install(&b);

    // The same serials move back in the next capture over the inverted base.
    d.rename("b/a", "a");
    d.rename("b", "a/b");
    let back = d.build("restored");
    assert_eq!(back.walked.serial("a/b/leaf"), leaf);
    back.release(&b);
}
