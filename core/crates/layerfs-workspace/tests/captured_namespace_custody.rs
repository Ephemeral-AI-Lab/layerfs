//! R4-9 on the producer side: a failing provider or a rejecting consumer ends
//! the one attempt with its documented label, the first original failure in
//! exactly one custody slot, no later request and nothing released.
mod common;
mod harness;
mod oracle;
mod producer;
use harness::Bench;
use layerfs_content::{ContentError, ObjectRole};
use layerfs_overlay::OverlayError;
use layerfs_workspace::{
    CapturedNamespaceAttempt, OperationRecordRefusal, OverlayCapturedNamespace, WorkspaceError,
};
use oracle::value;
use producer::{
    build_taken, construct, evidence, injected, record, release, retained, slots, take, Call,
    Drive, Recording, Refused, Reject, Slot, Taken, BACKING, CALLS, CONTEXT, FILE, ORIGINAL,
};

/// Labels a failing file may end with: its own, or the namespace's when the
/// file's preparation refused before any construction.
const FILE_LABELS: [&str; 3] = [FILE, "captured file original refusal", BACKING];

/// One capture that needs every kind of provider call: a changed base file,
/// a chunked new file, a symlink, new and removed names and a moved directory.
fn scenario(d: &mut Drive<'_>) {
    d.write(".git/index", 150_000, &[0x42; 30_000]);
    d.create("new", 0o644);
    d.write("new", 0, &[0x17; 120_000]);
    d.append("new", &[0x71; 100_000]);
    d.symlink("link", b"new");
    d.mkdir("dir", 0o755);
    d.create("dir/a", 0o600);
    d.mkdir("dir/empty", 0o700);
    d.remove("alias");
    d.rename("cache", "output/c");
    d.chmod("output", 0o750);
}
/// The label of a refused attempt.
fn label(attempt: &CapturedNamespaceAttempt, what: &str) -> &'static str {
    match &attempt.result {
        Err(ContentError::ProviderFailure { what: found }) => found,
        Err(other) => panic!("{what}: an unexpected refusal {other:?}"),
        Ok(_) => panic!("{what}: the attempt succeeded"),
    }
}
/// The unsealed or sealed marker of the namespace's own context record.
fn sealed(b: &Bench, taken: &Taken) -> Option<bool> {
    record(b, taken, CONTEXT, 0).map(|context| {
        assert_eq!(context[0], 1);
        context[1] == 1
    })
}

#[test]
fn the_scenario_succeeds_and_uses_every_provider_call() {
    let b = Bench::new("cu-clean");
    let mut d = Drive::new(&b);
    scenario(&mut d);
    let recording = Recording::new(&b.overlay);
    let taken = take(&b);
    let built = build_taken(&b, &recording, taken, &d.model, "clean");
    let log = recording.log.borrow();
    for call in [
        Call::InodePage,
        Call::InodePoint,
        Call::NamePage,
        Call::ParentPage,
        Call::NamePoint,
        Call::Symlink,
        Call::RunStep,
        Call::NamespaceApply,
        Call::NamespaceGet,
        Call::NamespaceKeys,
        Call::FileApply,
    ] {
        assert!(log.count(call) > 0, "{call:?} was never needed");
    }
    assert_eq!((log.failed, log.after_failure), (None, 0));
    assert_eq!(sealed(&b, &built.taken), Some(true));
    drop(log);
    built.release(&b);
}

#[test]
fn every_single_provider_failure_ends_the_attempt_with_one_original_in_one_slot() {
    // The call counts of the unfailed attempt select the first, a middle and
    // the last call of each kind. Every run repeats the same fixed scenario.
    let counts = {
        let b = Bench::new("cu-sweep-count");
        let mut d = Drive::new(&b);
        scenario(&mut d);
        let recording = Recording::new(&b.overlay);
        let taken = take(&b);
        let built = build_taken(&b, &recording, taken, &d.model, "count");
        let counts = recording.log.borrow().counts.clone();
        built.release(&b);
        counts
    };
    let mut attempts = 0;
    for call in CALLS {
        let count = counts.get(&call).copied().unwrap_or(0);
        if count == 0 {
            continue;
        }
        let mut selected = vec![0, count / 2, count - 1];
        selected.dedup();
        for nth in selected {
            attempts += 1;
            let what = format!("{call:?} #{nth} of {count}");
            let b = Bench::new(&format!("cu-sweep-{call:?}-{nth}"));
            let mut d = Drive::new(&b);
            scenario(&mut d);
            let taken = take(&b);
            let recording = Recording::failing(&b.overlay, call, nth);
            let attempt = construct(&b, &recording, &taken, &mut b.fixture.store.clone());
            let refusal = label(&attempt, &what);
            let log = recording.log.borrow();
            assert_eq!(log.failed, Some(call), "{what}: the call was never made");
            assert_eq!(
                log.after_failure, 0,
                "{what}: a request followed the failure"
            );
            let custody = &attempt.custody;
            assert_eq!(
                (custody.reader, custody.operation),
                (taken.reader, taken.operation)
            );
            let held = slots(custody);
            assert_eq!(held.len(), 1, "{what}: {held:?} in {custody:#?}");
            match held[0] {
                Slot::Failure => {
                    assert_eq!(injected(&custody.failure), Some(call), "{what}");
                    assert_eq!(refusal, ORIGINAL, "{what}");
                }
                Slot::Records => {
                    assert_eq!(injected(&custody.records.failure), Some(call), "{what}");
                    assert_eq!(refusal, BACKING, "{what}");
                    assert_eq!(custody.records.scope.file_scope, 0);
                }
                Slot::File => {
                    let file = custody.file.as_ref().unwrap();
                    assert!(
                        file.failure.is_some() != file.records.failure.is_some(),
                        "{what}: one original inside the file custody: {file:#?}"
                    );
                    assert_eq!(
                        injected(&file.failure).or(injected(&file.records.failure)),
                        Some(call),
                        "{what}"
                    );
                    assert_eq!(file.records.scope.file_scope, file.serial);
                    assert_eq!(file.records.scope.owner, taken.operation);
                    assert!(FILE_LABELS.contains(&refusal), "{what}: {refusal}");
                }
            }
            // Counted work stops where the failure happened: no update pass
            // can follow a failed normalization.
            if sealed(&b, &taken) != Some(true) {
                let work = custody.work;
                assert_eq!(work.header_opens + work.value_opens + work.fresh_opens, 0);
            }
            retained(&b, &taken);
            evidence(format_args!(
                "R4-9 sweep {what}: slot={:?} label={refusal:?} calls={} after_failure={}",
                held[0],
                log.total(),
                log.after_failure
            ));
            drop(log);
            release(&b, taken, attempt.custody);
        }
    }
    assert!(attempts >= 15, "only {attempts} failures were exercised");
}

/// The scenario, one injected failure and the refused attempt.
fn failed(tag: &str, call: Call, nth: u64) -> (Bench, Taken, CapturedNamespaceAttempt) {
    let b = Bench::new(tag);
    let (taken, attempt) = {
        let mut d = Drive::new(&b);
        scenario(&mut d);
        let taken = take(&b);
        let recording = Recording::failing(&b.overlay, call, nth);
        let attempt = construct(&b, &recording, &taken, &mut b.fixture.store.clone());
        let log = recording.log.borrow();
        assert_eq!((log.failed, log.after_failure), (Some(call), 0));
        (taken, attempt)
    };
    (b, taken, attempt)
}

#[test]
fn a_failed_name_page_is_the_namespace_failure_before_anything_is_sealed() {
    let (b, taken, attempt) = failed("cu-name-page", Call::NamePage, 0);
    assert_eq!(label(&attempt, "name page"), ORIGINAL);
    assert_eq!(slots(&attempt.custody), vec![Slot::Failure]);
    assert_eq!(injected(&attempt.custody.failure), Some(Call::NamePage));
    // The unsealed context is all this attempt wrote, and it remains.
    assert_eq!(sealed(&b, &taken), Some(false));
    let work = attempt.custody.work;
    assert_eq!(
        (work.entry_rows, work.inode_rows, work.values_written),
        (0, 0, 0)
    );
    assert_eq!(work.files_constructed, 0);
    retained(&b, &taken);
    release(&b, taken, attempt.custody);
}

#[test]
fn a_failed_inode_page_is_the_namespace_failure_after_the_names() {
    let (b, taken, attempt) = failed("cu-inode-page", Call::InodePage, 0);
    assert_eq!(label(&attempt, "inode page"), ORIGINAL);
    assert_eq!(slots(&attempt.custody), vec![Slot::Failure]);
    assert_eq!(injected(&attempt.custody.failure), Some(Call::InodePage));
    assert_eq!(sealed(&b, &taken), Some(false));
    let work = attempt.custody.work;
    assert!(work.entry_rows > 0 && work.headers_written > 0);
    assert_eq!((work.inode_rows, work.files_constructed), (0, 0));
    retained(&b, &taken);
    release(&b, taken, attempt.custody);
}

#[test]
fn a_failed_name_point_or_cursor_page_ends_the_sealed_update() {
    for (tag, call) in [
        ("cu-name-point", Call::NamePoint),
        ("cu-parent-page", Call::ParentPage),
    ] {
        let (b, taken, attempt) = failed(tag, call, 0);
        assert_eq!(label(&attempt, tag), ORIGINAL);
        assert_eq!(slots(&attempt.custody), vec![Slot::Failure]);
        assert_eq!(injected(&attempt.custody.failure), Some(call));
        // Normalization completed and stays sealed; the update did not.
        assert_eq!(sealed(&b, &taken), Some(true));
        assert!(attempt.custody.work.files_constructed > 0);
        retained(&b, &taken);
        release(&b, taken, attempt.custody);
    }
}

#[test]
fn a_failed_symlink_target_read_is_the_namespace_failure() {
    let (b, taken, attempt) = failed("cu-symlink", Call::Symlink, 0);
    assert_eq!(label(&attempt, "symlink"), ORIGINAL);
    assert_eq!(slots(&attempt.custody), vec![Slot::Failure]);
    assert_eq!(injected(&attempt.custody.failure), Some(Call::Symlink));
    assert_eq!(attempt.custody.work.symlinks, 0);
    assert_eq!(sealed(&b, &taken), Some(false));
    retained(&b, &taken);
    release(&b, taken, attempt.custody);
}

#[test]
fn a_failed_record_job_stays_in_the_record_custody() {
    // The second guarded job of the namespace's own scope: its first batch.
    let (b, taken, attempt) = failed("cu-records", Call::NamespaceApply, 1);
    assert_eq!(label(&attempt, "record job"), BACKING);
    assert_eq!(slots(&attempt.custody), vec![Slot::Records]);
    let records = &attempt.custody.records;
    assert_eq!(injected(&records.failure), Some(Call::NamespaceApply));
    assert_eq!(
        (records.scope.owner, records.scope.file_scope),
        (taken.operation, 0)
    );
    assert_eq!(
        records.work.terminal_calls, 0,
        "nothing was asked afterwards"
    );
    assert_eq!(sealed(&b, &taken), Some(false));
    retained(&b, &taken);
    release(&b, taken, attempt.custody);
}

#[test]
fn a_failed_captured_run_is_kept_as_the_whole_file_custody() {
    let (b, taken, attempt) = failed("cu-file-run", Call::RunStep, 0);
    let refusal = label(&attempt, "file run");
    assert!(FILE_LABELS.contains(&refusal), "{refusal}");
    assert_eq!(slots(&attempt.custody), vec![Slot::File]);
    let custody = &attempt.custody;
    let file = custody.file.as_ref().unwrap();
    assert_eq!(injected(&file.failure), Some(Call::RunStep));
    assert!(file.records.failure.is_none());
    assert_eq!(file.reader, taken.reader);
    assert_eq!(
        (file.records.scope.owner, file.records.scope.file_scope),
        (taken.operation, file.serial)
    );
    // The namespace's own slots are untouched and nothing was sealed.
    assert!(custody.failure.is_none() && custody.records.failure.is_none());
    assert_eq!(sealed(&b, &taken), Some(false));
    retained(&b, &taken);
    release(&b, taken, attempt.custody);
}

#[test]
fn a_failed_file_record_job_is_kept_inside_the_file_custody() {
    let (b, taken, attempt) = failed("cu-file-records", Call::FileApply, 0);
    let refusal = label(&attempt, "file records");
    assert!(FILE_LABELS.contains(&refusal), "{refusal}");
    assert_eq!(slots(&attempt.custody), vec![Slot::File]);
    let file = attempt.custody.file.as_ref().unwrap();
    assert_eq!(injected(&file.records.failure), Some(Call::FileApply));
    assert_eq!(file.records.scope.file_scope, file.serial);
    assert!(attempt.custody.records.failure.is_none());
    retained(&b, &taken);
    release(&b, taken, attempt.custody);
}

#[test]
fn a_released_reader_is_the_original_stale_refusal_of_the_engine() {
    let b = Bench::new("cu-stale");
    let mut d = Drive::new(&b);
    scenario(&mut d);
    let taken = take(&b);
    b.overlay.release_captured_reader(taken.reader).unwrap();
    let recording = Recording::new(&b.overlay);
    let attempt = construct(&b, &recording, &taken, &mut b.fixture.store.clone());
    assert_eq!(label(&attempt, "released reader"), ORIGINAL);
    assert_eq!(slots(&attempt.custody), vec![Slot::Failure]);
    // The engine's own error is retained as it was returned.
    let original = attempt.custody.failure.as_ref().unwrap();
    let direct =
        OverlayCapturedNamespace::captured_directory_entry_page(&b.overlay, taken.reader, None)
            .unwrap_err();
    assert!(
        matches!(original, WorkspaceError::Overlay(_)),
        "{original:?}"
    );
    assert_eq!(format!("{original:?}"), format!("{direct:?}"));
    // Exactly one reader call was made, and the operation is still owned.
    let log = recording.log.borrow();
    assert_eq!(log.count(Call::NamePage), 1);
    assert_eq!(log.count(Call::InodePage) + log.count(Call::InodePoint), 0);
    assert_eq!(
        b.overlay
            .retained_operation(b.route(), taken.operation_request)
            .unwrap(),
        Some(taken.operation)
    );
    drop(log);
    drop(attempt);
    b.overlay.release_operation(taken.operation).unwrap();
}

#[test]
fn a_reader_of_another_operation_route_is_refused_before_any_request() {
    let b = Bench::new("cu-route");
    let other = Bench::new("cu-route-other");
    let mut d = Drive::new(&b);
    scenario(&mut d);
    let taken = take(&b);
    let foreign = take(&other);
    // This Workspace, its reader, and an operation owned by another engine.
    let crossed = Taken {
        operation: foreign.operation,
        ..taken
    };
    let recording = Recording::new(&b.overlay);
    let attempt = construct(&b, &recording, &crossed, &mut b.fixture.store.clone());
    assert_eq!(label(&attempt, "crossed route"), ORIGINAL);
    assert_eq!(slots(&attempt.custody), vec![Slot::Failure]);
    assert!(matches!(
        attempt.custody.failure,
        Some(WorkspaceError::Overlay(OverlayError::Stale))
    ));
    assert_eq!(recording.log.borrow().total(), 0, "no request was made");
    drop(attempt);
    retained(&b, &taken);
    retained(&other, &foreign);
}

#[test]
fn a_second_attempt_in_the_same_scope_is_refused_by_its_deciding_record() {
    let b = Bench::new("cu-second");
    let mut d = Drive::new(&b);
    scenario(&mut d);
    let taken = take(&b);
    let first = build_taken(&b, &b.overlay, taken, &d.model, "first");
    let recording = Recording::new(&b.overlay);
    let again = construct(&b, &recording, &taken, &mut b.fixture.store.clone());
    assert_eq!(
        label(&again, "second attempt"),
        "captured namespace deciding record refusal"
    );
    assert_eq!(slots(&again.custody), vec![Slot::Records]);
    match &again.custody.records.failure {
        Some(WorkspaceError::Service(original)) => {
            assert!(original.downcast_ref::<OperationRecordRefusal>().is_some());
        }
        other => panic!("{other:?}"),
    }
    // One guarded job, refused; the first attempt's sealed records remain.
    let log = recording.log.borrow();
    assert_eq!(log.total(), 1);
    assert_eq!(log.count(Call::NamespaceApply), 1);
    assert_eq!(sealed(&b, &taken), Some(true));
    drop(log);
    drop(again);
    first.release(&b);
}

#[test]
fn a_consumer_that_rejects_a_file_root_leaves_the_file_custody() {
    // Attribute values are file-state objects too, so the role alone would
    // refuse the root's metadata first. The refused object is exactly the
    // changed base file's final root, known from the same scenario unfailed.
    let file_root = {
        let b = Bench::new("cu-reject-file-root");
        let mut d = Drive::new(&b);
        scenario(&mut d);
        let built = d.build("unfailed");
        let root = value(&b.fixture.store, built.root, 8).content_root;
        built.release(&b);
        root
    };
    let b = Bench::new("cu-reject-file");
    let mut d = Drive::new(&b);
    scenario(&mut d);
    let taken = take(&b);
    let recording = Recording::new(&b.overlay);
    let mut consumer = Reject::new(&b.fixture.store, Refused::Object(file_root), &recording);
    let attempt = construct(&b, &recording, &taken, &mut consumer);
    assert!(matches!(attempt.result, Err(ContentError::OutputRejected)));
    assert_eq!(slots(&attempt.custody), vec![Slot::File]);
    let file = attempt.custody.file.as_ref().unwrap();
    assert!(matches!(
        file.failure,
        Some(WorkspaceError::Content(ContentError::OutputRejected))
    ));
    assert!(file.records.failure.is_none());
    assert_eq!((file.serial, file.reader), (8, taken.reader));
    assert_eq!(
        recording.log.borrow().after_failure,
        0,
        "a provider request followed the consumer's refusal"
    );
    // Accepted children stay with the consumer; nothing followed the refusal.
    assert!(consumer.accepted > 0);
    assert_eq!((consumer.rejected, consumer.after_rejection), (1, 0));
    assert_eq!(sealed(&b, &taken), Some(false));
    retained(&b, &taken);
    release(&b, taken, attempt.custody);
}

#[test]
fn a_consumer_that_rejects_namespace_output_is_the_namespace_failure() {
    for (tag, role, update) in [
        ("cu-reject-root", ObjectRole::FilesystemRoot, true),
        ("cu-reject-directory", ObjectRole::DirectoryLeaf, true),
        ("cu-reject-inodes", ObjectRole::InodeLeaf, true),
        ("cu-reject-metadata", ObjectRole::AttributeLeaf, false),
        // The first file-state object is the root's own metadata value.
        ("cu-reject-value", ObjectRole::FileState, false),
        ("cu-reject-symlink", ObjectRole::Symlink, false),
    ] {
        let b = Bench::new(tag);
        let mut d = Drive::new(&b);
        scenario(&mut d);
        let taken = take(&b);
        let recording = Recording::new(&b.overlay);
        let mut consumer = Reject::new(&b.fixture.store, Refused::Role(role), &recording);
        let attempt = construct(&b, &recording, &taken, &mut consumer);
        assert!(
            matches!(attempt.result, Err(ContentError::OutputRejected)),
            "{tag}"
        );
        assert_eq!(slots(&attempt.custody), vec![Slot::Failure], "{tag}");
        assert!(
            matches!(
                attempt.custody.failure,
                Some(WorkspaceError::Content(ContentError::OutputRejected))
            ),
            "{tag}"
        );
        assert_eq!(
            (consumer.rejected, consumer.after_rejection),
            (1, 0),
            "{tag}"
        );
        // A refusal by the update leaves the normalization sealed; a refusal
        // while normalizing leaves it unsealed. Neither is undone.
        assert_eq!(sealed(&b, &taken), Some(update), "{tag}");
        assert_eq!(
            recording.log.borrow().after_failure,
            0,
            "{tag}: a provider request followed the consumer's refusal"
        );
        // A refusal while normalizing happens before any file is constructed
        // only for the root's own metadata; the others follow real work.
        evidence(format_args!(
            "R4-9 reject {role:?}: accepted={} files_constructed={} provider_calls={}",
            consumer.accepted,
            attempt.custody.work.files_constructed,
            recording.log.borrow().total()
        ));
        retained(&b, &taken);
        release(&b, taken, attempt.custody);
    }
}
