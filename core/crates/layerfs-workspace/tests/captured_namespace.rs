//! R4-1 and the producer side of R4-8: one captured reader becomes one complete
//! canonical root. Every result is walked completely and compared with an
//! independent model; counters are counted work, never timing.
mod common;
mod harness;
mod oracle;
mod producer;
use harness::Bench;
use oracle::{absent, assert_tree, value, Model};
use producer::{build_taken, take, Drive};

#[test]
fn the_model_of_the_fixture_is_the_base_root() {
    let b = Bench::new("cn-fixture");
    let model = Model::fixture(&b.fixture.bytes);
    let walked = assert_tree(&b.fixture.store, b.fixture.root, &model, "fixture");
    assert_eq!(walked.serial(""), 1);
    assert_eq!(walked.serial("file"), walked.serial("alias"));
    assert_eq!(walked.serial(".git/index"), walked.serial("output/result"));
}

#[test]
fn an_empty_capture_returns_the_base_root_identity() {
    let b = Bench::new("cn-empty");
    let d = Drive::new(&b);
    let built = d.build("empty");
    assert_eq!(built.root, b.fixture.root, "nothing changed");
    let work = built.work;
    assert_eq!((work.inode_rows, work.entry_rows), (0, 0));
    assert_eq!(
        (
            work.headers_written,
            work.values_written,
            work.fresh_serials,
            work.files_constructed,
            work.symlinks,
        ),
        (0, 0, 0, 0, 0)
    );
    built.release(&b);
}

#[test]
fn new_files_directories_and_symlinks_are_constructed() {
    let b = Bench::new("cn-new");
    let mut d = Drive::new(&b);
    let fresh = d.create("fresh", 0o600);
    d.write("fresh", 0, b"fresh bytes");
    let empty = d.create("empty", 0o640);
    let made = d.mkdir("made", 0o750);
    let inner = d.mkdir("made/inner", 0o700);
    let deep = d.create("made/inner/deep", 0o644);
    // A hole before the first byte and more than one chunk of data.
    d.write("made/inner/deep", 5, &[7; 70_000]);
    d.write("made/inner/deep", 300_000, b"tail after a hole");
    let bare = d.mkdir("bare", 0o1777);
    let pointer = d.symlink("made/pointer", b"../fresh\nnot utf8 \xff");
    let dangling = d.symlink(".git/dangling", b"nowhere");
    let added = d.create("cache/added", 0o444);
    d.append("cache/added", b"appended");

    let built = d.build("new");
    for (path, serial) in [
        ("fresh", fresh),
        ("empty", empty),
        ("made", made),
        ("made/inner", inner),
        ("made/inner/deep", deep),
        ("bare", bare),
        ("made/pointer", pointer),
        (".git/dangling", dangling),
        ("cache/added", added),
    ] {
        assert_eq!(built.walked.serial(path), serial, "{path}");
    }
    let work = built.work;
    assert_eq!(work.fresh_serials, 9);
    assert_eq!((work.files_constructed, work.files_unchanged), (4, 0));
    assert_eq!(work.symlinks, 2);
    // The touched base parents: the root, `.git` and `cache`.
    assert_eq!((work.metadata_built, work.metadata_patched), (9, 3));
    assert_eq!(work.values_written, 12);
    assert_eq!(work.tombstones_skipped, 0);
    // Untouched base inodes keep their stored values.
    for serial in [2, 3, 5, 7, 8] {
        assert_eq!(
            value(&b.fixture.store, built.root, serial),
            value(&b.fixture.store, b.fixture.root, serial),
            "serial {serial}"
        );
    }
    built.release(&b);
}

#[test]
fn deleted_files_directories_and_symlinks_leave_the_result() {
    let b = Bench::new("cn-deleted");
    let mut d = Drive::new(&b);
    d.remove("symlink");
    d.remove("alias");
    d.remove("file");
    d.remove("cache/state");
    d.remove("cache");

    let built = d.build("deleted");
    let store = &b.fixture.store;
    for gone in [2, 3, 6] {
        assert!(absent(store, built.root, gone), "serial {gone} remains");
    }
    assert!(!absent(store, built.root, 8));
    assert_eq!(value(store, built.root, 8).namespace_ref_count, 3);
    let work = built.work;
    assert_eq!(work.fresh_serials, 0);
    assert_eq!(work.symlinks, 0);
    // Only the surviving file with a changed count reaches the file
    // constructor, and its content is the base content.
    assert_eq!((work.files_constructed, work.files_unchanged), (1, 1));
    assert_eq!(
        value(store, built.root, 8).content_root,
        value(store, b.fixture.root, 8).content_root
    );
    built.release(&b);
}

#[test]
fn changed_content_mode_and_time_of_each_kind() {
    let b = Bench::new("cn-changed");
    let mut d = Drive::new(&b);
    d.write(".git/index", 100_000, &[0xab; 9000]);
    d.utimens(".git/index", (12, 34));
    d.write("file", 3, b"XYZ");
    d.append("alias", b" and a tail");
    d.chmod("file", 0o600);
    d.chmod("output", 0o700);
    d.utimens("cache", (-5, 999_999_999));
    // A symlink has no mode to change; its time is its only metadata change.
    d.utimens("symlink", (77, 1));
    d.chmod("", 0o755);
    d.resize("node_modules/pkg", 399_000);

    let built = d.build("changed");
    let store = &b.fixture.store;
    let (old, new) = (
        |serial| value(store, b.fixture.root, serial),
        |serial| value(store, built.root, serial),
    );
    // Content changes replace the file root; metadata changes never do.
    assert_ne!(new(2).content_root, old(2).content_root);
    assert_ne!(new(8).content_root, old(8).content_root);
    for serial in [1, 3, 6, 7] {
        assert_ne!(new(serial).metadata_root, old(serial).metadata_root);
    }
    assert_eq!(new(3).content_root, old(3).content_root);
    // Untouched inodes are the stored values.
    for serial in [4, 5] {
        assert_eq!(new(serial), old(serial));
    }
    let work = built.work;
    assert_eq!((work.files_constructed, work.files_unchanged), (2, 0));
    assert_eq!((work.metadata_built, work.metadata_patched), (0, 6));
    assert_eq!((work.fresh_serials, work.symlinks), (0, 0));
    built.release(&b);
}

#[test]
fn renamed_and_moved_names_of_each_kind_keep_their_inodes() {
    let b = Bench::new("cn-moved");
    let mut d = Drive::new(&b);
    d.rename("file", "renamed-file");
    d.rename("symlink", "renamed-link");
    d.rename("cache", "renamed-dir");
    d.rename("alias", "output/moved-file");
    d.rename("renamed-link", ".git/moved-link");
    d.rename("node_modules", "output/moved-dir");

    let built = d.build("moved");
    for (path, serial) in [
        ("renamed-file", 2),
        ("output/moved-file", 2),
        (".git/moved-link", 3),
        ("renamed-dir", 6),
        ("renamed-dir/state", 8),
        ("output/moved-dir", 5),
        ("output/moved-dir/pkg", 8),
    ] {
        assert_eq!(built.walked.serial(path), serial, "{path}");
    }
    let store = &b.fixture.store;
    // A moved inode has no captured row: its value is the stored one, and the
    // directory under a moved directory is not rebuilt.
    for serial in [2, 3, 5, 6, 8] {
        assert_eq!(
            value(store, built.root, serial),
            value(store, b.fixture.root, serial),
            "serial {serial}"
        );
    }
    let work = built.work;
    assert_eq!(work.files_constructed, 0);
    assert_eq!(work.fresh_serials, 0);
    // The three touched parents: the root, `.git` and `output`.
    assert_eq!(work.values_written, 3);
    built.release(&b);
}

#[test]
fn replacing_renames_and_fresh_moves_compose_in_one_capture() {
    let b = Bench::new("cn-mixed");
    let mut d = Drive::new(&b);
    // Replace a base name by another base inode, then by a fresh one.
    d.rename("symlink", "file");
    let fresh = d.create("draft", 0o644);
    d.write("draft", 0, b"draft bytes");
    d.rename("draft", "alias");
    // A fresh tree is built, filled, renamed and moved under a base parent.
    d.mkdir("stage", 0o755);
    d.mkdir("stage/sub", 0o755);
    d.create("stage/sub/leaf", 0o644);
    d.write("stage/sub/leaf", 0, &[9; 5000]);
    d.symlink("stage/link", b"sub/leaf");
    d.rename("stage/sub", "stage/renamed");
    d.rename("stage", "output/staged");
    // A base file gains a name in a fresh directory and loses a base name.
    d.link("output/result", "output/staged/renamed/hard");
    d.remove("node_modules/pkg");
    d.rename("cache/state", "output/staged/state");
    d.remove("cache");

    let built = d.build("mixed");
    let store = &b.fixture.store;
    assert_eq!(built.walked.serial("file"), 3);
    assert_eq!(built.walked.serial("alias"), fresh);
    assert!(absent(store, built.root, 2), "both names were replaced");
    assert!(absent(store, built.root, 6));
    assert_eq!(built.walked.serial("output/staged/state"), 8);
    assert_eq!(built.walked.serial("output/staged/renamed/hard"), 8);
    assert_eq!(value(store, built.root, 8).namespace_ref_count, 4);
    built.release(&b);
}

#[test]
fn a_deep_fresh_chain_ends_in_aliases_and_a_moved_base_directory() {
    let b = Bench::new("cn-deep");
    let mut d = Drive::new(&b);
    let mut path = String::from("output");
    for level in 0..48 {
        path = format!("{path}/d{level}");
        d.mkdir(&path, 0o755);
    }
    let leaf = d.create(&format!("{path}/leaf"), 0o644);
    d.write(&format!("{path}/leaf"), 0, b"at the bottom");
    d.link(&format!("{path}/leaf"), "leaf-alias");
    d.link(".git/index", &format!("{path}/alias"));
    d.rename("cache", &format!("{path}/cache"));

    let built = d.build("deep");
    assert_eq!(built.walked.serial("leaf-alias"), leaf);
    assert_eq!(built.walked.serial(&format!("{path}/alias")), 8);
    assert_eq!(built.walked.serial(&format!("{path}/cache")), 6);
    assert_eq!(built.walked.serial(&format!("{path}/cache/state")), 8);
    let work = built.work;
    assert_eq!(work.fresh_serials, 49);
    // The new file, and the base file whose link count changed.
    assert_eq!((work.files_constructed, work.files_unchanged), (2, 1));
    built.release(&b);
}

#[test]
fn mutations_after_the_capture_are_absent_from_its_root_and_present_in_the_next() {
    let b = Bench::new("cn-later");
    let mut d = Drive::new(&b);
    d.create("early", 0o644);
    d.write("early", 0, b"captured bytes");
    d.write(".git/index", 10, b"captured");
    d.mkdir("kept", 0o755);
    d.create("kept/inside", 0o600);
    d.remove("symlink");
    let taken = take(&b);
    let sealed = d.model.clone();

    // Later changes over the very rows the capture sealed.
    d.write("early", 0, b"LATER");
    d.append("early", &[0x5a; 40_000]);
    d.write(".git/index", 10, b"later!!!");
    d.resize("output/result", 1000);
    d.create("kept/late", 0o600);
    d.remove("kept/inside");
    d.rename("kept", "moved-late");
    d.symlink("symlink", b"late target");
    d.remove("file");
    d.chmod("output", 0o700);

    let built = build_taken(&b, &b.overlay, taken, &sealed, "sealed");
    assert!(built.walked.has("kept/inside") && !built.walked.has("moved-late"));
    assert!(!built.walked.has("symlink"));
    // The live view keeps every later change while the capture is constructed.
    assert_eq!(b.content(d.serial("early")), d.model.bytes("early"));
    assert_eq!(b.names(d.serial("moved-late")), vec!["late"]);
    let first = built.install(&b);
    assert_eq!(b.content(d.serial("early")), d.model.bytes("early"));

    // The next capture is exactly the later changes over the installed root.
    let next = d.build("next");
    assert_ne!(next.root, first);
    assert!(next.walked.has("moved-late/late") && !next.walked.has("kept"));
    let second = next.install(&b);

    // Nothing changed since: the installed root is returned as it is.
    let idle = d.build("idle");
    assert_eq!(idle.root, second);
    assert_eq!((idle.work.inode_rows, idle.work.entry_rows), (0, 0));
    idle.release(&b);
}
