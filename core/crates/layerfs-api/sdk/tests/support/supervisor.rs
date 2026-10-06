use layerfs_content::{
    filesystem::{
        attributes::{
            build_attribute_tree, emit_value, AttributeEntry, AttributeKey, PortableMetadata,
        },
        directory::{encode_directory_page, DirectoryPage},
        inode::{encode_inode_page, InodePage},
        profile_id, scope_for_seed,
    },
    object::{InodeKind, InodeValue},
    FilesystemRoot, FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId, StackInitialization, WorkspaceId,
};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_sdk::{Authorization, Config, Runtime, RuntimeError, RuntimeResult};
use layerfs_storage::{Storage, StoragePolicy};
use std::{cell::Cell, path::PathBuf, rc::Rc};

pub struct Fixture {
    path: PathBuf,
    pub runtime: Runtime,
    pub branch: BranchId,
    pub root: ObjectId,
    pub payload: ObjectId,
    pub denied: Rc<Cell<bool>>,
}
struct Authority(Rc<Cell<bool>>);
impl Authorization for Authority {
    fn workspace(&self, _: [u8; 32], _: WorkspaceId, _: BranchId) -> RuntimeResult<()> {
        if self.0.get() {
            Err(RuntimeError::Denied)
        } else {
            Ok(())
        }
    }
    fn objects(
        &self,
        _: [u8; 32],
        _: WorkspaceId,
        _: BranchId,
        _: &[ObjectId],
    ) -> RuntimeResult<()> {
        Ok(())
    }
}
impl Fixture {
    pub fn new(payload_bytes: usize) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../target/runtime-supervisor-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&path).unwrap();
        let mut config = PersistenceConfig::sqlite(path.join("store"));
        config.sqlite_profile = SqlitePersistenceProfile::Disposable;
        let handles = Handles::create(
            config,
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: b"supervisor".to_vec(),
                cursor_key: [9; 32],
                incarnation: 7,
            },
        )
        .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let save = storage.begin_save().unwrap();
        let bytes = layerfs_content::encode_whole_file_payload(&vec![0x37; payload_bytes]).unwrap();
        let payload = ObjectId::for_bytes(&bytes);
        save.accept(FinalizedObject::new(ObjectRole::WholeFile, bytes).unwrap())
            .unwrap();
        let directory = encode_directory_page(&DirectoryPage::Leaf { entries: vec![] }).unwrap();
        let dir_id = ObjectId::for_bytes(&directory);
        save.accept(FinalizedObject::new(ObjectRole::DirectoryLeaf, directory).unwrap())
            .unwrap();
        let metadata = PortableMetadata {
            mode: 0o755,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        };
        let meta_id = {
            let mut sink = save.sink();
            let mut objects = layerfs_content::FilesystemObjects::new(&save, &mut sink);
            let mode = emit_value(
                &mut objects,
                &metadata.mode_bytes(InodeKind::Directory).unwrap(),
            )
            .unwrap();
            let mtime = emit_value(&mut objects, &metadata.mtime_bytes().unwrap()).unwrap();
            build_attribute_tree(
                &mut objects,
                [
                    AttributeEntry {
                        key: AttributeKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                        value_root: mode,
                    },
                    AttributeEntry {
                        key: AttributeKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                        value_root: mtime,
                    },
                ]
                .into_iter()
                .map(Ok),
            )
            .unwrap()
            .0
        };
        let table = encode_inode_page(&InodePage::Leaf {
            entries: vec![(
                1,
                InodeValue {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 0,
                    content_root: dir_id,
                    metadata_root: meta_id,
                },
            )],
        })
        .unwrap();
        let table_id = ObjectId::for_bytes(&table);
        save.accept(
            FinalizedObject::new(ObjectRole::InodeLeaf, table)
                .unwrap()
                .with_references(vec![dir_id, meta_id]),
        )
        .unwrap();
        let scope = scope_for_seed([8; 32]);
        let root_bytes = FilesystemRoot::new(profile_id(), scope, 1, table_id)
            .unwrap()
            .encode()
            .unwrap();
        let root = ObjectId::for_bytes(&root_bytes);
        save.accept(
            FinalizedObject::new(ObjectRole::FilesystemRoot, root_bytes)
                .unwrap()
                .with_references(vec![table_id]),
        )
        .unwrap();
        save.finish().unwrap();
        let stack_id = LayerStackId::from_authority([5; 16]);
        let stack = handles
            .history
            .initialize_layerstack(&StackInitialization {
                stack: stack_id,
                name: HistoryName::new("supervisor").unwrap(),
                scope: scope.object(),
                profile: profile_id(),
                genesis_root: root,
            })
            .unwrap();
        let branch = BranchId::from_authority([6; 16]);
        handles
            .history
            .fork(&ForkRequest {
                stack: stack_id,
                branch,
                name: HistoryName::new("main").unwrap(),
                source: ForkSource::Layer(stack.head_layer),
            })
            .unwrap();
        let denied = Rc::new(Cell::new(false));
        let runtime = Runtime::new(
            handles,
            Config {
                incarnation: [7; 32],
                save_slots: 4,
            },
            Box::new(Authority(denied.clone())),
        )
        .unwrap();
        Self {
            path,
            runtime,
            branch,
            root,
            payload,
            denied,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}
