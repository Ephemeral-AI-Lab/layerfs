//! Thin C5 checks using independently sealed root bytes; no full filesystem/provider claim.
use layerfs_bridge::contract::*;
use layerfs_content::{filesystem::FilesystemRoot, FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{
    catalog::{HistoryCatalog, HistoryCatalogConfig},
    identity::{BranchId, HistoryName, LayerStackId, WorkspaceId},
    records::*,
    sqlite,
};
use layerfs_storage::{
    encoding::{decode_canonical, CompressionWorkspace, DecompressionWorkspace, GroupCache},
    sqlite::ObjectLocation,
    StorageCapacities,
};
use phase6_live_probe::{
    packing,
    reservations::Owner,
    strict_catalog::{LogicalUse, PlacementDomain, Registration, SaveContext, StrictCatalog},
    strict_publication::{self, Authority, Request},
    wire::Snapshot,
};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const ROOTS: [&str; 3] = [
    "f7bb395fe7359ddf87221a3928ef861d6ba0bc9e564727f15c79f1da38343461",
    "711b4bb80908c287a4c08efed2d1d8461340b0f201bc971739b7fe3d2d176d4d",
    "5ba61d8428c7c21f63330951748f2df148a6901f8e0038579856b4fbc0fd861b",
];
fn id(hex: &str) -> ObjectId {
    let raw: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    ObjectId::from_bytes(&raw).unwrap()
}
fn canonical(state: usize) -> Vec<u8> {
    let path=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/architecture/proposal/phase6-sqlite-minio/implementation/sp1/evidence/strict-oracle-v2/domain-packs").join(format!("{}.canonical",ROOTS[state]));
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(ObjectId::for_bytes(&bytes), id(ROOTS[state]));
    FilesystemRoot::decode(&bytes).unwrap();
    bytes
}
struct Fixture {
    path: PathBuf,
    a: Arc<Authority>,
    owner: Owner,
    base: Snapshot,
}
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "strict-publication-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let genesis = FilesystemRoot::decode(&canonical(0)).unwrap();
        let history = sqlite::create(
            &path.join("history.sqlite"),
            &HistoryCatalogConfig {
                binding_key: b"strict thin authority".to_vec(),
                incarnation: 1,
                cursor_key: [7; 32],
            },
        )
        .unwrap();
        let stack = LayerStackId::from_authority([1; 16]);
        let branch = BranchId::from_authority([2; 16]);
        let layer = history
            .initialize_layerstack(&StackInitialization {
                stack,
                name: HistoryName::new("strict-stack").unwrap(),
                scope: genesis.scope().object(),
                profile: genesis.profile(),
                genesis_root: id(ROOTS[0]),
            })
            .unwrap();
        history
            .fork(&ForkRequest {
                stack,
                branch,
                name: HistoryName::new("strict-main").unwrap(),
                source: ForkSource::Layer(layer.head_layer),
            })
            .unwrap();
        history
            .reserve_inodes(&ReserveRequest {
                scope: genesis.scope().object(),
                count: 1,
            })
            .unwrap();
        let catalog = Arc::new(StrictCatalog::create(&path.join("metadata.sqlite")).unwrap());
        let a = Arc::new(Authority {
            catalog,
            history,
            branch,
            reservations: Mutex::new(Default::default()),
            publication: Mutex::new(Default::default()),
        });
        let base = a.snapshot().unwrap();
        let owner = Owner {
            workspace: b"strict-workspace".to_vec(),
            incarnation: [5; 32],
            project: base.stack,
            branch: base.branch,
        };
        a.reservations
            .lock()
            .unwrap()
            .bootstrap(&a.history, owner.clone(), &base)
            .unwrap();
        let fixture = Self {
            path,
            a,
            owner,
            base,
        };
        // Bootstrap publishes the already sealed genesis metadata before any
        // Workspace Commit; no owned Workspace save is adopted by this setup.
        let genesis = fixture.stage_storage_context(0, true, None);
        fixture.a.catalog.publish(genesis).unwrap();
        fixture
    }
    fn stage_storage(&self, state: usize, ready: bool) -> i64 {
        let base = self.a.snapshot().unwrap();
        let generation = self.a.publication.lock().unwrap().generation + 1;
        self.stage_storage_context(
            state,
            ready,
            Some(SaveContext {
                owner: self.owner.clone(),
                generation,
                scope: base.scope,
                profile: base.profile,
                base_root: base.root,
            }),
        )
    }
    fn stage_storage_context(
        &self,
        state: usize,
        ready: bool,
        context: Option<SaveContext>,
    ) -> i64 {
        let bytes = canonical(state);
        let object = FinalizedObject::new(ObjectRole::FilesystemRoot, bytes).unwrap();
        let p = packing::build(vec![object], &mut CompressionWorkspace::new().unwrap())
            .unwrap()
            .pop()
            .unwrap();
        let save = match context {
            Some(context) => self.a.catalog.begin_save_owned(&context).unwrap(),
            None => self.a.catalog.begin_save().unwrap(),
        };
        let digest: [u8; 32] = Sha256::digest(&p.bytes).into();
        let order = self
            .a
            .catalog
            .register_body(PlacementDomain::Metadata, save, digest, Some(&p.bytes))
            .unwrap();
        let r = &p.rows[0];
        self.a
            .catalog
            .register(
                self.a.catalog.capture(Some(save)).unwrap(),
                save,
                &[Registration {
                    location: ObjectLocation {
                        object_id: r.id,
                        role: r.role,
                        canonical_length: r.length,
                        pack_id: order,
                        group_number: r.group as usize,
                        record_number: r.record as usize,
                    },
                    domain: PlacementDomain::Metadata,
                    logical_use: LogicalUse::MetadataGraph,
                    references: Vec::new(),
                    base: None,
                }],
            )
            .unwrap();
        if ready {
            self.a.catalog.finish_storage(save).unwrap();
        }
        save
    }
    fn submit(
        &self,
        owner: &Owner,
        generation: u64,
        revision: u64,
        base: &Snapshot,
        save: i64,
        state: usize,
    ) -> WorkspaceCommitOutcome {
        strict_publication::submit(
            &self.a,
            Request {
                owner,
                generation,
                revision,
                base,
                save,
                root: id(ROOTS[state]),
            },
        )
    }
    fn assert_retained_root(&self, state: usize) {
        let scope = self.a.catalog.capture(None).unwrap();
        let located = self
            .a
            .catalog
            .location(scope, PlacementDomain::Metadata, id(ROOTS[state]))
            .unwrap()
            .unwrap();
        let (_, bytes) = self
            .a
            .catalog
            .body(scope, PlacementDomain::Metadata, located.location.pack_id)
            .unwrap();
        let decoded = decode_canonical(
            &bytes.unwrap(),
            &located.location,
            &StorageCapacities::from_policy(Default::default()).unwrap(),
            None,
            &mut DecompressionWorkspace::new().unwrap(),
            &mut GroupCache::new(),
            &mut 0,
        )
        .unwrap();
        assert_eq!(decoded, canonical(state));
    }
    fn finish(self) {
        drop(self.a);
        std::fs::remove_dir_all(self.path).unwrap();
    }
}
fn failed(outcome: WorkspaceCommitOutcome, code: Code) -> Box<WorkspaceCommitFailureWire> {
    match outcome {
        WorkspaceCommitOutcome::Failed(f) => {
            assert_eq!(f.cause.code, code);
            assert!(!f.cause.unknown);
            assert_eq!(
                f.disposition,
                WorkspaceCommitFailureDisposition::KnownBeforeCommit
            );
            f
        }
        _ => panic!("expected definite refusal"),
    }
}
#[test]
fn two_conditional_commits_retain_both_sealed_sql_metadata_roots() {
    let f = Fixture::new();
    let first = f.stage_storage(1, true);
    let report = match f.submit(&f.owner, 1, 4, &f.base, first, 1) {
        WorkspaceCommitOutcome::Completed(r) => r,
        _ => panic!("first publication"),
    };
    let first_commit = match report.outcome {
        CommitOutcomeWire::Committed(c) => c,
        _ => panic!("first commit"),
    };
    assert_eq!(first_commit.root, *id(ROOTS[1]).as_bytes());
    assert!(report.stage_token.is_some());
    let base = f.a.snapshot().unwrap();
    assert_eq!(base.head, Some(first_commit.commit));
    assert_eq!(base.root, *id(ROOTS[1]).as_bytes());
    let second = f.stage_storage(2, true);
    let report = match f.submit(&f.owner, 2, 7, &base, second, 2) {
        WorkspaceCommitOutcome::Completed(r) => r,
        _ => panic!("second publication"),
    };
    let second_commit = match report.outcome {
        CommitOutcomeWire::Committed(c) => c,
        _ => panic!("second commit"),
    };
    assert_eq!(second_commit.parent, Some(first_commit.commit));
    for c in [&first_commit, &second_commit] {
        let record =
            f.a.history
                .commit(layerfs_history::CommitId::from_bytes(c.commit).unwrap())
                .unwrap()
                .unwrap();
        assert_eq!(record.root.to_bytes(), c.root);
    }
    assert!(f
        .a
        .history
        .stage(WorkspaceId::from_authority(f.owner.incarnation).unwrap())
        .unwrap()
        .is_none());
    f.assert_retained_root(0);
    f.assert_retained_root(1);
    f.assert_retained_root(2);
    let state = f.a.publication.lock().unwrap();
    assert_eq!((state.generation, state.revision), (2, 7));
    assert!(state.pending.is_none());
    drop(state);
    assert!(!f.a.catalog.ready_root(second, id(ROOTS[2])).unwrap());
    f.finish();
}
#[test]
fn moved_branch_refuses_before_storage_publication_or_stage_creation() {
    let f = Fixture::new();
    let first = f.stage_storage(1, true);
    assert!(matches!(
        f.submit(&f.owner, 1, 1, &f.base, first, 1),
        WorkspaceCommitOutcome::Completed(_)
    ));
    let second = f.stage_storage(2, true);
    let refusal = failed(
        f.submit(&f.owner, 2, 2, &f.base, second, 2),
        Code::HeadMoved,
    );
    assert!(refusal.known_stage.is_none());
    assert!(refusal.cause.history.is_some());
    assert!(f.a.catalog.ready_root(second, id(ROOTS[2])).unwrap());
    assert!(f.a.publication.lock().unwrap().pending.is_none());
    assert!(f
        .a
        .history
        .stage(WorkspaceId::from_authority(f.owner.incarnation).unwrap())
        .unwrap()
        .is_none());
    f.finish();
}
#[test]
fn grant_generation_and_ready_save_refusals_leave_no_submission() {
    let f = Fixture::new();
    let save = f.stage_storage(1, false);
    failed(
        f.submit(&f.owner, 1, 1, &f.base, save, 1),
        Code::MissingObject,
    );
    f.a.catalog.finish_storage(save).unwrap();
    let mut foreign = f.owner.clone();
    foreign.incarnation = [6; 32];
    failed(f.submit(&foreign, 1, 1, &f.base, save, 1), Code::Denied);
    failed(
        f.submit(&f.owner, 2, 1, &f.base, save, 1),
        Code::InvalidInput,
    );
    assert!(f.a.catalog.ready_root(save, id(ROOTS[1])).unwrap());
    assert!(f.a.publication.lock().unwrap().pending.is_none());
    f.finish();
}
#[test]
fn existing_real_stage_retains_custody_and_blocks_resubmission() {
    let f = Fixture::new();
    let save = f.stage_storage(1, true);
    let workspace = WorkspaceId::from_authority(f.owner.incarnation).unwrap();
    let stage =
        f.a.history
            .stage_changes(&StageRequest {
                workspace,
                branch: f.a.branch,
                expected_head: None,
                expected_base: layerfs_history::LayerId::from_bytes(f.base.base).unwrap(),
                expected_root: id(ROOTS[0]),
                construction_base_root: id(ROOTS[0]),
                intended_commit_base: layerfs_history::LayerId::from_bytes(f.base.base).unwrap(),
                candidate_root: id(ROOTS[1]),
                profile: ObjectId::from_bytes(&f.base.profile).unwrap(),
                scope: ObjectId::from_bytes(&f.base.scope).unwrap(),
                generation: 1,
            })
            .unwrap();
    failed(
        f.submit(&f.owner, 1, 1, &f.base, save, 1),
        Code::InvalidInput,
    );
    assert!(f.a.publication.lock().unwrap().pending.is_some());
    assert_eq!(
        f.a.history.stage(workspace).unwrap().unwrap().token,
        stage.token
    );
    failed(f.submit(&f.owner, 1, 1, &f.base, save, 1), Code::Busy);
    assert!(f.a.catalog.ready_root(save, id(ROOTS[1])).unwrap());
    f.finish();
}
#[test]
fn anonymous_and_mismatched_owned_saves_cannot_publish() {
    let f = Fixture::new();
    let anonymous = f.stage_storage_context(1, true, None);
    failed(
        f.submit(&f.owner, 1, 1, &f.base, anonymous, 1),
        Code::Denied,
    );
    assert!(f.a.catalog.ready_root(anonymous, id(ROOTS[1])).unwrap());
    let mut context = SaveContext {
        owner: f.owner.clone(),
        generation: 1,
        scope: f.base.scope,
        profile: f.base.profile,
        base_root: f.base.root,
    };
    context.owner.incarnation = [9; 32];
    let foreign = f.stage_storage_context(2, true, Some(context));
    failed(f.submit(&f.owner, 1, 1, &f.base, foreign, 2), Code::Denied);
    assert!(f.a.publication.lock().unwrap().pending.is_none());
    f.finish();
}
#[test]
fn one_owned_pending_capture_survives_storage_publish_until_known_completion() {
    let f = Fixture::new();
    let context = SaveContext {
        owner: f.owner.clone(),
        generation: 1,
        scope: f.base.scope,
        profile: f.base.profile,
        base_root: f.base.root,
    };
    let save = f.a.catalog.begin_save_owned(&context).unwrap();
    assert!(f.a.catalog.begin_save_owned(&context).is_err());
    f.a.catalog.finish_storage(save).unwrap();
    f.a.catalog.publish(save).unwrap();
    assert!(f.a.catalog.begin_save_owned(&context).is_err());
    f.a.catalog.complete_owned_publication(save).unwrap();
    let next =
        f.a.catalog
            .begin_save_owned(&SaveContext {
                generation: 2,
                ..context.clone()
            })
            .unwrap();
    f.a.catalog.quarantine(next).unwrap();
    assert!(f.a.catalog.begin_save_owned(&context).is_err());
    let mut other = context.clone();
    other.owner.incarnation = [10; 32];
    let abandoned = f.a.catalog.begin_save_owned(&other).unwrap();
    f.a.catalog.abandon(abandoned).unwrap();
    assert!(f.a.catalog.begin_save_owned(&other).is_ok());
    f.finish();
}
#[test]
fn every_capture_fact_is_bound_before_owned_publication() {
    let f = Fixture::new();
    let context = SaveContext {
        owner: f.owner.clone(),
        generation: 1,
        scope: f.base.scope,
        profile: f.base.profile,
        base_root: f.base.root,
    };
    let save = f.a.catalog.begin_save_owned(&context).unwrap();
    assert!(f.a.catalog.matches_save_owner(save, &context).unwrap());
    let mut mismatches = Vec::new();
    let mut c = context.clone();
    c.owner.workspace.push(b'X');
    mismatches.push(c);
    let mut c = context.clone();
    c.owner.incarnation[0] ^= 1;
    mismatches.push(c);
    let mut c = context.clone();
    c.owner.project = LayerStackId::from_authority([11; 16]).to_bytes();
    mismatches.push(c);
    let mut c = context.clone();
    c.owner.branch = BranchId::from_authority([12; 16]).to_bytes();
    mismatches.push(c);
    let mut c = context.clone();
    c.generation += 1;
    mismatches.push(c);
    let mut c = context.clone();
    c.scope[0] ^= 1;
    mismatches.push(c);
    let mut c = context.clone();
    c.profile[0] ^= 1;
    mismatches.push(c);
    let mut c = context.clone();
    c.base_root[0] ^= 1;
    mismatches.push(c);
    for c in mismatches {
        assert!(!f.a.catalog.matches_save_owner(save, &c).unwrap());
    }
    f.finish();
}
#[test]
fn reused_committed_root_records_owned_use_and_c5_reports_up_to_date() {
    let f = Fixture::new();
    let context = SaveContext {
        owner: f.owner.clone(),
        generation: 1,
        scope: f.base.scope,
        profile: f.base.profile,
        base_root: f.base.root,
    };
    let save = f.a.catalog.begin_save_owned(&context).unwrap();
    let root = id(ROOTS[0]);
    let original =
        f.a.catalog
            .location(
                f.a.catalog.capture(None).unwrap(),
                PlacementDomain::Metadata,
                root,
            )
            .unwrap()
            .unwrap();
    f.a.catalog
        .register_use(
            f.a.catalog.capture(Some(save)).unwrap(),
            save,
            PlacementDomain::Metadata,
            root,
            LogicalUse::MetadataGraph,
            &[],
        )
        .unwrap();
    f.a.catalog.finish_storage(save).unwrap();
    assert!(f.a.catalog.ready_root(save, root).unwrap());
    let report = match f.submit(&f.owner, 1, 1, &f.base, save, 0) {
        WorkspaceCommitOutcome::Completed(r) => r,
        _ => panic!("up to date publication"),
    };
    assert_eq!(
        report.outcome,
        CommitOutcomeWire::UpToDate {
            head: None,
            root: f.base.root
        }
    );
    assert!(f
        .a
        .history
        .stage(WorkspaceId::from_authority(f.owner.incarnation).unwrap())
        .unwrap()
        .is_none());
    let retained =
        f.a.catalog
            .location(
                f.a.catalog.capture(None).unwrap(),
                PlacementDomain::Metadata,
                root,
            )
            .unwrap()
            .unwrap();
    assert_eq!(retained.location.pack_id, original.location.pack_id);
    f.finish();
}
#[test]
fn owned_duplicate_constraint_is_definite_only_after_acknowledged_abort() {
    let f = Fixture::new();
    let context = SaveContext {
        owner: f.owner.clone(),
        generation: 1,
        scope: f.base.scope,
        profile: f.base.profile,
        base_root: f.base.root,
    };
    let save = f.a.catalog.begin_save_owned(&context).unwrap();
    let error = f.a.catalog.begin_save_owned(&context).unwrap_err();
    assert!(phase6_live_probe::strict_catalog::is_definite_catalog_error(&error));
    assert!(
        !phase6_live_probe::strict_catalog::is_definite_catalog_error(
            "unknown catalog custody: example"
        )
    );
    assert!(f.a.catalog.matches_save_owner(save, &context).unwrap());
    let db = rusqlite::Connection::open(f.path.join("metadata.sqlite")).unwrap();
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM save_context WHERE workspace=?1 AND incarnation=?2",
            rusqlite::params![
                context.owner.workspace,
                context.owner.incarnation.as_slice()
            ],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    drop(db);
    f.finish();
}
