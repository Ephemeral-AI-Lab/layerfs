//! Admitted runtime objects exercise real Store closure and reconstruction.
#![cfg(target_os = "macos")]
mod support;

use layerfs_content::filesystem::attributes::{encode_attribute_page, AttributePage};
use layerfs_content::filesystem::directory::{encode_directory_page, DirectoryPage};
use layerfs_content::filesystem::inode::{encode_inode_page, InodePage};
use layerfs_content::filesystem::root::FilesystemRootId;
use layerfs_content::filesystem::{
    profile_id, scope_for_seed, FilesystemRoot, PathName, SymlinkTarget,
};
use layerfs_content::object::{InodeKind, InodeValue};
use layerfs_content::{
    encode_whole_file_payload, read_range, AuthenticatedObjects, FilesystemRead, FinalizedObject,
    LogicalPath, ObjectId, ObjectRole,
};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{save::Save, Storage, StorageError, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use support::Temp;

fn handles(temp: &Temp) -> Handles {
    let mut config = PersistenceConfig::sqlite(temp.join("store"));
    config.sqlite_profile = SqlitePersistenceProfile::Disposable;
    Handles::create(
        config,
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"admission".to_vec(),
            cursor_key: [7; 32],
            incarnation: 9,
        },
    )
    .unwrap()
}

fn admit(storage: &Storage, role: ObjectRole, canonical: Vec<u8>) -> FinalizedObject {
    Timing::disabled("admission", |timing| {
        FinalizedObject::admit(
            ObjectId::for_bytes(&canonical),
            role,
            canonical,
            storage.policy().construction(),
            scope_for_seed([8; 32]),
            timing.child("object"),
        )
    })
    .0
    .unwrap()
}

fn accept(storage: &Storage, save: &Save<'_>, role: ObjectRole, canonical: Vec<u8>) -> ObjectId {
    let object = admit(storage, role, canonical);
    let id = object.id();
    save.accept(object).unwrap();
    id
}

#[test]
fn admitted_complete_root_closes_and_reconstructs_through_the_real_store() {
    let temp = Temp::new("semantic-root");
    let handles = handles(&temp);
    let storage = Storage::new(handles.storage).unwrap();
    let save = storage.begin_save().unwrap();
    let file = accept(
        &storage,
        &save,
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"index bytes").unwrap(),
    );
    let link = accept(
        &storage,
        &save,
        ObjectRole::Symlink,
        SymlinkTarget::new(b"index".to_vec())
            .unwrap()
            .encode()
            .unwrap(),
    );
    let metadata = accept(
        &storage,
        &save,
        ObjectRole::AttributeLeaf,
        encode_attribute_page(&AttributePage::Leaf {
            subtree_bytes: 0,
            entries: Vec::new(),
        })
        .unwrap(),
    );
    let directory = accept(
        &storage,
        &save,
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: vec![
                (PathName::new("index").unwrap(), 2),
                (PathName::new("link").unwrap(), 3),
            ],
        })
        .unwrap(),
    );
    let table_bytes = encode_inode_page(&InodePage::Leaf {
        entries: vec![
            (
                1,
                InodeValue {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 0,
                    content_root: directory,
                    metadata_root: metadata,
                },
            ),
            (
                2,
                InodeValue {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 1,
                    content_root: file,
                    metadata_root: metadata,
                },
            ),
            (
                3,
                InodeValue {
                    kind: InodeKind::Symlink,
                    namespace_ref_count: 1,
                    content_root: link,
                    metadata_root: metadata,
                },
            ),
        ],
    })
    .unwrap();
    let table_object = admit(&storage, ObjectRole::InodeLeaf, table_bytes);
    assert_eq!(
        table_object.references(),
        [directory, metadata, file, metadata, link, metadata]
    );
    let table = table_object.id();
    save.accept(table_object).unwrap();
    let root = accept(
        &storage,
        &save,
        ObjectRole::FilesystemRoot,
        FilesystemRoot::new(profile_id(), scope_for_seed([8; 32]), 1, table)
            .unwrap()
            .encode()
            .unwrap(),
    );
    // The same admitted objects are visible to the producer before finish.
    assert!(save.read_canonical(root).is_ok());
    save.finish().unwrap();
    let reader = storage.reader().unwrap();
    let mut fs = FilesystemRead::new(&reader, FilesystemRootId(root)).unwrap();
    let resolved = fs.resolve(&LogicalPath::new("index").unwrap()).unwrap();
    assert_eq!(resolved.value.content_root, file);
    assert_eq!(
        fs.readlink(&LogicalPath::new("link").unwrap())
            .unwrap()
            .as_bytes(),
        b"index"
    );
    let mut bytes = Vec::new();
    Timing::disabled("read", |timing| {
        read_range(&reader, file, 0..11, &mut bytes, timing.child("file"))
    })
    .0
    .unwrap();
    assert_eq!(bytes, b"index bytes");
}

#[test]
fn omitted_wire_references_cannot_hide_an_inode_dependency_from_save() {
    let temp = Temp::new("semantic-missing");
    let handles = handles(&temp);
    let storage = Storage::new(handles.storage).unwrap();
    let missing = ObjectId::from_bytes(&[19; 32]).unwrap();
    let canonical = encode_inode_page(&InodePage::Leaf {
        entries: vec![(
            2,
            InodeValue {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: missing,
                metadata_root: missing,
            },
        )],
    })
    .unwrap();
    // The generic envelope constructor has no references. The runtime must
    // admit its transferred bytes, deriving refs regardless of that claim.
    let untrusted = FinalizedObject::new(ObjectRole::InodeLeaf, canonical).unwrap();
    assert!(untrusted.references().is_empty());
    let parts = untrusted.into_parts();
    let checked = admit(&storage, parts.role, parts.canonical);
    let parent = checked.id();
    assert_eq!(checked.references(), [missing, missing]);
    let save = storage.begin_save().unwrap();
    save.accept(checked).unwrap();
    assert!(
        matches!(save.finish(), Err(StorageError::MissingDependency { object, reference })
        if object == parent && reference == missing)
    );
}
