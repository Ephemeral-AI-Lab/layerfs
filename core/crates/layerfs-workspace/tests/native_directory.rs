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
use layerfs_overlay::{NativeDirectory, NativeMount};
use layerfs_workspace::{NativeReadOperation, NativeReadValue};
use std::sync::Arc;
fn decide(
    b: &Bench,
    mount: NativeMount,
    request: u64,
    serial: u64,
    operation: NativeReadOperation,
) -> NativeReadValue {
    let source = b
        .overlay
        .acquire_native_source(mount, request, serial)
        .unwrap();
    let view = b.workspace.view_for_source(source).unwrap();
    let mut plan = view.native_read_plan(mount, operation).unwrap();
    for _ in 0..5 {
        let before = b.demand();
        let original = Arc::new(plan.job().unwrap().perform(&b.overlay));
        assert_eq!(b.demand(), before, "owner made provider demand");
        if let Some(value) = plan.accept(original).unwrap() {
            b.overlay.release_base_source(source).unwrap();
            return value;
        }
        plan.supply(&view).unwrap();
    }
    panic!("directory decision did not finish its bounded fact rounds")
}
fn open(b: &Bench, mount: NativeMount, request: u64, serial: u64) -> NativeDirectory {
    let value = decide(
        b,
        mount,
        request,
        serial,
        NativeReadOperation::Opendir { serial },
    );
    b.overlay.release_file_read(value.read).unwrap();
    value.directory.unwrap()
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
    let read = b
        .overlay
        .acquire_native_directory_read(directory, 2, 2)
        .unwrap();
    let view = b.workspace.view_for_source(read.source()).unwrap();
    // READDIR needs inode kinds, not file lengths or portable attributes.
    // Make the otherwise valid fixture's length provider unavailable here.
    let lengths = std::mem::take(&mut *b.fixture.store.lengths.lock().unwrap());
    let mut after = None;
    let mut final_listing = None;
    for turn in 0..3 {
        let before = b.demand();
        let page = b
            .overlay
            .native_directory_page(&read, after.as_deref())
            .unwrap();
        assert_eq!(b.demand(), before);
        let listing = view.native_directory_listing(&page).unwrap();
        assert!(listing.visited <= 64);
        if turn < 2 {
            assert!(listing.entries.is_empty());
            assert!(listing.continuation.is_some());
            after = listing.continuation;
        } else {
            final_listing = Some(listing);
        }
    }
    let listing = final_listing.unwrap();
    assert!(b.fixture.store.lengths.lock().unwrap().is_empty());
    *b.fixture.store.lengths.lock().unwrap() = lengths;
    assert_eq!(listing.entries.len(), 8);
    assert!(listing
        .entries
        .iter()
        .any(|e| e.name == b"symlink" && e.kind == InodeKind::Symlink));
    assert!(listing
        .entries
        .iter()
        .any(|e| e.serial == local && e.kind == InodeKind::RegularFile));
    assert_eq!(
        b.overlay.native_lookup_count(mount, 2).unwrap(),
        None,
        "READDIR acquired no lookup"
    );
    let names: Vec<_> = listing.entries.iter().map(|e| e.name.clone()).collect();
    let plan = b.overlay.prepare_native_cookies(&read, &names).unwrap();
    b.overlay.publish_native_cookies(&plan, 2).unwrap();
    let cookie = plan.entries()[1].cookie();
    assert_eq!(plan.entries()[1].name(), b"alias");
    b.overlay.release_base_source(read.source()).unwrap();
    b.applied(unlink(1, "alias"), T2);
    b.applied(unlink(1, "file"), T2);
    let old = b
        .overlay
        .acquire_native_directory_read(directory, 3, cookie)
        .unwrap();
    assert_eq!(old.cursor().after_name(), Some(b"alias".as_slice()));
    let view = b.workspace.view_for_source(old.source()).unwrap();
    let page = b
        .overlay
        .native_directory_page(&old, old.cursor().after_name())
        .unwrap();
    let next = view.native_directory_listing(&page).unwrap();
    assert!(next
        .entries
        .iter()
        .all(|e| e.name.as_slice() > b"alias" && e.name != b"file"));
    b.overlay.release_base_source(old.source()).unwrap();
    b.overlay.close_native_directory(directory).unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}
#[test]
fn opened_directory_retains_parent_and_metadata_after_forget_and_rmdir() {
    let b = Bench::new("native-directory-removed");
    let serial = b.applied(mkdir(1, "empty"), T1).unwrap().serial;
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let value = decide(
        &b,
        mount,
        1,
        1,
        NativeReadOperation::Lookup {
            parent: 1,
            name: common::name("empty"),
        },
    );
    assert_eq!(value.stat.serial, serial);
    b.overlay.release_file_read(value.read).unwrap();
    let directory = open(&b, mount, 2, serial);
    b.overlay.forget_native(mount, serial, 1).unwrap();
    b.applied(rmdir(1, "empty"), T2);
    let read = b
        .overlay
        .acquire_native_directory_read(directory, 3, 0)
        .unwrap();
    assert_eq!(read.parent(), 1);
    b.overlay.close_native_directory(directory).unwrap();
    let inode = b
        .overlay
        .source_inode(read.source(), serial)
        .unwrap()
        .unwrap();
    assert_eq!(inode.nlink, 0);
    let view = b.workspace.view_for_source(read.source()).unwrap();
    let page = b.overlay.native_directory_page(&read, None).unwrap();
    assert!(view
        .native_directory_listing(&page)
        .unwrap()
        .entries
        .is_empty());
    b.overlay.release_base_source(read.source()).unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}

#[test]
fn next_directory_read_observes_parent_moved_by_the_atomic_namespace_job() {
    let b = Bench::new("native-directory-moved");
    let serial = b.applied(mkdir(1, "moving"), T1).unwrap().serial;
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let value = decide(
        &b,
        mount,
        1,
        1,
        NativeReadOperation::Lookup {
            parent: 1,
            name: common::name("moving"),
        },
    );
    b.overlay.release_file_read(value.read).unwrap();
    let directory = open(&b, mount, 2, serial);
    b.overlay.forget_native(mount, serial, 1).unwrap();
    let before = b
        .overlay
        .acquire_native_directory_read(directory, 3, 0)
        .unwrap();
    assert_eq!(before.parent(), 1);
    b.applied(
        rename((1, "moving"), (7, "moved"), true, path(&["output"])),
        T2,
    );
    let after = b
        .overlay
        .acquire_native_directory_read(directory, 4, 0)
        .unwrap();
    assert_eq!(after.parent(), 7);
    assert_eq!(
        b.overlay.retained_native_directory_read(mount, 3).unwrap(),
        Some(before.clone()),
        "original read keeps its observed parent"
    );
    b.overlay.release_base_source(before.source()).unwrap();
    b.overlay.release_base_source(after.source()).unwrap();
    b.overlay.close_native_directory(directory).unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}
