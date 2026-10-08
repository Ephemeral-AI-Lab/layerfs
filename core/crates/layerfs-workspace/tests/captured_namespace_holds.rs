//! R3 rows 1, 2 and 7 with real holds: an inode that lost its last name while
//! a real descriptor or a real lookup still owns it is absent from the
//! constructed root and is never constructed, in the capture that removed it
//! and in the next capture over the installed result. Each case states which
//! hold it uses; the result is always the complete walk against the model,
//! including the decoded inode table.
mod common;
mod harness;
mod oracle;
mod producer;
use harness::Bench;
use layerfs_overlay::Inode;
use oracle::absent;
use producer::{
    build_taken, drain, evidence, hold, look, record, take, Built, Drive, Recording, FRESH, HEADER,
    VALUE,
};

/// One capture and attempt over a recording provider.
fn capture<'b>(d: &Drive<'b>, what: &str) -> (Built, Recording<'b>) {
    let recording = Recording::new(&d.b.overlay);
    let taken = take(d.b);
    let built = build_taken(d.b, &recording, taken, &d.model, what);
    (built, recording)
}
/// Nothing was built, declared or recorded for this serial, and the result
/// has no inode for it.
fn never_constructed(b: &Bench, built: &Built, recording: &Recording<'_>, serial: u64, what: &str) {
    let log = recording.log.borrow();
    assert_eq!(
        log.scoped(serial),
        0,
        "{what}: a file record job for serial {serial}"
    );
    assert!(
        log.symlinks.iter().all(|asked| *asked != serial),
        "{what}: a target read for serial {serial}"
    );
    for kind in [VALUE, FRESH, HEADER] {
        assert_eq!(
            record(b, &built.taken, kind, serial),
            None,
            "{what}: a record of kind {kind:#x} for serial {serial}"
        );
    }
    assert!(
        absent(&b.fixture.store, built.root, serial),
        "{what}: serial {serial} is in the inode table"
    );
}
/// A held inode without a name lives on in the engine's independent orphan
/// domain: its descriptor writes after the detach are not rows of any later
/// capture, so the next capture holds no row for it at all.
fn no_row(recording: &Recording<'_>, serial: u64, what: &str) {
    let row = recording.log.borrow().inode(serial);
    assert!(
        row.is_none(),
        "{what}: a captured row of the orphan: {row:?}"
    );
}
/// The captured row of a held inode that has no name: a tombstone.
fn tombstone(recording: &Recording<'_>, serial: u64, what: &str) -> Inode {
    let row = recording
        .log
        .borrow()
        .inode(serial)
        .unwrap_or_else(|| panic!("{what}: the held inode {serial} has no captured row"));
    assert_eq!(row.nlink, 0, "{what}: serial {serial}");
    row
}

// R3 row 1: unlinked while open.

#[test]
fn a_file_created_held_written_unlinked_and_written_again_is_never_constructed() {
    let b = Bench::new("ch-created");
    let mut d = Drive::new(&b);
    let serial = d.create("held", 0o644);
    let file = hold(&b, serial);
    d.write_open(file, "held", 0, &[5; 70_000]);
    d.remove("held");
    d.orphan_write(file, 10, b"after the last name is gone");

    let (built, recording) = capture(&d, "created and held");
    let row = tombstone(&recording, serial, "created and held");
    assert!(
        built.taken.reader.created_above(row.born),
        "the file was created in this capture: born {} above the floor",
        row.born
    );
    // It is the only tombstone, and no file at all was constructed.
    assert_eq!(built.work.tombstones_skipped, 1);
    assert_eq!(built.work.files_constructed, 0);
    assert_eq!(built.work.fresh_serials, 0);
    assert_eq!(recording.log.borrow().file_calls(), 0);
    never_constructed(&b, &built, &recording, serial, "created and held");
    // Only the touched root has a value: its time changed, its names did not.
    assert_eq!(built.work.values_written, 1);
    drop(recording);
    built.install(&b);
    drain(&b);

    // The descriptor still works over the installed result, which never had
    // the file. Its write publishes into the orphan's own domain, so the next
    // capture is empty and builds nothing. (My first expectation, a tombstone
    // row again, was wrong about the engine: W-attempt1 of this binary.)
    d.orphan_write(file, 0, b"written after the install");
    let (again, recording) = capture(&d, "created and held, next capture");
    no_row(&recording, serial, "next capture");
    evidence(format_args!(
        "R3 row 1 next capture: inode_rows={} entry_rows={} tombstones_skipped={}",
        again.work.inode_rows, again.work.entry_rows, again.work.tombstones_skipped
    ));
    assert_eq!((again.work.inode_rows, again.work.entry_rows), (0, 0));
    assert_eq!(again.work.files_constructed, 0);
    assert_eq!(recording.log.borrow().file_calls(), 0);
    never_constructed(&b, &again, &recording, serial, "next capture");
    assert_eq!(again.root, b.workspace.base().unwrap().identity());
    drop(recording);
    again.release(&b);
    b.overlay.close_file(file).unwrap();
}

#[test]
fn a_base_file_held_written_unlinked_and_written_again_is_never_constructed() {
    let b = Bench::new("ch-base");
    let mut d = Drive::new(&b);
    let file = hold(&b, 2);
    d.write_open(file, "file", 0, b"written before the unlink");
    d.remove("file");
    d.remove("alias");
    d.orphan_write(file, 4, b"held");

    let (built, recording) = capture(&d, "base and held");
    let row = tombstone(&recording, 2, "base and held");
    assert_eq!(row.born, 0, "an inherited base inode");
    assert!(!built.taken.reader.created_above(row.born));
    assert_eq!(built.work.tombstones_skipped, 1);
    assert_eq!(built.work.files_constructed, 0);
    assert_eq!(recording.log.borrow().file_calls(), 0);
    never_constructed(&b, &built, &recording, 2, "base and held");
    drop(recording);
    built.install(&b);
    drain(&b);

    // The installed base no longer has serial 2 at all.
    d.orphan_write(file, 0, b"written after the install");
    let (again, recording) = capture(&d, "base and held, next capture");
    no_row(&recording, 2, "next capture");
    assert_eq!((again.work.inode_rows, again.work.entry_rows), (0, 0));
    assert_eq!(again.work.files_constructed, 0);
    assert_eq!(again.root, b.workspace.base().unwrap().identity());
    assert_eq!(again.work.base_lookups, 0, "no base record is demanded");
    never_constructed(&b, &again, &recording, 2, "next capture");
    drop(recording);
    again.release(&b);
    b.overlay.close_file(file).unwrap();
}

// R3 row 1: held only by a kernel lookup.

#[test]
fn files_and_a_symlink_held_only_by_a_lookup_are_absent_after_their_unlink() {
    let b = Bench::new("ch-lookup");
    let mut d = Drive::new(&b);
    let fresh = d.create("looked", 0o600);
    d.write("looked", 0, &[9; 9000]);
    let link = d.symlink("looked-link", b"looked");
    let holds = [look(&b, fresh), look(&b, link), look(&b, 3)];
    d.remove("looked");
    d.remove("looked-link");
    d.remove("symlink");

    let (built, recording) = capture(&d, "lookup holds");
    for serial in [fresh, link] {
        let row = tombstone(&recording, serial, "lookup holds");
        assert!(built.taken.reader.created_above(row.born));
    }
    assert_eq!(tombstone(&recording, 3, "lookup holds").born, 0);
    assert_eq!(built.work.tombstones_skipped, 3);
    assert_eq!(
        (built.work.files_constructed, built.work.symlinks),
        (0, 0),
        "no content of a held orphan is read"
    );
    for serial in [fresh, link, 3] {
        never_constructed(&b, &built, &recording, serial, "lookup holds");
    }
    drop(recording);
    built.install(&b);
    drain(&b);

    // Another publication, then the next capture over the installed result.
    d.create("afterwards", 0o644);
    let (again, recording) = capture(&d, "lookup holds, next capture");
    for serial in [fresh, link, 3] {
        never_constructed(&b, &again, &recording, serial, "next capture");
    }
    assert_eq!(again.work.files_constructed, 1);
    drop(recording);
    again.release(&b);
    for hold in holds {
        b.overlay.release_lookup(hold).unwrap();
    }
}

// R3 row 7: a directory removed while a process still has it.

#[test]
fn a_base_directory_removed_while_held_is_absent_and_keeps_its_whiteout() {
    let b = Bench::new("ch-rmdir-base");
    let mut d = Drive::new(&b);
    let held = look(&b, 6);
    d.remove("cache/state");
    d.remove("cache");

    let (built, recording) = capture(&d, "held base directory");
    let row = tombstone(&recording, 6, "held base directory");
    assert_eq!(row.born, 0);
    // The removal under the removed base parent is kept as its one change.
    assert_eq!(recording.log.borrow().bound(6, "state"), Some(None));
    assert_eq!(
        record(&b, &built.taken, HEADER, 6),
        Some(1_u64.to_be_bytes().to_vec())
    );
    assert_eq!(built.work.headers_dropped, 0);
    assert_eq!(built.work.tombstones_skipped, 1);
    for kind in [VALUE, FRESH] {
        assert_eq!(record(&b, &built.taken, kind, 6), None);
    }
    assert!(absent(&b.fixture.store, built.root, 6));
    drop(recording);
    built.install(&b);
    drain(&b);

    // The same name is made again over the installed result.
    let again = d.mkdir("cache", 0o755);
    d.create("cache/state", 0o644);
    let (next, recording) = capture(&d, "held base directory, next capture");
    assert_eq!(next.walked.serial("cache"), again);
    assert!(absent(&b.fixture.store, next.root, 6));
    for kind in [VALUE, FRESH, HEADER] {
        assert_eq!(record(&b, &next.taken, kind, 6), None);
    }
    drop(recording);
    next.release(&b);
    b.overlay.release_lookup(held).unwrap();
}

#[test]
fn a_directory_created_filled_emptied_and_removed_while_held_is_dropped() {
    let b = Bench::new("ch-rmdir-fresh");
    let mut d = Drive::new(&b);
    let gone = d.mkdir("gone", 0o755);
    let held = look(&b, gone);
    let child = d.create("gone/child", 0o644);
    d.write("gone/child", 0, b"never part of any result");
    // A base file passes through the held directory and leaves again.
    d.rename("file", "gone/visitor");
    d.rename("gone/visitor", "file");
    d.remove("gone/child");
    d.remove("gone");

    let (built, recording) = capture(&d, "held fresh directory");
    let row = tombstone(&recording, gone, "held fresh directory");
    assert!(built.taken.reader.created_above(row.born));
    // Whether a name row survives under the tombstone is the engine's
    // business; if one does, its parent's header is dropped exactly once.
    let under = recording.log.borrow().parents().contains(&gone);
    evidence(format_args!(
        "R3 row 7 held fresh directory: captured name rows under the tombstone = {under}, headers_dropped = {}",
        built.work.headers_dropped
    ));
    assert_eq!(built.work.headers_dropped, u64::from(under));
    for serial in [gone, child] {
        never_constructed(&b, &built, &recording, serial, "held fresh directory");
    }
    {
        let log = recording.log.borrow();
        assert!(log.parent_pages.iter().all(|page| page.parent != gone));
        assert!(log.points.iter().all(|point| point.parent != gone));
    }
    assert_eq!(built.walked.serial("file"), 2);
    assert_eq!(built.work.fresh_serials, 0);
    drop(recording);
    built.install(&b);
    drain(&b);

    // The directory was never installed. The next capture, still holding its
    // lookup, builds the new publication and nothing for the tombstone.
    d.create("afterwards", 0o644);
    let (next, recording) = capture(&d, "held fresh directory, next capture");
    for serial in [gone, child] {
        never_constructed(&b, &next, &recording, serial, "next capture");
    }
    assert_eq!(next.work.headers_dropped, 0);
    drop(recording);
    next.release(&b);
    b.overlay.release_lookup(held).unwrap();
}

// R3 row 2: a name replaced by rename while the old inode is still held.

#[test]
fn a_stored_file_replaced_by_rename_while_open_leaves_only_the_new_inode() {
    let b = Bench::new("ch-replaced-base");
    let mut d = Drive::new(&b);
    let victim = d.create("victim", 0o644);
    d.write("victim", 0, &[1; 20_000]);
    d.build("a stored single-name file").install(&b);
    drain(&b);

    let file = hold(&b, victim);
    let newer = d.create("newer", 0o600);
    d.write("newer", 0, b"the replacement");
    d.rename("newer", "victim");
    d.orphan_write(file, 100, b"through the old descriptor");

    let (built, recording) = capture(&d, "replaced while open");
    assert_eq!(built.walked.serial("victim"), newer);
    assert!(!built.walked.has("newer"));
    let row = tombstone(&recording, victim, "replaced while open");
    assert!(
        !built.taken.reader.created_above(row.born),
        "a stored inode"
    );
    assert_eq!(built.work.tombstones_skipped, 1);
    // Exactly the replacement was constructed, under its own scope.
    assert_eq!(built.work.files_constructed, 1);
    assert!(recording.log.borrow().scoped(newer) > 0);
    never_constructed(&b, &built, &recording, victim, "replaced while open");
    drop(recording);
    built.install(&b);
    drain(&b);

    d.orphan_write(file, 0, b"again, after the install");
    d.write("victim", 0, b"and the new inode changes too");
    let (next, recording) = capture(&d, "replaced while open, next capture");
    assert_eq!(next.walked.serial("victim"), newer);
    no_row(&recording, victim, "next capture");
    assert_eq!(next.work.tombstones_skipped, 0);
    assert_eq!(next.work.files_constructed, 1);
    never_constructed(&b, &next, &recording, victim, "next capture");
    drop(recording);
    next.release(&b);
    b.overlay.close_file(file).unwrap();
}

#[test]
fn fresh_and_looked_up_inodes_replaced_by_rename_leave_only_the_new_inodes() {
    let b = Bench::new("ch-replaced-fresh");
    let mut d = Drive::new(&b);
    // A file created in this capture, open, then replaced.
    let old = d.create("target", 0o644);
    let file = hold(&b, old);
    d.write_open(file, "target", 0, &[8; 5000]);
    let new = d.create("source", 0o644);
    d.write("source", 0, b"wins");
    d.rename("source", "target");
    d.orphan_write(file, 0, b"loses");
    // The base symlink, held only by a lookup, replaced by a new symlink.
    let looked = look(&b, 3);
    let link = d.symlink("other-link", b"elsewhere");
    d.rename("other-link", "symlink");

    let (built, recording) = capture(&d, "replaced fresh and looked up");
    assert_eq!(built.walked.serial("target"), new);
    assert_eq!(built.walked.serial("symlink"), link);
    assert!(built
        .taken
        .reader
        .created_above(tombstone(&recording, old, "replaced").born));
    assert_eq!(tombstone(&recording, 3, "replaced").born, 0);
    assert_eq!(built.work.tombstones_skipped, 2);
    assert_eq!(
        (built.work.files_constructed, built.work.symlinks),
        (1, 1),
        "only the two replacements"
    );
    for serial in [old, 3] {
        never_constructed(&b, &built, &recording, serial, "replaced");
    }
    drop(recording);
    built.release(&b);
    b.overlay.close_file(file).unwrap();
    b.overlay.release_lookup(looked).unwrap();
}
