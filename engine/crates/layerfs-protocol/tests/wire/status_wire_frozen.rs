//! The Status record keeps the bytes the 2.0.0 runtimes' daemons send
//! (`layerfs_v2_0_0` at 7d2ac982c3). A CLI and a Sandbox daemon of
//! different builds meet whenever a home switches runtime while Sandboxes
//! run: a field added in the middle of this record made every Status
//! exchange with an older daemon unreadable (RF re-check N-A). New
//! Workspace facts travel elsewhere (the operator's changed-path answer);
//! this test fails when the record's layout moves.
use layerfs_content::ObjectId;
use layerfs_history::{BranchId, BranchRecord, BranchSnapshot, CommitId, HistoryName, LayerId};
use layerfs_history::{LayerStackId, WorkspaceId};
use layerfs_protocol::daemon::*;

fn status(local: bool) -> WorkspaceStatus {
    let root = ObjectId::for_bytes(b"control root");
    WorkspaceStatus {
        token: WorkspaceToken {
            workspace: WorkspaceId::from_authority([3; 32]).unwrap(),
            namespace: 7,
        },
        binding: BranchSnapshot {
            branch: BranchRecord {
                forked_from: None,
                id: BranchId::from_authority([1; 16]),
                stack: LayerStackId::from_authority([2; 16]),
                name: HistoryName::new("agent").unwrap(),
                base_layer: LayerId::from_bytes([0x32; 33]).unwrap(),
                head_commit: Some(CommitId::from_bytes([0x12; 33]).unwrap()),
            },
            head_root: Some(root),
            base_root: root,
            effective_root: root,
            scope: ObjectId::for_bytes(b"scope"),
            profile: layerfs_content::filesystem::profile_id(),
        },
        activity: Activity::Committing,
        epoch: 19,
        epoch_saturated: false,
        published: None,
        local: local.then_some(LocalObservation {
            revision: 8,
            active: 3,
            captured: Some(2),
            captured_revision: None,
            base_root: root.to_bytes(),
            dirty_inodes: 11,
            dirty_directory_entries: 2,
            closed: false,
            base_readers: 1,
        }),
        local_failure: (!local).then(|| ControlRefusal {
            code: ControlCode::Failed,
            phase: "observe".into(),
            moved: None,
            published: None,
            detail: "engine row".into(),
        }),
        kernel: None,
    }
}

/// The record's bytes as a digest, and their length.
fn frozen(reply: Reply) -> (String, usize) {
    let bytes = Answer { id: 2, reply }.encode().unwrap();
    (ObjectId::for_bytes(&bytes).to_string(), bytes.len())
}

#[test]
fn the_status_record_keeps_the_2_0_0_runtime_bytes() {
    let observed = [
        frozen(Reply::Status(Box::new(status(true)))),
        frozen(Reply::Status(Box::new(status(false)))),
        // Located carries the same record under its own tag
        frozen(Reply::Located(Box::new(status(true)))),
    ];
    let expected = FROZEN.map(|(digest, len)| (digest.to_owned(), len));
    assert_eq!(observed, expected, "the Status record's layout moved");
}

/// Observed with a local observation, with an engine refusal, and Located.
const FROZEN: [(&str, usize); 3] = [
    (
        "a9b6d82abbd5e815b931afafcc3c39743344a743055f343b991d8d12c79979c8",
        419,
    ),
    (
        "48cd7025b905ca8066e02be423dc784e6fff52f2fa97504f71413e62e4d2017b",
        360,
    ),
    (
        "d75c427871c696461ce13b96a681668019b4406c14ff213a549d6b4c47f6b0cb",
        420,
    ),
];
