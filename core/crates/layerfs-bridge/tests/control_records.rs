//! Bounded public control records preserve exact identities and failure knowledge.
use layerfs_bridge::control::*;
use layerfs_content::ObjectId;
use layerfs_history::{
    error::MovedState, BranchId, BranchRecord, BranchSnapshot, CommitHistoryRequest, CommitId,
    CommitRecord, CommitStagedOutcome, ForkRequest, ForkSource, HistoryName, LayerId, LayerStackId,
    PageResult, WorkspaceId,
};
fn binding() -> BranchSnapshot {
    let root = ObjectId::for_bytes(b"control root");
    BranchSnapshot {
        branch: BranchRecord {
            id: BranchId::from_authority([1; 16]),
            stack: LayerStackId::from_authority([2; 16]),
            name: HistoryName::new("main").unwrap(),
            base_layer: LayerId::from_bytes([0x32; 33]).unwrap(),
            head_commit: Some(CommitId::from_bytes([0x12; 33]).unwrap()),
        },
        head_root: Some(root),
        base_root: root,
        effective_root: root,
        scope: ObjectId::for_bytes(b"scope"),
        profile: layerfs_content::filesystem::profile_id(),
    }
}
fn token() -> WorkspaceToken {
    WorkspaceToken {
        workspace: WorkspaceId::from_authority([3; 32]).unwrap(),
        namespace: 7,
    }
}
fn record() -> CommitRecord {
    let value = binding();
    CommitRecord {
        id: value.branch.head_commit.unwrap(),
        stack: value.branch.stack,
        root: value.effective_root,
        parent: None,
        base_layer: value.branch.base_layer,
    }
}
#[test]
fn commands_are_bounded_exact_and_reject_every_truncated_prefix() {
    let binding = binding();
    let token = token();
    let commands = [
        Request::Mount {
            workspace: token.workspace,
            branch: binding.branch.id,
        },
        Request::Commit(token),
        Request::Status(token),
        Request::Unmount(token),
        Request::Fork(ForkRequest {
            stack: binding.branch.stack,
            branch: BranchId::from_authority([4; 16]),
            name: HistoryName::new("fork").unwrap(),
            source: ForkSource::Commit {
                branch: binding.branch.id,
                commit: record().id,
            },
        }),
        Request::History(CommitHistoryRequest {
            branch: binding.branch.id,
            start: None,
            cursor: Some(vec![5; 160]),
            limit: 1,
        }),
    ];
    for (index, request) in commands.into_iter().enumerate() {
        let call = Call {
            id: index as u64 + 1,
            request,
        };
        let bytes = call.encode().unwrap();
        assert_eq!(Call::decode(&bytes).unwrap(), call);
        assert!(bytes.len() < 8192);
        for end in 0..bytes.len() {
            assert!(Call::decode(&bytes[..end]).is_err());
        }
        let mut extra = bytes;
        extra.push(0);
        assert!(Call::decode(&extra).is_err());
    }
    assert!(Call {
        id: 1,
        request: Request::History(CommitHistoryRequest {
            branch: binding.branch.id,
            start: None,
            cursor: None,
            limit: HISTORY_WINDOW + 1
        })
    }
    .encode()
    .is_err());
    assert!(Call {
        id: 1,
        request: Request::Status(WorkspaceToken {
            namespace: 0,
            ..token
        })
    }
    .encode()
    .is_err());
}
#[test]
fn replies_preserve_scoped_status_conflict_and_known_publication() {
    let binding = binding();
    let token = token();
    let outcome = CommitStagedOutcome::Committed(record());
    let replies = [
        Reply::Bound {
            token,
            binding: binding.clone(),
        },
        Reply::Committed(outcome.clone()),
        Reply::Status(Box::new(WorkspaceStatus {
            token,
            binding: binding.clone(),
            activity: Activity::LocalFailure,
            epoch: 19,
            epoch_saturated: false,
            published: Some(outcome.clone()),
            local: Some(LocalObservation {
                revision: 8,
                active: 3,
                captured: Some(2),
                captured_revision: Some(7),
                base_root: binding.effective_root.to_bytes(),
                dirty_inodes: 11,
                dirty_directory_entries: 2,
                closed: false,
                base_readers: 1,
            }),
            local_failure: None,
        })),
        Reply::Unmounted(token),
        Reply::Forked(binding.clone()),
        Reply::History(PageResult {
            records: vec![record(); HISTORY_WINDOW as usize],
            continuation: Some(vec![8; 160]),
        }),
        Reply::Refused(ControlRefusal {
            code: ControlCode::HeadMoved,
            phase: "Commit Publish".into(),
            moved: Some(MovedState {
                expected_head: None,
                actual_head: binding.branch.head_commit,
                expected_base: binding.branch.base_layer,
                actual_base: binding.branch.base_layer,
            }),
            published: None,
            detail: "original deciding conflict".into(),
        }),
        Reply::Refused(ControlRefusal {
            code: ControlCode::Unknown,
            phase: "Commit Install".into(),
            moved: None,
            published: Some(outcome),
            detail: "known history, uncertain local completion".into(),
        }),
    ];
    for (index, reply) in replies.into_iter().enumerate() {
        let answer = Answer {
            id: index as u64 + 1,
            reply,
        };
        let bytes = answer.encode().unwrap();
        assert!(bytes.len() <= 8192);
        assert_eq!(Answer::decode(&bytes).unwrap(), answer);
        for end in 0..bytes.len() {
            assert!(Answer::decode(&bytes[..end]).is_err());
        }
        let mut extra = bytes;
        extra.push(0);
        assert!(Answer::decode(&extra).is_err());
    }
}
