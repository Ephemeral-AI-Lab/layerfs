//! Root binding validates demanded context through the real host Store.
#![cfg(target_os = "macos")]

#[path = "support/native.rs"]
#[allow(dead_code)]
mod native;
use layerfs_content::filesystem::{
    attributes::{
        build_attribute_tree, emit_value, encode_attribute_page, AttributeEntry, AttributeKey,
        AttributePage,
    },
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
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};

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
    denied: Rc<Cell<Option<ObjectId>>>,
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
        if self.denied.get().is_some_and(|id| ids.contains(&id)) {
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
    storage: Storage,
    directory: ObjectId,
    metadata: ObjectId,
    denied: Rc<Cell<Option<ObjectId>>>,
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
        let denied = Rc::new(Cell::new(if deny_table {
            Some(table)
        } else if matches!(shape, Shape::DeniedMode) {
            Some(mode)
        } else if matches!(shape, Shape::DeniedChild) {
            child
        } else {
            None
        }));
        let runtime = Runtime::new(
            handles,
            Config {
                incarnation: [7; 32],
                save_slots: 1,
            },
            Box::new(Authority {
                denied: denied.clone(),
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
            storage,
            directory: dir,
            metadata: meta,
            denied,
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

#[derive(Clone, Copy)]
enum CandidateShape {
    ChangedSerial,
    RegularRoot,
    LinkedRoot,
    WrongContent,
    WrongMetadata,
    EmptyMetadata,
    InvalidMode,
    Branched,
    WrongChildSummary,
    MissingTable,
}
struct Candidate {
    root: ObjectId,
    table: ObjectId,
    child: Option<ObjectId>,
}
fn save_candidate(
    storage: &Storage,
    directory: ObjectId,
    metadata: ObjectId,
    shape: CandidateShape,
) -> Candidate {
    let save = storage.begin_save().unwrap();
    let metadata = if matches!(shape, CandidateShape::EmptyMetadata) {
        let bytes = encode_attribute_page(&AttributePage::Leaf {
            subtree_bytes: 0,
            entries: vec![],
        })
        .unwrap();
        let id = ObjectId::for_bytes(&bytes);
        save.accept(FinalizedObject::new(ObjectRole::AttributeLeaf, bytes).unwrap())
            .unwrap();
        id
    } else if matches!(shape, CandidateShape::InvalidMode) {
        let mut sink = save.sink();
        let mut objects = layerfs_content::FilesystemObjects::new(&save, &mut sink);
        let mode = emit_value(&mut objects, &0o10000_u32.to_be_bytes()).unwrap();
        let mtime = emit_value(&mut objects, &[0; 12]).unwrap();
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
    } else {
        metadata
    };
    let serial = if matches!(shape, CandidateShape::ChangedSerial) {
        2
    } else {
        1
    };
    let value = InodeValue {
        kind: if matches!(shape, CandidateShape::RegularRoot) {
            InodeKind::RegularFile
        } else {
            InodeKind::Directory
        },
        namespace_ref_count: u64::from(matches!(
            shape,
            CandidateShape::RegularRoot | CandidateShape::LinkedRoot
        )),
        content_root: if matches!(shape, CandidateShape::WrongContent) {
            metadata
        } else {
            directory
        },
        metadata_root: if matches!(shape, CandidateShape::WrongMetadata) {
            directory
        } else {
            metadata
        },
    };
    let branched = matches!(
        shape,
        CandidateShape::Branched | CandidateShape::WrongChildSummary
    );
    let file = if branched {
        let bytes = layerfs_content::encode_whole_file_payload(b"candidate sibling").unwrap();
        let id = ObjectId::for_bytes(&bytes);
        save.accept(FinalizedObject::new(ObjectRole::WholeFile, bytes).unwrap())
            .unwrap();
        Some(id)
    } else {
        None
    };
    let sibling = InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: file.unwrap_or(directory),
        metadata_root: metadata,
    };
    let mut entries = vec![(serial, value)];
    if branched {
        entries.extend((2..=50).map(|serial| (serial, sibling)));
    }
    let bytes = encode_inode_page(&InodePage::Leaf { entries }).unwrap();
    let mut table = ObjectId::for_bytes(&bytes);
    save.accept(
        FinalizedObject::new(ObjectRole::InodeLeaf, bytes)
            .unwrap()
            .with_references(if let Some(file) = file {
                vec![directory, metadata, file]
            } else {
                vec![directory, metadata]
            }),
    )
    .unwrap();
    let child = if branched {
        let lower = table;
        let bytes = encode_inode_page(&InodePage::Leaf {
            entries: (51..=100).map(|serial| (serial, sibling)).collect(),
        })
        .unwrap();
        let upper = ObjectId::for_bytes(&bytes);
        save.accept(
            FinalizedObject::new(ObjectRole::InodeLeaf, bytes)
                .unwrap()
                .with_references(vec![file.unwrap(), metadata]),
        )
        .unwrap();
        let bytes = encode_inode_page(&InodePage::Branch {
            level: 1,
            subtree_count: 100,
            children: vec![
                (
                    if matches!(shape, CandidateShape::WrongChildSummary) {
                        51
                    } else {
                        50
                    },
                    lower,
                ),
                (100, upper),
            ],
        })
        .unwrap();
        table = ObjectId::for_bytes(&bytes);
        save.accept(
            FinalizedObject::new(ObjectRole::InodeBranch, bytes)
                .unwrap()
                .with_references(vec![lower, upper]),
        )
        .unwrap();
        Some(lower)
    } else {
        None
    };
    if matches!(shape, CandidateShape::MissingTable) {
        table = ObjectId::for_bytes(b"absent saved candidate inode table");
    }
    let bytes = FilesystemRoot::new(profile_id(), scope_for_seed([3; 32]), serial, table)
        .unwrap()
        .encode()
        .unwrap();
    let root = ObjectId::for_bytes(&bytes);
    // The existing public trusted wrapper can seed a saved malformed root for
    // negative contextual-reader tests. MissingTable intentionally omits its
    // declared reference, as the initial-bind missing-table fixture does above.
    save.accept(
        FinalizedObject::new(ObjectRole::FilesystemRoot, bytes)
            .unwrap()
            .with_references(if matches!(shape, CandidateShape::MissingTable) {
                vec![]
            } else {
                vec![table]
            }),
    )
    .unwrap();
    save.finish().unwrap();
    Candidate { root, table, child }
}

#[test]
fn stage_saved_candidate_cannot_change_the_captured_root_inode_serial() {
    for profile in [
        SqlitePersistenceProfile::Durable,
        SqlitePersistenceProfile::Disposable,
    ] {
        let mut fixture = Fixture::new(Shape::Valid, false, profile);
        let binding = fixture.bind().unwrap();
        assert_eq!(binding.root_serial(), 1);
        let candidate = save_candidate(
            &fixture.storage,
            fixture.directory,
            fixture.metadata,
            CandidateShape::ChangedSerial,
        );
        let mut sessions = fixture.runtime.sessions();
        let save = sessions.begin(&binding).unwrap();
        assert!(sessions.finish(&binding, save).unwrap().outcome().is_ok());
        assert!(matches!(
            sessions
                .stage_saved(&binding, save, candidate.root, 1)
                .unwrap(),
            Err(RuntimeError::Content(_))
        ));
        assert!(matches!(
            sessions.stage_saved(&binding, save, candidate.root, 1),
            Err(RuntimeError::AlreadyAttempted)
        ));
        let history = sessions.history_receipts(&binding, save).unwrap();
        assert!(matches!(
            history.stage().unwrap(),
            Err(RuntimeError::Content(_))
        ));
        assert!(history.commit().is_none());
        assert!(history.discard().is_none());
        assert!(!history.terminal_unknown());
    }
}

#[test]
fn stage_saved_candidate_validates_actual_root_inode_metadata_and_descended_summary() {
    for profile in [
        SqlitePersistenceProfile::Durable,
        SqlitePersistenceProfile::Disposable,
    ] {
        let mut fixture = Fixture::new(Shape::Valid, false, profile);
        let binding = fixture.bind().unwrap();
        for shape in [
            CandidateShape::RegularRoot,
            CandidateShape::LinkedRoot,
            CandidateShape::WrongContent,
            CandidateShape::WrongMetadata,
            CandidateShape::EmptyMetadata,
            CandidateShape::InvalidMode,
            CandidateShape::WrongChildSummary,
        ] {
            let candidate =
                save_candidate(&fixture.storage, fixture.directory, fixture.metadata, shape);
            let mut sessions = fixture.runtime.sessions();
            let save = sessions.begin(&binding).unwrap();
            assert!(sessions.finish(&binding, save).unwrap().outcome().is_ok());
            assert!(matches!(
                sessions
                    .stage_saved(&binding, save, candidate.root, 1)
                    .unwrap(),
                Err(RuntimeError::Content(_))
            ));
            assert!(matches!(
                sessions.stage_saved(&binding, save, candidate.root, 1),
                Err(RuntimeError::AlreadyAttempted)
            ));
            let history = sessions.history_receipts(&binding, save).unwrap();
            assert!(matches!(
                history.stage().unwrap(),
                Err(RuntimeError::Content(_))
            ));
            assert!(history.commit().is_none());
            assert!(!history.terminal_unknown());
        }
    }
}

#[test]
fn stage_saved_candidate_retains_original_denied_and_absent_descendant_failures() {
    for profile in [
        SqlitePersistenceProfile::Durable,
        SqlitePersistenceProfile::Disposable,
    ] {
        let mut fixture = Fixture::new(Shape::Valid, false, profile);
        let binding = fixture.bind().unwrap();
        let candidate = save_candidate(
            &fixture.storage,
            fixture.directory,
            fixture.metadata,
            CandidateShape::Branched,
        );
        fixture.denied.set(candidate.child);
        fixture.demands.borrow_mut().clear();
        let mut sessions = fixture.runtime.sessions();
        let save = sessions.begin(&binding).unwrap();
        assert!(sessions.finish(&binding, save).unwrap().outcome().is_ok());
        assert!(matches!(
            sessions
                .stage_saved(&binding, save, candidate.root, 1)
                .unwrap(),
            Err(RuntimeError::Denied)
        ));
        assert_eq!(fixture.demands.borrow().last().copied(), candidate.child);
        assert!(matches!(
            sessions.stage_saved(&binding, save, candidate.root, 1),
            Err(RuntimeError::AlreadyAttempted)
        ));
        assert!(matches!(
            sessions
                .history_receipts(&binding, save)
                .unwrap()
                .stage()
                .unwrap(),
            Err(RuntimeError::Denied)
        ));
        drop(sessions);
        fixture.denied.set(None);
        let candidate = save_candidate(
            &fixture.storage,
            fixture.directory,
            fixture.metadata,
            CandidateShape::MissingTable,
        );
        let mut sessions = fixture.runtime.sessions();
        let save = sessions.begin(&binding).unwrap();
        assert!(sessions.finish(&binding, save).unwrap().outcome().is_ok());
        assert!(
            matches!(sessions.stage_saved(&binding, save, candidate.root, 1).unwrap(), Err(RuntimeError::Storage(error)) if matches!(error.as_ref(), layerfs_storage::StorageError::ObjectMissing(id) if *id == candidate.table))
        );
        assert!(matches!(
            sessions.stage_saved(&binding, save, candidate.root, 1),
            Err(RuntimeError::AlreadyAttempted)
        ));
        let history = sessions.history_receipts(&binding, save).unwrap();
        assert!(
            matches!(history.stage().unwrap(), Err(RuntimeError::Storage(error)) if matches!(error.as_ref(), layerfs_storage::StorageError::ObjectMissing(id) if *id == candidate.table))
        );
        assert!(history.commit().is_none());
        assert!(history.discard().is_none());
        assert!(!history.terminal_unknown());
    }
}
