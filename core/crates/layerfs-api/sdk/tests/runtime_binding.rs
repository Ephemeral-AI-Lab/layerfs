//! Root binding validates demanded context through the real host Store.
#![cfg(target_os = "macos")]

#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
use layerfs_content::filesystem::{
    attributes::{build_attribute_tree, emit_value, AttributeEntry, AttributeKey},
    directory::{encode_directory_page, DirectoryPage},
    inode::{encode_inode_page, InodePage},
    profile_id, scope_for_seed, FilesystemRoot,
};
use layerfs_content::object::{InodeKind, InodeValue};
use layerfs_content::{ContentError, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId, StackInitialization, WorkspaceId,
};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_sdk::{Authorization, Config, Runtime, RuntimeError, RuntimeResult};
use layerfs_storage::{Storage, StoragePolicy};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

#[derive(Clone, Copy)]
enum Shape {
    Valid,
    MissingRoot,
    RegularRoot,
    LinkedRoot,
    WrongContent,
    WrongMetadata,
    WrongScope,
    MissingMode,
    MissingMtime,
    InvalidMode,
    InvalidNanoseconds,
    DeniedMode,
    MissingTable,
    BranchedRoot,
    WrongChildSummary,
    DeniedChild,
}
struct Authority {
    denied: Option<ObjectId>,
    demands: Rc<RefCell<Vec<ObjectId>>>,
}
impl Authorization for Authority {
    fn workspace(&self, _: [u8; 32], _: WorkspaceId, _: BranchId) -> RuntimeResult<()> {
        Ok(())
    }
    fn objects(
        &self,
        _: [u8; 32],
        _: WorkspaceId,
        _: BranchId,
        ids: &[ObjectId],
    ) -> RuntimeResult<()> {
        self.demands.borrow_mut().extend_from_slice(ids);
        if self.denied.is_some_and(|id| ids.contains(&id)) {
            Err(RuntimeError::Denied)
        } else {
            Ok(())
        }
    }
}
struct Fixture {
    runtime: Runtime,
    branch: BranchId,
    path: PathBuf,
    table: ObjectId,
    child: Option<ObjectId>,
    demands: Rc<RefCell<Vec<ObjectId>>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}
impl Fixture {
    fn new(shape: Shape, deny_table: bool, profile: SqlitePersistenceProfile) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../target/runtime-binding-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&path).unwrap();
        let mut config = PersistenceConfig::sqlite(path.join("store"));
        config.sqlite_profile = profile;
        let handles = Handles::create(
            config,
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: b"binding".to_vec(),
                cursor_key: [6; 32],
                incarnation: 1,
            },
        )
        .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let save = storage.begin_save().unwrap();
        let branched = matches!(
            shape,
            Shape::BranchedRoot | Shape::WrongChildSummary | Shape::DeniedChild
        );
        let directory = encode_directory_page(&DirectoryPage::Leaf {
            entries: if branched {
                (2..=100)
                    .map(|serial| {
                        (
                            layerfs_content::filesystem::PathName::from_bytes(
                                format!("file{serial:03}").as_bytes(),
                            )
                            .unwrap(),
                            serial,
                        )
                    })
                    .collect()
            } else {
                vec![]
            },
        })
        .unwrap();
        let dir = ObjectId::for_bytes(&directory);
        save.accept(FinalizedObject::new(ObjectRole::DirectoryLeaf, directory).unwrap())
            .unwrap();
        let (meta, mode) = {
            let mut sink = save.sink();
            let mut objects = layerfs_content::FilesystemObjects::new(&save, &mut sink);
            let mode = emit_value(
                &mut objects,
                &(if matches!(shape, Shape::InvalidMode) {
                    0o10000_u32
                } else {
                    0o755_u32
                })
                .to_be_bytes(),
            )
            .unwrap();
            let mut mtime = [0_u8; 12];
            if matches!(shape, Shape::InvalidNanoseconds) {
                mtime[8..].copy_from_slice(&1_000_000_000_u32.to_be_bytes());
            }
            let mtime = emit_value(&mut objects, &mtime).unwrap();
            let mut entries = Vec::new();
            if !matches!(shape, Shape::MissingMode) {
                entries.push(AttributeEntry {
                    key: AttributeKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                    value_root: mode,
                });
            }
            if !matches!(shape, Shape::MissingMtime) {
                entries.push(AttributeEntry {
                    key: AttributeKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                    value_root: mtime,
                });
            }
            (
                build_attribute_tree(&mut objects, entries.into_iter().map(Ok))
                    .unwrap()
                    .0,
                mode,
            )
        };
        let value = InodeValue {
            kind: if matches!(shape, Shape::RegularRoot) {
                InodeKind::RegularFile
            } else {
                InodeKind::Directory
            },
            namespace_ref_count: u64::from(matches!(shape, Shape::LinkedRoot | Shape::RegularRoot)),
            content_root: if matches!(shape, Shape::WrongContent) {
                meta
            } else {
                dir
            },
            metadata_root: if matches!(shape, Shape::WrongMetadata) {
                dir
            } else {
                meta
            },
        };
        let serial = if matches!(shape, Shape::MissingRoot) {
            2
        } else {
            1
        };
        let file = if branched {
            let bytes = layerfs_content::encode_whole_file_payload(b"bounded context").unwrap();
            let id = ObjectId::for_bytes(&bytes);
            save.accept(FinalizedObject::new(ObjectRole::WholeFile, bytes).unwrap())
                .unwrap();
            Some(id)
        } else {
            None
        };
        let nonroot = InodeValue {
            kind: InodeKind::RegularFile,
            namespace_ref_count: 1,
            content_root: file.unwrap_or(dir),
            metadata_root: meta,
        };
        let mut entries = vec![(serial, value)];
        if branched {
            entries.extend((2..=50).map(|serial| (serial, nonroot)));
        }
        let bytes = encode_inode_page(&InodePage::Leaf { entries }).unwrap();
        let mut table = ObjectId::for_bytes(&bytes);
        if !matches!(shape, Shape::MissingTable) {
            save.accept(
                FinalizedObject::new(ObjectRole::InodeLeaf, bytes)
                    .unwrap()
                    .with_references(if let Some(file) = file {
                        vec![dir, meta, file]
                    } else {
                        vec![dir, meta]
                    }),
            )
            .unwrap();
        }
        let child = if branched {
            let child = table;
            let bytes = encode_inode_page(&InodePage::Leaf {
                entries: (51..=100).map(|serial| (serial, nonroot)).collect(),
            })
            .unwrap();
            let upper = ObjectId::for_bytes(&bytes);
            save.accept(
                FinalizedObject::new(ObjectRole::InodeLeaf, bytes)
                    .unwrap()
                    .with_references(vec![file.unwrap(), meta]),
            )
            .unwrap();
            let bytes = encode_inode_page(&InodePage::Branch {
                level: 1,
                subtree_count: 100,
                children: vec![
                    (
                        if matches!(shape, Shape::WrongChildSummary) {
                            51
                        } else {
                            50
                        },
                        child,
                    ),
                    (100, upper),
                ],
            })
            .unwrap();
            table = ObjectId::for_bytes(&bytes);
            save.accept(
                FinalizedObject::new(ObjectRole::InodeBranch, bytes)
                    .unwrap()
                    .with_references(vec![child, upper]),
            )
            .unwrap();
            Some(child)
        } else {
            None
        };
        let scope = scope_for_seed([3; 32]);
        let bytes = FilesystemRoot::new(profile_id(), scope, 1, table)
            .unwrap()
            .encode()
            .unwrap();
        let root = ObjectId::for_bytes(&bytes);
        save.accept(
            FinalizedObject::new(ObjectRole::FilesystemRoot, bytes)
                .unwrap()
                .with_references(if matches!(shape, Shape::MissingTable) {
                    vec![]
                } else {
                    vec![table]
                }),
        )
        .unwrap();
        save.finish().unwrap();
        let stack_id = LayerStackId::from_authority([2; 16]);
        let stack = handles
            .history
            .initialize_layerstack(&StackInitialization {
                stack: stack_id,
                name: HistoryName::new("binding").unwrap(),
                scope: if matches!(shape, Shape::WrongScope) {
                    scope_for_seed([4; 32]).object()
                } else {
                    scope.object()
                },
                profile: profile_id(),
                genesis_root: root,
            })
            .unwrap();
        let branch = BranchId::from_authority([3; 16]);
        handles
            .history
            .fork(&ForkRequest {
                stack: stack_id,
                branch,
                name: HistoryName::new("main").unwrap(),
                source: ForkSource::Layer(stack.head_layer),
            })
            .unwrap();
        let demands = Rc::new(RefCell::new(Vec::new()));
        let runtime = Runtime::new(
            handles,
            Config {
                incarnation: [7; 32],
                save_slots: 1,
            },
            Box::new(Authority {
                denied: if deny_table {
                    Some(table)
                } else if matches!(shape, Shape::DeniedMode) {
                    Some(mode)
                } else if matches!(shape, Shape::DeniedChild) {
                    child
                } else {
                    None
                },
                demands: demands.clone(),
            }),
        )
        .unwrap();
        Self {
            runtime,
            branch,
            path,
            table,
            child,
            demands,
        }
    }
    fn bind(&mut self) -> RuntimeResult<layerfs_sdk::Binding> {
        let (_, connection) = native::pair();
        self.runtime.sessions().bind(
            &connection.peer,
            WorkspaceId::from_authority([8; 32]).unwrap(),
            self.branch,
        )
    }
}

#[test]
fn binding_checks_actual_root_directory_and_child_roles_under_both_store_profiles() {
    for profile in [
        SqlitePersistenceProfile::Durable,
        SqlitePersistenceProfile::Disposable,
    ] {
        let mut valid = Fixture::new(Shape::Valid, false, profile);
        assert_eq!(valid.bind().unwrap().root_serial(), 1);
        assert!(valid.demands.borrow().contains(&valid.table));
        for shape in [
            Shape::MissingRoot,
            Shape::RegularRoot,
            Shape::LinkedRoot,
            Shape::WrongContent,
            Shape::WrongMetadata,
            Shape::WrongScope,
            Shape::MissingMode,
            Shape::MissingMtime,
            Shape::InvalidMode,
            Shape::InvalidNanoseconds,
        ] {
            assert!(matches!(
                Fixture::new(shape, false, profile).bind(),
                Err(RuntimeError::Content(_))
            ));
        }
    }
}

#[test]
fn denied_descendant_keeps_exact_authority_failure_and_stops_the_demand_path() {
    let mut fixture = Fixture::new(Shape::Valid, true, SqlitePersistenceProfile::Disposable);
    assert!(matches!(fixture.bind(), Err(RuntimeError::Denied)));
    assert_eq!(fixture.demands.borrow().len(), 2);
    assert_eq!(fixture.demands.borrow()[1], fixture.table);
}

#[test]
fn root_missing_from_inode_table_is_a_context_error() {
    let mut fixture = Fixture::new(
        Shape::MissingRoot,
        false,
        SqlitePersistenceProfile::Disposable,
    );
    assert!(matches!(
        fixture.bind(),
        Err(RuntimeError::Content(ContentError::InvalidRecord(
            "missing filesystem root inode"
        )))
    ));
}

#[test]
fn denial_of_root_metadata_value_keeps_the_exact_failure() {
    let mut fixture = Fixture::new(
        Shape::DeniedMode,
        false,
        SqlitePersistenceProfile::Disposable,
    );
    assert!(matches!(fixture.bind(), Err(RuntimeError::Denied)));
}

#[test]
fn denied_missing_table_is_refused_before_provider_absence() {
    let mut fixture = Fixture::new(
        Shape::MissingTable,
        true,
        SqlitePersistenceProfile::Disposable,
    );
    assert!(matches!(fixture.bind(), Err(RuntimeError::Denied)));
}

#[test]
fn authorized_missing_table_retains_original_storage_identity() {
    let mut fixture = Fixture::new(
        Shape::MissingTable,
        false,
        SqlitePersistenceProfile::Disposable,
    );
    assert!(
        matches!(fixture.bind(), Err(RuntimeError::Storage(error)) if matches!(*error, layerfs_storage::StorageError::ObjectMissing(id) if id == fixture.table))
    );
}

#[test]
fn multi_level_root_demand_authorizes_the_selected_child_and_checks_its_summary() {
    let mut valid = Fixture::new(
        Shape::BranchedRoot,
        false,
        SqlitePersistenceProfile::Disposable,
    );
    assert_eq!(valid.bind().unwrap().root_serial(), 1);
    assert_eq!(
        valid.demands.borrow()[1..3],
        [valid.table, valid.child.unwrap()]
    );
    let mut malformed = Fixture::new(
        Shape::WrongChildSummary,
        false,
        SqlitePersistenceProfile::Disposable,
    );
    assert!(matches!(
        malformed.bind(),
        Err(RuntimeError::Content(ContentError::InvalidRecord(
            "inode child summary"
        )))
    ));
    let mut denied = Fixture::new(
        Shape::DeniedChild,
        false,
        SqlitePersistenceProfile::Disposable,
    );
    assert!(matches!(denied.bind(), Err(RuntimeError::Denied)));
    assert_eq!(denied.demands.borrow().last().copied(), denied.child);
}
