use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_server::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::{
    io::Cursor,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
#[allow(dead_code)]
#[path = "support/file_save.rs"]
mod file_save;
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
        "layerfs-server-{}-{}-{}",
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
        response_bytes: if matches!(operation, Operation::SaveFile { .. }) {
            0
        } else {
            MAX_FILE
        },
        operation,
    }
}
#[test]
fn construct_read_edit_inspect_and_reopen() {
    let (temp, service, peer) = fixture();
    for (index, length) in [0, 131071, 131072, 131073, 500000].into_iter().enumerate() {
        let bytes: Vec<u8> = (0..length).map(|n| (n % 251) as u8).collect();
        let request = request(index as u64 + 1, file_save::fresh(bytes.len() as u64));
        let saved = service
            .handle(
                &peer,
                &request,
                &mut Cursor::new(file_save::fresh_body(&bytes)),
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
                    file_save::existing(root, length, length + 3, u64::from(length > 0) + 1, 3),
                ),
                &mut Cursor::new(if length == 0 {
                    file_save::body(&[(1, 0, 3)], b"new")
                } else {
                    file_save::body(&[(1, 0, 3), (0, 0, length)], b"new")
                }),
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
fn ten_mib_final_replacement_saves_and_keeps_the_previous_root() {
    let (_temp, service, peer) = fixture();
    let old: Vec<u8> = (0..10 * 1024 * 1024).map(|n| (n % 251) as u8).collect();
    let Response::Saved { root: previous, .. } = service
        .handle(
            &peer,
            &request(1, file_save::fresh(old.len() as u64)),
            &mut Cursor::new(file_save::fresh_body(&old)),
            &mut std::io::sink(),
        )
        .0
        .unwrap()
    else {
        panic!("base file")
    };
    let mut final_bytes = vec![37u8; 4096];
    final_bytes.extend_from_slice(&old);
    let Response::Saved {
        root: current,
        length,
        ..
    } = service
        .handle(
            &peer,
            &request(
                2,
                file_save::existing(
                    previous,
                    old.len() as u64,
                    final_bytes.len() as u64,
                    1,
                    final_bytes.len() as u64,
                ),
            ),
            &mut Cursor::new(file_save::body(
                &[(1, 0, final_bytes.len() as u64)],
                &final_bytes,
            )),
            &mut std::io::sink(),
        )
        .0
        .unwrap()
    else {
        panic!("saved replacement")
    };
    assert_eq!(length, final_bytes.len() as u64);
    for (root, expected) in [(previous, &old), (current, &final_bytes)] {
        let mut observed = Vec::new();
        service
            .handle(
                &peer,
                &request(
                    3,
                    Operation::ReadFile {
                        root,
                        start: 0,
                        end: expected.len() as u64,
                    },
                ),
                &mut std::io::empty(),
                &mut observed,
            )
            .0
            .unwrap();
        assert_eq!(&observed, expected);
    }
}

#[test]
fn final_extents_match_the_sealed_reference_root_and_partition() {
    use layerfs_content::{
        file::mapping::{decode_file_state, decode_node_with_context, ExtentNode},
        AuthenticatedObjects, ObjectId,
    };
    // Independent reference: docs/roadmap/0.1/0.1.7/evidence/
    // stages-3-4-oracle-20260916T222738Z-corrected/join-80-100.json.
    fn noise(length: usize) -> Vec<u8> {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        (0..length)
            .map(|_| {
                state ^= state.wrapping_shl(7);
                state ^= state.wrapping_shr(9);
                state ^= state.wrapping_shl(8);
                state as u8
            })
            .collect()
    }
    let (_temp, service, peer) = fixture();
    let left = 1_479_513u64;
    let mut base = noise(left as usize);
    base.extend(noise(3_356_500 - left as usize));
    let Response::Saved { root: old, .. } = service
        .handle(
            &peer,
            &request(1, file_save::fresh(base.len() as u64)),
            &mut Cursor::new(file_save::fresh_body(&base)),
            &mut std::io::sink(),
        )
        .0
        .unwrap()
    else {
        panic!("base")
    };
    assert_eq!(
        hex(&old),
        "42670567953f6fec19b076d8ce181b1362cda277cee7526dffe34f18f977e58b"
    );
    let replacement = noise(200_000);
    let final_len = base.len() as u64 + replacement.len() as u64;
    let Response::Saved { root, length, .. } = service
        .handle(
            &peer,
            &request(
                2,
                file_save::existing(
                    old,
                    base.len() as u64,
                    final_len,
                    3,
                    replacement.len() as u64,
                ),
            ),
            &mut Cursor::new(file_save::body(
                &[
                    (0, 0, left),
                    (1, 0, 200_000),
                    (0, left, base.len() as u64 - left),
                ],
                &replacement,
            )),
            &mut std::io::sink(),
        )
        .0
        .unwrap()
    else {
        panic!("saved")
    };
    assert_eq!(length, final_len);
    assert_eq!(
        hex(&root),
        "333d54974516c788dc731ee2e7c7522d0ff56a6982889b66e3caef5d469c310b"
    );
    let store = Timing::disabled("open", |scope| {
        Store::open(_temp.0.join("store.sqlite"), scope.child("store"))
    })
    .0
    .unwrap();
    let provider = layerfs_storage::StoreProvider::new(&store);
    let state = decode_file_state(
        &provider
            .read_canonical(ObjectId::from_bytes(&root).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        format!("{}", state.mapping_root),
        "94f0c9bd755938ca564036ef70fdf2a7991a9693230fe16bfd28e185f2a6c1b1"
    );
    let ExtentNode::Branch { children, .. } =
        decode_node_with_context(&provider.read_canonical(state.mapping_root).unwrap(), true)
            .unwrap()
    else {
        panic!("branch mapping")
    };
    assert_eq!(children.len(), 2);
    assert_eq!(
        format!("{}", children[0].child_object_id),
        "8940b1613783642d47e128642d5716c1beda63b16f9a80ccd25a89d5a8dfc6af"
    );
    assert_eq!(
        format!("{}", children[1].child_object_id),
        "414e45e5f85ba76572ba598c604f258f58a78211b99e859e8bcd707edcaf79c4"
    );
}

#[test]
fn final_file_stream_handles_zero_delete_noop_and_malformed_input() {
    let (_temp, service, peer) = fixture();
    let Response::Saved { root: base, .. } = service
        .handle(
            &peer,
            &request(1, file_save::fresh(5)),
            &mut Cursor::new(file_save::fresh_body(b"abcde")),
            &mut std::io::sink(),
        )
        .0
        .unwrap()
    else {
        panic!("base")
    };
    let save = |id, length, extents, replacement, body: Vec<u8>| {
        service
            .handle(
                &peer,
                &request(
                    id,
                    file_save::existing(base, 5, length, extents, replacement),
                ),
                &mut Cursor::new(body),
                &mut std::io::sink(),
            )
            .0
    };
    let Response::Saved {
        root: unchanged, ..
    } = save(2, 5, 1, 0, file_save::body(&[(0, 0, 5)], b"")).unwrap()
    else {
        panic!("no-op")
    };
    assert_eq!(unchanged, base);
    let Response::Saved { root: deleted, .. } =
        save(3, 3, 2, 0, file_save::body(&[(0, 0, 2), (0, 4, 1)], b"")).unwrap()
    else {
        panic!("delete")
    };
    let Response::Saved { root: zeroed, .. } = save(
        4,
        6,
        3,
        3,
        file_save::body(&[(0, 0, 2), (2, 0, 3), (0, 4, 1)], &[0; 3]),
    )
    .unwrap() else {
        panic!("zero")
    };
    for (root, wanted) in [
        (base, b"abcde".as_slice()),
        (deleted, b"abe"),
        (zeroed, b"ab\0\0\0e"),
    ] {
        let mut observed = Vec::new();
        service
            .handle(
                &peer,
                &request(
                    5,
                    Operation::ReadFile {
                        root,
                        start: 0,
                        end: wanted.len() as u64,
                    },
                ),
                &mut std::io::empty(),
                &mut observed,
            )
            .0
            .unwrap();
        assert_eq!(observed, wanted);
    }
    for (extents, replacement, body) in [
        (2, 0, file_save::body(&[(0, 2, 2), (0, 1, 1)], b"")),
        (2, 0, file_save::body(&[(0, 0, 2)], b"")),
        (1, 5, file_save::body(&[(1, 0, 5)], b"four")),
        (
            3,
            3,
            file_save::body(&[(0, 0, 2), (2, 0, 3), (0, 4, 1)], b"bad"),
        ),
    ] {
        assert!(save(6, 5, extents, replacement, body).is_err());
    }
}

#[test]
#[ignore = "already passed once; owner directed no further 4097 runs"]
fn authenticated_generic_save_accepts_4097_separated_final_runs() {
    use layerfs_bridge::adapters::native::{
        client::Client,
        connection::{accept, connect, Peer},
        server::serve,
    };
    let (_temp, service, peer) = fixture();
    let base: Vec<u8> = (0..8_194).map(|i| (i % 251) as u8).collect();
    let Response::Saved { root: old, .. } = service
        .handle(
            &peer,
            &request(1, file_save::fresh(base.len() as u64)),
            &mut Cursor::new(file_save::fresh_body(&base)),
            &mut std::io::sink(),
        )
        .0
        .unwrap()
    else {
        panic!("base")
    };
    let mut extents = Vec::new();
    let mut expected = base.clone();
    for i in 0..4_097u64 {
        extents.push((1, 0, 1));
        extents.push((0, i * 2 + 1, 1));
        expected[(i * 2) as usize] = b'X';
    }
    let body = file_save::body(&extents, &vec![b'X'; 4_097]);
    let listener =
        layerfs_bridge::adapters::native::listen("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let server_public = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
    std::thread::scope(|threads| {
        let server = threads.spawn(|| {
            let (socket, _) = listener.accept().unwrap();
            let connection = accept(
                socket,
                &[9; 32],
                &[Peer {
                    selector: 1,
                    public: *peer.public_key(),
                    expires_unix: u64::MAX,
                }],
            )
            .unwrap();
            let _ = serve(connection, |caller, r, input, out, deadline| {
                service.handle_until(caller, r, input, out, deadline).0
            });
        });
        let mut client =
            Client::new(connect(address, 1, &[7; 32], &server_public).unwrap()).unwrap();
        let mut source = body.as_slice();
        let response = client
            .call(
                &request(
                    2,
                    file_save::existing(
                        old,
                        base.len() as u64,
                        base.len() as u64,
                        extents.len() as u64,
                        4_097,
                    ),
                ),
                &mut source,
                &mut std::io::sink(),
            )
            .unwrap();
        let Response::Saved { root: saved, .. } = response else {
            panic!("saved")
        };
        drop(client);
        server.join().unwrap();
        for (root, wanted) in [(old, &base), (saved, &expected)] {
            let mut observed = Vec::new();
            service
                .handle(
                    &peer,
                    &request(
                        3,
                        Operation::ReadFile {
                            root,
                            start: 0,
                            end: wanted.len() as u64,
                        },
                    ),
                    &mut std::io::empty(),
                    &mut observed,
                )
                .0
                .unwrap();
            assert_eq!(&observed, wanted);
        }
    });
}

/// The frozen generic save is a *replay*: the service validates and spools the
/// declared stream, then C1 consumes it through the ordered sequence/source
/// interface. This case drives many separated final runs whose replacement
/// bytes exceed the fixed 64 KiB in-memory window, so both spools are files, and
/// checks the exact bytes of the new root and of the root it replaced.
#[test]
fn a_spooled_final_stream_replays_many_runs_with_exact_bytes() {
    const RUNS: u64 = 2_049;
    const CHUNK: u64 = 64;
    const STRIDE: u64 = 2 * CHUNK;
    let (_temp, service, peer) = fixture();
    let length = RUNS * STRIDE;
    let base: Vec<u8> = (0..length).map(|i| (i % 251) as u8).collect();
    let Response::Saved { root: old, .. } = service
        .handle(
            &peer,
            &request(1, file_save::fresh(length)),
            &mut Cursor::new(file_save::fresh_body(&base)),
            &mut std::io::sink(),
        )
        .0
        .unwrap()
    else {
        panic!("base")
    };
    // One replacement run per stride: the retained canonical span between two
    // runs is what closes each run, so the stream carries exactly RUNS of them.
    let mut extents = Vec::new();
    let mut replacement = Vec::new();
    let mut expected = base.clone();
    for index in 0..RUNS {
        let at = index * STRIDE;
        let bytes: Vec<u8> = (0..CHUNK)
            .map(|offset| (index as u8).wrapping_mul(7).wrapping_add(offset as u8 + 1))
            .collect();
        extents.push((1, 0, CHUNK));
        extents.push((0, at + CHUNK, CHUNK));
        expected[at as usize..(at + CHUNK) as usize].copy_from_slice(&bytes);
        replacement.extend_from_slice(&bytes);
    }
    let replacement_bytes = replacement.len() as u64;
    assert!(
        replacement_bytes > 64 * 1024,
        "the case must exceed the in-memory window so the spool is a file"
    );
    let Response::Saved {
        root: saved,
        length: saved_length,
        ..
    } = service
        .handle(
            &peer,
            &request(
                2,
                file_save::existing(old, length, length, extents.len() as u64, replacement_bytes),
            ),
            &mut Cursor::new(file_save::body(&extents, &replacement)),
            &mut std::io::sink(),
        )
        .0
        .unwrap()
    else {
        panic!("saved")
    };
    assert_eq!(saved_length, length);
    for (root, wanted) in [(old, &base), (saved, &expected)] {
        let mut observed = Vec::new();
        assert_eq!(
            service
                .handle(
                    &peer,
                    &request(
                        3,
                        Operation::ReadFile {
                            root,
                            start: 0,
                            end: length
                        }
                    ),
                    &mut std::io::empty(),
                    &mut observed
                )
                .0
                .unwrap(),
            Response::Read { length }
        );
        assert_eq!(&observed, wanted);
    }
    println!(
        "FINAL_STREAM_REPLAY runs={RUNS} extents={} replacement={replacement_bytes} spooled=file",
        extents.len()
    );
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut result, "{byte:02x}").unwrap();
    }
    result
}
#[test]
fn denial_partial_and_invalid_inputs_never_succeed() {
    let (_temp, service, peer) = fixture();
    let r = request(1, file_save::fresh(4));
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
                &mut Cursor::new(file_save::fresh_body(b"good")),
                &mut std::io::sink()
            )
            .0
            .unwrap_err()
            .code,
        Code::Denied
    );
    assert!(service
        .handle(
            &peer,
            &r,
            &mut Cursor::new(file_save::fresh_body(b"good")),
            &mut std::io::sink()
        )
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
    let body = file_save::fresh_body(b"hello");
    let mut source = body.as_slice();
    let response = client
        .call(
            &request(1, file_save::fresh(5)),
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
    let r = request(1, file_save::fresh(0));
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
    let r = request(1, file_save::fresh(payload.len() as u64));
    let (ready_tx, ready_rx) = mpsc::channel();
    let (a_tx, a_rx) = mpsc::channel();
    let (b_tx, b_rx) = mpsc::channel();
    std::thread::scope(|threads| {
        let mut a = Paused {
            announced: Some(ready_tx.clone()),
            release: a_rx,
            bytes: Cursor::new(file_save::fresh_body(&payload)),
        };
        let mut b = Paused {
            announced: Some(ready_tx),
            release: b_rx,
            bytes: Cursor::new(file_save::fresh_body(&payload)),
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
            .handle(
                &peer,
                &r,
                &mut Cursor::new(file_save::fresh_body(&payload)),
                &mut std::io::sink(),
            )
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
