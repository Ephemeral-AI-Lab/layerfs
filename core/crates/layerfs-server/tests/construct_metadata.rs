//! Portable metadata construction through public Service/C1/C2 interfaces.
use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_content::{
    filesystem::attributes::{
        patch::visit_keys,
        read::{read_portable, AttributeReadWork},
    },
    object::inode_leaf::InodeKind,
    ObjectId,
};
use layerfs_server::{Grant, Service, StoreAccess};
use layerfs_storage::{Store, StoreProvider};
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::{
    io::{Cursor, Read},
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

struct Fixture {
    service: Service,
    store: Store,
    path: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-construct-metadata-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let store = Timing::disabled("create", |s| {
            Store::create(
                path.join("store.sqlite"),
                Store::default_policy(),
                s.child("store"),
            )
        })
        .0
        .unwrap();
        let grants = [(7, 128), (8, 31), (10, 127), (11, 255)]
            .into_iter()
            .map(|(key, operations)| Grant {
                public_key: *VerifiedPeer::from_private(&[key; 32]).unwrap().public_key(),
                operations,
                expires_unix: u64::MAX,
            })
            .collect();
        let service = Service::new(
            vec![StoreAccess {
                id: 1,
                store,
                grants,
                history: None,
            }],
            OperationRecorder::disabled(),
        )
        .unwrap();
        let store = Timing::disabled("open", |s| {
            Store::open(path.join("store.sqlite"), s.child("store"))
        })
        .0
        .unwrap();
        Self {
            service,
            store,
            path,
        }
    }
    fn call(&self, key: u8, request: &Request, input: &mut dyn Read) -> Result<Response, Failure> {
        let mut output = Vec::new();
        let result = self
            .service
            .handle(
                &VerifiedPeer::from_private(&[key; 32]).unwrap(),
                request,
                input,
                &mut output,
            )
            .0;
        assert!(output.is_empty());
        result
    }
    fn native(&self, request: &Request) -> Result<Response, Failure> {
        use layerfs_bridge::adapters::native::{
            client::Client,
            connection::{accept, connect, Peer},
            listen,
            server::serve,
        };
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let public = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
        std::thread::scope(|threads| {
            let server = threads.spawn(|| {
                let (socket, _) = listener.accept().unwrap();
                let connection = accept(
                    socket,
                    &[9; 32],
                    &[Peer {
                        selector: 1,
                        public: *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key(),
                        expires_unix: u64::MAX,
                    }],
                )
                .unwrap();
                let _ = serve(connection, |peer, request, input, output, deadline| {
                    self.service
                        .handle_until(peer, request, input, output, deadline)
                        .0
                });
            });
            let mut client = Client::new(connect(address, 1, &[7; 32], &public).unwrap()).unwrap();
            let mut output = Vec::new();
            let result = client.call(request, &mut &[][..], &mut output);
            assert!(output.is_empty());
            drop(client);
            server.join().unwrap();
            result
        })
    }
}
fn request(kind: u8, mode: u32, seconds: i64, nanos: u32) -> Request {
    Request {
        id: 1,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 10_000,
        response_bytes: 0,
        operation: Operation::ConstructPortableMetadata {
            kind,
            mode,
            mtime_seconds: seconds,
            mtime_nanoseconds: nanos,
        },
    }
}
fn metadata(response: Response, kind: u8, mode: u32, seconds: i64, nanos: u32) -> (Root, u64, u64) {
    let Response::MetadataConstructed {
        kind: actual_kind,
        mode: actual_mode,
        mtime_seconds,
        mtime_nanoseconds,
        metadata,
        inserted,
        reused,
    } = response
    else {
        panic!("typed constructor result")
    };
    assert_eq!(
        (actual_kind, actual_mode, mtime_seconds, mtime_nanoseconds),
        (kind, mode, seconds, nanos)
    );
    (metadata, inserted, reused)
}

#[test]
fn constructor_without_history_saves_portable_trees_and_reuses_exact_roots() {
    for native in [false, true] {
        let f = Fixture::new();
        for (kind, mode, seconds, nanos) in [
            (1, 0o640, i64::MIN, 0),
            (2, 0o1777, -2, 750_000_000),
            (3, 0o777, i64::MAX, 999_999_999),
        ] {
            let req = request(kind, mode, seconds, nanos);
            let call = || {
                if native {
                    f.native(&req)
                } else {
                    f.call(7, &req, &mut std::io::empty())
                }
            };
            let (root, inserted, reused) = metadata(call().unwrap(), kind, mode, seconds, nanos);
            assert!(inserted > 0 && inserted + reused > 0);
            let reader = StoreProvider::new(&f.store);
            let id = ObjectId::from_bytes(&root).unwrap();
            let parsed = read_portable(
                &reader,
                id,
                InodeKind::from_code(kind).unwrap(),
                &mut AttributeReadWork::default(),
            )
            .unwrap();
            assert_eq!(
                (parsed.mode, parsed.mtime_seconds, parsed.mtime_nanoseconds),
                (mode, seconds, nanos)
            );
            let mut keys = Vec::new();
            visit_keys(&reader, id, |key, _| {
                keys.push((key.domain().to_owned(), key.key().to_vec()));
                Ok(())
            })
            .unwrap();
            assert_eq!(
                keys,
                vec![
                    ("portable".to_owned(), b"mode".to_vec()),
                    ("portable".to_owned(), b"mtime".to_vec())
                ]
            );
            let (again, inserted, reused) = metadata(call().unwrap(), kind, mode, seconds, nanos);
            assert_eq!(again, root);
            assert_eq!(inserted, 0);
            assert!(reused > 0);
        }
        // Grant 128 is sufficient and no HistoryCatalog is available to allocate
        // an inode, stage a filesystem or create a Commit behind this operation.
        assert!(f
            .call(11, &request(1, 0o600, 5, 6), &mut std::io::empty())
            .is_ok());
    }
}

#[test]
fn validation_authorization_empty_input_and_deadline_fail_before_save() {
    let f = Fixture::new();
    let before = std::fs::read(f.path.join("store.sqlite")).unwrap();
    for key in [8, 10] {
        let error = f
            .call(key, &request(1, 0o640, 1, 2), &mut std::io::empty())
            .unwrap_err();
        assert_eq!(error.code, Code::Denied);
        assert!(!error.unknown);
        assert_eq!(error.cleanup, None);
    }
    for (kind, mode, nanos) in [
        (0, 0o644, 0),
        (4, 0o644, 0),
        (1, 0o4755, 0),
        (2, 0o2777, 0),
        (3, 0o755, 0),
        (1, 0o644, 1_000_000_000),
    ] {
        let error = f
            .call(7, &request(kind, mode, 0, nanos), &mut std::io::empty())
            .unwrap_err();
        assert_eq!(error.code, Code::InvalidInput);
        assert!(!error.unknown);
        assert_eq!(error.cleanup, None);
    }
    assert_eq!(
        f.call(7, &request(1, 0o644, 0, 0), &mut Cursor::new([1]))
            .unwrap_err()
            .code,
        Code::InvalidInput
    );
    let mut req = request(1, 0o644, 0, 0);
    req.response_bytes = 1;
    assert_eq!(
        f.call(7, &req, &mut std::io::empty()).unwrap_err().code,
        Code::InvalidInput
    );
    struct Unread;
    impl Read for Unread {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("expired request read input")
        }
    }
    let mut output = Vec::new();
    let error = f
        .service
        .handle_until(
            &VerifiedPeer::from_private(&[7; 32]).unwrap(),
            &request(1, 0o644, 0, 0),
            &mut Unread,
            &mut output,
            Instant::now(),
        )
        .0
        .unwrap_err();
    assert_eq!(error.code, Code::Deadline);
    assert!(!error.unknown);
    assert!(output.is_empty());
    assert_eq!(std::fs::read(f.path.join("store.sqlite")).unwrap(), before);
    assert!(f
        .call(7, &request(1, 0o644, 0, 0), &mut std::io::empty())
        .is_ok());
}

#[test]
fn real_store_admission_failure_is_definite_and_explicit_abort_releases_the_slot() {
    let f = Fixture::new();
    Timing::disabled("limit", |s| {
        f.store.set_max_concurrent_writes(1, s.child("configure"))
    })
    .0
    .unwrap();
    let held = Timing::disabled("held-save", |s| f.store.begin_save(s.child("begin")))
        .0
        .unwrap();
    let req = request(2, 0o755, -1, 17);
    let error = f.call(7, &req, &mut std::io::empty()).unwrap_err();
    assert_eq!(error.code, Code::Ownership);
    assert!(!error.unknown);
    assert_eq!(error.cleanup, None);
    // This is real public C2 admission and explicit owner abort, not a claim
    // that a constructor save had entered or that a late SQL failure was tested.
    Timing::disabled("release-held-save", |s| held.abort(s.child("abort")))
        .0
        .unwrap();
    let (root, inserted, _) = metadata(f.native(&req).unwrap(), 2, 0o755, -1, 17);
    assert!(inserted > 0);
    let parsed = read_portable(
        &StoreProvider::new(&f.store),
        ObjectId::from_bytes(&root).unwrap(),
        InodeKind::Directory,
        &mut AttributeReadWork::default(),
    )
    .unwrap();
    assert_eq!(
        (parsed.mode, parsed.mtime_seconds, parsed.mtime_nanoseconds),
        (0o755, -1, 17)
    );
}
