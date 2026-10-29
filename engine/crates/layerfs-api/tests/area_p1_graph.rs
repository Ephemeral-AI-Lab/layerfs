//! P1 graph pages bound their SQL work by skip and output, not total history.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_bridge::sdk_values::hex;
use layerfs_history::{
    BranchId, CommitId, CommitMeta, CommitOutcome, CommitVia, HistoryName, LayerStackId,
};
use support::{Fixture, Work};

fn graph(fixture: &Fixture, branch: &str, pos: i64) -> SdkValue {
    fixture.call(
        "branch.graph",
        vec![
            ("branch", SdkValue::text(branch)),
            ("pageSize", SdkValue::Int(1)),
            ("pagePos", SdkValue::Int(pos)),
        ],
    )
}

fn graph_two(fixture: &Fixture, branch: &str) -> SdkValue {
    fixture.call(
        "branch.graph",
        vec![
            ("branch", SdkValue::text(branch)),
            ("pageSize", SdkValue::Int(2)),
            ("pagePos", SdkValue::Int(0)),
        ],
    )
}

fn graph_ids(value: &SdkValue) -> Vec<&str> {
    match value.get("commits") {
        Some(SdkValue::List(commits)) => commits
            .iter()
            .map(|commit| commit.get_text("id").unwrap())
            .collect(),
        other => panic!("commits: {other:?}"),
    }
}

fn published(value: &SdkValue) -> Vec<bool> {
    match value.get("commits") {
        Some(SdkValue::List(commits)) => commits
            .iter()
            .map(|commit| commit.get_bool("published"))
            .collect(),
        other => panic!("commits: {other:?}"),
    }
}

fn branch_id(name: &str) -> BranchId {
    BranchId::named(
        LayerStackId::from_authority([23; 16]),
        &HistoryName::new(name).unwrap(),
    )
}

fn commit_to(fixture: &Fixture, name: &str, n: usize) -> CommitId {
    let branch = branch_id(name);
    let binding = fixture
        .service
        .store
        .history()
        .branch_snapshot(branch)
        .unwrap()
        .unwrap();
    let outcome = fixture
        .service
        .store
        .history()
        .commit_branch(&layerfs_history::CommitRequest {
            branch,
            expected_head: binding.branch.head_commit,
            expected_base: binding.branch.base_layer,
            expected_root: binding.effective_root,
            candidate_root: layerfs_content::ObjectId::for_bytes(format!("{name}-{n}").as_bytes()),
            profile: binding.profile,
            scope: binding.scope,
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
        })
        .unwrap();
    match outcome {
        CommitOutcome::Committed { record, .. } => record.id,
        other => panic!("commit to {name} was not committed: {other:?}"),
    }
}

fn publish(fixture: &Fixture, branch: &str) {
    fixture.call("branch.publish", vec![("branch", SdkValue::text(branch))]);
}

fn grow_main(fixture: &Fixture, expected: &mut Vec<String>, from: usize, to: usize) {
    for n in from..to {
        let commit = commit_to(fixture, "mainline", n);
        publish(fixture, "mainline");
        expected.push(hex(commit.as_slice()));
    }
}

fn assert_main_pages(expected: &[String], first: &SdkValue, later: &SdkValue) {
    assert_eq!(graph_ids(first), [expected[expected.len() - 1].as_str()]);
    assert_eq!(published(first), [true]);
    assert!(first.get_bool("hasMore"));
    assert_eq!(first.get_int("nextPagePos"), Some(1));
    assert_eq!(graph_ids(later), [expected[expected.len() - 51].as_str()]);
    assert_eq!(published(later), [true]);
    assert!(later.get_bool("hasMore"));
    assert_eq!(later.get_int("nextPagePos"), Some(51));
}

fn assert_bounded(size: &str, small: Work, large: Work) {
    println!(
        "P1_GRAPH_{size} rows_100={} rows_200={} statements_100={} statements_200={}",
        small.rows, large.rows, small.statements, large.statements
    );
    assert!(
        large.rows <= small.rows + 16,
        "{size} rows grew with history: 100->{}, 200->{}",
        small.rows,
        large.rows
    );
    assert!(
        large.statements <= small.statements + 2,
        "{size} statements grew with history: 100->{}, 200->{}",
        small.statements,
        large.statements
    );
}

#[test]
fn main_graph_streams_multi_page_history_with_bounded_sql_work() {
    let fixture = Fixture::new("area-p1-main-graph-work", |_source| 0);
    fixture.fork("mainline");
    let mut expected = Vec::new();
    grow_main(&fixture, &mut expected, 0, 100);
    let before = fixture.work();
    let first100 = graph(&fixture, "main", 0);
    let later100 = graph(&fixture, "main", 50);
    let work100 = fixture.since(before);
    assert_main_pages(&expected, &first100, &later100);

    grow_main(&fixture, &mut expected, 100, 200);
    let before = fixture.work();
    let first200 = graph(&fixture, "main", 0);
    let later200 = graph(&fixture, "main", 50);
    let work200 = fixture.since(before);
    assert_main_pages(&expected, &first200, &later200);
    assert_bounded("MAIN", work100, work200);
    fixture.cleanup();
}

#[test]
fn branch_graph_checks_current_publication_with_bounded_sql_work() {
    let fixture = Fixture::new("area-p1-branch-graph-work", |_source| 0);
    fixture.fork("target");
    commit_to(&fixture, "target", 0);
    let middle = commit_to(&fixture, "target", 1);
    let newest = commit_to(&fixture, "target", 2);
    publish(&fixture, "target");
    fixture.fork("mainline");
    let mut main = Vec::new();
    grow_main(&fixture, &mut main, 0, 100);

    let before = fixture.work();
    let page100 = graph_two(&fixture, "target");
    let work100 = fixture.since(before);
    assert_eq!(
        graph_ids(&page100),
        [hex(newest.as_slice()), hex(middle.as_slice())]
    );
    assert_eq!(
        published(&page100),
        [true, false],
        "absorbing a parent into a published Layer does not directly publish it"
    );
    assert!(page100.get_bool("hasMore"));
    assert_eq!(page100.get_int("nextPagePos"), Some(2));

    grow_main(&fixture, &mut main, 100, 200);
    let before = fixture.work();
    let page200 = graph_two(&fixture, "target");
    let work200 = fixture.since(before);
    assert_eq!(graph_ids(&page200), graph_ids(&page100));
    assert_eq!(published(&page200), [true, false]);
    assert!(page200.get_bool("hasMore"));
    assert_eq!(page200.get_int("nextPagePos"), Some(2));
    assert_bounded("BRANCH", work100, work200);
    fixture.cleanup();
}
