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
        Request::Attach(token),
        Request::Locate(token.workspace),
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
            native: None,
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
    let replies = replies.into_iter().chain(native_replies(&binding, token));
    for (index, reply) in replies.enumerate() {
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
fn native_replies(binding: &BranchSnapshot, token: WorkspaceToken) -> Vec<Reply> {
    let ready = ReadyMount {
        token,
        directory: "/mnt/layerfs/".to_owned() + &"d".repeat(MOUNT_DIRECTORY_LIMIT - 13),
        receipt: NativeReceipt {
            mount: u64::MAX,
            root: 7,
            mount_id: 911,
            device_major: 0,
            device_minor: 64,
            abi_major: 7,
            abi_minor: 41,
            offered: u64::MAX - 1,
            selected: (1 << 22) | 1,
            max_write: 131072,
            max_readahead: 131072,
            max_background: 1,
            congestion_threshold: 1,
            page_size: 4096,
            loops: 2,
            abort_bound: true,
        },
    };
    let work = NativeWork {
        loops_configured: 2,
        loops_entered: 2,
        loops_exited: 1,
        loops_joined: 0,
        received: 2,
        admitted: 16,
        queued: 3,
        running: 4,
        parked: 9,
        retained: 1,
        completed: u64::MAX,
        handoffs: 12,
        inline: 5,
        refused: 6,
        terminal: 7,
        unadmitted: 8,
        forget_units: 9,
    };
    let status = |activity, native| WorkspaceStatus {
        token,
        binding: binding.clone(),
        activity,
        epoch: 3,
        epoch_saturated: false,
        published: None,
        local: None,
        local_failure: Some(ControlRefusal {
            code: ControlCode::Failed,
            phase: "x".repeat(64),
            moved: None,
            published: None,
            detail: "y".repeat(2048),
        }),
        native,
    };
    let native = |phase, ready, detached, work| NativeStatus {
        phase,
        ready,
        detached,
        work,
    };
    vec![
        Reply::Ready(Box::new(ready.clone())),
        Reply::Status(Box::new(status(
            Activity::Attaching,
            Some(native(NativePhase::Attaching, None, false, None)),
        ))),
        Reply::Status(Box::new(status(
            Activity::Idle,
            Some(native(
                NativePhase::Ready,
                Some(ready.clone()),
                false,
                Some(work),
            )),
        ))),
        Reply::Located(Box::new(status(Activity::Idle, None))),
        Reply::Located(Box::new(status(
            Activity::Closing,
            Some(native(NativePhase::Draining, Some(ready), true, Some(work))),
        ))),
        Reply::Retained(Box::new(TeardownCustody {
            token,
            stage: TeardownStage::Requests,
            detached: true,
            work: Some(work),
            detail: "z".repeat(2048),
            forced: None,
        })),
    ]
}
#[test]
fn native_block_is_additive_and_bounded_fields_are_refused_before_send() {
    let binding = binding();
    let token = token();
    let plain = WorkspaceStatus {
        token,
        binding: binding.clone(),
        activity: Activity::Idle,
        epoch: 1,
        epoch_saturated: false,
        published: None,
        local: Some(LocalObservation {
            revision: 1,
            active: 1,
            captured: None,
            captured_revision: None,
            base_root: binding.effective_root.to_bytes(),
            dirty_inodes: 0,
            dirty_directory_entries: 0,
            closed: false,
            base_readers: 0,
        }),
        local_failure: None,
        native: None,
    };
    let original = Answer {
        id: 9,
        reply: Reply::Status(Box::new(plain.clone())),
    }
    .encode()
    .unwrap();
    let mut extended = plain;
    extended.native = Some(NativeStatus {
        phase: NativePhase::Unattached,
        ready: None,
        detached: false,
        work: None,
    });
    let additive = Answer {
        id: 9,
        reply: Reply::Status(Box::new(extended)),
    }
    .encode()
    .unwrap();
    // Same fields under a distinct tag: the original record is unchanged and
    // is not a decodable prefix of the extended one.
    // Five magic bytes and the eight-byte correlation precede the reply tag.
    let tag = 13;
    assert_eq!(original[tag], 3);
    assert_eq!(additive[tag], 13);
    assert_eq!(original[tag + 1..], additive[tag + 1..original.len()]);
    assert!(Answer::decode(&additive[..original.len()]).is_err());
    let Reply::Ready(ready) = native_replies(&binding, token).remove(0) else {
        panic!("Ready fixture")
    };
    for invalid in [
        ReadyMount {
            directory: String::new(),
            ..(*ready).clone()
        },
        ReadyMount {
            directory: "d".repeat(MOUNT_DIRECTORY_LIMIT + 1),
            ..(*ready).clone()
        },
        ReadyMount {
            receipt: NativeReceipt {
                mount: 0,
                ..ready.receipt
            },
            ..(*ready).clone()
        },
    ] {
        assert!(Answer {
            id: 1,
            reply: Reply::Ready(Box::new(invalid))
        }
        .encode()
        .is_err());
    }
}
