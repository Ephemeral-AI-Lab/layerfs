//! Canonical directory merge, original cookie boundaries and removed-dir custody.
mod common;
mod harness;
use harness::{create, mkdir, path, rename, rmdir, unlink, Bench, T1, T2};
use layerfs_content::{
    filesystem::{
        attributes::PortableMetadata, build_filesystem, scope_for_seed, update_filesystem,
        DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources, InodeUpdate,
    },
    object::inode_leaf::{InodeKind, InodeMetadata, InodeValue},
    ObjectId,
};
use layerfs_overlay::{KernelDirectory, KernelMount, OverlayError, PAGE_ROWS};
use layerfs_workspace::{
    CanonicalCache, CanonicalClient, KernelDirectoryBatch, KernelDirectoryRead,
    KernelDirectoryWindow, KernelReadDecision, KernelReadOperation, VisitFacts,
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
fn decide(b: &Bench, mount: KernelMount, request: u64, serial: u64) -> Option<KernelDirectory> {
    let mut facts = VisitFacts::default();
    for _ in 0..4 {
        let visit = match request {
            0 => b.workspace.kernel_read_visit(
                empty(),
                mount,
                1,
                None,
                KernelReadOperation::Lookup {
                    parent: 1,
                    name: common::name(["empty", "moving"][serial as usize]),
                },
                Arc::new(facts.clone()),
            ),
            _ => b.workspace.kernel_opendir_visit(
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
            Some(KernelReadDecision::Needs(needs)) => facts
                .supply(&b.workspace.base().unwrap(), &needs, None)
                .unwrap(),
            Some(KernelReadDecision::Value(_)) => return outcome.directory_candidate,
            other => panic!("undecided visit: {other:?}"),
        }
    }
    panic!("directory decision did not finish its bounded fact rounds")
}
fn open(b: &Bench, mount: KernelMount, request: u64, serial: u64) -> KernelDirectory {
    decide(b, mount, request, serial).expect("an open directory")
}
/// One READDIR reading visit as one owner job that asks the provider nothing.
fn visit(
    b: &Bench,
    directory: KernelDirectory,
    offset: u64,
    after: Option<Vec<u8>>,
) -> Result<KernelDirectoryWindow, OverlayError> {
    visit_with(b, directory, offset, after, false)
}
/// The same, for a reply that shows each entry's facts (READDIRPLUS).
fn visit_with(
    b: &Bench,
    directory: KernelDirectory,
    offset: u64,
    after: Option<Vec<u8>>,
    attributes: bool,
) -> Result<KernelDirectoryWindow, OverlayError> {
    let visit = b
        .workspace
        .kernel_directory_visit(
            empty(),
            directory.mount(),
            directory.serial(),
            directory.owner_id(),
            KernelDirectoryRead {
                offset,
                after,
                attributes,
            },
        )
        .unwrap();
    let before = b.demand();
    let window = visit.perform(&b.overlay);
    assert_eq!(b.demand(), before, "owner made provider demand");
    window
}
/// The reply's entries, merged outside the owner when memory held too little.
fn batch(b: &Bench, window: KernelDirectoryWindow) -> KernelDirectoryBatch {
    window.finish(Some(&b.workspace.base().unwrap())).unwrap()
}
#[test]
fn empty_whiteout_pages_continue_and_only_accepted_names_become_resume_cookies() {
    let b = Bench::with_cache("kernel-directory-whiteouts", 0);
    // Fixture setup uses the actual canonical builder and installed-base port.
    // This is no Commit construction or kernel mounted acceptance claim.
    let store = &b.fixture.store;
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(store, &mut sink);
    let updates = [DirectoryUpdate {
        parent: 1,
        changes: (0..2 * PAGE_ROWS + 2)
            .map(|n| (common::name(&format!("!{n:04}")), Some(2)))
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
    let capture = b.overlay.capture().unwrap();
    let prepared = b.workspace.prepare_base_install(capture, next).unwrap();
    b.workspace
        .install_prepared_base(&b.overlay, prepared)
        .unwrap();
    for n in 0..2 * PAGE_ROWS + 2 {
        b.applied(unlink(1, &format!("!{n:04}")), T1);
    }
    let local = b.applied(create(1, "zz-local"), T2).unwrap().serial;
    let mount = b.overlay.create_kernel_mount(1).unwrap();
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
        assert!(window.page.local.active.len() <= PAGE_ROWS);
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
    // The reply accepted two names: exactly those become offsets.
    let names: Vec<_> = listed.entries.iter().map(|e| e.name.clone()).collect();
    assert_eq!(names[1], b"alias");
    let offer = listed.publish.expect("a fresh reply publishes its names");
    assert_eq!(offer.first(), listed.first);
    b.overlay
        .publish_kernel_cookies(&offer, &names[..2])
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
        .close_kernel_directory(directory.mount(), directory.serial(), directory.owner_id())
        .unwrap();
    assert!(matches!(
        visit(&b, directory, 2, None),
        Err(OverlayError::Stale)
    ));
    b.overlay.revoke_kernel_mount(mount).unwrap();
}
#[test]
fn a_reply_listed_from_offset_0_again_is_published_afresh_and_earlier_offsets_list_nothing() {
    let b = Bench::new("kernel-directory-rewind");
    b.applied(create(1, "zz-local"), T1);
    let mount = b.overlay.create_kernel_mount(1).unwrap();
    let directory = open(&b, mount, 1, 1);
    let publish = |listed: &KernelDirectoryBatch, accepted: usize| {
        let names: Vec<_> = listed.entries.iter().map(|e| e.name.clone()).collect();
        let offer = listed.publish.as_ref().expect("a reply to publish");
        b.overlay
            .publish_kernel_cookies(offer, &names[..accepted])
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
        .close_kernel_directory(directory.mount(), directory.serial(), directory.owner_id())
        .unwrap();
    b.overlay.revoke_kernel_mount(mount).unwrap();
}

#[test]
fn opened_directory_retains_parent_and_metadata_after_forget_and_rmdir() {
    let b = Bench::new("kernel-directory-removed");
    let serial = b.applied(mkdir(1, "empty"), T1).unwrap().serial;
    let mount = b.overlay.create_kernel_mount(1).unwrap();
    assert_eq!(decide(&b, mount, 0, 0), None);
    let directory = open(&b, mount, 2, serial);
    b.overlay.kernel_forget(mount, serial, 1).unwrap();
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
        .close_kernel_directory(directory.mount(), directory.serial(), directory.owner_id())
        .unwrap();
    assert!(matches!(
        visit(&b, directory, 0, None),
        Err(OverlayError::Stale)
    ));
    b.overlay.revoke_kernel_mount(mount).unwrap();
}

#[test]
fn next_directory_read_observes_parent_moved_by_the_atomic_namespace_job() {
    let b = Bench::new("kernel-directory-moved");
    let serial = b.applied(mkdir(1, "moving"), T1).unwrap().serial;
    let mount = b.overlay.create_kernel_mount(1).unwrap();
    assert_eq!(decide(&b, mount, 0, 1), None);
    let directory = open(&b, mount, 2, serial);
    b.overlay.kernel_forget(mount, serial, 1).unwrap();
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
        .close_kernel_directory(directory.mount(), directory.serial(), directory.owner_id())
        .unwrap();
    b.overlay.revoke_kernel_mount(mount).unwrap();
}
/// A root holding `files` inherited regular files, each with its own content,
/// and one inherited subdirectory with a file of its own: a window whose
/// names, kinds, lengths and child-directory count all come from the base.
fn window_fixture(files: u64) -> common::Fixture {
    let store = common::Store::default();
    let scope = scope_for_seed([21; 32]);
    let directory = InodeMetadata::Inline(PortableMetadata {
        mode: 0o755,
        mtime_seconds: 7,
        mtime_nanoseconds: 3,
    });
    let regular = InodeMetadata::Inline(PortableMetadata {
        mode: 0o644,
        mtime_seconds: 7,
        mtime_nanoseconds: 3,
    });
    let mut inodes = Vec::new();
    for serial in 1..=2 {
        inodes.push(InodeUpdate {
            serial,
            value: InodeValue {
                kind: InodeKind::Directory,
                content_root: ObjectId::for_bytes(&[serial as u8; 8]),
                metadata_root: directory,
                namespace_ref_count: 0,
            },
        });
    }
    let mut changes = Vec::new();
    for n in 0..files {
        let root = common::file(&store, format!("file {n} of the window").as_bytes());
        inodes.push(InodeUpdate {
            serial: 3 + n,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                content_root: root,
                metadata_root: regular,
                namespace_ref_count: 0,
            },
        });
        changes.push((common::name(&format!("f{n:04}")), Some(3 + n)));
    }
    // Names bind in byte order: the subdirectory follows the files.
    changes.push((common::name("sub"), Some(2)));
    let inner = common::file(&store, b"the subdirectory's only name");
    inodes.push(InodeUpdate {
        serial: 3 + files,
        value: InodeValue {
            kind: InodeKind::RegularFile,
            content_root: inner,
            metadata_root: regular,
            namespace_ref_count: 0,
        },
    });
    let directories = vec![
        DirectoryUpdate { parent: 1, changes },
        DirectoryUpdate {
            parent: 2,
            changes: vec![(common::name("only"), Some(3 + files))],
        },
    ];
    let new: Vec<u64> = (1..=3 + files).collect();
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(&store, &mut sink);
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new,
        resources: FilesystemResources::default(),
    };
    let root = build_filesystem(&mut objects, &input, None).unwrap().root;
    common::Fixture {
        store,
        root,
        scope,
        bytes: Vec::new(),
    }
}
/// One READDIRPLUS window over `files` inherited names, as a reply that shows each
/// entry's facts reads it: (object batches, length batches, single length
/// demands, entries listed).
fn window_reads(files: u64) -> (u64, u64, u64, usize) {
    use std::sync::atomic::Ordering;
    let b = Bench::over(
        &format!("kernel-directory-facts-{files}"),
        window_fixture(files),
        4 << 20,
    );
    let mount = b.overlay.create_kernel_mount(1).unwrap();
    let directory = open(&b, mount, 1, 1);
    // The cache starts empty, so the window's names and facts come from the
    // Store; the OPENDIR and reading visit below fill the inode table's upper
    // level into the same cache before the window's own reads are counted.
    let window = visit_with(&b, directory, 2, None, true).unwrap();
    assert!(window.listing.is_none() && window.needs_base(true));
    let store = &b.fixture.store;
    let before = (
        store.batches.load(Ordering::Relaxed),
        store.length_batches.load(Ordering::Relaxed),
        store.length_demands.load(Ordering::Relaxed),
    );
    let base = b.workspace.base().unwrap();
    let listed = window.finish_with(Some(&base), true).unwrap();
    let after = (
        store.batches.load(Ordering::Relaxed),
        store.length_batches.load(Ordering::Relaxed),
        store.length_demands.load(Ordering::Relaxed),
    );
    // Every inherited name, the subdirectory included, carries its facts.
    for entry in &listed.entries {
        assert!(entry.stat.is_some(), "facts of {:?}", entry.name);
    }
    let sub = listed
        .entries
        .iter()
        .find(|entry| entry.name == b"sub")
        .expect("the subdirectory");
    let sub = sub.stat.as_ref().unwrap();
    assert_eq!((sub.kind, sub.subdirs), (InodeKind::Directory, 0));
    assert_eq!(listed.entries.len() as u64, files + 1);
    (
        after.0 - before.0,
        after.1 - before.1,
        after.2 - before.2,
        listed.entries.len(),
    )
}
#[test]
fn a_window_that_shows_facts_reads_it_in_batches() {
    let small = window_reads(8);
    let large = window_reads(200);
    assert_eq!((small.3, large.3), (9, 201), "every inherited name");
    // One length demand for the whole window, and it is a batch: no entry
    // asks the provider for its own length.
    assert_eq!((small.1, large.1), (1, 1), "one batch length demand");
    assert_eq!((small.2, large.2), (0, 0), "no single length demand");
    // The window's object batches do not grow with the names it lists: an
    // inode tree level whose pages are resident costs no demand, and the rest
    // come one batch per level, not one read per name. (What the count is
    // here depends on the levels the visits before the window left unread: in
    // this fixture they read the tree's upper level into the same cache, so
    // this is not a fully cold window's count.) This fixture
    // keeps every metadata root inline and builds its subdirectory's child
    // count during construction, so neither costs a demand here; a tree whose
    // metadata roots are objects, or a subdirectory counted by a scan, pays
    // one read per entry that needs it, as before.
    assert_eq!(
        small.0, large.0,
        "the same object batches for 8 and 200 names"
    );
    assert!(large.0 < 3, "{} object batches", large.0);
}
#[test]
fn a_local_window_shows_its_rows_without_a_store_reader() {
    use std::sync::atomic::Ordering;
    let b = Bench::new("kernel-directory-local-facts");
    let made = b.applied(mkdir(1, "all-local"), T1).unwrap().serial;
    let mount = b.overlay.create_kernel_mount(1).unwrap();
    // The kernel's own first LOOKUP of the name records the directory's
    // parent, which its OPENDIR reads.
    let lookup = b
        .workspace
        .kernel_read_visit(
            empty(),
            mount,
            1,
            None,
            KernelReadOperation::Lookup {
                parent: 1,
                name: common::name("all-local"),
            },
            Arc::new(VisitFacts::default()),
        )
        .unwrap();
    assert!(lookup.perform(&b.overlay).result.is_ok());
    let directory = open(&b, mount, 2, made);
    b.applied(create(made, "one"), T1);
    b.applied(create(made, "two"), T2);
    let store = &b.fixture.store;
    let before = (
        store.batches.load(Ordering::Relaxed),
        store.length_demands.load(Ordering::Relaxed),
    );
    // Every name of this window is a local row the visit already read: the
    // reply shows all their facts, and the request takes no Store reader.
    let window = visit_with(&b, directory, 2, None, true).unwrap();
    assert!(window.listing.is_some());
    assert!(!window.needs_base(true), "memory answered every fact");
    let listed = window.finish_with(None, true).unwrap();
    assert_eq!(listed.entries.len(), 2);
    assert!(listed.entries.iter().all(|entry| entry.stat.is_some()));
    assert_eq!(
        (
            store.batches.load(Ordering::Relaxed),
            store.length_demands.load(Ordering::Relaxed)
        ),
        before,
        "a local window reads nothing"
    );
    b.overlay
        .close_kernel_directory(directory.mount(), directory.serial(), directory.owner_id())
        .unwrap();
    b.overlay.revoke_kernel_mount(mount).unwrap();
}
