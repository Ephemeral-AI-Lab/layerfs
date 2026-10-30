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
    BranchId, CommitHistoryRequest, CommitId, CommitMeta, CommitOutcome, CommitRecord,
    CommitRequest, CommitStatistics, CommitVia, HistoryError, HistoryName, LayerId, LayerStackId,
    NodeId, PageResult, WorkspaceId, MAXIMUM_ATTRIBUTION_BYTES, MAXIMUM_LABELS,
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
    match fixture
        .service
        .store
        .history()
        .commit_branch(&CommitRequest {
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
        Some(SdkValue::List(commits)) => {
            attribution(&commits[0], Some("agent-p7"), &["call-7", "call-7", "last"])
        }
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
        ids.extend(
            values
                .iter()
                .map(|value| value.get_text("id").unwrap().to_owned()),
        );
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

fn merge_meta(units: Vec<layerfs_content::merge::MergeUnit>) -> CommitMeta {
    CommitMeta {
        message: Some("merge with diagnostics".into()),
        lww: (0..32)
            .map(|n| layerfs_content::merge::LwwDecision {
                path: format!("/source/{n}/{}", "p".repeat(80)),
                kept_ours: true,
                deleted: false,
                rule: format!("rule-{n}-{}", "r".repeat(32)),
            })
            .collect(),
        units,
        ..CommitMeta::new(CommitVia::Merge)
    }
}

fn small_units(commit: usize) -> Vec<layerfs_content::merge::MergeUnit> {
    (0..4)
        .map(|unit| layerfs_content::merge::MergeUnit {
            name: format!("unit-{commit}-{unit}"),
            patterns: (0..8)
                .map(|pattern| format!("/unit/{unit}/{pattern}/{}", "u".repeat(180)))
                .collect(),
        })
        .collect()
}

fn merge_pages(
    fixture: &Fixture,
    branch: &str,
    graphed: bool,
    limit: i64,
    expected: &[String],
) -> Vec<SdkValue> {
    let mut ids = std::collections::HashSet::new();
    let mut found = Vec::new();
    let mut pages = Vec::new();
    let mut pos = 0;
    loop {
        assert!(pages.len() <= expected.len(), "paging did not terminate");
        let page = if graphed {
            fixture.call(
                "branch.graph",
                vec![
                    ("branch", text(branch)),
                    ("pageSize", SdkValue::Int(limit)),
                    ("pagePos", SdkValue::Int(pos)),
                ],
            )
        } else {
            fixture.call(
                "branch.list_commits",
                vec![
                    ("branch", text(branch)),
                    ("pageSize", SdkValue::Int(limit)),
                    ("pagePos", SdkValue::Int(pos)),
                ],
            )
        };
        let answer = Answer {
            id: 1,
            reply: Reply::Sdk(page.clone()),
        };
        let encoded = answer.encode().unwrap();
        assert!(encoded.len() <= 65_519, "control bytes: {}", encoded.len());
        assert_eq!(Answer::decode(&encoded).unwrap(), answer);
        let values = if graphed {
            match page.get("commits") {
                Some(SdkValue::List(values)) => values,
                other => panic!("graph commits: {other:?}"),
            }
        } else {
            support::items(&page)
        };
        assert!(!values.is_empty(), "empty page at {pos}: {page:?}");
        for value in values {
            let id = value.get_text("id").unwrap().to_owned();
            assert!(ids.insert(id.clone()), "repeated Commit {id}");
            found.push(id);
        }
        match page.get_int("nextPagePos") {
            Some(next) => {
                assert!(page.get_bool("hasMore"));
                assert!(next > pos, "cursor stalled at {pos}");
                pos = next;
            }
            None => {
                assert!(!page.get_bool("hasMore"));
                pages.push(page);
                break;
            }
        }
        pages.push(page);
    }
    assert_eq!(found, expected);
    pages
}

#[test]
fn merge_diagnostics_use_actual_cost_and_page_without_repeats() {
    let fixture = Fixture::new("area-p7-merge-budget", |_source| 0);
    fixture.fork("merge-budget");
    let branch = BranchId::named(
        LayerStackId::from_authority([23; 16]),
        &HistoryName::new("merge-budget").unwrap(),
    );
    let mut expected = Vec::new();
    for n in 0..8 {
        let record = direct_commit(
            &fixture,
            branch,
            ObjectId::for_bytes(format!("P7 merge budget root {n}").as_bytes()),
            merge_meta(small_units(n)),
        )
        .unwrap();
        expected.push(record.id.to_string());
    }
    expected.reverse();
    for graphed in [false, true] {
        let pages = merge_pages(&fixture, "merge-budget", graphed, 1000, &expected);
        assert!(pages.len() > 1, "actual costs must shorten the pages");
        for page in pages {
            let values = if graphed {
                match page.get("commits") {
                    Some(SdkValue::List(values)) => values,
                    other => panic!("graph commits: {other:?}"),
                }
            } else {
                support::items(&page)
            };
            for value in values {
                assert!(
                    matches!(value.get("units"), Some(SdkValue::List(units)) if !units.is_empty())
                );
                assert_eq!(value.get("unitsTruncated"), Some(&SdkValue::Bool(false)));
                assert!(matches!(value.get("lww"), Some(SdkValue::List(lww)) if !lww.is_empty()));
            }
        }
    }
    fixture.cleanup();
}

#[test]
fn oversized_units_are_disclosed_and_single_item_pages_advance() {
    let fixture = Fixture::new("area-p7-oversized-units", |_source| 0);
    fixture.fork("oversized-units");
    let branch = BranchId::named(
        LayerStackId::from_authority([23; 16]),
        &HistoryName::new("oversized-units").unwrap(),
    );
    let small = direct_commit(
        &fixture,
        branch,
        ObjectId::for_bytes(b"P7 small merge root"),
        merge_meta(small_units(0)),
    )
    .unwrap();
    let oversized = direct_commit(
        &fixture,
        branch,
        ObjectId::for_bytes(b"P7 oversized merge root"),
        merge_meta(vec![layerfs_content::merge::MergeUnit {
            name: "oversized".into(),
            patterns: vec![format!("/{}", "u".repeat(70 * 1024))],
        }]),
    )
    .unwrap();
    let expected = vec![oversized.id.to_string(), small.id.to_string()];
    for graphed in [false, true] {
        let pages = merge_pages(&fixture, "oversized-units", graphed, 1, &expected);
        assert_eq!(pages.len(), 2);
        let first = if graphed {
            match pages[0].get("commits") {
                Some(SdkValue::List(values)) => &values[0],
                other => panic!("graph commits: {other:?}"),
            }
        } else {
            &support::items(&pages[0])[0]
        };
        assert_eq!(first.get("units"), None);
        assert_eq!(first.get("unitsTruncated"), Some(&SdkValue::Bool(true)));
        assert!(matches!(first.get("lww"), Some(SdkValue::List(lww)) if !lww.is_empty()));
    }
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
        second_parent: Some(NodeId::Layer(LayerId::from_bytes([0x32; 33]).unwrap())),
        meta: CommitMeta {
            branch: Some(BranchId::from_authority([0x11; 16])),
            via: CommitVia::Workspace,
            message: Some("m".repeat(MAXIMUM_ATTRIBUTION_BYTES)),
            author: Some("a".repeat(MAXIMUM_ATTRIBUTION_BYTES)),
            labels: boundary_labels(),
            merged_from: Some(NodeId::Layer(LayerId::from_bytes([0x32; 33]).unwrap())),
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

fn maximal_meta() -> CommitMeta {
    CommitMeta {
        message: Some("m".repeat(MAXIMUM_ATTRIBUTION_BYTES)),
        author: Some("a".repeat(MAXIMUM_ATTRIBUTION_BYTES)),
        labels: boundary_labels(),
        ..CommitMeta::new(CommitVia::Workspace)
    }
}

fn provider_pages(
    fixture: &Fixture,
    branch: BranchId,
    records: usize,
) -> Vec<PageResult<CommitRecord>> {
    let connection = rusqlite::Connection::open(&fixture.host.config.path).unwrap();
    let mut explain = connection
        .prepare(
            "EXPLAIN QUERY PLAN SELECT commit_id, layer_stack_id, root_id, parent_commit_id, base_layer_id, second_parent_id, branch_id, via, message, merged_from_id, created_ms, logical, new_canonical, new_stored, new_metadata, lww, displaced_id, author, labels FROM \"commit\" WHERE commit_id = $1",
        )
        .unwrap();
    let id = CommitId::from_bytes([0x12; 33]).unwrap();
    let plan = explain
        .query_map([id.as_slice()], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    println!("P7_PROVIDER_PLAN {}", plan.join(" | "));
    drop(explain);
    drop(connection);
    let before = fixture.work();
    let mut cursor = None;
    let mut pages = Vec::new();
    loop {
        assert!(pages.len() <= records, "provider paging did not terminate");
        let page = fixture
            .service
            .store
            .history()
            .commit_history(&CommitHistoryRequest {
                branch,
                start: None,
                cursor,
                limit: 128,
            })
            .unwrap();
        let answer = Answer {
            id: 1,
            reply: Reply::History(page.clone()),
        };
        let encoded = answer.encode().unwrap();
        assert!(
            encoded.len() - 17 <= layerfs_history::records::MAXIMUM_RESULT_BYTES,
            "encoded provider page bytes: {}",
            encoded.len() - 17
        );
        assert_eq!(Answer::decode(&encoded).unwrap(), answer);
        cursor = page.continuation.clone();
        pages.push(page);
        if cursor.is_none() {
            break;
        }
    }
    let work = fixture.since(before);
    println!(
        "P7_PROVIDER records={records} pages={} statements={} rows={} vm_steps={}",
        pages.len(),
        work.statements,
        work.rows,
        work.vm_steps
    );
    pages
}

#[test]
fn tiny_commit_records_share_one_encoded_provider_page() {
    let fixture = Fixture::new("area-p7-tiny-provider-pages", |_source| 0);
    fixture.fork("tiny-provider");
    let branch = BranchId::named(
        LayerStackId::from_authority([23; 16]),
        &HistoryName::new("tiny-provider").unwrap(),
    );
    let mut expected = Vec::new();
    for n in 0..16 {
        expected.push(
            direct_commit(
                &fixture,
                branch,
                ObjectId::for_bytes(format!("P7 tiny provider root {n}").as_bytes()),
                CommitMeta {
                    message: Some(format!("tiny {n}")),
                    ..CommitMeta::new(CommitVia::Workspace)
                },
            )
            .unwrap()
            .id,
        );
    }
    expected.reverse();
    let pages = provider_pages(&fixture, branch, expected.len());
    assert_eq!(pages.len(), 1, "tiny records must share a provider page");
    assert_eq!(pages[0].continuation, None);
    assert_eq!(
        pages
            .iter()
            .flat_map(|page| page.records.iter().map(|record| record.id))
            .collect::<Vec<_>>(),
        expected
    );
    fixture.cleanup();
}

#[test]
fn maximal_commit_records_still_take_one_provider_page_each() {
    let fixture = Fixture::new("area-p7-maximal-provider-pages", |_source| 0);
    fixture.fork("maximal-provider");
    let branch = BranchId::named(
        LayerStackId::from_authority([23; 16]),
        &HistoryName::new("maximal-provider").unwrap(),
    );
    let mut expected = Vec::new();
    for n in 0..3 {
        expected.push(
            direct_commit(
                &fixture,
                branch,
                ObjectId::for_bytes(format!("P7 maximal provider root {n}").as_bytes()),
                maximal_meta(),
            )
            .unwrap()
            .id,
        );
    }
    expected.reverse();
    let pages = provider_pages(&fixture, branch, expected.len());
    assert_eq!(pages.len(), 3);
    assert!(pages.iter().all(|page| page.records.len() == 1));
    assert_eq!(
        pages
            .iter()
            .flat_map(|page| page.records.iter().map(|record| record.id))
            .collect::<Vec<_>>(),
        expected
    );
    fixture.cleanup();
}

#[test]
fn mixed_commit_records_pack_by_actual_width_and_resume_exactly_once() {
    let fixture = Fixture::new("area-p7-mixed-provider-pages", |_source| 0);
    fixture.fork("mixed-provider");
    let branch = BranchId::named(
        LayerStackId::from_authority([23; 16]),
        &HistoryName::new("mixed-provider").unwrap(),
    );
    let old_tiny = direct_commit(
        &fixture,
        branch,
        ObjectId::for_bytes(b"P7 mixed old tiny"),
        CommitMeta::new(CommitVia::Workspace),
    )
    .unwrap();
    let maximal = direct_commit(
        &fixture,
        branch,
        ObjectId::for_bytes(b"P7 mixed maximal"),
        maximal_meta(),
    )
    .unwrap();
    let big = direct_commit(
        &fixture,
        branch,
        ObjectId::for_bytes(b"P7 mixed big"),
        CommitMeta {
            message: Some("m".repeat(4000)),
            author: Some("a".repeat(4000)),
            ..CommitMeta::new(CommitVia::Workspace)
        },
    )
    .unwrap();
    let new_tiny = direct_commit(
        &fixture,
        branch,
        ObjectId::for_bytes(b"P7 mixed new tiny"),
        CommitMeta::new(CommitVia::Workspace),
    )
    .unwrap();
    let expected = vec![new_tiny.id, big.id, maximal.id, old_tiny.id];
    let pages = provider_pages(&fixture, branch, expected.len());
    assert_eq!(
        pages
            .iter()
            .map(|page| page.records.len())
            .collect::<Vec<_>>(),
        [2, 2],
        "actual widths must stop before the next record would overflow"
    );
    let found = pages
        .iter()
        .flat_map(|page| page.records.iter().map(|record| record.id))
        .collect::<Vec<_>>();
    assert_eq!(
        found, expected,
        "provider pages repeated or skipped a Commit"
    );
    let unique = found.iter().collect::<std::collections::HashSet<_>>();
    assert_eq!(unique.len(), found.len());
    fixture.cleanup();
}
