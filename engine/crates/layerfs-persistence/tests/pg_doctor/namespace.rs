//! Real canonical empty namespace, shared by access-fault cases.
use layerfs_content::{
    filesystem::{
        attributes::PortableMetadata,
        directory::{encode_directory_page, DirectoryPage},
        inode::{encode_inode_page, InodePage},
        scope_for_seed, FilesystemRoot,
    },
    inode_leaf::{InodeKind, InodeMetadata, InodeValue},
    FinalizedObject, ObjectRole,
};
use layerfs_history::{HistoryCatalog, HistoryName, LayerStackId, StackInitialization};
use layerfs_persistence::PostgresHandles;
use layerfs_storage::Storage;
pub fn seed(writer: &PostgresHandles) {
    project(writer, 51);
}
pub fn project(writer: &PostgresHandles, n: u8) {
    let storage = Storage::new(writer.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    let directory = FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf { entries: vec![] }).unwrap(),
    )
    .unwrap();
    let directory_id = directory.id();
    save.accept(directory).unwrap();
    let inode = FinalizedObject::new(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: vec![(
                1,
                InodeValue {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 0,
                    content_root: directory_id,
                    metadata_root: InodeMetadata::Inline(PortableMetadata {
                        mode: 0o755,
                        mtime_seconds: 0,
                        mtime_nanoseconds: 0,
                    }),
                },
            )],
        })
        .unwrap(),
    )
    .unwrap()
    .with_references(vec![directory_id]);
    let inode_id = inode.id();
    save.accept(inode).unwrap();
    let scope = scope_for_seed([n.wrapping_add(1); 32]);
    let root = FilesystemRoot::new(
        layerfs_content::filesystem::root::profile_id(),
        scope,
        1,
        inode_id,
    )
    .unwrap();
    let object = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode().unwrap())
        .unwrap()
        .with_references(vec![inode_id]);
    let root_id = object.id();
    save.accept(object).unwrap();
    save.finish().unwrap();
    writer
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 0,
            stack: LayerStackId::from_authority([n; 16]),
            name: HistoryName::new(&format!("project-{n}")).unwrap(),
            scope: scope.object(),
            profile: root.profile(),
            genesis_root: root_id,
        })
        .unwrap();
}
