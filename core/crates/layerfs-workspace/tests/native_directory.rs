//! Canonical directory merge, original cookie boundaries and removed-dir custody.
mod common;
mod harness;
use harness::{create, mkdir, path, rename, rmdir, unlink, Bench, T1, T2};
use layerfs_content::{
    filesystem::{
        update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources,
    },
    object::inode_leaf::InodeKind,
};
use layerfs_overlay::{NativeDirectory, NativeMount, OverlayError};
use layerfs_workspace::{
    CanonicalCache, CanonicalClient, NativeDirectoryBatch, NativeDirectoryWindow,
    NativeReadDecision, NativeReadOperation, VisitFacts,
};
use std::sync::Arc;
/// A memory-only client over a cache that holds nothing.
fn empty() -> Arc<CanonicalClient> {
    Arc::new(CanonicalClient::resident(Arc::new(CanonicalCache::new(
        1 << 20,
    ))))
}
/// One opening or reading visit per turn until it decides, reading the facts
/// an undecided visit names. A job itself asks the provider nothing.
fn decide(b: &Bench, mount: NativeMount, request: u64, serial: u64) -> Option<NativeDirectory> {
    let mut facts = VisitFacts::default();
    for _ in 0..4 {
        let visit = match request {
            0 => b.workspace.native_read_visit(
                empty(),
                mount,
                1,
                None,
                NativeReadOperation::Lookup {
                    parent: 1,
                    name: common::name(["empty", "moving"][serial as usize]),
                },
                Arc::new(facts.clone()),
            ),
            _ => b.workspace.native_opendir_visit(
                empty(),
                mount,
                request,
                serial,
                Arc::new(facts.clone()),
            ),
        }
        .unwrap();
        let before = b.demand();
        let outcome = visit.perform(&b.overlay);
        assert_eq!(b.demand(), before, "owner made provider demand");
        assert!(matches!(outcome.result, Ok(None)), "{:?}", outcome.result);
        match outcome.decision {
            Some(NativeReadDecision::Needs(needs)) => facts
                .supply(&b.workspace.base().unwrap(), &needs, None)
                .unwrap(),
            Some(NativeReadDecision::Value(_)) => return outcome.directory_candidate,
            other => panic!("undecided visit: {other:?}"),
        }
    }
    panic!("directory decision did not finish its bounded fact rounds")
}
fn open(b: &Bench, mount: NativeMount, request: u64, serial: u64) -> NativeDirectory {
    decide(b, mount, request, serial).expect("an open directory")
}
/// One READDIR reading visit as one owner job that asks the provider nothing.
fn visit(
    b: &Bench,
    directory: NativeDirectory,
    offset: u64,
    after: Option<Vec<u8>>,
) -> Result<NativeDirectoryWindow, OverlayError> {
    let visit = b
        .workspace
        .native_directory_visit(
            empty(),
            directory.mount(),
            directory.serial(),
            directory.owner_id(),
            offset,
            after,
        )
        .unwrap();
    let before = b.demand();
    let window = visit.perform(&b.overlay);
    assert_eq!(b.demand(), before, "owner made provider demand");
    window
}
/// The reply's entries, merged outside the owner when memory held too little.
fn batch(b: &Bench, window: NativeDirectoryWindow) -> NativeDirectoryBatch {
    window.finish(Some(&b.workspace.base().unwrap())).unwrap()
}
#[test]
fn empty_whiteout_pages_continue_and_only_accepted_names_become_resume_cookies() {
    let b = Bench::with_cache("native-directory-whiteouts", 0);
    // Fixture setup uses the actual canonical builder and installed-base port.
    // This is no Commit construction or native mounted acceptance claim.
    let store = &b.fixture.store;
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(store, &mut sink);
    let updates = [DirectoryUpdate {
        parent: 1,
        changes: (0..130)
            .map(|n| (common::name(&format!("!{n:03}")), Some(2)))
            .collect(),
    }];
    let next = update_filesystem(
        &mut objects,
        &FilesystemInput {
            base: Some(b.fixture.root),
            scope: b.fixture.scope,
            root_serial: 1,
            directories: &updates,
            inodes: &[],
            new_inodes: &[],
            resources: FilesystemResources::default(),
        },
        None,
    )
    .unwrap()
    .root;
    let capture = b.overlay.capture(b.route()).unwrap();
    let prepared = b.workspace.prepare_base_install(capture, next).unwrap();
    b.workspace
        .install_prepared_base(&b.overlay, prepared)
        .unwrap();
    for n in 0..130 {
        b.applied(unlink(1, &format!("!{n:03}")), T1);
    }
    let local = b.applied(create(1, "zz-local"), T2).unwrap().serial;
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let directory = open(&b, mount, 1, 1);
    // READDIR needs inode kinds, not file lengths or portable attributes.
    // Make the otherwise valid fixture's length provider unavailable here.
    let lengths = std::mem::take(&mut *b.fixture.store.lengths.lock().unwrap());
    let mut after = None;
    let mut last = None;
    for turn in 0..3 {
        // Nothing of the base is resident: the job leaves the merge to its
        // request and still offers the reply's offsets.
        let window = visit(&b, directory, 2, after.take()).unwrap();
        assert!(window.listing.is_none() && window.offer.is_some());
        assert!(window.page.local.active.len() <= 64);
        let batch = batch(&b, window);
        if turn < 2 {
            assert!(batch.entries.is_empty());
            assert!(batch.continuation.is_some());
            after = batch.continuation;
        } else {
            last = Some(batch);
        }
    }
    let listed = last.unwrap();
    assert!(b.fixture.store.lengths.lock().unwrap().is_empty());
    *b.fixture.store.lengths.lock().unwrap() = lengths;
    assert_eq!(listed.entries.len(), 8);
    assert!(listed
        .entries
        .iter()
        .any(|e| e.name == b"symlink" && e.kind == InodeKind::Symlink));
    assert!(listed
        .entries
        .iter()
        .any(|e| e.serial == local && e.kind == InodeKind::RegularFile));
    assert_eq!(
        b.overlay.native_lookup_count(mount, 2).unwrap(),
        None,
        "READDIR acquired no lookup"
    );
    // The reply accepted two names: exactly those become offsets.
    let names: Vec<_> = listed.entries.iter().map(|e| e.name.clone()).collect();
    assert_eq!(names[1], b"alias");
    let offer = listed.publish.expect("a fresh reply publishes its names");
    assert_eq!(offer.first(), listed.first);
    b.overlay
        .publish_native_cookies(&offer, &names[..2])
        .unwrap();
    let cookie = listed.first + 1;
    assert!(matches!(
        visit(&b, directory, listed.first + 2, None),
        Err(OverlayError::Stale)
    ));
    // A handle that seeks back to its first name is answered with the reply
    // it was given: the same two names at the same offsets, and nothing to
    // publish.
    let mut after = None;
    let again = loop {
        let again = batch(&b, visit(&b, directory, 2, after.take()).unwrap());
        if !again.entries.is_empty() {
            break again;
        }
        after = Some(again.continuation.expect("more names"));
    };
    assert_eq!(again.first, listed.first);
    assert!(again.publish.is_none());
    assert!(again.entries.iter().map(|e| &e.name).eq(&names[..2]));
    b.applied(unlink(1, "alias"), T2);
    b.applied(unlink(1, "file"), T2);
    // The offset resumes strictly after its name, which is gone by now.
    let window = visit(&b, directory, cookie, None).unwrap();
    assert_eq!(window.page.cursor().after_name(), Some(b"alias".as_slice()));
    let next = batch(&b, window);
    assert!(!next.entries.is_empty() && next.publish.is_some());
    assert!(next
        .entries
        .iter()
        .all(|e| e.name.as_slice() > b"alias" && e.name != b"file"));
    b.overlay
        .close_native_directory(directory.mount(), directory.serial(), directory.owner_id())
        .unwrap();
    assert!(matches!(
        visit(&b, directory, 2, None),
        Err(OverlayError::Stale)
    ));
    b.overlay.revoke_native_mount(mount).unwrap();
}
#[test]
fn a_reply_listed_from_offset_0_again_is_published_afresh_and_earlier_offsets_list_nothing() {
    let b = Bench::new("native-directory-rewind");
    b.applied(create(1, "zz-local"), T1);
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let directory = open(&b, mount, 1, 1);
    let publish = |listed: &NativeDirectoryBatch, accepted: usize| {
        let names: Vec<_> = listed.entries.iter().map(|e| e.name.clone()).collect();
        let offer = listed.publish.as_ref().expect("a reply to publish");
        b.overlay
            .publish_native_cookies(offer, &names[..accepted])
            .unwrap();
    };
    // The first listing from offset 0 is an ordinary one.
    let first = batch(&b, visit(&b, directory, 0, None).unwrap());
    assert!(first.entries.len() > 2);
    assert!(!first.publish.as_ref().unwrap().rewinds());
    publish(&first, first.entries.len());
    let last = first.first + first.entries.len() as u64 - 1;
    // A seek back to the first name reuses that reply and publishes nothing.
    let back = batch(&b, visit(&b, directory, 2, None).unwrap());
    assert_eq!((back.first, back.publish.is_none()), (first.first, true));

    // The same names from offset 0 again: a rewind. The reply is not the
    // earlier one at its offsets but a fresh one, to be published.
    let again = batch(&b, visit(&b, directory, 0, None).unwrap());
    assert_eq!(again.entries, first.entries);
    assert!(again.first > last);
    assert!(again.publish.as_ref().unwrap().rewinds());
    // Until it is published the earlier offsets stand.
    assert!(!visit(&b, directory, last, None).unwrap().rewound());
    publish(&again, 2);
    for offset in [first.first, last] {
        let window = visit(&b, directory, offset, None).unwrap();
        assert!(window.rewound());
        assert!(window.offer.is_none() && window.page.local.active.is_empty());
        let listed = window.finish(None).unwrap();
        assert!(listed.entries.is_empty() && listed.continuation.is_none());
        assert!(listed.publish.is_none());
    }
    // An offset of the new reply resumes strictly after its name.
    let next = visit(&b, directory, again.first + 1, None).unwrap();
    assert!(!next.rewound());
    assert_eq!(
        next.page.cursor().after_name(),
        Some(first.entries[1].name.as_slice())
    );
    let next = batch(&b, next);
    assert_eq!(next.entries[..], first.entries[2..]);
    b.overlay
        .close_native_directory(directory.mount(), directory.serial(), directory.owner_id())
        .unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}

#[test]
fn opened_directory_retains_parent_and_metadata_after_forget_and_rmdir() {
    let b = Bench::new("native-directory-removed");
    let serial = b.applied(mkdir(1, "empty"), T1).unwrap().serial;
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    assert_eq!(decide(&b, mount, 0, 0), None);
    let directory = open(&b, mount, 2, serial);
    b.overlay.forget_native(mount, serial, 1).unwrap();
    b.applied(rmdir(1, "empty"), T2);
    // The removed directory is still listed through its descriptor: its
    // parent as it was, and no name. The job decides that alone.
    let window = visit(&b, directory, 0, None).unwrap();
    assert_eq!(window.page.parent(), Some(1));
    assert_eq!(window.page.local.parent_inode.as_ref().unwrap().nlink, 0);
    assert!(window.listing.is_some() && window.offer.is_none());
    let listed = window.finish(None).unwrap();
    assert!(listed.entries.is_empty() && listed.continuation.is_none());
    assert!(listed.publish.is_none());
    b.overlay
        .close_native_directory(directory.mount(), directory.serial(), directory.owner_id())
        .unwrap();
    assert!(matches!(
        visit(&b, directory, 0, None),
        Err(OverlayError::Stale)
    ));
    b.overlay.revoke_native_mount(mount).unwrap();
}

#[test]
fn next_directory_read_observes_parent_moved_by_the_atomic_namespace_job() {
    let b = Bench::new("native-directory-moved");
    let serial = b.applied(mkdir(1, "moving"), T1).unwrap().serial;
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    assert_eq!(decide(&b, mount, 0, 1), None);
    let directory = open(&b, mount, 2, serial);
    b.overlay.forget_native(mount, serial, 1).unwrap();
    let before = visit(&b, directory, 0, None).unwrap();
    assert_eq!(before.page.parent(), Some(1));
    b.applied(
        rename((1, "moving"), (7, "moved"), true, path(&["output"])),
        T2,
    );
    // Each reading visit observes the parent of its own moment; a reply
    // past the dots reads none.
    let after = visit(&b, directory, 0, None).unwrap();
    assert_eq!(after.page.parent(), Some(7));
    assert_eq!(visit(&b, directory, 2, None).unwrap().page.parent(), None);
    b.overlay
        .close_native_directory(directory.mount(), directory.serial(), directory.owner_id())
        .unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}
