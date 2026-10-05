mod common;
mod harness;
use common::name;
use harness::*;
use layerfs_content::filesystem::SymlinkTarget;
use layerfs_overlay::{Inode, InodeKind, MaintenanceCursor};
use layerfs_workspace::Operation;
fn drain(b: &Bench) {
    let mut cursor = MaintenanceCursor::default();
    for _ in 0..2000 {
        let Some(step) = b.overlay.maintain(cursor).unwrap() else {
            return;
        };
        assert!(step.work.rows <= 64 && step.work.data_bytes <= 65536);
        cursor = step.cursor;
    }
    panic!("maintenance stalled");
}
fn local(b: &Bench, serial: u64) -> Inode {
    b.overlay.inode(b.route(), serial).unwrap().unwrap()
}
#[test]
fn linked_symlink_targets_follow_effective_layers_through_repeated_failed_captures() {
    let b = Bench::new("symlink-fold");
    let target = SymlinkTarget::new(b"../.git/index\nbyte-target".to_vec()).unwrap();
    let sym = b
        .applied(
            Operation::Symlink {
                parent: 1,
                name: name("local-link"),
                target: target.clone(),
            },
            T1,
        )
        .unwrap();
    for round in 0..20 {
        let capture = b.overlay.capture(b.route()).unwrap();
        let reader = b
            .overlay
            .acquire_captured_reader(capture, round + 1)
            .unwrap();
        b.window(|view| assert_eq!(view.readlink(&b.overlay, sym.serial).unwrap(), target));
        b.overlay.resolve_failed_capture(capture).unwrap();
        drain(&b);
        b.window(|view| {
            assert_eq!(
                view.readlink_captured(&b.overlay, reader, sym.serial)
                    .unwrap(),
                target
            )
        });
        b.overlay.release_captured_reader(reader).unwrap();
        drain(&b);
        b.window(|view| assert_eq!(view.readlink(&b.overlay, sym.serial).unwrap(), target));
    }
}
#[test]
fn non_file_lookup_and_processing_owners_preserve_removed_metadata_and_target() {
    let b = Bench::new("nonfile-owners");
    let target = SymlinkTarget::new(b"target\nbytes".to_vec()).unwrap();
    let sym = b
        .applied(
            Operation::Symlink {
                parent: 1,
                name: name("retained-link"),
                target: target.clone(),
            },
            T1,
        )
        .unwrap();
    let directory = b.applied(mkdir(1, "retained-dir"), T1).unwrap();
    let (lookup, dir_lookup, root_lookup) = b.window(|view| {
        let lookup = b
            .overlay
            .acquire_lookup(view.source(), 1, &local(&b, sym.serial), 1)
            .unwrap();
        let dir = b
            .overlay
            .acquire_lookup(view.source(), 2, &local(&b, directory.serial), 1)
            .unwrap();
        let root = local(&b, 1);
        assert_eq!((root.kind, root.nlink), (InodeKind::Directory, 0));
        let root = b
            .overlay
            .acquire_lookup(view.source(), 3, &root, 1)
            .unwrap();
        (lookup, dir, root)
    });
    assert_eq!(
        b.overlay.retained_lookup(b.route(), 1).unwrap(),
        Some(lookup)
    );
    b.applied(unlink(1, "retained-link"), T2);
    b.applied(rmdir(1, "retained-dir"), T2);
    drain(&b);
    b.window(|view| {
        assert_eq!(view.readlink(&b.overlay, sym.serial).unwrap(), target);
        assert_eq!(
            view.stat(&b.overlay, directory.serial)
                .unwrap()
                .namespace_refs,
            0
        );
        // Root zero count never detaches it into an orphan.
        assert_eq!(
            b.overlay
                .source_read(view.source(), 1, 0, 0)
                .unwrap()
                .unwrap()
                .base_root,
            None
        );
    });
    for _ in 0..12 {
        let capture = b.overlay.capture(b.route()).unwrap();
        b.overlay.resolve_failed_capture(capture).unwrap();
        drain(&b);
    }
    let read = b.window(|view| {
        b.overlay
            .acquire_lookup_read(view.source(), lookup, 100)
            .unwrap()
    });
    b.overlay.release_lookup(lookup).unwrap();
    assert!(b.overlay.check_lookup(lookup).is_err());
    drain(&b);
    let view = b.workspace.view_for_source(read.source()).unwrap();
    assert_eq!(view.readlink_owned(&b.overlay, read).unwrap(), target);
    b.overlay.close(b.route()).unwrap();
    assert_eq!(view.readlink_owned(&b.overlay, read).unwrap(), target);
    b.overlay.release_file_read(read).unwrap();
    b.overlay.release_lookup(dir_lookup).unwrap();
    b.overlay.release_lookup(root_lookup).unwrap();
    drain(&b);
    for _ in 0..1000 {
        if b.overlay.reclaim_closed(0).unwrap().unwrap().done {
            return;
        }
    }
    panic!("terminal cleanup stalled");
}

#[test]
fn failed_name_composition_drops_only_whiteouts_over_proven_absent_base_names() {
    let b = Bench::new("whiteout-fold");
    b.applied(create(1, "temporary"), T1);
    let capture = b.overlay.capture(b.route()).unwrap();
    b.applied(unlink(1, "temporary"), T2);
    let row = b
        .overlay
        .dentry(b.route(), 1, b"temporary")
        .unwrap()
        .unwrap();
    assert_eq!((row.serial, row.inherited), (None, true));
    b.overlay.resolve_failed_capture(capture).unwrap();
    drain(&b);
    assert_eq!(b.overlay.dentry(b.route(), 1, b"temporary").unwrap(), None);
    assert_eq!(b.overlay.state(b.route()).unwrap().dirty_names, 0);
    assert_eq!(b.lookup(1, "temporary"), None);
    // A base alias still needs its whiteout after a locally recreated name is
    // removed from the upper domain and the captured domain is composed.
    b.applied(unlink(1, "alias"), T1);
    let capture = b.overlay.capture(b.route()).unwrap();
    b.applied(create(1, "alias"), T2);
    b.applied(unlink(1, "alias"), T2);
    b.overlay.resolve_failed_capture(capture).unwrap();
    drain(&b);
    let row = b.overlay.dentry(b.route(), 1, b"alias").unwrap().unwrap();
    assert_eq!((row.serial, row.inherited), (None, true));
    assert_eq!(b.lookup(1, "alias"), None);
    assert_eq!(b.lookup(1, "file").unwrap().namespace_refs, 1);
}
