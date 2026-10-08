//! Names rebound to an inode of another kind, a directory made again over a
//! directory, and directories moved more than once in one capture. A replaced
//! inode, and every inode of a replaced directory's subtree, is absent from
//! the decoded inode table, not merely unreachable; the complete walk against
//! the model asserts the same for every inode the model does not name.
mod common;
mod harness;
mod oracle;
mod producer;
use harness::Bench;
use oracle::{absent, value};
use producer::{build_over, drain, evidence, record, Built, Drive, Recording, VALUE};

/// The decoded table has none of these serials.
fn gone(b: &Bench, built: &Built, serials: &[u64], what: &str) {
    for serial in serials {
        assert!(
            absent(&b.fixture.store, built.root, *serial),
            "{what}: replaced serial {serial} is still in the inode table"
        );
        assert!(!built.walked.table.contains_key(serial), "{what}");
    }
}
/// A stored tree `d/{x, sub/leaf}` whose inodes have no name outside it.
fn stored_tree(b: &Bench, d: &mut Drive<'_>) -> [u64; 4] {
    let tree = d.mkdir("d", 0o755);
    let x = d.create("d/x", 0o644);
    d.write("d/x", 0, b"x");
    let sub = d.mkdir("d/sub", 0o755);
    let leaf = d.create("d/sub/leaf", 0o644);
    d.write("d/sub/leaf", 0, &[4; 9000]);
    d.build("stored tree").install(b);
    drain(b);
    [tree, x, sub, leaf]
}
fn territory(built: &Built) -> (u64, u64) {
    let validation = built.content.validation;
    (
        validation.territory_directories,
        validation.territory_entries,
    )
}

// Item 2: names rebound to another kind, and directory over directory.

#[test]
fn a_removed_stored_directory_tree_is_replaced_by_a_file() {
    let b = Bench::new("cr-tree-file");
    let mut d = Drive::new(&b);
    let old = stored_tree(&b, &mut d);
    // rm -r d; touch d
    d.remove("d/sub/leaf");
    d.remove("d/sub");
    d.remove("d/x");
    d.remove("d");
    let file = d.create("d", 0o600);
    d.write("d", 0, b"a file where the tree was");
    // The same over a base directory of the shared fixture.
    d.remove("cache/state");
    d.remove("cache");
    let cache = d.create("cache", 0o644);

    let built = d.build("tree replaced by a file");
    assert_eq!(built.walked.serial("d"), file);
    assert_eq!(built.walked.serial("cache"), cache);
    gone(&b, &built, &old, "tree replaced by a file");
    gone(&b, &built, &[6], "base directory replaced by a file");
    assert_eq!(
        value(&b.fixture.store, built.root, 8).namespace_ref_count,
        3
    );
    built.release(&b);
}

#[test]
fn a_removed_file_and_symlink_are_replaced_by_directories_with_children() {
    let b = Bench::new("cr-file-dir");
    let mut d = Drive::new(&b);
    // rm f; mkdir f; touch f/x, for the file's only remaining name.
    d.remove("alias");
    d.remove("file");
    let directory = d.mkdir("file", 0o755);
    let child = d.create("file/x", 0o644);
    d.remove("symlink");
    let other = d.mkdir("symlink", 0o700);
    d.symlink("symlink/x", b"..");

    let built = d.build("file replaced by a directory");
    assert_eq!(built.walked.serial("file"), directory);
    assert_eq!(built.walked.serial("file/x"), child);
    assert_eq!(built.walked.serial("symlink"), other);
    gone(&b, &built, &[2, 3], "file and symlink replaced");
    built.release(&b);
}

#[test]
fn a_directory_removed_and_made_again_under_its_name_is_a_new_inode() {
    let b = Bench::new("cr-dir-dir");
    let mut d = Drive::new(&b);
    let old = stored_tree(&b, &mut d);
    // rmdir d; mkdir d over a base directory of the fixture, kept empty.
    d.remove("output/result");
    d.remove("output");
    let output = d.mkdir("output", 0o755);
    // The same with a new child of the old child's name.
    d.remove("cache/state");
    d.remove("cache");
    let cache = d.mkdir("cache", 0o700);
    let state = d.create("cache/state", 0o644);
    // And below a stored non-root directory.
    d.remove("d/sub/leaf");
    d.remove("d/sub");
    let sub = d.mkdir("d/sub", 0o755);
    let leaf = d.create("d/sub/leaf", 0o644);

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "directory over directory");
    for (path, serial) in [
        ("output", output),
        ("cache", cache),
        ("cache/state", state),
        ("d/sub", sub),
        ("d/sub/leaf", leaf),
    ] {
        assert_eq!(built.walked.serial(path), serial, "{path}");
    }
    assert_eq!(built.walked.serial("d"), old[0]);
    assert_eq!(built.walked.serial("d/x"), old[1]);
    gone(
        &b,
        &built,
        &[6, 7, old[2], old[3]],
        "directory over directory",
    );
    assert_eq!(
        value(&b.fixture.store, built.root, 8).namespace_ref_count,
        2
    );
    // The stored directories that were removed have no value of their own.
    for serial in [6, 7, old[2]] {
        assert_eq!(record(&b, &built.taken, VALUE, serial), None);
    }
    built.release(&b);
}

#[test]
fn a_directory_is_replaced_by_a_symlink_and_a_symlink_by_a_directory() {
    let b = Bench::new("cr-dir-link");
    let mut d = Drive::new(&b);
    d.remove("output/result");
    d.remove("output");
    let link = d.symlink("output", b"cache");
    d.remove("symlink");
    let directory = d.mkdir("symlink", 0o755);
    d.create("symlink/inside", 0o644);

    let built = d.build("directory and symlink exchanged kinds");
    assert_eq!(built.walked.serial("output"), link);
    assert_eq!(built.walked.serial("symlink"), directory);
    gone(&b, &built, &[7, 3], "directory and symlink exchanged kinds");
    assert_eq!(built.work.symlinks, 1);
    built.release(&b);
}

#[test]
fn a_directory_renamed_over_an_empty_directory_replaces_it() {
    let b = Bench::new("cr-dir-over-dir");
    let mut d = Drive::new(&b);
    // A base directory over an emptied base directory.
    d.remove("cache/state");
    d.rename("node_modules", "cache");
    // A fresh directory with a child over an emptied base directory.
    d.remove("output/result");
    let fresh = d.mkdir("staged", 0o755);
    let inside = d.create("staged/inside", 0o644);
    d.rename("staged", "output");
    // A base directory over a fresh empty directory.
    let landing = d.mkdir("landing", 0o700);
    d.rename(".git", "landing");

    let built = d.build("directory renamed over an empty directory");
    assert_eq!(built.walked.serial("cache"), 5);
    assert_eq!(built.walked.serial("cache/pkg"), 8);
    assert_eq!(built.walked.serial("output"), fresh);
    assert_eq!(built.walked.serial("output/inside"), inside);
    assert_eq!(built.walked.serial("landing"), 4);
    assert_eq!(built.walked.serial("landing/index"), 8);
    for path in ["node_modules", "staged", ".git"] {
        assert!(!built.walked.has(path), "{path}");
    }
    gone(
        &b,
        &built,
        &[6, 7, landing],
        "directory renamed over an empty directory",
    );
    assert_eq!(
        value(&b.fixture.store, built.root, 8).namespace_ref_count,
        2
    );
    built.release(&b);
}

// Item 3: directories moved more than once in one capture.

#[test]
fn a_base_directory_moved_twice_in_one_capture_is_bound_once() {
    let b = Bench::new("cr-moved-twice");
    let mut d = Drive::new(&b);
    d.rename("cache", "output/c");
    d.rename("output/c", ".git/c");
    // Through a directory that exists only in this capture.
    let via = d.mkdir("via", 0o755);
    d.rename("node_modules", "via/n");
    d.rename("via/n", "output/n");

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "moved twice");
    assert_eq!(built.walked.serial(".git/c"), 6);
    assert_eq!(built.walked.serial(".git/c/state"), 8);
    assert_eq!(built.walked.serial("output/n"), 5);
    assert_eq!(built.walked.serial("output/n/pkg"), 8);
    assert_eq!(built.walked.serial("via"), via);
    // Neither moved directory nor anything below it has a value of its own.
    let store = &b.fixture.store;
    for serial in [5, 6, 8] {
        assert_eq!(record(&b, &built.taken, VALUE, serial), None);
        assert_eq!(
            value(store, built.root, serial),
            value(store, b.fixture.root, serial),
            "serial {serial}"
        );
    }
    // The intermediate names are removals over nothing, if they are rows.
    let log = recording.log.borrow();
    for (parent, name) in [(7, "c"), (via, "n")] {
        assert!(matches!(log.bound(parent, name), None | Some(None)));
    }
    assert_eq!(log.bound(4, "c"), Some(Some(6)));
    assert_eq!(log.bound(7, "n"), Some(Some(5)));
    evidence(format_args!(
        "R3 row 4 moved twice: validation {:?}",
        built.content.validation
    ));
    // Each moved directory ends under a stored non-root directory, holds one
    // name and is listed exactly once.
    assert_eq!(territory(&built), (2, 2));
    drop(log);
    built.release(&b);
}

#[test]
fn a_base_directory_moved_away_and_back_keeps_its_stored_values_with_no_territory_walk() {
    let b = Bench::new("cr-returned");
    let mut d = Drive::new(&b);
    d.rename("cache", "output/c");
    d.rename("output/c", "cache");

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "moved and returned");
    let store = &b.fixture.store;
    // The capture restates the base binding; the other name never existed.
    let log = recording.log.borrow();
    assert_eq!(log.bound(1, "cache"), Some(Some(6)));
    assert!(matches!(log.bound(7, "c"), None | Some(None)));
    for serial in [6, 8] {
        assert_eq!(record(&b, &built.taken, VALUE, serial), None);
        assert_eq!(
            value(store, built.root, serial),
            value(store, b.fixture.root, serial),
            "serial {serial}"
        );
    }
    // Both touched parents keep their stored directories: no name changed.
    for serial in [1, 7] {
        assert_eq!(
            value(store, built.root, serial).content_root,
            value(store, b.fixture.root, serial).content_root,
            "serial {serial}"
        );
    }
    evidence(format_args!(
        "R3 row 4 moved and returned: validation {:?}",
        built.content.validation
    ));
    assert_eq!(territory(&built), (0, 0), "no territory walk");
    drop(log);
    built.release(&b);
}

#[test]
fn a_directory_rescued_from_a_directory_removed_in_the_same_capture_survives() {
    let b = Bench::new("cr-rescued");
    let mut d = Drive::new(&b);
    let [tree, x, sub, leaf] = stored_tree(&b, &mut d);
    // A stored directory leaves its stored parent, which is then removed.
    d.rename("d/sub", "rescued");
    d.remove("d/x");
    d.remove("d");
    // A base directory enters a new directory and leaves it before that
    // directory is removed again.
    let shelter = d.mkdir("shelter", 0o755);
    d.rename("cache", "shelter/cache");
    d.rename("shelter/cache", "saved");
    d.remove("shelter");

    let recording = Recording::new(&b.overlay);
    let built = build_over(&b, &recording, &d.model, "rescued");
    assert_eq!(built.walked.serial("rescued"), sub);
    assert_eq!(built.walked.serial("rescued/leaf"), leaf);
    assert_eq!(built.walked.serial("saved"), 6);
    assert_eq!(built.walked.serial("saved/state"), 8);
    gone(&b, &built, &[tree, x, shelter], "rescued");
    let store = &b.fixture.store;
    let installed = b.workspace.base().unwrap().identity();
    for serial in [sub, leaf, 6] {
        assert_eq!(record(&b, &built.taken, VALUE, serial), None);
        assert_eq!(
            value(store, built.root, serial),
            value(store, installed, serial),
            "serial {serial}"
        );
    }
    let under = recording.log.borrow().parents().contains(&shelter);
    evidence(format_args!(
        "R3 row 4 rescued: captured name rows under the removed new directory = {under}, headers_dropped = {}, validation {:?}",
        built.work.headers_dropped, built.content.validation
    ));
    assert_eq!(built.work.headers_dropped, u64::from(under));
    built.release(&b);
}
