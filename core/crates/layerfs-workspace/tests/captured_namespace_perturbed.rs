//! Captured shapes a Workspace never writes, reached by changing rows rather
//! than failing calls: the provider adds name rows to what the capture really
//! holds, the same in the whole-capture sequence, the parent's own sequence
//! and the name point. Content must refuse a second parent and a cycle after
//! the producer sealed its rows; a name under a parent created and removed in
//! the capture must be dropped; a whiteout over nothing must change nothing.
mod common;
mod harness;
mod oracle;
mod producer;
use harness::Bench;
use layerfs_content::{ContentError, ObjectRole};
use layerfs_overlay::DirectoryEntry;
use layerfs_workspace::{CapturedNamespaceAttempt, WorkspaceError};
use oracle::value;
use producer::{
    build_taken, construct, drain, evidence, look, record, release, retained, slots, take,
    Counting, Drive, Perturbed, Slot, Taken, CONTEXT, HEADER, VALUE,
};

fn row(parent: u64, name: &str, serial: Option<u64>) -> DirectoryEntry {
    DirectoryEntry {
        inherited: false,
        parent,
        name: name.as_bytes().to_vec(),
        serial,
    }
}
/// The attempt was refused by Content with exactly this label, the cause is
/// the one failure of the attempt and sits in the namespace's own slot as the
/// Content error itself, and the sealed context record remains sealed.
fn refused_by_content(b: &Bench, taken: &Taken, attempt: &CapturedNamespaceAttempt, label: &str) {
    match &attempt.result {
        Err(ContentError::InvalidRecord(found)) => assert_eq!(*found, label),
        other => panic!("expected {label:?}, got {other:?}"),
    }
    assert_eq!(slots(&attempt.custody), vec![Slot::Failure]);
    match &attempt.custody.failure {
        Some(WorkspaceError::Content(ContentError::InvalidRecord(found))) => {
            assert_eq!(*found, label);
        }
        other => panic!("the cause is not Content's refusal: {other:?}"),
    }
    let context = record(b, taken, CONTEXT, 0).expect("the context record remains");
    assert_eq!(
        (context[0], context[1]),
        (1, 1),
        "the normalization stays sealed"
    );
    retained(b, taken);
}

#[test]
fn a_second_binding_of_a_stored_directory_is_refused_as_multiple_parents() {
    let b = Bench::new("cp-two-parents");
    let mut d = Drive::new(&b);
    // One real publication, so the capture is an ordinary one.
    d.create("real", 0o644);
    d.write("real", 0, b"an ordinary change");
    let taken = take(&b);
    // The base directory 6 is bound at the root; the row binds it again.
    let provider = Perturbed::new(&b.overlay, vec![row(7, "second", Some(6))]);
    let mut consumer = Counting::new(&b.fixture.store, &provider.inner);
    let attempt = construct(&b, &provider, &taken, &mut consumer);
    refused_by_content(&b, &taken, &attempt, "multiple parents");

    let log = provider.inner.log.borrow();
    // The row was served consistently on every route the producer used.
    assert_eq!(log.bound(7, "second"), Some(Some(6)));
    assert!(log
        .parent_pages
        .iter()
        .filter(|page| page.parent == 7)
        .all(|page| page.rows.iter().any(|row| row.name == b"second")));
    assert!(log
        .points
        .iter()
        .filter(|point| point.parent == 7 && point.name == b"second")
        .all(|point| point.answer.as_ref().and_then(|row| row.serial) == Some(6)));
    assert_eq!(
        record(&b, &taken, HEADER, 7),
        Some(1_u64.to_be_bytes().to_vec())
    );
    assert_eq!(
        consumer.of(ObjectRole::FilesystemRoot),
        0,
        "a root was offered"
    );
    let sealed = log.sealed_at().expect("the attempt sealed its rows");
    let late: Vec<ObjectRole> = consumer
        .offered
        .iter()
        .filter(|offer| offer.1 >= sealed)
        .map(|offer| offer.0)
        .collect();
    evidence(format_args!(
        "R4-2 multiple parents: offered {} objects, {} after the seal with roles {late:?}, parent pages of 7 = {}, points = {}",
        consumer.offered.len(),
        consumer.after(sealed),
        log.parent_pages.iter().filter(|page| page.parent == 7).count(),
        log.points.len()
    ));
    drop(log);
    release(&b, taken, attempt.custody);
}

#[test]
fn a_stored_directory_bound_only_below_itself_is_refused_as_a_cycle() {
    let b = Bench::new("cp-cycle");
    let mut d = Drive::new(&b);
    let outer = d.mkdir("a", 0o755);
    let inner = d.mkdir("a/b", 0o755);
    d.create("a/b/leaf", 0o644);
    d.build("nested").install(&b);
    drain(&b);

    // Nothing is published: the capture holds only the two injected rows,
    // which move the stored directory `a` below its own child.
    let taken = take(&b);
    let provider = Perturbed::new(
        &b.overlay,
        vec![
            DirectoryEntry {
                inherited: true,
                ..row(1, "a", None)
            },
            row(inner, "a", Some(outer)),
        ],
    );
    let mut consumer = Counting::new(&b.fixture.store, &provider.inner);
    let attempt = construct(&b, &provider, &taken, &mut consumer);
    refused_by_content(&b, &taken, &attempt, "effective tree cycle");

    let log = provider.inner.log.borrow();
    assert_eq!(log.bound(1, "a"), Some(None));
    assert_eq!(log.bound(inner, "a"), Some(Some(outer)));
    let sealed = log.sealed_at().expect("the attempt sealed its rows");
    assert_eq!(
        consumer.after(sealed),
        0,
        "the update offered {:?}",
        consumer.offered
    );
    assert_eq!(consumer.offered.len(), 0, "nothing was offered at all");
    drop(log);
    release(&b, taken, attempt.custody);
}

#[test]
fn a_name_under_a_parent_created_and_removed_in_the_capture_is_dropped() {
    let b = Bench::new("cp-dropped");
    let mut d = Drive::new(&b);
    d.create("kept", 0o644);
    // A real directory, created and removed in this capture while a real
    // lookup holds it, so its tombstone row is certainly captured.
    let ghost = d.mkdir("ghost", 0o755);
    let held = look(&b, ghost);
    d.remove("ghost");
    let taken = take(&b);
    // The one injected row: a name under that tombstone.
    let provider = Perturbed::new(&b.overlay, vec![row(ghost, "left-behind", None)]);
    let built = build_taken(&b, &provider, taken, &d.model, "dropped header");

    let log = provider.inner.log.borrow();
    let tombstone = log.inode(ghost).expect("the held directory keeps its row");
    assert_eq!(tombstone.nlink, 0);
    assert!(built.taken.reader.created_above(tombstone.born));
    assert_eq!(
        log.parents()
            .iter()
            .filter(|parent| **parent == ghost)
            .count(),
        1,
        "the producer saw the injected row"
    );
    assert_eq!(built.work.headers_dropped, 1);
    // The root's header is the only one written.
    assert_eq!(built.work.headers_written, 1);
    assert_eq!(record(&b, &built.taken, HEADER, ghost), None);
    assert_eq!(record(&b, &built.taken, VALUE, ghost), None);
    assert!(log.parent_pages.iter().all(|page| page.parent != ghost));
    assert!(log.points.iter().all(|point| point.parent != ghost));
    drop(log);
    built.release(&b);
    b.overlay.release_lookup(held).unwrap();
}

#[test]
fn a_whiteout_over_a_name_the_base_does_not_bind_changes_nothing() {
    // Alone: the result is the base root itself.
    let b = Bench::new("cp-whiteout-alone");
    let d = Drive::new(&b);
    let taken = take(&b);
    let provider = Perturbed::new(
        &b.overlay,
        vec![row(1, "never-bound", None), row(7, "nor-this", None)],
    );
    let built = build_taken(&b, &provider, taken, &d.model, "whiteouts alone");
    assert_eq!(built.root, b.fixture.root, "a no-op changed the root");
    assert_eq!(built.work.headers_written, 2);
    assert_eq!(built.work.entry_rows, 2);
    built.release(&b);

    // Beside real changes of the same parents: only the real changes land.
    let b = Bench::new("cp-whiteout-beside");
    let mut d = Drive::new(&b);
    d.create("real", 0o644);
    d.remove("output/result");
    let fresh = d.mkdir("fresh", 0o755);
    let taken = take(&b);
    let provider = Perturbed::new(
        &b.overlay,
        vec![
            row(1, "never-bound", None),
            row(6, "nor-this", None),
            row(7, "zz-nor-this", None),
            row(fresh, "nor-here", None),
        ],
    );
    let built = build_taken(&b, &provider, taken, &d.model, "whiteouts beside changes");
    let store = &b.fixture.store;
    // Directory 6 received only the no-op: it is the stored directory.
    assert_eq!(value(store, built.root, 6), value(store, b.fixture.root, 6));
    assert_eq!(built.work.headers_dropped, 0);
    assert_eq!(
        record(&b, &built.taken, HEADER, fresh),
        Some(1_u64.to_be_bytes().to_vec())
    );
    built.release(&b);
}
