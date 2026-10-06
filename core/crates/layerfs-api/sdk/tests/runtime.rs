//! Actual macOS provider through SDK-owned runtime sessions.
#![cfg(target_os = "macos")]

use layerfs_content::filesystem::{
    attributes::{encode_attribute_page, AttributePage},
    directory::{encode_directory_page, DirectoryPage},
    inode::{encode_inode_page, InodePage},
    profile_id, scope_for_seed, FilesystemRoot,
};
use layerfs_content::object::{InodeKind, InodeValue};
use layerfs_content::{encode_whole_file_payload, ObjectId, ObjectRole};
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId, StackInitialization, WorkspaceId,
};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_sdk::{
    Authorization, Binding, CompletionPhase, Config, ObjectReply, Runtime, RuntimeError,
    RuntimeResult, SaveId, Sessions,
};
use layerfs_storage::{Storage, StorageError, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::{cell::Cell, path::PathBuf, rc::Rc};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../target/cluster2-runtime-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

struct Allow {
    revoked: Rc<Cell<bool>>,
    deny: Rc<Cell<Option<ObjectId>>>,
    peers: [[u8; 32]; 2],
}
impl Authorization for Allow {
    fn workspace(&self, peer: [u8; 32], _: WorkspaceId, _: BranchId) -> RuntimeResult<()> {
        if self.revoked.get() || !self.peers.contains(&peer) {
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
        ids: &[ObjectId],
    ) -> RuntimeResult<()> {
        if self.deny.get().is_some_and(|denied| ids.contains(&denied)) {
            Err(RuntimeError::Denied)
        } else {
            Ok(())
        }
    }
}
struct Fixture {
    _temp: Temp,
    runtime: Runtime,
    branch: BranchId,
    revoked: Rc<Cell<bool>>,
    deny: Rc<Cell<Option<ObjectId>>>,
}
impl Fixture {
    fn new() -> Self {
        let temp = Temp::new();
        let mut config = PersistenceConfig::sqlite(temp.0.join("store"));
        config.sqlite_profile = SqlitePersistenceProfile::Disposable;
        let handles = Handles::create(
            config,
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: b"runtime".to_vec(),
                cursor_key: [9; 32],
                incarnation: 7,
            },
        )
        .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let save = storage.begin_save().unwrap();
        let directory = encode_directory_page(&DirectoryPage::Leaf { entries: vec![] }).unwrap();
        let dir_id = ObjectId::for_bytes(&directory);
        save.accept(
            layerfs_content::FinalizedObject::new(ObjectRole::DirectoryLeaf, directory).unwrap(),
        )
        .unwrap();
        let metadata = encode_attribute_page(&AttributePage::Leaf {
            subtree_bytes: 0,
            entries: vec![],
        })
        .unwrap();
        let meta_id = ObjectId::for_bytes(&metadata);
        save.accept(
            layerfs_content::FinalizedObject::new(ObjectRole::AttributeLeaf, metadata).unwrap(),
        )
        .unwrap();
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
            layerfs_content::FinalizedObject::new(ObjectRole::InodeLeaf, table)
                .unwrap()
                .with_references(vec![dir_id, meta_id]),
        )
        .unwrap();
        let scope = scope_for_seed([8; 32]);
        let root = FilesystemRoot::new(profile_id(), scope, 1, table_id)
            .unwrap()
            .encode()
            .unwrap();
        let root_id = ObjectId::for_bytes(&root);
        save.accept(
            layerfs_content::FinalizedObject::new(ObjectRole::FilesystemRoot, root)
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
                name: HistoryName::new("runtime").unwrap(),
                scope: scope.object(),
                profile: profile_id(),
                genesis_root: root_id,
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
        let revoked = Rc::new(Cell::new(false));
        let deny = Rc::new(Cell::new(None));
        let runtime = Runtime::new(
            handles,
            Config {
                incarnation: [7; 32],
                save_slots: 2,
            },
            Box::new(Allow {
                revoked: revoked.clone(),
                deny: deny.clone(),
                peers: [
                    layerfs_bridge::native::public_key(&[1; 32]).unwrap(),
                    layerfs_bridge::native::public_key(&[2; 32]).unwrap(),
                ],
            }),
        )
        .unwrap();
        Self {
            _temp: temp,
            runtime,
            branch,
            revoked,
            deny,
        }
    }
}
#[derive(Default)]
struct Reply(Vec<(ObjectId, Vec<u8>)>);
impl ObjectReply for Reply {
    fn object(&mut self, id: ObjectId, bytes: &[u8]) -> RuntimeResult<()> {
        self.0.push((id, bytes.to_vec()));
        Ok(())
    }
}
#[derive(Default)]
struct Lengths(Vec<(ObjectId, u64)>);
impl layerfs_sdk::LengthReply for Lengths {
    fn file_length(&mut self, id: ObjectId, length: u64) -> RuntimeResult<()> {
        self.0.push((id, length));
        Ok(())
    }
}
fn binding(s: &Sessions<'_>, branch: BranchId, workspace: u8, peer: u8) -> Binding {
    let verified = verified(peer);
    s.bind(
        &verified,
        WorkspaceId::from_authority([workspace; 32]).unwrap(),
        branch,
    )
    .unwrap()
}
fn verified(peer: u8) -> layerfs_bridge::native::VerifiedPeer {
    use layerfs_bridge::native::{accept, initiate, public_key};
    use std::{
        net::{TcpListener, TcpStream},
        thread,
        time::Duration,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let client_public = public_key(&[peer; 32]).unwrap();
    let server_public = public_key(&[33; 32]).unwrap();
    listener.set_nonblocking(true).unwrap();
    let worker = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "handshake accept deadline"
                    );
                    thread::yield_now();
                }
                Err(error) => panic!("handshake accept: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        accept(stream, &[33; 32], client_public).unwrap().peer
    });
    let stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let _connection = initiate(stream, &[peer; 32], server_public).unwrap();
    worker.join().unwrap()
}
fn accept(s: &mut Sessions<'_>, b: &Binding, id: SaveId, bytes: &[u8]) -> ObjectId {
    let canonical = encode_whole_file_payload(bytes).unwrap();
    let object = ObjectId::for_bytes(&canonical);
    Timing::disabled("accept", |scope| {
        s.accept(
            b,
            id,
            object,
            ObjectRole::WholeFile,
            canonical,
            scope.child("object"),
        )
    })
    .0
    .unwrap()
}

#[test]
fn independent_saves_pending_reads_retained_finish_and_slot_reuse() {
    let mut f = Fixture::new();
    let branch = f.branch;
    let mut s = f.runtime.sessions();
    let a = binding(&s, branch, 10, 1);
    let b = binding(&s, branch, 11, 1);
    let sa = s.begin(&a).unwrap();
    let sb = s.begin(&b).unwrap();
    assert!(matches!(
        s.begin(&a),
        Err(RuntimeError::AdmissionUnavailable)
    ));
    let ra = accept(&mut s, &a, sa, b"A pending");
    let rb = accept(&mut s, &b, sb, b"B pending");
    let mut reply = Reply::default();
    s.read_objects(&a, Some(sa), &[ra, ra], &mut reply).unwrap();
    assert_eq!(reply.0.len(), 2);
    assert_eq!(reply.0[0].1, reply.0[1].1);
    let completed = s.finish(&b, sb).unwrap();
    assert_eq!(completed.phase(), CompletionPhase::Finish);
    assert!(completed.outcome().is_ok());
    assert!(s.completion(&b, sb).unwrap().outcome().is_ok());
    assert!(matches!(
        s.finish(&b, sb),
        Err(RuntimeError::AlreadyAttempted)
    ));
    let mut saved = Reply::default();
    s.read_objects(&b, None, &[rb], &mut saved).unwrap();
    assert_eq!(saved.0.len(), 1);
    let mut lengths = Lengths::default();
    s.file_lengths(&b, &[rb, rb], &mut lengths).unwrap();
    assert_eq!(lengths.0, [(rb, 9), (rb, 9)]);
    assert!(
        matches!(s.begin(&b), Err(RuntimeError::AdmissionUnavailable)),
        "held receipt still consumes slot"
    );
    s.release(&b, sb).unwrap();
    let next = s.begin(&b).unwrap();
    assert_ne!(next, sb);
    assert!(matches!(
        s.read_objects(&b, Some(sb), &[rb], &mut Reply::default()),
        Err(RuntimeError::StaleCapability)
    ));
    assert_eq!(s.abort(&b, next).unwrap().phase(), CompletionPhase::Abort);
    s.release(&b, next).unwrap();
    assert!(s.finish(&a, sa).unwrap().outcome().is_ok());
    s.release(&a, sa).unwrap();
    drop(s);
    let mut restarted = f.runtime.sessions();
    let b = binding(&restarted, branch, 10, 1);
    let fresh = restarted.begin(&b).unwrap();
    assert_ne!(fresh, sa);
}

#[test]
fn capability_scope_authority_and_read_window_are_checked_before_service() {
    let mut f = Fixture::new();
    let branch = f.branch;
    let revoked = f.revoked.clone();
    let deny = f.deny.clone();
    let mut s = f.runtime.sessions();
    let a = binding(&s, branch, 10, 1);
    let other = binding(&s, branch, 11, 1);
    let peer = binding(&s, branch, 10, 2);
    let id = s.begin(&a).unwrap();
    assert!(matches!(
        s.finish(&other, id),
        Err(RuntimeError::StaleCapability)
    ));
    assert!(matches!(
        s.abort(&peer, id),
        Err(RuntimeError::StaleCapability)
    ));
    revoked.set(true);
    assert!(matches!(s.policy(&a), Err(RuntimeError::Denied)));
    revoked.set(false);
    let object = accept(&mut s, &a, id, b"private");
    deny.set(Some(object));
    let mut reply = Reply::default();
    assert!(matches!(
        s.read_objects(&a, Some(id), &[object], &mut reply),
        Err(RuntimeError::Denied)
    ));
    assert!(reply.0.is_empty());
    deny.set(None);
    assert!(matches!(
        s.read_objects(&a, Some(id), &vec![object; 4097], &mut reply),
        Err(RuntimeError::Invalid("object demand window"))
    ));
    s.read_objects(&a, Some(id), &[object], &mut reply).unwrap();
    assert_eq!(reply.0.len(), 1);
}

#[test]
fn refused_admission_retains_the_exact_phase_without_replay() {
    let mut f = Fixture::new();
    let branch = f.branch;
    let mut s = f.runtime.sessions();
    let a = binding(&s, branch, 10, 1);
    let id = s.begin(&a).unwrap();
    let canonical = encode_whole_file_payload(b"wrong id").unwrap();
    let result = Timing::disabled("refused", |scope| {
        s.accept(
            &a,
            id,
            ObjectId::from_bytes(&[99; 32]).unwrap(),
            ObjectRole::WholeFile,
            canonical,
            scope.child("object"),
        )
    })
    .0;
    assert!(matches!(
        result,
        Err(RuntimeError::Content(
            layerfs_content::ContentError::IdentityMismatch
        ))
    ));
    let receipt = s.completion(&a, id).unwrap();
    assert_eq!(receipt.phase(), CompletionPhase::Accept);
    assert!(matches!(
        receipt.outcome(),
        Err(RuntimeError::Content(
            layerfs_content::ContentError::IdentityMismatch
        ))
    ));
    assert!(matches!(
        s.finish(&a, id),
        Err(RuntimeError::AlreadyAttempted)
    ));
    s.release(&a, id).unwrap();
}

#[test]
fn missing_dependency_finish_is_retained_as_the_original_storage_error() {
    let mut f = Fixture::new();
    let branch = f.branch;
    let mut s = f.runtime.sessions();
    let a = binding(&s, branch, 10, 1);
    let id = s.begin(&a).unwrap();
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
    let object = ObjectId::for_bytes(&canonical);
    Timing::disabled("accept", |scope| {
        s.accept(
            &a,
            id,
            object,
            ObjectRole::InodeLeaf,
            canonical,
            scope.child("object"),
        )
    })
    .0
    .unwrap();
    let receipt = s.finish(&a, id).unwrap();
    assert!(
        matches!(receipt.outcome(),Err(RuntimeError::Storage(error)) if matches!(error.as_ref(),StorageError::MissingDependency { object: parent,reference } if *parent==object && *reference==missing))
    );
    assert!(matches!(
        s.finish(&a, id),
        Err(RuntimeError::AlreadyAttempted)
    ));
    s.release(&a, id).unwrap();
}

type EmittedObject = (ObjectId, ObjectRole, Vec<u8>);
#[derive(Clone, Default)]
struct Archive {
    objects: std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<ObjectId, Vec<u8>>>>,
    emitted: std::sync::Arc<std::sync::Mutex<Vec<EmittedObject>>>,
}
impl layerfs_content::AuthenticatedObjects for Archive {
    fn read_canonical_batch(
        &self,
        ids: &[ObjectId],
    ) -> layerfs_content::ContentResult<Vec<Vec<u8>>> {
        let objects = self.objects.lock().unwrap();
        ids.iter()
            .map(|id| {
                objects
                    .get(id)
                    .cloned()
                    .ok_or(layerfs_content::ContentError::MissingObject)
            })
            .collect()
    }
}
impl layerfs_content::FinalizedConsumer for Archive {
    fn accept(
        &mut self,
        object: layerfs_content::FinalizedObject,
    ) -> layerfs_content::ContentResult<()> {
        self.objects
            .lock()
            .unwrap()
            .insert(object.id(), object.canonical().to_vec());
        self.emitted.lock().unwrap().push((
            object.id(),
            object.role(),
            object.canonical().to_vec(),
        ));
        Ok(())
    }
}
fn portable(archive: &Archive, kind: InodeKind) -> ObjectId {
    use layerfs_content::filesystem::attributes::{
        build_attribute_tree, emit_value, AttributeEntry, AttributeKey, PortableMetadata,
    };
    let mut sink = archive.clone();
    let mut objects = layerfs_content::filesystem::FilesystemObjects::new(archive, &mut sink);
    let meta = PortableMetadata {
        mode: if kind == InodeKind::Directory {
            0o1777
        } else {
            0o644
        },
        mtime_seconds: 9,
        mtime_nanoseconds: 7,
    };
    let mode = emit_value(&mut objects, &meta.mode_bytes(kind).unwrap()).unwrap();
    let time = emit_value(&mut objects, &meta.mtime_bytes().unwrap()).unwrap();
    build_attribute_tree(
        &mut objects,
        [
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_root: mode,
            }),
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_root: time,
            }),
        ]
        .into_iter(),
    )
    .unwrap()
    .0
}
#[test]
fn workspace_stat_uses_authenticated_scoped_owning_lengths_without_payload_or_fallback() {
    use layerfs_content::filesystem::{
        build_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources,
        InodeUpdate, PathName,
    };
    use layerfs_content::{FinalizedConsumer, FinalizedObject};
    use layerfs_workspace::{BaseView, CanonicalClient, WorkspaceError};
    let mut f = Fixture::new();
    let s = f.runtime.sessions();
    let b = binding(&s, f.branch, 90, 1);
    let archive = Archive::default();
    let canonical = encode_whole_file_payload(&vec![0xa7; 131071]).unwrap();
    let file = ObjectId::for_bytes(&canonical);
    archive
        .clone()
        .accept(FinalizedObject::new(ObjectRole::WholeFile, canonical).unwrap())
        .unwrap();
    let file_meta = portable(&archive, InodeKind::RegularFile);
    let dir_meta = portable(&archive, InodeKind::Directory);
    let inodes = [
        InodeUpdate {
            serial: 1,
            value: InodeValue {
                kind: InodeKind::Directory,
                content_root: ObjectId::for_bytes(b"new-dir"),
                metadata_root: dir_meta,
                namespace_ref_count: 0,
            },
        },
        InodeUpdate {
            serial: 2,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                content_root: file,
                metadata_root: file_meta,
                namespace_ref_count: 0,
            },
        },
    ];
    let dirs = [DirectoryUpdate {
        parent: 1,
        changes: vec![
            (PathName::new("alias").unwrap(), Some(2)),
            (PathName::new("file").unwrap(), Some(2)),
        ],
    }];
    let scope = scope_for_seed([8; 32]);
    let mut sink = archive.clone();
    let mut objects = FilesystemObjects::new(&archive, &mut sink);
    let root = build_filesystem(
        &mut objects,
        &FilesystemInput {
            base: None,
            scope,
            root_serial: 1,
            directories: &dirs,
            inodes: &inodes,
            new_inodes: &[1, 2],
            resources: FilesystemResources::default(),
        },
        None,
    )
    .unwrap()
    .root;
    let mut s = s;
    let id = s.begin(&b).unwrap();
    for (claimed, role, bytes) in archive.emitted.lock().unwrap().iter() {
        Timing::disabled("stat-save", |timing| {
            s.accept(
                &b,
                id,
                *claimed,
                *role,
                bytes.clone(),
                timing.child("object"),
            )
        })
        .0
        .unwrap();
    }
    assert!(s.finish(&b, id).unwrap().outcome().is_ok());
    let client = std::sync::Arc::new(CanonicalClient::new(std::sync::Arc::new(archive), 0));
    let base = BaseView::open(client, root, scope).unwrap();
    assert!(matches!(
        base.stat(2),
        Err(WorkspaceError::MissingLengthProvider)
    ));
    let before = s.demand_diagnostics();
    let port = s.length_port(&b);
    let stat = base.stat_with_lengths(2, &port).unwrap();
    let after = s.demand_diagnostics();
    assert_eq!(
        (
            stat.logical_len,
            stat.value.namespace_ref_count,
            stat.metadata.mode
        ),
        (131071, 2, 0o644)
    );
    assert_eq!(after.payload_read_bytes - before.payload_read_bytes, 0);
    assert_eq!(after.pack_read_bytes - before.pack_read_bytes, 0);
    assert_eq!(after.read_packs - before.read_packs, 0);
    println!("S3_OWNING_STAT logical=131071 namespace_refs=2 owning_payload_bytes=0 owning_pack_bytes=0 locate={}",after.locate-before.locate);
    f.deny.set(Some(file));
    let error = base.stat_with_lengths(2, &port).unwrap_err();
    assert!(
        matches!(error,WorkspaceError::Service(error) if matches!(error.downcast_ref::<RuntimeError>(),Some(RuntimeError::Denied)))
    );
}

#[test]
fn workspace_serials_come_from_the_owning_scope_allocator_under_authority() {
    use layerfs_workspace::{InodeSerials, WorkspaceError};
    let mut f = Fixture::new();
    let branch = f.branch;
    let revoked = f.revoked.clone();
    let s = f.runtime.sessions();
    let b = binding(&s, branch, 10, 1);
    let port = s.serial_port(&b);
    // Consecutive consumed ranges of the Branch scope, never reissued.
    let (first, count) = port.reserve(layerfs_workspace::SERIAL_REFILL).unwrap();
    assert_eq!(count, layerfs_workspace::SERIAL_REFILL);
    assert!(first > 0);
    let (second, _) = port.reserve(1).unwrap();
    assert_eq!(second, first + count);
    assert_eq!(s.reserve_serials(&b, 3).unwrap(), (second + 1, 3));
    for invalid in [0, layerfs_sdk::SERIAL_WINDOW + 1] {
        assert!(matches!(
            s.reserve_serials(&b, invalid),
            Err(RuntimeError::Invalid("inode serial window"))
        ));
    }
    // Revocation refuses before the allocator: nothing is consumed.
    revoked.set(true);
    let denied = port.reserve(8).unwrap_err();
    assert!(
        matches!(denied, WorkspaceError::Service(error) if matches!(error.downcast_ref::<RuntimeError>(), Some(RuntimeError::Denied)))
    );
    revoked.set(false);
    assert_eq!(port.reserve(2).unwrap(), (second + 4, 2));
    println!(
        "S4_OWNING_SERIALS first={first} refill={count} next={} scope_allocator=history_catalog",
        second + 6
    );
}

#[test]
fn history_up_to_date_retains_exact_stage_and_transition_without_replay() {
    let mut f = Fixture::new();
    let branch = f.branch;
    let mut s = f.runtime.sessions();
    let a = binding(&s, branch, 71, 1);
    assert_eq!(a.root_serial(), 1);
    let save = s.begin(&a).unwrap();
    let root = a.snapshot().effective_root;
    assert!(matches!(
        s.stage_saved(&a, save, root, 1),
        Err(RuntimeError::Invalid(_))
    ));
    assert!(s.finish(&a, save).unwrap().outcome().is_ok());
    let stage = s
        .stage_saved(&a, save, root, 1)
        .unwrap()
        .as_ref()
        .unwrap()
        .clone();
    assert_eq!(stage.workspace, a.workspace());
    assert_eq!(stage.branch, a.snapshot().branch.id);
    assert_eq!(stage.expected_root, root);
    assert!(matches!(
        s.release(&a, save),
        Err(RuntimeError::RetainedCustody)
    ));
    assert!(matches!(
        s.stage_saved(&a, save, root, 1),
        Err(RuntimeError::AlreadyAttempted)
    ));
    assert!(
        matches!(s.commit_saved(&a, save).unwrap(), Ok(layerfs_history::CommitStagedOutcome::UpToDate { root: actual, .. }) if *actual == root)
    );
    assert!(matches!(
        s.commit_saved(&a, save),
        Err(RuntimeError::AlreadyAttempted)
    ));
    assert!(s
        .history_receipts(&a, save)
        .unwrap()
        .commit()
        .unwrap()
        .is_ok());
    assert!(matches!(
        s.discard_saved(&a, save),
        Err(RuntimeError::RetainedCustody)
    ));
    s.release(&a, save).unwrap();
    assert!(matches!(
        s.history_receipts(&a, save),
        Err(RuntimeError::StaleCapability)
    ));
}

#[test]
fn history_candidate_authority_and_grammar_refusals_preserve_save_and_stage_custody() {
    let mut f = Fixture::new();
    let branch = f.branch;
    let deny = f.deny.clone();
    let mut s = f.runtime.sessions();
    let a = binding(&s, branch, 72, 1);
    let b = binding(&s, branch, 73, 2);
    let sa = s.begin(&a).unwrap();
    let sb = s.begin(&b).unwrap();
    let wrong = accept(&mut s, &a, sa, b"saved file is not a namespace");
    assert!(s.finish(&a, sa).unwrap().outcome().is_ok());
    assert!(s.finish(&b, sb).unwrap().outcome().is_ok());
    assert!(matches!(
        s.stage_saved(&b, sa, wrong, 1),
        Err(RuntimeError::StaleCapability)
    ));
    assert!(matches!(
        s.stage_saved(&a, sa, wrong, 1).unwrap(),
        Err(RuntimeError::Content(_))
    ));
    assert!(matches!(
        s.stage_saved(&a, sa, a.snapshot().effective_root, 1),
        Err(RuntimeError::AlreadyAttempted)
    ));
    s.release(&a, sa).unwrap();
    deny.set(Some(b.snapshot().effective_root));
    assert!(matches!(
        s.stage_saved(&b, sb, b.snapshot().effective_root, 1)
            .unwrap(),
        Err(RuntimeError::Denied)
    ));
    deny.set(None);
    assert!(matches!(
        s.stage_saved(&b, sb, b.snapshot().effective_root, 1),
        Err(RuntimeError::AlreadyAttempted)
    ));
    s.release(&b, sb).unwrap();
    let fresh = s.begin(&a).unwrap();
    assert!(s.finish(&a, fresh).unwrap().outcome().is_ok());
    assert!(s
        .stage_saved(&a, fresh, a.snapshot().effective_root, 1)
        .unwrap()
        .is_ok());
    assert!(matches!(
        s.discard_saved(&b, fresh),
        Err(RuntimeError::StaleCapability)
    ));
    assert_eq!(
        s.discard_saved(&a, fresh).unwrap().as_ref().unwrap(),
        &layerfs_history::DiscardOutcome::Removed
    );
    assert!(matches!(
        s.discard_saved(&a, fresh),
        Err(RuntimeError::AlreadyAttempted)
    ));
    assert!(matches!(
        s.commit_saved(&a, fresh),
        Err(RuntimeError::AlreadyAttempted)
    ));
    s.release(&a, fresh).unwrap();
}

fn candidate(s: &mut Sessions<'_>, b: &Binding, save: SaveId, payload: &[u8]) -> ObjectId {
    use layerfs_content::filesystem::PathName;
    let file = accept(s, b, save, payload);
    let (start, _) = s.reserve_serials(b, 2).unwrap();
    let serial = start + 1;
    let mut base = Reply::default();
    s.read_objects(b, None, &[b.snapshot().effective_root], &mut base)
        .unwrap();
    let root = FilesystemRoot::decode(&base.0[0].1).unwrap();
    base.0.clear();
    s.read_objects(b, None, &[root.inode_table()], &mut base)
        .unwrap();
    let layerfs_content::filesystem::inode::InodePage::Leaf { entries } =
        layerfs_content::filesystem::inode::decode_inode_page(&base.0[0].1).unwrap()
    else {
        panic!("fixture leaf")
    };
    let meta = entries[0].1.metadata_root;
    let mut emit = |role, canonical: Vec<u8>| {
        let claimed = ObjectId::for_bytes(&canonical);
        Timing::disabled("candidate", |timing| {
            s.accept(b, save, claimed, role, canonical, timing.child("admit"))
        })
        .0
        .unwrap()
    };
    let dir = emit(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&DirectoryPage::Leaf {
            entries: vec![(PathName::new("new").unwrap(), serial)],
        })
        .unwrap(),
    );
    let table = emit(
        ObjectRole::InodeLeaf,
        encode_inode_page(&InodePage::Leaf {
            entries: vec![
                (
                    1,
                    InodeValue {
                        kind: InodeKind::Directory,
                        namespace_ref_count: 0,
                        content_root: dir,
                        metadata_root: meta,
                    },
                ),
                (
                    serial,
                    InodeValue {
                        kind: InodeKind::RegularFile,
                        namespace_ref_count: 1,
                        content_root: file,
                        metadata_root: meta,
                    },
                ),
            ],
        })
        .unwrap(),
    );
    emit(
        ObjectRole::FilesystemRoot,
        root.with_inode_table(table).encode().unwrap(),
    )
}

#[test]
fn history_conflict_retains_deciding_stage_and_only_exact_discard_releases_it() {
    use layerfs_history::{error::StageDisposition, CommitStagedOutcome, HistoryError};
    let mut f = Fixture::new();
    let branch = f.branch;
    let mut s = f.runtime.sessions();
    let a = binding(&s, branch, 74, 1);
    let b = binding(&s, branch, 75, 2);
    let sa = s.begin(&a).unwrap();
    let sb = s.begin(&b).unwrap();
    let ra = candidate(&mut s, &a, sa, b"A candidate");
    let rb = candidate(&mut s, &b, sb, b"B candidate");
    assert!(s.finish(&a, sa).unwrap().outcome().is_ok());
    assert!(s.finish(&b, sb).unwrap().outcome().is_ok());
    let stage_a = s
        .stage_saved(&a, sa, ra, 1)
        .unwrap()
        .as_ref()
        .unwrap()
        .clone();
    let stage_b = s
        .stage_saved(&b, sb, rb, 1)
        .unwrap()
        .as_ref()
        .unwrap()
        .clone();
    assert_ne!(stage_a.token, stage_b.token);
    let winner = s.commit_saved(&b, sb).unwrap().as_ref().unwrap().clone();
    assert!(matches!(&winner, CommitStagedOutcome::Committed(record) if record.root == rb));
    assert!(
        matches!(s.commit_saved(&a, sa).unwrap(), Err(RuntimeError::History(HistoryError::WithStage { cause, stage:StageDisposition::Retained(stage) })) if matches!(cause.as_ref(),HistoryError::HeadMoved(_)) && stage.token == stage_a.token)
    );
    assert!(matches!(
        s.release(&a, sa),
        Err(RuntimeError::RetainedCustody)
    ));
    assert!(matches!(
        s.commit_saved(&a, sa),
        Err(RuntimeError::AlreadyAttempted)
    ));
    assert!(matches!(
        s.discard_saved(&b, sa),
        Err(RuntimeError::StaleCapability)
    ));
    assert_eq!(
        s.discard_saved(&a, sa).unwrap().as_ref().unwrap(),
        &layerfs_history::DiscardOutcome::Removed
    );
    s.release(&a, sa).unwrap();
    s.release(&b, sb).unwrap();
    let fresh = binding(&s, branch, 76, 1);
    assert_eq!(fresh.snapshot().effective_root, rb);
    assert_eq!(a.snapshot().branch.head_commit, None);
}
