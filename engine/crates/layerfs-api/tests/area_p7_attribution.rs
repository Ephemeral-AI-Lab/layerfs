//! P7 Commit author/labels over real disposable Stores and the public SDK values.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{Answer, Reply, SdkValue};
use layerfs_content::ObjectId;
use layerfs_history::{
    BranchId, CommitHistoryRequest, CommitId, CommitMeta, CommitOutcome, CommitRecord, CommitRequest,
    CommitStatistics, CommitVia, HistoryError, HistoryName, LayerId, LayerStackId, NodeId, PageResult,
    WorkspaceId, MAXIMUM_ATTRIBUTION_BYTES, MAXIMUM_LABELS,
};
use layerfs_store_service::store::{BindRequest, CommitOptions, CommitSuccess};
use std::fs;
use support::{Change, Fixture};

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn options(author: Option<String>, labels: Vec<String>) -> CommitOptions {
    CommitOptions {
        force: false,
        message: Some("P7 attributed Workspace change".into()),
        author,
        labels,
    }
}

fn committed(success: CommitSuccess) -> CommitRecord {
    match success.history {
        Some(CommitOutcome::Committed { record, .. }) => *record,
        other => panic!("Commit outcome: {other:?}"),
    }
}

fn list(fixture: &Fixture, branch: &str, pos: i64) -> SdkValue {
    fixture.call(
        "branch.list_commits",
        vec![
            ("branch", text(branch)),
            ("pageSize", SdkValue::Int(1000)),
            ("pagePos", SdkValue::Int(pos)),
        ],
    )
}

fn graph(fixture: &Fixture, branch: &str, pos: i64) -> SdkValue {
    fixture.call(
        "branch.graph",
        vec![
            ("branch", text(branch)),
            ("pageSize", SdkValue::Int(1000)),
            ("pagePos", SdkValue::Int(pos)),
        ],
    )
}

fn attribution(value: &SdkValue, author: Option<&str>, labels: &[&str]) {
    assert_eq!(
        value.get("author"),
        Some(&SdkValue::opt_text(author)),
        "{value:?}"
    );
    assert_eq!(
        value.get("labels"),
        Some(&SdkValue::List(
            labels.iter().map(|label| text(label)).collect()
        )),
        "{value:?}"
    );
}

fn boundary_labels() -> Vec<String> {
    let mut labels = vec![String::new(); MAXIMUM_LABELS];
    labels[0] = "x".repeat(MAXIMUM_ATTRIBUTION_BYTES - 193);
    assert_eq!(layerfs_history::labels::encode(&labels).len(), 4096);
    labels
}

fn direct_commit(
    fixture: &Fixture,
    branch: BranchId,
    root: ObjectId,
    meta: CommitMeta,
) -> Result<CommitRecord, HistoryError> {
    let snapshot = fixture
        .service
        .store
        .history()
        .branch_snapshot(branch)?
        .unwrap();
    match fixture.service.store.history().commit_branch(&CommitRequest {
        branch,
        expected_head: snapshot.branch.head_commit,
        expected_base: snapshot.branch.base_layer,
        expected_root: snapshot.effective_root,
        candidate_root: root,
        profile: snapshot.profile,
        scope: snapshot.scope,
        second_parent: None,
        meta,
        force: false,
    })? {
        CommitOutcome::Committed { record, .. } => Ok(*record),
        other => panic!("direct Commit outcome: {other:?}"),
    }
}

#[test]
fn workspace_attribution_round_trips_and_null_columns_read_as_unset() {
    let fixture = Fixture::new("area-p7-round-trip", |source| {
        fs::write(source.join("base.txt"), b"base\n").unwrap();
        1
    });
    fixture.fork("p7");
    let first = committed(fixture.workspace_commit_with(
        "p7",
        &[Change::Write("/base.txt", b"first\n")],
        71,
        || {},
        options(
            Some("agent-p7".into()),
            vec!["call-7".into(), "call-7".into(), "last".into()],
        ),
    ));
    assert_eq!(first.meta.author.as_deref(), Some("agent-p7"));
    assert_eq!(first.meta.labels, ["call-7", "call-7", "last"]);

    let listed = list(&fixture, "p7", 0);
    attribution(
        &support::items(&listed)[0],
        Some("agent-p7"),
        &["call-7", "call-7", "last"],
    );
    let graphed = graph(&fixture, "p7", 0);
    match graphed.get("commits") {
        Some(SdkValue::List(commits)) => attribution(
            &commits[0],
            Some("agent-p7"),
            &["call-7", "call-7", "last"],
        ),
        other => panic!("graph commits: {other:?}"),
    }

    committed(fixture.workspace_commit_with(
        "p7",
        &[Change::Write("/base.txt", b"second\n")],
        72,
        || {},
        options(None, Vec::new()),
    ));
    let listed = list(&fixture, "p7", 0);
    attribution(&support::items(&listed)[0], None, &[]);

    let connection = rusqlite::Connection::open(&fixture.host.config.path).unwrap();
    connection
        .execute(
            "UPDATE \"commit\" SET author=NULL,labels=NULL WHERE commit_id=?1",
            [first.id.as_slice()],
        )
        .unwrap();
    let listed = list(&fixture, "p7", 0);
    attribution(&support::items(&listed)[1], None, &[]);
    let graphed = graph(&fixture, "p7", 0);
    match graphed.get("commits") {
        Some(SdkValue::List(commits)) => attribution(&commits[1], None, &[]),
        other => panic!("graph commits: {other:?}"),
    }

    connection
        .execute(
            "UPDATE \"commit\" SET labels='not JSON' WHERE commit_id=?1",
            [first.id.as_slice()],
        )
        .unwrap();
    let branch = BranchId::named(
        LayerStackId::from_authority([23; 16]),
        &HistoryName::new("p7").unwrap(),
    );
    let failure = fixture
        .service
        .store
        .history()
        .commit_history(&CommitHistoryRequest {
            branch,
            start: None,
            cursor: None,
            limit: 128,
        })
        .unwrap_err();
    assert!(matches!(
        failure,
        HistoryError::Integrity("Commit labels JSON")
    ));
    fixture.cleanup();
}

#[test]
fn attribution_bounds_accept_exact_maxima_and_refuse_each_excess() {
    let fixture = Fixture::new("area-p7-bounds", |_source| 0);
    let valid = CommitMeta {
        author: Some("a".repeat(MAXIMUM_ATTRIBUTION_BYTES)),
        labels: boundary_labels(),
        ..CommitMeta::new(CommitVia::Workspace)
    };
    valid.check().unwrap();

    let mut invalid = valid.clone();
    invalid.author = Some("a".repeat(MAXIMUM_ATTRIBUTION_BYTES + 1));
    assert!(matches!(
        invalid.check(),
        Err(HistoryError::InvalidInput("Commit author"))
    ));
    let failure = direct_commit(
        &fixture,
        fixture.host.branch,
        ObjectId::for_bytes(b"P7 invalid author"),
        invalid,
    );
    assert!(matches!(
        failure,
        Err(HistoryError::InvalidInput("Commit author"))
    ));

    let mut invalid = valid.clone();
    invalid.labels[0].push('\u{07}');
    assert!(matches!(
        invalid.check(),
        Err(HistoryError::InvalidInput("Commit label"))
    ));
    let mut invalid = valid.clone();
    invalid.labels.push("extra".into());
    assert!(matches!(
        invalid.check(),
        Err(HistoryError::InvalidInput("Commit labels"))
    ));
    let mut invalid = valid;
    invalid.labels[0].push('x');
    assert_eq!(layerfs_history::labels::encode(&invalid.labels).len(), 4097);
    assert!(matches!(
        invalid.check(),
        Err(HistoryError::InvalidInput("Commit labels JSON"))
    ));
    fixture.cleanup();
}

#[test]
fn repeated_derived_identity_keeps_original_attribution() {
    let fixture = Fixture::new("area-p7-first-wins", |_source| 0);
    let root = ObjectId::for_bytes(b"P7 first wins root");
    let first = direct_commit(
        &fixture,
        fixture.host.branch,
        root,
        CommitMeta {
            message: Some("same identity".into()),
            author: Some("original-author".into()),
            labels: vec!["original".into(), "original".into()],
            ..CommitMeta::new(CommitVia::Workspace)
        },
    )
    .unwrap();
    let connection = rusqlite::Connection::open(&fixture.host.config.path).unwrap();
    connection
        .execute(
            "UPDATE branch SET head_commit_id=NULL WHERE branch_id=?1",
            [fixture.host.branch.as_slice()],
        )
        .unwrap();
    let repeated = direct_commit(
        &fixture,
        fixture.host.branch,
        root,
        CommitMeta {
            message: Some("same identity".into()),
            author: Some("replacement-author".into()),
            labels: vec!["replacement".into()],
            ..CommitMeta::new(CommitVia::Workspace)
        },
    )
    .unwrap();
    assert_eq!(repeated.id, first.id);
    assert_eq!(repeated.meta.author, first.meta.author);
    assert_eq!(repeated.meta.labels, first.meta.labels);
    assert_eq!(repeated.meta.time_ms, first.meta.time_ms);
    fixture.cleanup();
}

#[test]
fn identical_workspace_recommit_is_up_to_date_with_attribution() {
    let fixture = Fixture::new("area-p7-up-to-date", |_source| 0);
    let bound = fixture
        .service
        .store
        .bind(
            fixture.owner.client(),
            BindRequest {
                workspace: WorkspaceId::from_authority([73; 32]).unwrap(),
                branch: fixture.host.branch,
            },
        )
        .unwrap();
    let result = bound
        .workspace
        .commit_captured(&options(
            Some("not-recorded-for-a-no-op".into()),
            vec!["also-not-recorded".into()],
        ))
        .unwrap();
    assert!(matches!(
        result.history,
        Some(CommitOutcome::UpToDate { head: None, .. })
    ));
    bound.workspace.close().unwrap();
    fixture.cleanup();
}

fn collect_pages(fixture: &Fixture, branch: &str, graphed: bool) -> (Vec<String>, usize) {
    let mut ids = Vec::new();
    let mut pos = 0;
    let mut pages = 0;
    loop {
        pages += 1;
        assert!(pages <= 6, "paging did not terminate");
        let page = if graphed {
            graph(fixture, branch, pos)
        } else {
            list(fixture, branch, pos)
        };
        let values = if graphed {
            match page.get("commits") {
                Some(SdkValue::List(values)) => values,
                other => panic!("graph commits: {other:?}"),
            }
        } else {
            support::items(&page)
        };
        assert!((1..=3).contains(&values.len()), "page width: {page:?}");
        ids.extend(values.iter().map(|value| value.get_text("id").unwrap().to_owned()));
        match page.get_int("nextPagePos") {
            Some(next) => pos = next,
            None => return (ids, pages),
        }
    }
}

#[test]
fn attributed_cost_makes_short_list_and_graph_pages() {
    let fixture = Fixture::new("area-p7-budget", |_source| 0);
    fixture.fork("budget");
    let branch = BranchId::named(
        LayerStackId::from_authority([23; 16]),
        &HistoryName::new("budget").unwrap(),
    );
    let mut expected = Vec::new();
    for n in 0..12 {
        let record = direct_commit(
            &fixture,
            branch,
            ObjectId::for_bytes(format!("P7 budget root {n}").as_bytes()),
            CommitMeta {
                message: Some("m".repeat(MAXIMUM_ATTRIBUTION_BYTES)),
                author: Some("a".repeat(MAXIMUM_ATTRIBUTION_BYTES)),
                labels: boundary_labels(),
                ..CommitMeta::new(CommitVia::Workspace)
            },
        )
        .unwrap();
        expected.push(record.id.to_string());
    }
    expected.reverse();
    let (listed, list_pages) = collect_pages(&fixture, "budget", false);
    let (graphed, graph_pages) = collect_pages(&fixture, "budget", true);
    assert_eq!(listed, expected);
    assert_eq!(graphed, expected);
    assert_eq!((list_pages, graph_pages), (4, 4));
    fixture.cleanup();
}

#[test]
fn maximal_commit_record_matches_declared_encoded_width() {
    let record = CommitRecord {
        id: CommitId::from_bytes([0x12; 33]).unwrap(),
        stack: LayerStackId::from_authority([0x31; 16]),
        root: ObjectId::for_bytes(b"P7 maximum record"),
        parent: Some(CommitId::from_bytes([0x12; 33]).unwrap()),
        base_layer: LayerId::from_bytes([0x32; 33]).unwrap(),
        second_parent: Some(NodeId::Layer(
            LayerId::from_bytes([0x32; 33]).unwrap(),
        )),
        meta: CommitMeta {
            branch: Some(BranchId::from_authority([0x11; 16])),
            via: CommitVia::Workspace,
            message: Some("m".repeat(MAXIMUM_ATTRIBUTION_BYTES)),
            author: Some("a".repeat(MAXIMUM_ATTRIBUTION_BYTES)),
            labels: boundary_labels(),
            merged_from: Some(NodeId::Layer(
                LayerId::from_bytes([0x32; 33]).unwrap(),
            )),
            lww: Vec::new(),
            lww_truncated: true,
            units: Vec::new(),
            statistics: CommitStatistics {
                logical: u64::MAX,
                new_canonical: u64::MAX,
                new_stored: u64::MAX,
                new_metadata: u64::MAX,
            },
            displaced: Some(CommitId::from_bytes([0x12; 33]).unwrap()),
            time_ms: i64::MAX,
        },
    };
    let answer = Answer {
        id: 1,
        reply: Reply::History(PageResult {
            records: vec![record],
            continuation: None,
        }),
    };
    let encoded = answer.encode().unwrap();
    assert_eq!(encoded.len() - 17, CommitRecord::MAXIMUM_ENCODED_BYTES);
    assert_eq!(Answer::decode(&encoded).unwrap(), answer);
}
