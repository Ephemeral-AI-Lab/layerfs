//! The inline metadata form and the no-change rule (STORAGE_BATCH_SPEC 4.8).
//!
//! Every base row here carries the root form, exactly as an Init-written row
//! does. An attempt that captures no real change keeps the base root - and so
//! each of its rows, byte for byte - while a real change writes the inline
//! form with the live mode and mtime. Untouched root-form rows keep their
//! exact stored bytes.
mod common;
mod harness;
mod oracle;
mod producer;
use common::{file, metadata, name, Fixture, Store};
use harness::Bench;
use layerfs_content::compare::{
    sorted::DifferenceWork,
    tree::{inode_changes, TreeTable},
};
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemResources, InodeUpdate,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
use layerfs_content::ObjectId;
use oracle::{value, Model, BASE};
use producer::{build_over, Drive};

const A_BYTES: &[u8] = b"file a";
const B_BYTES: &[u8] = b"file b";

/// Root 1 holds file "a" (2) and directory "d" (3) holding file "b" (4); every
/// metadata is the root form holding only the portable pair, like an Init row.
fn base() -> (Fixture, Model) {
    let store = Store::default();
    let scope = scope_for_seed([19; 32]);
    let rows = [
        (
            1,
            InodeKind::Directory,
            ObjectId::for_bytes(b"root-directory"),
        ),
        (2, InodeKind::RegularFile, file(&store, A_BYTES)),
        (
            3,
            InodeKind::Directory,
            ObjectId::for_bytes(b"inner-directory"),
        ),
        (4, InodeKind::RegularFile, file(&store, B_BYTES)),
    ];
    let inodes: Vec<InodeUpdate> = rows
        .into_iter()
        .map(|(serial, kind, content_root)| InodeUpdate {
            serial,
            value: InodeValue {
                kind,
                content_root,
                metadata_root: InodeMetadata::Root(metadata(&store, kind)),
                namespace_ref_count: 0,
            },
        })
        .collect();
    let directories = vec![
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("a"), Some(2)), (name("d"), Some(3))],
        },
        DirectoryUpdate {
            parent: 3,
            changes: vec![(name("b"), Some(4))],
        },
    ];
    let new: Vec<u64> = (1..=4).collect();
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

    let mut model = Model::empty(0o1777, BASE);
    model.mkdir("d", 0o1777, BASE);
    model.create("a", 0o644, BASE);
    model.write("a", 0, A_BYTES, BASE);
    model.create("d/b", 0o644, BASE);
    model.write("d/b", 0, B_BYTES, BASE);
    let fixture = Fixture {
        store,
        root,
        scope,
        bytes: Vec::new(),
    };
    (fixture, model)
}

/// Serials whose stored values differ between two roots.
fn changed(
    store: &Store,
    before: layerfs_content::filesystem::FilesystemRootId,
    after: layerfs_content::filesystem::FilesystemRootId,
) -> Vec<u64> {
    let (l, r) = (
        TreeTable::open(store, before).unwrap(),
        TreeTable::open(store, after).unwrap(),
    );
    let mut serials = Vec::new();
    inode_changes(
        store,
        &l,
        &r,
        &mut DifferenceWork::default(),
        |serial, _, _| {
            serials.push(serial);
            Ok(())
        },
    )
    .unwrap();
    serials
}

#[test]
fn an_mtime_only_capture_keeps_the_base_root_and_row() {
    let (fixture, model) = base();
    let b = Bench::over("ci-mtime", fixture, 4 * 1024 * 1024);
    let mut d = Drive::with_model(&b, model.clone());
    d.utimens("a", (9, 10));
    // The live view holds the new time; the kept row holds the base's.
    let built = build_over(&b, &b.overlay, &model, "mtime only");
    assert_eq!(built.root, b.fixture.root);
    assert_eq!(
        changed(&b.fixture.store, b.fixture.root, built.root),
        Vec::<u64>::new()
    );
    built.release(&b);
}

#[test]
fn an_identical_rewrite_and_a_net_zero_directory_keep_the_base_root() {
    let (fixture, model) = base();
    let b = Bench::over("ci-noops", fixture, 4 * 1024 * 1024);
    let mut d = Drive::with_model(&b, model.clone());
    d.write("a", 0, A_BYTES);
    d.create("d/tmp", 0o600);
    d.remove("d/tmp");
    let quiet = model.clone();
    let built = build_over(&b, &b.overlay, &quiet, "no real change");
    assert_eq!(built.root, b.fixture.root);
    built.release(&b);
}

#[test]
fn a_real_change_writes_the_inline_form_beside_three_no_ops() {
    let (fixture, model) = base();
    let b = Bench::over("ci-inline", fixture, 4 * 1024 * 1024);
    let mut d = Drive::with_model(&b, model.clone());
    // "a" really changes: its utimens and rewrite ride along on the chmod.
    d.utimens("a", (9, 10));
    d.write("a", 0, A_BYTES);
    d.chmod("a", 0o600);
    // The net-zero directory edit comes after the model snapshot: the stored
    // row keeps the base time the snapshot still holds.
    let quiet = d.model.clone();
    d.create("d/tmp", 0o600);
    d.remove("d/tmp");
    let built = build_over(&b, &b.overlay, &quiet, "one change");
    assert_ne!(built.root, b.fixture.root);
    let store = &b.fixture.store;
    assert_eq!(changed(store, b.fixture.root, built.root), vec![2]);
    let (old, new) = (value(store, b.fixture.root, 2), value(store, built.root, 2));
    let InodeMetadata::Inline(portable) = new.metadata_root else {
        panic!("serial 2: expected the inline metadata form");
    };
    assert_eq!(portable.mode, 0o600);
    let inode = producer::fact(&b, 2);
    assert_eq!(
        (portable.mtime_seconds, portable.mtime_nanoseconds),
        (inode.mtime_seconds, inode.mtime_nanoseconds),
        "the inline value carries the live mtime"
    );
    assert_eq!(new.content_root, old.content_root);
    // Untouched root-form rows keep their exact stored bytes.
    for serial in [3, 4] {
        assert_eq!(
            value(store, built.root, serial),
            value(store, b.fixture.root, serial),
            "serial {serial}: the untouched base row kept its exact bytes"
        );
    }
    built.release(&b);
}
