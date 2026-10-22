#![cfg(target_os = "macos")]

use layerfs_content::filesystem::{
    DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources, FilesystemRootId,
    InodeScope, InodeUpdate, PathName,
    attributes::{
        AttributeEntry, AttributeKey, PortableMetadata, build_attribute_tree, emit_value,
    },
    build_filesystem, scope_for_seed,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeMetadata, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_fuse::{
    MountRequests,
    attributes::Identity,
    ports::{
        Fence, Fenced, MountServices, RequestServices, ServiceError, ServiceFuture, ServiceReply,
    },
    session::{Detach, KernelSession, RECEIVE_LOOPS, SessionConfig},
};
use layerfs_overlay::{KernelCookieOffer, KernelMount, Overlay, ProfileConfig, Publication};
use layerfs_workspace::{
    BaseView, CanonicalCache, CanonicalClient, FileLengths, KernelDirectoryWindow,
    KernelMutationOutcome, KernelReadOperation, KernelReadOutcome, KernelVisitRequest,
    KernelWindow, VisitFacts, Workspace, WorkspaceResult, WriteData,
};
use nix::unistd::{getegid, geteuid};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    os::unix::fs::MetadataExt,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Default)]
struct MemoryStore {
    objects: Arc<Mutex<BTreeMap<ObjectId, Vec<u8>>>>,
    lengths: Arc<Mutex<BTreeMap<ObjectId, u64>>>,
}

impl AuthenticatedObjects for MemoryStore {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let objects = self.objects.lock().unwrap();
        ids.iter()
            .map(|id| objects.get(id).cloned().ok_or(ContentError::MissingObject))
            .collect()
    }
}

impl FileLengths for MemoryStore {
    fn file_length(&self, id: ObjectId) -> WorkspaceResult<u64> {
        self.lengths
            .lock()
            .unwrap()
            .get(&id)
            .copied()
            .ok_or(ContentError::MissingObject.into())
    }
}

impl FinalizedConsumer for MemoryStore {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.objects
            .lock()
            .unwrap()
            .insert(object.id(), object.canonical().to_vec());
        Ok(())
    }
}

fn build_metadata(store: &MemoryStore, kind: InodeKind, mode: u32) -> ObjectId {
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(store, &mut sink);
    let portable = PortableMetadata {
        mode,
        mtime_seconds: 1_700_000_000,
        mtime_nanoseconds: 123_456_789,
    };
    let mode_id = emit_value(&mut objects, &portable.mode_bytes(kind).unwrap()).unwrap();
    let time_id = emit_value(&mut objects, &portable.mtime_bytes().unwrap()).unwrap();
    build_attribute_tree(
        &mut objects,
        [
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_root: mode_id,
            }),
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_root: time_id,
            }),
        ]
        .into_iter(),
    )
    .unwrap()
    .0
}

fn build_file(store: &MemoryStore, bytes: &[u8]) -> ObjectId {
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(store, &mut sink);
    let root = emit_value(&mut objects, bytes).unwrap();
    store
        .lengths
        .lock()
        .unwrap()
        .insert(root, bytes.len() as u64);
    root
}

fn build_root_fixture() -> (MemoryStore, FilesystemRootId, InodeScope) {
    let store = MemoryStore::default();
    let scope = scope_for_seed([7; 32]);
    let hello = build_file(&store, b"hello from layerfs base\n");
    let dir_meta = build_metadata(&store, InodeKind::Directory, 0o755);
    let file_meta = build_metadata(&store, InodeKind::RegularFile, 0o644);
    let inodes = vec![
        InodeUpdate {
            serial: 1,
            value: InodeValue {
                kind: InodeKind::Directory,
                content_root: ObjectId::for_bytes(b"new-directory"),
                metadata_root: InodeMetadata::Root(dir_meta),
                namespace_ref_count: 0,
            },
        },
        InodeUpdate {
            serial: 2,
            value: InodeValue {
                kind: InodeKind::RegularFile,
                content_root: hello,
                metadata_root: InodeMetadata::Root(file_meta),
                namespace_ref_count: 0,
            },
        },
    ];
    let directories = vec![DirectoryUpdate {
        parent: 1,
        changes: vec![(PathName::new("hello.txt").unwrap(), Some(2))],
    }];
    let new_inodes = vec![1, 2];
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(&store, &mut sink);
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources::default(),
    };
    let root = build_filesystem(&mut objects, &input, None).unwrap().root;
    (store, root, scope)
}

struct TestWorkspace {
    overlay: Mutex<Overlay>,
    workspace: Workspace,
    client: Arc<CanonicalClient>,
    resident: Arc<CanonicalClient>,
    next_serial: AtomicU64,
}

struct TestMountServices(Arc<TestWorkspace>);

struct TestRequestServices {
    shared: Arc<TestWorkspace>,
    fence: Fence,
}

impl MountServices for TestMountServices {
    fn request(&self, fence: &Fence) -> Result<Arc<dyn RequestServices>, ServiceError> {
        Ok(Arc::new(TestRequestServices {
            shared: self.0.clone(),
            fence: fence.clone(),
        }))
    }
}

impl TestRequestServices {
    fn check_fence(&self) -> Result<(), ServiceError> {
        if self.fence.stopped() {
            Err(Box::new(Fenced))
        } else {
            Ok(())
        }
    }
}

impl RequestServices for TestRequestServices {
    fn directory_visit(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: u64,
        offset: u64,
        after: Option<Vec<u8>>,
    ) -> ServiceFuture<'_, ServiceReply<Arc<KernelDirectoryWindow>>> {
        Box::pin(async move {
            self.check_fence()?;
            let visit = self.shared.workspace.kernel_directory_visit(
                self.shared.resident.clone(),
                mount,
                serial,
                handle,
                offset,
                after,
            )?;
            let overlay = self.shared.overlay.lock().unwrap();
            let window = visit.perform(&overlay)?;
            Ok(ServiceReply::new(Arc::new(window), ()))
        })
    }

    fn publish_cookies(
        &self,
        offer: KernelCookieOffer,
        names: Vec<Vec<u8>>,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        Box::pin(async move {
            self.check_fence()?;
            let overlay = self.shared.overlay.lock().unwrap();
            overlay.publish_kernel_cookies(&offer, &names)?;
            Ok(ServiceReply::new((), ()))
        })
    }

    fn close_directory(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        Box::pin(async move {
            let overlay = self.shared.overlay.lock().unwrap();
            overlay.close_kernel_directory(mount, serial, handle)?;
            Ok(ServiceReply::new((), ()))
        })
    }

    fn read_visit(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: Option<u64>,
        offset: u64,
        length: u32,
    ) -> ServiceFuture<'_, ServiceReply<Arc<KernelWindow>>> {
        Box::pin(async move {
            self.check_fence()?;
            let visit = self.shared.workspace.kernel_data_visit(
                self.shared.resident.clone(),
                mount,
                serial,
                handle,
                offset,
                length,
            )?;
            let overlay = self.shared.overlay.lock().unwrap();
            let window = visit.perform(&overlay)?;
            Ok(ServiceReply::new(Arc::new(window), ()))
        })
    }

    fn close_file(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        Box::pin(async move {
            let overlay = self.shared.overlay.lock().unwrap();
            overlay.kernel_close_file(mount, serial, handle)?;
            Ok(ServiceReply::new((), ()))
        })
    }

    fn provider_failure(&self) -> Result<Option<ServiceError>, ServiceError> {
        Ok(None)
    }

    fn failed_base_read(&self, step: ServiceError) -> ServiceError {
        step
    }

    fn observe_visit(
        &self,
        mount: KernelMount,
        request: u64,
        serial: u64,
        handle: Option<u64>,
        operation: KernelReadOperation,
        facts: Arc<VisitFacts>,
    ) -> ServiceFuture<'_, ServiceReply<Arc<KernelReadOutcome>>> {
        Box::pin(async move {
            self.check_fence()?;
            let resident = self.shared.resident.clone();
            let visit = match operation {
                KernelReadOperation::Open { writable, .. } => self
                    .shared
                    .workspace
                    .kernel_open_visit(resident, mount, request, serial, writable, facts),
                KernelReadOperation::Opendir { .. } => self
                    .shared
                    .workspace
                    .kernel_opendir_visit(resident, mount, request, serial, facts),
                other => self
                    .shared
                    .workspace
                    .kernel_read_visit(resident, mount, serial, handle, other, facts),
            }?;
            let overlay = self.shared.overlay.lock().unwrap();
            let outcome = visit.perform(&overlay);
            Ok(ServiceReply::new(Arc::new(outcome), ()))
        })
    }

    fn base(&self) -> ServiceFuture<'_, BaseView> {
        Box::pin(async move {
            self.check_fence()?;
            let base = self.shared.workspace.base()?;
            Ok(base.with_client(self.shared.client.clone()))
        })
    }

    fn mutate_visit(
        &self,
        request: KernelVisitRequest,
    ) -> ServiceFuture<'_, ServiceReply<Arc<KernelMutationOutcome>>> {
        Box::pin(async move {
            self.check_fence()?;
            let visit = self
                .shared
                .workspace
                .kernel_mutation_visit(self.shared.resident.clone(), request)?;
            let overlay = self.shared.overlay.lock().unwrap();
            let outcome = visit.perform(&overlay);
            Ok(ServiceReply::new(Arc::new(outcome), ()))
        })
    }

    fn recognize(
        &self,
        _serial: u64,
        _offset: u64,
        data: WriteData,
    ) -> ServiceFuture<'_, WriteData> {
        Box::pin(async move {
            self.check_fence()?;
            Ok(data)
        })
    }

    fn forget(
        &self,
        mount: KernelMount,
        serial: u64,
        count: u64,
    ) -> ServiceFuture<'_, ServiceReply<()>> {
        Box::pin(async move {
            let overlay = self.shared.overlay.lock().unwrap();
            overlay.kernel_forget(mount, serial, count)?;
            Ok(ServiceReply::new((), ()))
        })
    }

    fn reserve_serial(&self) -> Result<Option<u64>, ServiceError> {
        self.check_fence()?;
        Ok(Some(
            self.shared.next_serial.fetch_add(1, Ordering::Relaxed),
        ))
    }

    fn reply_attempted(&self, publication: Publication) -> ServiceFuture<'_, ServiceReply<()>> {
        Box::pin(async move {
            let overlay = self.shared.overlay.lock().unwrap();
            overlay.reply_attempted(publication)?;
            Ok(ServiceReply::new((), ()))
        })
    }
}

struct TempRoot(PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn fuse_t_mount_serves_stat_readdir_write_read_and_drains_cleanly() {
    let (store, root, scope) = build_root_fixture();
    let cache = Arc::new(CanonicalCache::new(4 * 1024 * 1024));
    let client = Arc::new(CanonicalClient::with_cache(
        Arc::new(store.clone()),
        Some(Arc::new(store)),
        cache.clone(),
    ));
    let resident = Arc::new(CanonicalClient::resident(cache));

    let temp_root = TempRoot(std::env::temp_dir().join(format!(
        "layerfs-fuse-t-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )));
    fs::create_dir_all(&temp_root.0).unwrap();

    let overlay_path = temp_root.0.join("overlay.sqlite");
    let overlay = Overlay::create(&overlay_path, ProfileConfig::default()).unwrap();
    let workspace = Workspace::open(&overlay, client.clone(), root, scope, [9; 32]).unwrap();
    let kernel_mount = overlay.create_kernel_mount(1).unwrap();

    let shared = Arc::new(TestWorkspace {
        overlay: Mutex::new(overlay),
        workspace,
        client,
        resident,
        next_serial: AtomicU64::new(100),
    });

    let mount_dir = temp_root.0.join("mnt");
    let uid = geteuid().as_raw();
    let gid = getegid().as_raw();
    let identity = Identity { root: 1, uid, gid };
    let queue = MountRequests::new(kernel_mount);
    let services: Arc<dyn MountServices> = Arc::new(TestMountServices(shared.clone()));
    let config = SessionConfig {
        directory: mount_dir.clone(),
        owner_uid: uid,
        owner_gid: gid,
        ready_wait: Duration::from_secs(10),
        drain_wait: Duration::from_secs(10),
    };

    let mut session =
        KernelSession::attach(config, queue, services, identity).expect("attach FUSE-T session");

    let facts = session.facts();
    assert!(facts.serving, "expected serving session: {facts:?}");
    assert_eq!(
        facts.loops.map(|loops| (loops.configured, loops.entered)),
        Some((RECEIVE_LOOPS, RECEIVE_LOOPS))
    );
    let negotiation = session.negotiation().expect("negotiation recorded");
    assert_eq!(negotiation.abi_version().0, 7);
    let entry = session.entry().expect("mount table entry recorded");
    assert!(entry.id > 0, "expected positive mount id: {entry:?}");

    let root_meta = fs::metadata(&mount_dir).expect("stat root directory");
    assert!(root_meta.is_dir());
    assert_eq!(root_meta.ino(), 1);

    let hello_path = mount_dir.join("hello.txt");
    let hello_meta = fs::metadata(&hello_path).expect("stat hello.txt");
    assert!(hello_meta.is_file());
    assert_eq!(hello_meta.len(), 24);
    assert_eq!(hello_meta.mode() & 0o777, 0o644);
    assert_eq!((hello_meta.uid(), hello_meta.gid()), (uid, gid));

    let initial_entries: BTreeSet<OsString> = fs::read_dir(&mount_dir)
        .expect("readdir root")
        .map(|entry| entry.expect("dir entry").file_name())
        .collect();
    assert_eq!(
        initial_entries,
        BTreeSet::from([OsString::from("hello.txt")])
    );

    let initial_bytes = fs::read(&hello_path).expect("read hello.txt");
    assert_eq!(initial_bytes, b"hello from layerfs base\n");

    let written_path = mount_dir.join("written.txt");
    fs::write(&written_path, b"written through fuse-t\n").expect("write written.txt");
    assert_eq!(
        fs::read(&written_path).expect("read written.txt"),
        b"written through fuse-t\n"
    );

    fs::write(&hello_path, b"updated base file\n").expect("overwrite hello.txt");
    assert_eq!(
        fs::read(&hello_path).expect("read updated hello.txt"),
        b"updated base file\n"
    );

    let updated_entries: BTreeSet<OsString> = fs::read_dir(&mount_dir)
        .expect("readdir root after write")
        .map(|entry| entry.expect("dir entry").file_name())
        .collect();
    assert_eq!(
        updated_entries,
        BTreeSet::from([OsString::from("hello.txt"), OsString::from("written.txt")])
    );

    assert_eq!(session.detach(), Ok(Detach::Detached));
    let drained = session.drain().expect("drain FUSE-T session");
    let outcome = drained.outcome.as_ref().expect("session outcome recorded");
    assert!(
        outcome.startup_error.is_none(),
        "unexpected startup error: {outcome:?}"
    );
    assert_eq!(outcome.receivers.len(), RECEIVE_LOOPS);
    assert!(
        matches!(outcome.cleanup, Ok(Ok(()))),
        "expected clean filesystem destroy: {outcome:?}"
    );
    assert_eq!((drained.work.active, drained.work.retained), (0, 0));
    assert!(drained.work.terminal);
    assert!(
        drained.removed.is_ok(),
        "expected mount dir removed: {drained:?}"
    );
    assert!(!mount_dir.exists());
}
