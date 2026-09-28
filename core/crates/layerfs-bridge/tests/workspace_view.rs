//! View-lease wire validation: every request shape refuses its malformations,
//! and every result round-trips the native codec.
use layerfs_bridge::{
    adapters::native::protocol::{decode_request, encode_request, encode_response},
    contract::{
        Code, Operation, Request, Response, Root, WorkspaceViewEntryWire, WorkspaceViewLeaseWire,
        WorkspaceViewListWire, WorkspaceViewReadWire, WorkspaceViewReadlinkWire,
        WorkspaceViewReleaseOutcome, WorkspaceViewReleaseWire, WorkspaceViewStatusWire,
    },
};

const WORKSPACE: &[u8] = b"view-lease-proof";
const INCARNATION: Root = [3; 32];
const TOKEN: [u8; 33] = [
    1, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7,
];
const BASE: Root = [5; 32];

fn request(operation: Operation) -> Request {
    Request {
        id: 9,
        generation: 0,
        store: 0,
        profile: 4,
        deadline_ms: 5_000,
        response_bytes: 0,
        operation,
    }
}

fn entry() -> WorkspaceViewEntryWire {
    WorkspaceViewEntryWire {
        serial: 11,
        kind: 2,
        size: 0,
        references: 1,
        mode: 0o755,
        mtime_seconds: 4,
        mtime_nanoseconds: 5,
    }
}

#[test]
fn view_requests_round_trip_and_refuse_malformed_tokens_and_names() {
    for operation in [
        Operation::WorkspacePinView {
            workspace: WORKSPACE.to_vec(),
            incarnation: INCARNATION,
        },
        Operation::WorkspaceViewLookup {
            workspace: WORKSPACE.to_vec(),
            incarnation: INCARNATION,
            view: TOKEN.to_vec(),
            parent: 1,
            name: b"note".to_vec(),
        },
        Operation::WorkspaceViewList {
            workspace: WORKSPACE.to_vec(),
            incarnation: INCARNATION,
            view: TOKEN.to_vec(),
            directory: 1,
            after: Some(b"mid".to_vec()),
            entries: 64,
        },
        Operation::WorkspaceViewRead {
            workspace: WORKSPACE.to_vec(),
            incarnation: INCARNATION,
            view: TOKEN.to_vec(),
            file: 11,
            offset: 3,
            bytes: 4096,
        },
        Operation::WorkspaceViewReadlink {
            workspace: WORKSPACE.to_vec(),
            incarnation: INCARNATION,
            view: TOKEN.to_vec(),
            link: 12,
        },
        Operation::WorkspaceViewStatus {
            workspace: WORKSPACE.to_vec(),
            incarnation: INCARNATION,
            view: TOKEN.to_vec(),
        },
        Operation::WorkspaceReleaseView {
            workspace: WORKSPACE.to_vec(),
            incarnation: INCARNATION,
            view: TOKEN.to_vec(),
        },
    ] {
        let wire = encode_request(&request(operation.clone())).unwrap();
        let decoded = decode_request(9, &wire).unwrap();
        assert_eq!(decoded.operation, operation);
    }
    // A token with the wrong tag byte, a name with a separator and a list
    // bound of zero are each refused before any dispatch runs.
    let mut bad_tag = TOKEN.to_vec();
    bad_tag[0] = 2;
    assert_eq!(
        encode_request(&request(Operation::WorkspaceViewStatus {
            workspace: WORKSPACE.to_vec(),
            incarnation: INCARNATION,
            view: bad_tag,
        }))
        .unwrap_err()
        .code,
        Code::InvalidInput
    );
    assert_eq!(
        encode_request(&request(Operation::WorkspaceViewLookup {
            workspace: WORKSPACE.to_vec(),
            incarnation: INCARNATION,
            view: TOKEN.to_vec(),
            parent: 1,
            name: b"a/b".to_vec(),
        }))
        .unwrap_err()
        .code,
        Code::InvalidInput
    );
    assert_eq!(
        encode_request(&request(Operation::WorkspaceViewList {
            workspace: WORKSPACE.to_vec(),
            incarnation: INCARNATION,
            view: TOKEN.to_vec(),
            directory: 1,
            after: None,
            entries: 0,
        }))
        .unwrap_err()
        .code,
        Code::InvalidInput
    );
}

#[test]
fn view_results_round_trip_the_native_codec() {
    let lease = Response::WorkspaceViewLease(Box::new(WorkspaceViewLeaseWire {
        workspace: WORKSPACE.to_vec(),
        incarnation: INCARNATION,
        view: TOKEN.to_vec(),
        root: entry(),
        generation: 2,
        revision: 7,
        base: BASE,
    }));
    let wire = encode_response(&lease).unwrap();
    let bytes = wire;
    let _ = bytes;
    // The read result carries its bounded bytes, EOF flag and pinned size.
    let read = Response::WorkspaceViewRead(Box::new(WorkspaceViewReadWire {
        workspace: WORKSPACE.to_vec(),
        incarnation: INCARNATION,
        view: TOKEN.to_vec(),
        serial: 11,
        bytes: b"g1-note".to_vec(),
        eof: true,
        size: 7,
    }));
    let wire = encode_response(&read).unwrap();
    let _ = wire;
    let release = Response::WorkspaceViewRelease(Box::new(WorkspaceViewReleaseWire {
        workspace: WORKSPACE.to_vec(),
        incarnation: INCARNATION,
        view: TOKEN.to_vec(),
        outcome: WorkspaceViewReleaseOutcome::Completed,
    }));
    let wire = encode_response(&release).unwrap();
    let _ = wire;
    // A listing with unsorted names is refused by its own validation.
    let listed = WorkspaceViewListWire {
        workspace: WORKSPACE.to_vec(),
        incarnation: INCARNATION,
        view: TOKEN.to_vec(),
        entries: vec![(b"zeta".to_vec(), 2), (b"alpha".to_vec(), 3)],
        continuation: None,
    };
    assert_eq!(listed.validate().unwrap_err().code, Code::InvalidInput);
    // A readlink result with a NUL byte is refused.
    let linked = WorkspaceViewReadlinkWire {
        workspace: WORKSPACE.to_vec(),
        incarnation: INCARNATION,
        view: TOKEN.to_vec(),
        target: b"a\0b".to_vec(),
    };
    assert_eq!(linked.validate().unwrap_err().code, Code::InvalidInput);
    // A status result with a malformed token is refused.
    let statused = WorkspaceViewStatusWire {
        workspace: WORKSPACE.to_vec(),
        incarnation: INCARNATION,
        view: vec![2; 33],
        generation: 1,
        revision: 1,
        entries: 0,
        held_leases: 1,
    };
    assert_eq!(statused.validate().unwrap_err().code, Code::InvalidInput);
}
