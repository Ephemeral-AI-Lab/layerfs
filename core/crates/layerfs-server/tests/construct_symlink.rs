//! Symlink object construction through public Service/C1/C2 interfaces.
use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_content::{filesystem::symlink::SymlinkTarget, object::AuthenticatedObjects, ObjectId};
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
            "layerfs-construct-symlink-{}-{}-{}",
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
        let grants = [(7, 4), (8, 31), (10, 128), (11, 64), (12, 255)]
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
fn request(target: &[u8]) -> Request {
    Request {
        id: 1,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 10_000,
        response_bytes: 0,
        operation: Operation::ConstructSymlink {
            target: target.to_vec(),
        },
    }
}
fn saved(response: Response, expected: &[u8]) -> (Root, u64, u64) {
    let Response::Saved {
        root,
        length,
        inserted,
        reused,
        ..
    } = response
    else {
        panic!("Saved result");
    };
    assert_eq!(length, expected.len() as u64);
    (root, inserted, reused)
}
fn verify(f: &Fixture, root: Root, target: &[u8]) {
    let expected = SymlinkTarget::new(target.to_vec()).unwrap();
    assert_eq!(expected.finalize().unwrap().id().as_bytes(), &root);
    let bytes = StoreProvider::new(&f.store)
        .read_canonical(ObjectId::from_bytes(&root).unwrap())
        .unwrap();
    assert_eq!(bytes, expected.encode().unwrap());
    assert_eq!(SymlinkTarget::decode(&bytes).unwrap().as_bytes(), target);
}

#[test]
fn constructor_without_history_preserves_exact_targets_and_canonical_reuse() {
    for native in [false, true] {
        let f = Fixture::new();
        for target in [
            b"../relative/file".to_vec(),
            b"/absolute/file".to_vec(),
            vec![0xff, b'/', 0x80],
            vec![],
            vec![b'x'; 4096],
        ] {
            let req = request(&target);
            let call = || {
                if native {
                    f.native(&req)
                } else {
                    f.call(7, &req, &mut std::io::empty())
                }
            };
            let (root, inserted, reused) = saved(call().unwrap(), &target);
            assert_eq!((inserted, reused), (1, 0));
            verify(&f, root, &target);
            let (again, inserted, reused) = saved(call().unwrap(), &target);
            assert_eq!(again, root);
            assert_eq!((inserted, reused), (0, 1));
        }
        // Grant4 suffices; legacy31 also carries it. No HistoryCatalog exists,
        // so this public operation has no inode/Branch/Stage allocation authority.
        for key in [8, 12] {
            let (root, _, _) = saved(
                f.call(key, &request(b"legacy"), &mut std::io::empty())
                    .unwrap(),
                b"legacy",
            );
            verify(&f, root, b"legacy");
        }
    }
}

#[test]
fn target_authority_body_and_deadline_refusals_do_not_begin_a_save() {
    let f = Fixture::new();
    let before = std::fs::read(f.path.join("store.sqlite")).unwrap();
    for key in [10, 11] {
        let error = f
            .call(key, &request(b"target"), &mut std::io::empty())
            .unwrap_err();
        assert_eq!(error.code, Code::Denied);
        assert!(!error.unknown);
        assert_eq!(error.cleanup, None);
    }
    for (target, code) in [
        (b"a\0b".to_vec(), Code::InvalidInput),
        (vec![b'x'; 4097], Code::Capacity),
    ] {
        let error = f
            .call(7, &request(&target), &mut std::io::empty())
            .unwrap_err();
        assert_eq!(error.code, code);
        assert!(!error.unknown);
        assert_eq!(error.cleanup, None);
    }
    assert_eq!(
        f.call(7, &request(b"target"), &mut Cursor::new([1]))
            .unwrap_err()
            .code,
        Code::InvalidInput
    );
    let mut req = request(b"target");
    req.response_bytes = 1;
    assert_eq!(
        f.call(7, &req, &mut std::io::empty()).unwrap_err().code,
        Code::InvalidInput
    );
    req = request(b"target");
    req.operation = Operation::ConstructPortableMetadata {
        kind: 3,
        mode: 0o777,
        mtime_seconds: 0,
        mtime_nanoseconds: 0,
    };
    assert_eq!(
        f.call(7, &req, &mut std::io::empty()).unwrap_err().code,
        Code::Denied
    );
    req.operation = Operation::HistoryQuery(HistoryQuery::GetBranch { branch: [0x11; 17] });
    req.profile = 2;
    assert_eq!(
        f.call(7, &req, &mut std::io::empty()).unwrap_err().code,
        Code::Denied
    );
    struct Unread;
    impl Read for Unread {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("expired request read input");
        }
    }
    let mut output = Vec::new();
    let error = f
        .service
        .handle_until(
            &VerifiedPeer::from_private(&[7; 32]).unwrap(),
            &request(b"target"),
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
        .call(7, &request(b"target"), &mut std::io::empty())
        .is_ok());
}

#[test]
fn occupied_save_refuses_definitely_and_explicit_owner_abort_restores_admission() {
    let f = Fixture::new();
    Timing::disabled("limit", |s| {
        f.store.set_max_concurrent_writes(1, s.child("configure"))
    })
    .0
    .unwrap();
    let held = Timing::disabled("held", |s| f.store.begin_save(s.child("begin")))
        .0
        .unwrap();
    let error = f
        .call(7, &request(b"../after-abort"), &mut std::io::empty())
        .unwrap_err();
    assert_eq!(error.code, Code::Ownership);
    assert!(!error.unknown);
    assert_eq!(error.cleanup, None);
    // A real occupied C2 slot, not an injected late constructor-save failure.
    Timing::disabled("abort", |s| held.abort(s.child("held-owner")))
        .0
        .unwrap();
    let (root, inserted, reused) = saved(
        f.native(&request(b"../after-abort")).unwrap(),
        b"../after-abort",
    );
    assert_eq!((inserted, reused), (1, 0));
    verify(&f, root, b"../after-abort");
}
