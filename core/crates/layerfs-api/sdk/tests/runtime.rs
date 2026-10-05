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
    let worker = thread::spawn(move || {
        let stream = listener.accept().unwrap().0;
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
