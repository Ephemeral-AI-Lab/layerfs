use layerfs_bridge::{
    adapters::native::protocol::{
        decode_request, decode_response, encode_request, encode_response,
    },
    contract::{
        Operation, Request, Response, SandboxHelloWire, WorkspaceExecWire, WORKSPACE_STATUS_PROFILE,
    },
};

#[test]
fn selected_mount_exec_and_identity_round_trip() {
    let mut project = [3; 17];
    project[0] = 0x31;
    let mut branch = [4; 17];
    branch[0] = 0x11;
    let mut commit = [5; 33];
    commit[0] = 0x12;
    for operation in [
        Operation::WorkspaceOpen {
            workspace: b"work".to_vec(),
            incarnation: [9; 32],
            instance: [8; 32],
            project,
            branch,
            commit: Some(commit),
        },
        Operation::WorkspaceExec {
            workspace: b"work".to_vec(),
            incarnation: [9; 32],
            command: b"printf hello".to_vec(),
        },
        Operation::SandboxHello,
    ] {
        let request = Request {
            id: 7,
            generation: 0,
            store: 0,
            profile: WORKSPACE_STATUS_PROFILE,
            deadline_ms: 5_000,
            response_bytes: if matches!(&operation, Operation::WorkspaceExec { .. }) {
                5
            } else {
                0
            },
            operation,
        };
        assert_eq!(
            decode_request(7, &encode_request(&request).unwrap()).unwrap(),
            request
        );
    }
    for response in [
        Response::SandboxHello(SandboxHelloWire {
            sandbox: [1; 16],
            instance: [2; 32],
        }),
        Response::WorkspaceExec(Box::new(WorkspaceExecWire {
            workspace: b"work".to_vec(),
            incarnation: [9; 32],
            exit_status: Some(3),
            stdout: b"hello".to_vec(),
            stderr: vec![],
            stdout_truncated: false,
            stderr_truncated: false,
        })),
    ] {
        assert_eq!(
            decode_response(&encode_response(&response).unwrap()).unwrap(),
            response
        );
    }
}
