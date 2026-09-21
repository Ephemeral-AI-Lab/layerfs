use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_service::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::{
    io::Cursor,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn fixture() -> (Temp, Service, VerifiedPeer) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "layerfs-service-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    let store = Timing::disabled("create", |s| {
        Store::create(
            path.join("store.sqlite"),
            Store::default_policy(),
            s.child("create"),
        )
    })
    .0
    .unwrap();
    let peer = VerifiedPeer::from_private(&[7; 32]).unwrap();
    let service = Service::new(
        vec![StoreAccess {
            id: 1,
            store,
            history: None,
            grants: vec![Grant {
                public_key: *peer.public_key(),
                operations: 31,
                expires_unix: u64::MAX,
            }],
        }],
        OperationRecorder::disabled(),
    )
    .unwrap();
    (Temp(path), service, peer)
}
fn request(id: u64, operation: Operation) -> Request {
    Request {
        id,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 10000,
        response_bytes: MAX_FILE,
        operation,
    }
}
#[test]
fn construct_read_edit_inspect_and_reopen() {
    let (temp, service, peer) = fixture();
    for (index, length) in [0, 131071, 131072, 131073, 500000].into_iter().enumerate() {
        let bytes: Vec<u8> = (0..length).map(|n| (n % 251) as u8).collect();
        let request = request(
            index as u64 + 1,
            Operation::ConstructFile {
                length: bytes.len() as u64,
            },
        );
        let saved = service
            .handle(
                &peer,
                &request,
                &mut Cursor::new(&bytes),
                &mut std::io::sink(),
            )
            .0
            .unwrap();
        let Response::Saved { root, length, .. } = saved else {
            panic!("saved")
        };
        let mut output = Vec::new();
        assert_eq!(
            service
                .handle(
                    &peer,
                    &self::request(
                        10,
                        Operation::ReadFile {
                            root,
                            start: 0,
                            end: length
                        }
                    ),
                    &mut std::io::empty(),
                    &mut output
                )
                .0
                .unwrap(),
            Response::Read { length }
        );
        assert_eq!(output, bytes);
        assert!(
            matches!(service.handle(&peer,&self::request(11,Operation::Inspect{root,query:Inspect::File}),&mut std::io::empty(),&mut std::io::sink()).0.unwrap(),Response::File{length:l,..} if l==length)
        );
        let result = service
            .handle(
                &peer,
                &self::request(
                    12,
                    Operation::EditFile {
                        root,
                        base_length: length,
                        edits: vec![Edit {
                            start: 0,
                            end: 0,
                            replacement: 3,
                        }],
                    },
                ),
                &mut Cursor::new(b"new"),
                &mut std::io::sink(),
            )
            .0
            .unwrap();
        let Response::Saved {
            root: edited,
            length: edited_length,
            ..
        } = result
        else {
            panic!("saved")
        };
        let mut output = Vec::new();
        service
            .handle(
                &peer,
                &self::request(
                    13,
                    Operation::ReadFile {
                        root: edited,
                        start: 0,
                        end: edited_length,
                    },
                ),
                &mut std::io::empty(),
                &mut output,
            )
            .0
            .unwrap();
        assert_eq!(&output[..3], b"new");
        assert_eq!(&output[3..], bytes);
        let opened = Timing::disabled("open", |s| {
            Store::open(temp.0.join("store.sqlite"), s.child("open"))
        })
        .0
        .unwrap();
        let provider = layerfs_storage::StoreProvider::new(&opened);
        let mut old = Vec::new();
        Timing::disabled("old", |s| {
            layerfs_content::read_all(
                &provider,
                layerfs_content::ObjectId::from_bytes(&root).unwrap(),
                &mut old,
                s.child("read"),
            )
        })
        .0
        .unwrap();
        assert_eq!(old, bytes);
    }
}
#[test]
fn denial_partial_and_invalid_inputs_never_succeed() {
    let (_temp, service, peer) = fixture();
    let r = request(1, Operation::ConstructFile { length: 4 });
    assert!(service
        .handle(&peer, &r, &mut Cursor::new(b"bad"), &mut std::io::sink())
        .0
        .is_err());
    assert!(service
        .handle(&peer, &r, &mut Cursor::new(b"excess"), &mut std::io::sink())
        .0
        .is_err());
    let foreign = VerifiedPeer::from_private(&[8; 32]).unwrap();
    assert_eq!(
        service
            .handle(
                &foreign,
                &r,
                &mut Cursor::new(b"good"),
                &mut std::io::sink()
            )
            .0
            .unwrap_err()
            .code,
        Code::Denied
    );
    assert!(service
        .handle(&peer, &r, &mut Cursor::new(b"good"), &mut std::io::sink())
        .0
        .is_ok());
}

#[test]
fn authenticated_network_after_nonblocking_accept() {
    use layerfs_bridge::adapters::native::{
        client::Client,
        connection::{accept, connect, Peer},
        server::serve,
    };
    use std::thread;
    let (_temp, service, peer) = fixture();
    let listener =
        layerfs_bridge::adapters::native::listen("127.0.0.1:0".parse().unwrap()).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let server_public = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
    let handle = thread::spawn(move || {
        let (stream, _) = loop {
            match listener.accept() {
                Ok(s) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::yield_now(),
                Err(e) => panic!("{e}"),
            }
        };
        // Capture inherited native descriptor mode before the production adapter.
        use nix::fcntl::{fcntl, FcntlArg};
        eprintln!(
            "accepted descriptor flags {}",
            fcntl(&stream, FcntlArg::F_GETFL).unwrap()
        );
        let connection = accept(
            stream,
            &[9; 32],
            &[Peer {
                selector: 1,
                public: *peer.public_key(),
                expires_unix: u64::MAX,
            }],
        )
        .unwrap();
        let _ = serve(connection, |peer, r, input, out, deadline| {
            service.handle_until(peer, r, input, out, deadline).0
        });
    });
    let mut client = Client::new(connect(address, 1, &[7; 32], &server_public).unwrap()).unwrap();
    let mut source = b"hello".as_slice();
    let response = client
        .call(
            &request(1, Operation::ConstructFile { length: 5 }),
            &mut source,
            &mut std::io::sink(),
        )
        .unwrap();
    assert!(matches!(response, Response::Saved { length: 5, .. }));
    drop(client);
    handle.join().unwrap();
}

#[test]
fn inherited_deadline_expires_before_direct_input_or_mutation() {
    struct Unread;
    impl std::io::Read for Unread {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("expired operation consumed input")
        }
    }
    let (_temp, service, peer) = fixture();
    let r = request(1, Operation::ConstructFile { length: 0 });
    let failure = service
        .handle_until(
            &peer,
            &r,
            &mut Unread,
            &mut std::io::sink(),
            std::time::Instant::now(),
        )
        .0
        .unwrap_err();
    assert_eq!(failure.code, Code::Deadline);
    assert!(!failure.unknown);
    // Expiration cannot occupy the one currently supported operation slot.
    assert!(service
        .handle(&peer, &r, &mut Cursor::new([]), &mut std::io::sink())
        .0
        .is_ok());
}

#[test]
fn two_writers_overlap_and_capacity_is_reclaimed_after_reverse_completion() {
    use std::{io::Read, sync::mpsc, time::Duration};
    struct Paused {
        announced: Option<mpsc::Sender<()>>,
        release: mpsc::Receiver<()>,
        bytes: Cursor<Vec<u8>>,
    }
    impl Read for Paused {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if let Some(ready) = self.announced.take() {
                ready.send(()).map_err(std::io::Error::other)?;
                self.release
                    .recv_timeout(Duration::from_secs(2))
                    .map_err(std::io::Error::other)?;
            }
            self.bytes.read(out)
        }
    }
    let (_temp, service, peer) = fixture();
    let payload = vec![31; 4096];
    let r = request(
        1,
        Operation::ConstructFile {
            length: payload.len() as u64,
        },
    );
    let (ready_tx, ready_rx) = mpsc::channel();
    let (a_tx, a_rx) = mpsc::channel();
    let (b_tx, b_rx) = mpsc::channel();
    std::thread::scope(|threads| {
        let mut a = Paused {
            announced: Some(ready_tx.clone()),
            release: a_rx,
            bytes: Cursor::new(payload.clone()),
        };
        let mut b = Paused {
            announced: Some(ready_tx),
            release: b_rx,
            bytes: Cursor::new(payload.clone()),
        };
        let shared = &service;
        let request_ref = &r;
        let first = threads.spawn(move || {
            shared
                .handle(&peer, request_ref, &mut a, &mut std::io::sink())
                .0
        });
        let second = threads.spawn(move || {
            shared
                .handle(&peer, request_ref, &mut b, &mut std::io::sink())
                .0
        });
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let refused = service
            .handle(&peer, &r, &mut Cursor::new(&payload), &mut std::io::sink())
            .0
            .unwrap_err();
        assert_eq!(refused.code, Code::Capacity);
        b_tx.send(()).unwrap();
        let Response::Saved { root: b_root, .. } = second.join().unwrap().unwrap() else {
            panic!("saved")
        };
        let read = request(
            2,
            Operation::ReadFile {
                root: b_root,
                start: 0,
                end: payload.len() as u64,
            },
        );
        let mut output = Vec::new();
        service
            .handle(&peer, &read, &mut Cursor::new([]), &mut output)
            .0
            .unwrap();
        assert_eq!(output, payload, "B is readable while A still owns a save");
        a_tx.send(()).unwrap();
        let Response::Saved { root: a_root, .. } = first.join().unwrap().unwrap() else {
            panic!("saved")
        };
        assert_eq!(a_root, b_root);
    });
}
