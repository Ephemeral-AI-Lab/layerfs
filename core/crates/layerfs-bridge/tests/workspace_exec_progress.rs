#![cfg(feature = "native")]
use layerfs_bridge::{
    adapters::native::{
        client::Client,
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
        server::serve,
    },
    contract::{Code, Operation, Request, Response, WorkspaceExecWire, WORKSPACE_STATUS_PROFILE},
};

#[test]
fn exec_progress_requires_a_bounded_marker_and_a_terminal_result() {
    for marker in [0u8, 1] {
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let client_private = [7; 32];
        let server_private = [9; 32];
        let client_public = *VerifiedPeer::from_private(&client_private)
            .unwrap()
            .public_key();
        let server_public = *VerifiedPeer::from_private(&server_private)
            .unwrap()
            .public_key();
        std::thread::scope(|threads| {
            let server = threads.spawn(|| {
                let (socket, _) = listener.accept().unwrap();
                let connection = accept(
                    socket,
                    &server_private,
                    &[Peer {
                        selector: 1,
                        public: client_public,
                        expires_unix: u64::MAX,
                    }],
                )
                .unwrap();
                let _ = serve(connection, |_, request, input, output, _| {
                    let mut body = Vec::new();
                    input.read_to_end(&mut body)?;
                    assert!(body.is_empty());
                    assert_eq!(request.response_bytes, 5);
                    output.write_all(&[marker])?;
                    output.flush()?;
                    Ok(Response::WorkspaceExec(Box::new(WorkspaceExecWire {
                        workspace: b"work".to_vec(),
                        incarnation: [4; 32],
                        exit_status: Some(0),
                        stdout: b"done".to_vec(),
                        stderr: vec![],
                        stdout_truncated: false,
                        stderr_truncated: false,
                    })))
                });
            });
            let mut client =
                Client::new(connect(address, 1, &client_private, &server_public).unwrap()).unwrap();
            let request = Request {
                id: 1,
                generation: 0,
                store: 0,
                profile: WORKSPACE_STATUS_PROFILE,
                deadline_ms: 5_000,
                response_bytes: 5,
                operation: Operation::WorkspaceExec {
                    workspace: b"work".to_vec(),
                    incarnation: [4; 32],
                    command: b"printf done".to_vec(),
                },
            };
            let mut output = Vec::new();
            let result = client.call(&request, &mut &[][..], &mut output);
            assert!(output.is_empty());
            if marker == 0 {
                assert!(matches!(result, Ok(Response::WorkspaceExec(_))));
            } else {
                assert_eq!(result.unwrap_err().code, Code::Unknown);
            }
            drop(client);
            server.join().unwrap();
        });
    }
}
