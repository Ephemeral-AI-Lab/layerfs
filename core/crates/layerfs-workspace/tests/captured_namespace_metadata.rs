//! A captured mode or time change patches the stored attribute tree: every
//! other key keeps its stored value root. This file owns a small base of its
//! own whose inodes carry attributes outside the portable domain; the shared
//! fixture and every other test's roots are untouched. The model walker
//! compares each inode's complete attribute key set with its values.
mod common;
mod harness;
mod oracle;
mod producer;
use common::{file, metadata, name, Fixture, Store};
use harness::Bench;
use layerfs_content::filesystem::{
    attributes::{
        build_attribute_tree, emit_value, AttributeEntry, AttributeKey, PortableMetadata,
    },
    build_filesystem, scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemResources, FilesystemRootId, InodeUpdate,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::ObjectId;
use oracle::{assert_tree, attributes, value, walk, Model, BASE};
use producer::Drive;

/// Keys in a domain that sorts before `portable` and in one that sorts after.
const BEFORE: (&str, &[u8], &[u8]) = ("a.first", b"k", b"sorts before the portable keys");
const AFTER: (&str, &[u8], &[u8]) = ("user", b"note", b"kept across chmod and utimens");
const BYTES: &[u8] = b"the tagged file's bytes";

/// Mode and mtime as the shared fixture writes them, with both extra keys.
fn tagged_metadata(store: &Store, kind: InodeKind, mode: u32) -> ObjectId {
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(store, &mut sink);
    let portable = PortableMetadata {
        mode,
        mtime_seconds: BASE.0,
        mtime_nanoseconds: BASE.1,
    };
    let key = |domain: &str, key: &[u8]| AttributeKey::new(domain.into(), key.to_vec()).unwrap();
    let entries = [
        AttributeEntry {
            key: key(BEFORE.0, BEFORE.1),
            value_root: emit_value(&mut objects, BEFORE.2).unwrap(),
        },
        AttributeEntry {
            key: key("portable", b"mode"),
            value_root: emit_value(&mut objects, &portable.mode_bytes(kind).unwrap()).unwrap(),
        },
        AttributeEntry {
            key: key("portable", b"mtime"),
            value_root: emit_value(&mut objects, &portable.mtime_bytes().unwrap()).unwrap(),
        },
        AttributeEntry {
            key: key(AFTER.0, AFTER.1),
            value_root: emit_value(&mut objects, AFTER.2).unwrap(),
        },
    ];
    build_attribute_tree(&mut objects, entries.into_iter().map(Ok))
        .unwrap()
        .0
}
/// Root 1 holds the tagged file 2, the tagged directory 3 and a plain file 4.
fn tagged() -> (Fixture, Model) {
    let store = Store::default();
    let scope = scope_for_seed([12; 32]);
    let content = file(&store, BYTES);
    let plain = file(&store, b"plain");
    let directory = ObjectId::for_bytes(b"new-directory");
    let rows = [
        (
            1,
            InodeKind::Directory,
            directory,
            metadata(&store, InodeKind::Directory),
        ),
        (
            2,
            InodeKind::RegularFile,
            content,
            tagged_metadata(&store, InodeKind::RegularFile, 0o644),
        ),
        (
            3,
            InodeKind::Directory,
            directory,
            tagged_metadata(&store, InodeKind::Directory, 0o755),
        ),
        (
            4,
            InodeKind::RegularFile,
            plain,
            metadata(&store, InodeKind::RegularFile),
        ),
    ];
    let inodes: Vec<InodeUpdate> = rows
        .into_iter()
        .map(|(serial, kind, content_root, metadata_root)| InodeUpdate {
            serial,
            value: InodeValue {
                kind,
                content_root,
                metadata_root,
                namespace_ref_count: 0,
            },
        })
        .collect();
    let directories = vec![
        DirectoryUpdate {
            parent: 1,
            changes: vec![
                (name("kept"), Some(3)),
                (name("plain"), Some(4)),
                (name("tagged"), Some(2)),
            ],
        },
        // The empty new directory, declared the way Content's own tests do.
        DirectoryUpdate {
            parent: 3,
            changes: Vec::new(),
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

    // The same base restated by the model's own rules.
    let mut model = Model::empty(0o1777, BASE);
    model.mkdir("kept", 0o755, BASE);
    model.create("plain", 0o644, BASE);
    model.write("plain", 0, b"plain", BASE);
    model.create("tagged", 0o644, BASE);
    model.write("tagged", 0, BYTES, BASE);
    for path in ["tagged", "kept"] {
        model.attribute(path, BEFORE.0, BEFORE.1, BEFORE.2);
        model.attribute(path, AFTER.0, AFTER.1, AFTER.2);
    }
    let fixture = Fixture {
        store,
        root,
        scope,
        bytes: Vec::new(),
    };
    (fixture, model)
}
/// The value root of one attribute of one inode of one root.
fn value_root(b: &Bench, root: FilesystemRootId, serial: u64, key: (&str, &[u8])) -> ObjectId {
    let metadata_root = value(&b.fixture.store, root, serial).metadata_root;
    attributes(&b.fixture.store, metadata_root)
        .into_iter()
        .find(|entry| entry.key.domain() == key.0 && entry.key.key() == key.1)
        .unwrap_or_else(|| panic!("serial {serial}: no attribute {key:?}"))
        .value_root
}

#[test]
fn the_walker_compares_the_complete_attribute_key_set() {
    let (fixture, model) = tagged();
    let walked = assert_tree(&fixture.store, fixture.root, &model, "tagged base");
    assert_eq!(walked.flat[b"tagged".as_slice()].extra.len(), 2);
    assert_eq!(walked.flat[b"kept".as_slice()].extra.len(), 2);
    assert!(walked.flat[b"plain".as_slice()].extra.is_empty());
    // A model that lacks one key, or holds another value, is a difference.
    let mut without = Model::empty(0o1777, BASE);
    without.mkdir("kept", 0o755, BASE);
    without.attribute("kept", AFTER.0, AFTER.1, AFTER.2);
    let mut other = Model::empty(0o1777, BASE);
    other.mkdir("kept", 0o755, BASE);
    other.attribute("kept", BEFORE.0, BEFORE.1, BEFORE.2);
    other.attribute("kept", AFTER.0, AFTER.1, b"another value");
    let seen = &walk(&fixture.store, fixture.root).flat[b"kept".as_slice()];
    for (what, wrong) in [("a missing key", without), ("another value", other)] {
        assert_ne!(
            &wrong.flat()[b"kept".as_slice()],
            seen,
            "{what} compared equal"
        );
    }
}

#[test]
fn a_metadata_patch_keeps_every_other_key_and_its_value_root() {
    let (fixture, model) = tagged();
    let base = fixture.root;
    let b = Bench::over("cm-tagged", fixture, 4 * 1024 * 1024);
    let mut d = Drive::with_model(&b, model);
    d.chmod("tagged", 0o600);
    d.utimens("tagged", (7, 8));
    d.chmod("kept", 0o700);
    d.utimens("plain", (9, 10));
    // A content change of the tagged file patches the same tree again.
    d.write("tagged", 4, b"TAGGED");
    // A new name under the tagged directory changes its time.
    d.create("kept/inside", 0o644);

    // The complete walk compares mode, time and both extra keys with values.
    let built = d.build("patched metadata");
    for serial in [2, 3] {
        let (old, new) = (
            value(&b.fixture.store, base, serial),
            value(&b.fixture.store, built.root, serial),
        );
        assert_ne!(new.metadata_root, old.metadata_root, "serial {serial}");
        for key in [(BEFORE.0, BEFORE.1), (AFTER.0, AFTER.1)] {
            assert_eq!(
                value_root(&b, built.root, serial, key),
                value_root(&b, base, serial, key),
                "serial {serial}: the value root of {key:?} was replaced"
            );
        }
        for key in [("portable", b"mode".as_slice()), ("portable", b"mtime")] {
            assert_ne!(
                value_root(&b, built.root, serial, key),
                value_root(&b, base, serial, key),
                "serial {serial}: {key:?} was not patched"
            );
        }
        let keys = attributes(&b.fixture.store, new.metadata_root).len();
        assert_eq!(keys, 4, "serial {serial}: exactly the four stored keys");
    }
    // Base inodes 2, 3 and 4 are patched and the new file is built. The root
    // has no row: no name of it changed. (I first counted it: W-attempt1.)
    assert_eq!(
        (built.work.metadata_patched, built.work.metadata_built),
        (3, 1)
    );
    built.install(&b);

    // The patched tree is the next base: a second patch keeps the keys again.
    let installed = b.workspace.base().unwrap().identity();
    d.chmod("tagged", 0o640);
    let again = d.build("patched again");
    for key in [(BEFORE.0, BEFORE.1), (AFTER.0, AFTER.1)] {
        assert_eq!(
            value_root(&b, again.root, 2, key),
            value_root(&b, installed, 2, key)
        );
    }
    again.release(&b);
}
