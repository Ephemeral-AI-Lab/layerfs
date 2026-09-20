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
    let path = std::env::temp_dir().join(format!(
        "layerfs-service-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
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
    use std::{net::TcpListener, thread};
    let (_temp, service, peer) = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
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
        let _ = serve(connection, |peer, r, input, out| {
            service.handle(peer, r, input, out).0
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
