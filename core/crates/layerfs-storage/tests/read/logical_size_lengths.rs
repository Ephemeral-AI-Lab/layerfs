//! A Commit's or merge's logical size from catalogue lengths equals the after
//! tree counted from its file roots (#352 record 034, O1-a; #364).
//!
//! `logical_size_after_with` asks `Reader::file_lengths` a window at a time;
//! `logical_size` opens every file's root. On real constructed files of every
//! representation (empty, a few bytes, whole-file, chunked), a hard link, a
//! deleted file and a replaced one, both give the same size. More changed
//! files than `LENGTH_WINDOW` are asked in several demands. (The per-file
//! payload route `logical_size_after` was removed by #364.)
#[path = "../support/memory_metadata.rs"]
mod metadata;

use layerfs_content::filesystem::attributes::portable::PortableMetadata;
use layerfs_content::filesystem::statistics::{
    logical_size, logical_size_after_with, LENGTH_WINDOW,
};
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, update_filesystem, DirectoryUpdate, FilesystemInput,
    FilesystemObjects, FilesystemResources, FilesystemRootId, InodeScope, InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
use layerfs_content::{
    ConstructionPolicy, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_storage::{Storage, StoragePolicy};
use std::sync::Arc;

#[derive(Default)]
struct Emitted(Vec<FinalizedObject>);
impl FinalizedConsumer for Emitted {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.push(object);
        Ok(())
    }
}

/// Constructs `length` bytes of content into `out` and returns its root.
fn construct(out: &mut Emitted, length: usize, seed: u8) -> ObjectId {
    let policy = ConstructionPolicy::frozen_default();
    let bytes: Vec<u8> = (0..length)
        .map(|index| (index as u8).wrapping_mul(31).wrapping_add(seed))
        .collect();
    layerfs_telemetry::timer::Timing::disabled("construct", |scope| {
        layerfs_content::construct_bytes(
            policy,
            &policy.capacities(),
            &bytes,
            out,
            scope.child("construct"),
        )
    })
    .0
    .unwrap()
    .root
}

fn value(kind: InodeKind, content_root: ObjectId) -> InodeValue {
    InodeValue {
        kind,
        namespace_ref_count: 0,
        content_root,
        metadata_root: InodeMetadata::Inline(PortableMetadata {
            mode: if kind == InodeKind::Directory {
                0o755
            } else {
                0o644
            },
            mtime_seconds: 1_700_000_000,
            mtime_nanoseconds: 0,
        }),
    }
}

fn name(text: &str) -> PathName {
    PathName::new(text).unwrap()
}

/// Builds or updates a tree on `base` and saves it with the content in
/// `emitted`, in one Save.
fn apply(
    storage: &Storage,
    scope: InodeScope,
    base: Option<ObjectId>,
    mut emitted: Emitted,
    directories: &[DirectoryUpdate],
    inodes: &[InodeUpdate],
    new_inodes: &[u64],
) -> ObjectId {
    let input = FilesystemInput {
        base: base.map(FilesystemRootId),
        scope,
        root_serial: 1,
        directories,
        inodes,
        new_inodes,
        resources: FilesystemResources::default(),
    };
    let reader = storage.reader().unwrap();
    let root = {
        let mut objects = FilesystemObjects::new(&reader, &mut emitted);
        if base.is_some() {
            update_filesystem(&mut objects, &input, None)
        } else {
            build_filesystem(&mut objects, &input, None)
        }
        .unwrap()
        .root
        .0
    };
    drop(reader);
    let save = storage.begin_save().unwrap();
    for object in emitted.0 {
        save.accept(object).unwrap();
    }
    save.finish().unwrap();
    root
}

/// The lengths route (and its demand count) and the after tree counted whole.
fn sizes(storage: &Storage, before: ObjectId, after: ObjectId) -> (u64, u64, usize) {
    let reader = storage.reader().unwrap();
    let known = logical_size(&reader, FilesystemRootId(before)).unwrap();
    let (before, after) = (FilesystemRootId(before), FilesystemRootId(after));
    let mut demands = 0;
    let lengths = logical_size_after_with(&reader, before, after, known, &mut |roots| {
        assert!(roots.len() <= LENGTH_WINDOW);
        demands += 1;
        Ok(reader.file_lengths(roots).unwrap())
    })
    .unwrap();
    let whole = logical_size(&reader, after).unwrap();
    (lengths, whole, demands)
}

fn storage() -> Storage {
    let shared = Arc::new(metadata::MemoryMetadata::default());
    shared.state.lock().unwrap().policy = StoragePolicy::default();
    Storage::new(shared).unwrap()
}

#[test]
fn catalogue_lengths_give_the_payload_route_size_for_every_representation() {
    let storage = storage();
    let scope = scope_for_seed([0x34; 32]);
    // Serials 2..=8: empty, tiny, whole-file, chunked, a hard link bound under
    // two names, a file deleted later and one replaced later.
    let mut emitted = Emitted::default();
    let lengths = [0, 5, 100_000, 300_000, 70_000, 2_000, 150_000];
    let roots: Vec<ObjectId> = lengths
        .iter()
        .enumerate()
        .map(|(index, length)| construct(&mut emitted, *length, index as u8))
        .collect();
    let mut changes = vec![
        (name("chunked"), Some(5)),
        (name("deleted"), Some(7)),
        (name("empty"), Some(2)),
        (name("link-a"), Some(6)),
        (name("link-b"), Some(6)),
        (name("replaced"), Some(8)),
        (name("tiny"), Some(3)),
        (name("whole"), Some(4)),
    ];
    changes.sort_by(|a, b| a.0.cmp(&b.0));
    let mut inodes = vec![InodeUpdate {
        serial: 1,
        value: value(InodeKind::Directory, ObjectId::for_bytes(b"root")),
    }];
    for (index, root) in roots.iter().enumerate() {
        inodes.push(InodeUpdate {
            serial: index as u64 + 2,
            value: value(InodeKind::RegularFile, *root),
        });
    }
    let new: Vec<u64> = (1..=8).collect();
    let directories = [DirectoryUpdate { parent: 1, changes }];
    let before = apply(&storage, scope, None, emitted, &directories, &inodes, &new);
    let reader = storage.reader().unwrap();
    let counted = logical_size(&reader, FilesystemRootId(before)).unwrap();
    drop(reader);
    assert_eq!(
        counted,
        5 + 100_000 + 300_000 + 2 * 70_000 + 2_000 + 150_000
    );

    // Delete one file, replace one, add one of each kind, drop one name of
    // the hard link.
    let mut emitted = Emitted::default();
    let replaced = construct(&mut emitted, 4_000, 50);
    let added: Vec<ObjectId> = [0, 7, 90_000, 400_000]
        .iter()
        .enumerate()
        .map(|(index, length)| construct(&mut emitted, *length, 60 + index as u8))
        .collect();
    let mut changes = vec![
        (name("deleted"), None),
        (name("link-b"), None),
        (name("new-chunked"), Some(12)),
        (name("new-empty"), Some(9)),
        (name("new-tiny"), Some(10)),
        (name("new-whole"), Some(11)),
    ];
    changes.sort_by(|a, b| a.0.cmp(&b.0));
    let mut inodes = vec![InodeUpdate {
        serial: 8,
        value: value(InodeKind::RegularFile, replaced),
    }];
    for (index, root) in added.iter().enumerate() {
        inodes.push(InodeUpdate {
            serial: index as u64 + 9,
            value: value(InodeKind::RegularFile, *root),
        });
    }
    let directories = [DirectoryUpdate { parent: 1, changes }];
    let after = apply(
        &storage,
        scope,
        Some(before),
        emitted,
        &directories,
        &inodes,
        &[9, 10, 11, 12],
    );

    let (lengths, whole, demands) = sizes(&storage, before, after);
    let expected = 5 + 100_000 + 300_000 + 70_000 + 4_000 + 7 + 90_000 + 400_000;
    assert_eq!(whole, expected, "the after tree counted whole");
    assert_eq!(lengths, whole, "catalogue lengths route");
    assert_eq!(demands, 1);
    // And back: the same change undone.
    let (lengths, whole, _) = sizes(&storage, after, before);
    assert_eq!(lengths, whole);
    assert_eq!(whole, counted);
}

#[test]
fn more_changed_files_than_one_window_are_asked_in_several_demands() {
    let storage = storage();
    let scope = scope_for_seed([0x35; 32]);
    let files = LENGTH_WINDOW as u64 * 2 + 7;
    let root = value(InodeKind::Directory, ObjectId::for_bytes(b"root"));
    let before = apply(
        &storage,
        scope,
        None,
        Emitted::default(),
        &[DirectoryUpdate {
            parent: 1,
            changes: Vec::new(),
        }],
        &[InodeUpdate {
            serial: 1,
            value: root,
        }],
        &[1],
    );
    let mut emitted = Emitted::default();
    let mut inodes = Vec::new();
    for serial in 2..files + 2 {
        let content = construct(&mut emitted, 1 + (serial as usize % 97), serial as u8);
        inodes.push(InodeUpdate {
            serial,
            value: value(InodeKind::RegularFile, content),
        });
    }
    let changes = (2..files + 2)
        .map(|serial| (name(&format!("f{serial:05}")), Some(serial)))
        .collect();
    let new: Vec<u64> = (2..files + 2).collect();
    let directories = [DirectoryUpdate { parent: 1, changes }];
    let after = apply(
        &storage,
        scope,
        Some(before),
        emitted,
        &directories,
        &inodes,
        &new,
    );
    let (lengths, whole, demands) = sizes(&storage, before, after);
    let expected: u64 = (2..files + 2).map(|serial| 1 + serial % 97).sum();
    assert_eq!((lengths, whole), (expected, expected));
    assert_eq!(
        demands, 3,
        "{files} changed files in windows of {LENGTH_WINDOW}"
    );
}
