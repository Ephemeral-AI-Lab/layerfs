use layerfs_bridge::{
    adapters::native::{
        client::Client,
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
        protocol::*,
    },
    contract::*,
};

#[test]
fn mutation_result_data_is_rejected_before_any_output() {
    for history in [false, true] {
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener.local_addr().unwrap();
        let ck = [1; 32];
        let sk = [2; 32];
        let sp = *VerifiedPeer::from_private(&sk).unwrap().public_key();
        let peers = [Peer {
            selector: 1,
            public: *VerifiedPeer::from_private(&ck).unwrap().public_key(),
            expires_unix: u64::MAX,
        }];
        std::thread::scope(|threads| {
            let server = threads.spawn(|| {
                let (socket, _) = listener.accept().unwrap();
                let mut c = accept(socket, &sk, &peers).unwrap();
                let hello = c.receive.read().unwrap();
                c.send.write(&hello).unwrap();
                assert_eq!(c.receive.read().unwrap().kind, Kind::Begin);
                assert_eq!(c.receive.read().unwrap().kind, Kind::EndInput);
                c.send
                    .write(&Frame {
                        kind: Kind::ResultData,
                        id: 1,
                        bytes: b"unexpected".to_vec(),
                    })
                    .unwrap();
                let _ = c.send.write(&Frame {
                    kind: Kind::Failure,
                    id: 1,
                    bytes: encode_failure(Code::InvalidInput.into()).to_vec(),
                });
            });
            let mut client = Client::new(connect(addr, 1, &ck, &sp).unwrap()).unwrap();
            let mut input: &[u8] = &[];
            let mut output = Vec::new();
            let request = Request {
                id: 1,
                generation: 1,
                store: 1,
                profile: if history { 2 } else { 1 },
                deadline_ms: 10000,
                response_bytes: if history { 1024 } else { 0 },
                operation: if history {
                    Operation::HistoryCommand(HistoryCommand::ReserveInodes {
                        scope: [1; 32],
                        count: 1,
                    })
                } else {
                    Operation::SaveFile {
                        base: None,
                        base_length: 0,
                        length: 0,
                        extents: 0,
                        replacement: 0,
                    }
                },
            };
            let error = client.call(&request, &mut input, &mut output).unwrap_err();
            println!(
                "legacy mutation output={:?}, failure={error:?}",
                String::from_utf8_lossy(&output)
            );
            assert!(output.is_empty());
            assert_eq!(error.code, Code::Unknown);
            assert!(error.unknown);
            server.join().unwrap();
        });
    }
}
