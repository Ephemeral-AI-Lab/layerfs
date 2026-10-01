//! Actual C5 conditional history and trusted locator boundary, without provider reads.
use layerfs_bridge::contract::*;
use layerfs_content::{
    filesystem::{profile_id, scope_for_seed, FilesystemRoot},
    ObjectId, ObjectRole,
};
use layerfs_history::{
    catalog::{HistoryCatalog, HistoryCatalogConfig},
    identity::{BranchId, HistoryName, LayerStackId},
    records::*,
    sqlite,
};
use phase6_live_probe::{
    metadata::{Authority, LocatorDb},
    minio::Minio,
    objects::{Locator, Locators},
    publication::{self, Request},
    reservations::Owner,
    wire::Snapshot,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
struct Fixture {
    path: PathBuf,
    a: Arc<Authority>,
    owner: Owner,
    base: Snapshot,
    candidate: ObjectId,
}
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "p6-publication-{}",
            phase6_live_probe::minio::hex(&layerfs_sandbox::random::<16>().unwrap())
        ));
        std::fs::create_dir(&path).unwrap();
        let scope = scope_for_seed([8; 32]);
        let profile = profile_id();
        let genesis = ObjectId::for_bytes(
            &FilesystemRoot::new(profile, scope, 1, ObjectId::for_bytes(b"genesis table"))
                .unwrap()
                .encode()
                .unwrap(),
        );
        let canonical =
            FilesystemRoot::new(profile, scope, 1, ObjectId::for_bytes(b"candidate table"))
                .unwrap()
                .encode()
                .unwrap();
        let candidate = ObjectId::for_bytes(&canonical);
        let history = sqlite::create(
            &path.join("history.sqlite"),
            &HistoryCatalogConfig {
                binding_key: b"thin authority".to_vec(),
                incarnation: 1,
                cursor_key: [9; 32],
            },
        )
        .unwrap();
        let stack = LayerStackId::from_authority([1; 16]);
        let branch = BranchId::from_authority([2; 16]);
        let layer = history
            .initialize_layerstack(&StackInitialization {
                stack,
                name: HistoryName::new("stack").unwrap(),
                scope: scope.object(),
                profile,
                genesis_root: genesis,
            })
            .unwrap();
        history
            .fork(&ForkRequest {
                stack,
                branch,
                name: HistoryName::new("main").unwrap(),
                source: ForkSource::Layer(layer.head_layer),
            })
            .unwrap();
        history
            .reserve_inodes(&ReserveRequest {
                scope: scope.object(),
                count: 1,
            })
            .unwrap();
        let locators = Arc::new(LocatorDb::create(&path.join("locators.sqlite")).unwrap());
        locators
            .register_many(&[Locator {
                id: candidate,
                role: ObjectRole::FilesystemRoot,
                length: canonical.len(),
                pack: [3; 32],
                group: 0,
                record: 0,
                pack_id: 0,
            }])
            .unwrap();
        let a = Arc::new(Authority {
            locators,
            history,
            branch,
            daemon_s3: Minio {
                authority: "127.0.0.1:1".into(),
                bucket: "unavailable".into(),
                access: "unavailable".into(),
                secret: "unavailable".into(),
                stats: Arc::new(Mutex::new(Default::default())),
            },
            reservations: Mutex::new(Default::default()),
            publication: Mutex::new(Default::default()),
        });
        let base = a.snapshot().unwrap();
        let owner = Owner {
            workspace: b"workspace".to_vec(),
            incarnation: [5; 32],
            project: base.stack,
            branch: base.branch,
        };
        a.reservations
            .lock()
            .unwrap()
            .bootstrap(&a.history, owner.clone(), &base)
            .unwrap();
        Self {
            path,
            a,
            owner,
            base,
            candidate,
        }
    }
    fn submit(
        &self,
        owner: &Owner,
        generation: u64,
        revision: u64,
        base: &Snapshot,
        root: ObjectId,
    ) -> WorkspaceCommitOutcome {
        publication::submit(
            &self.a,
            Request {
                owner,
                generation,
                revision,
                base,
                root,
            },
        )
    }
    fn finish(self) {
        drop(self.a);
        std::fs::remove_dir_all(self.path).unwrap();
    }
}
fn failure(outcome: WorkspaceCommitOutcome, code: Code) {
    match outcome {
        WorkspaceCommitOutcome::Failed(f) => {
            assert_eq!(f.cause.code, code);
            assert!(!f.cause.unknown);
            assert_eq!(
                f.disposition,
                WorkspaceCommitFailureDisposition::KnownBeforeCommit
            );
        }
        _ => panic!("expected failure"),
    }
}
#[test]
fn known_commit_and_clean_successor_need_no_provider_reads() {
    let f = Fixture::new();
    let report = match f.submit(&f.owner, 1, 4, &f.base, f.candidate) {
        WorkspaceCommitOutcome::Completed(r) => r,
        _ => panic!("commit"),
    };
    let commit = match report.outcome {
        CommitOutcomeWire::Committed(c) => c,
        _ => panic!("new head"),
    };
    assert_eq!(commit.root, *f.candidate.as_bytes());
    assert_eq!(commit.parent, None);
    let next = f.a.snapshot().unwrap();
    let clean = match f.submit(&f.owner, 2, 4, &next, f.candidate) {
        WorkspaceCommitOutcome::Completed(r) => r,
        _ => panic!("clean"),
    };
    assert!(matches!(
        clean.outcome,
        CommitOutcomeWire::UpToDate { head: Some(_), .. }
    ));
    assert_eq!(f.a.daemon_s3.statistics().unwrap().get_calls, 0);
    f.finish();
}
#[test]
fn identity_context_generation_and_candidate_refuse_before_history_effects() {
    let f = Fixture::new();
    let mut owner = f.owner.clone();
    owner.incarnation = [6; 32];
    failure(f.submit(&owner, 1, 1, &f.base, f.candidate), Code::Denied);
    let mut base = f.base.clone();
    base.scope = [7; 32];
    failure(f.submit(&f.owner, 1, 1, &base, f.candidate), Code::Denied);
    failure(
        f.submit(&f.owner, 2, 1, &f.base, f.candidate),
        Code::InvalidInput,
    );
    failure(
        f.submit(&f.owner, 1, 0, &f.base, f.candidate),
        Code::InvalidInput,
    );
    failure(
        f.submit(&f.owner, 1, 1, &f.base, ObjectId::for_bytes(b"absent")),
        Code::MissingObject,
    );
    assert_eq!(f.a.snapshot().unwrap().head, None);
    assert!(matches!(
        f.submit(&f.owner, 1, 1, &f.base, f.candidate),
        WorkspaceCommitOutcome::Completed(_)
    ));
    f.finish();
}
#[test]
fn competing_branch_head_is_explicit_and_not_adopted_or_replayed() {
    let f = Fixture::new();
    assert!(matches!(
        f.submit(&f.owner, 1, 1, &f.base, f.candidate),
        WorkspaceCommitOutcome::Completed(_)
    ));
    let published = f.a.snapshot().unwrap();
    failure(
        f.submit(&f.owner, 2, 2, &f.base, f.candidate),
        Code::HeadMoved,
    );
    assert_eq!(f.a.snapshot().unwrap().head, published.head);
    failure(
        f.submit(&f.owner, 1, 2, &published, f.candidate),
        Code::InvalidInput,
    );
    assert!(matches!(
        f.submit(&f.owner, 2, 2, &published, f.candidate),
        WorkspaceCommitOutcome::Completed(_)
    ));
    f.finish();
}
#[test]
fn wrong_registered_role_cannot_be_published() {
    let f = Fixture::new();
    let id = ObjectId::for_bytes(b"payload");
    f.a.locators
        .register_many(&[Locator {
            id,
            role: ObjectRole::WholeFile,
            length: 7,
            pack: [4; 32],
            group: 0,
            record: 0,
            pack_id: 0,
        }])
        .unwrap();
    failure(f.submit(&f.owner, 1, 1, &f.base, id), Code::Integrity);
    assert!(f.a.snapshot().unwrap().head.is_none());
    f.finish();
}

#[test]
fn exact_typed_conflict_roundtrips_public_bridge_framing() {
    use layerfs_bridge::adapters::native::protocol::{decode_response, encode_response};
    let f = Fixture::new();
    assert!(matches!(
        f.submit(&f.owner, 1, 1, &f.base, f.candidate),
        WorkspaceCommitOutcome::Completed(_)
    ));
    let outcome = f.submit(&f.owner, 2, 2, &f.base, f.candidate);
    let response = Response::WorkspaceCommit(Box::new(WorkspaceCommitWire {
        workspace: f.owner.workspace.clone(),
        incarnation: f.owner.incarnation,
        outcome,
    }));
    let encoded = encode_response(&response).unwrap();
    assert_eq!(decode_response(&encoded).unwrap(), response);
    f.finish();
}
#[test]
fn lost_reply_after_real_c5_commit_is_unknown_and_never_resent() {
    use layerfs_bridge::adapters::native::{
        connection::{self, Peer, VerifiedPeer},
        protocol::Kind,
    };
    use phase6_live_probe::{
        metadata::Remote,
        metadata_session::Session,
        wire::{self, Bytes},
    };
    use std::{
        net::TcpListener,
        time::{SystemTime, UNIX_EPOCH},
    };
    let f = Fixture::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap().to_string();
    let authority = f.a.clone();
    let expected_owner = f.owner.clone();
    let expected_root = f.candidate;
    let server = std::thread::spawn(move || {
        let peer = Peer {
            selector: 1,
            public: *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key(),
            expires_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 60,
        };
        let (stream, _) = listener.accept().unwrap();
        let mut c = connection::accept(stream, &[8; 32], &[peer]).unwrap();
        let frame = c.receive.read().unwrap();
        assert_eq!(frame.kind, Kind::Begin);
        assert!(frame.bytes.starts_with(wire::PREFIX));
        assert_eq!(frame.bytes[7], 4);
        let mut b = Bytes::new(&frame.bytes[8..]);
        let workspace = b.blob().unwrap();
        let incarnation = b.take().unwrap();
        let generation = b.u64().unwrap();
        let revision = b.u64().unwrap();
        let base = Snapshot::read(&mut b).unwrap();
        let root = ObjectId::from_bytes(&b.take::<32>().unwrap()).unwrap();
        b.done().unwrap();
        assert_eq!(workspace, expected_owner.workspace);
        assert_eq!(incarnation, expected_owner.incarnation);
        assert_eq!(root, expected_root);
        assert!(matches!(
            publication::submit(
                &authority,
                Request {
                    owner: &expected_owner,
                    generation,
                    revision,
                    base: &base,
                    root
                }
            ),
            WorkspaceCommitOutcome::Completed(_)
        ));
        c.send.close();
        c.receive.close();
    });
    let remote = Remote {
        endpoint,
        selector: 1,
        private: [7; 32],
        server: *VerifiedPeer::from_private(&[8; 32]).unwrap().public_key(),
        session: Arc::new(Mutex::new(Session::default())),
    };
    let error = remote
        .publish(
            &f.owner.workspace,
            f.owner.incarnation,
            1,
            1,
            &f.base,
            *f.candidate.as_bytes(),
        )
        .unwrap_err();
    assert!(error.unknown);
    assert!(
        remote
            .publish(
                &f.owner.workspace,
                f.owner.incarnation,
                1,
                1,
                &f.base,
                *f.candidate.as_bytes()
            )
            .unwrap_err()
            .unknown
    );
    assert_eq!(remote.statistics().unwrap().connect_attempts, 1);
    assert_eq!(remote.statistics().unwrap().calls[4], 1);
    server.join().unwrap();
    assert_eq!(f.a.snapshot().unwrap().root, *f.candidate.as_bytes());
    drop(remote);
    f.finish();
}

#[test]
fn actual_c5_stage_race_retains_loser_and_roundtrips_context() {
    use layerfs_bridge::adapters::native::protocol::{decode_response, encode_response};
    use layerfs_history::identity::{CommitId, LayerId, WorkspaceId};
    let f = Fixture::new();
    let stage = |incarnation| {
        f.a.history
            .stage_changes(&StageRequest {
                workspace: WorkspaceId::from_authority(incarnation).unwrap(),
                branch: f.a.branch,
                expected_head: None,
                expected_base: LayerId::from_bytes(f.base.base).unwrap(),
                expected_root: ObjectId::from_bytes(&f.base.root).unwrap(),
                construction_base_root: ObjectId::from_bytes(&f.base.root).unwrap(),
                intended_commit_base: LayerId::from_bytes(f.base.base).unwrap(),
                candidate_root: f.candidate,
                profile: ObjectId::from_bytes(&f.base.profile).unwrap(),
                scope: ObjectId::from_bytes(&f.base.scope).unwrap(),
                generation: 1,
            })
            .unwrap()
    };
    let first = stage([7; 32]);
    let loser = stage(f.owner.incarnation);
    let winner =
        f.a.history
            .commit_staged(&CommitStagedRequest {
                workspace: first.workspace,
                token: first.token,
            })
            .unwrap();
    let head = match winner {
        CommitStagedOutcome::Committed(c) => c.id,
        _ => panic!("winner"),
    };
    let error =
        f.a.history
            .commit_staged(&CommitStagedRequest {
                workspace: loser.workspace,
                token: loser.token,
            })
            .unwrap_err();
    let cause = phase6_live_probe::publication_failure::catalog(error);
    assert_eq!(cause.code, Code::HeadMoved);
    assert!(!cause.unknown);
    let context = cause.history.as_ref().unwrap();
    assert!(
        matches!(context.conflict,Some(HistoryConflict::BranchMoved{expected_head:None,actual_head:Some(h),..})if h==CommitId::to_bytes(head))
    );
    assert!(matches!(&context.stage,StageObservation::Retained(s)if s.token==loser.token.value()));
    assert_eq!(
        f.a.history.stage(loser.workspace).unwrap().unwrap().token,
        loser.token
    );
    let response = Response::WorkspaceCommit(Box::new(WorkspaceCommitWire {
        workspace: f.owner.workspace.clone(),
        incarnation: f.owner.incarnation,
        outcome: WorkspaceCommitOutcome::Failed(Box::new(WorkspaceCommitFailureWire {
            generation: 1,
            phase: WorkspaceCommitPhase::CompositeCommit,
            disposition: WorkspaceCommitFailureDisposition::KnownBeforeCommit,
            cause,
            known_stage: None,
            observed_stage: None,
            known_outcome: None,
            observed_outcome: None,
            installed_revision: None,
        })),
    }));
    let bytes = encode_response(&response).unwrap();
    assert_eq!(decode_response(&bytes).unwrap(), response);
    f.finish();
}
