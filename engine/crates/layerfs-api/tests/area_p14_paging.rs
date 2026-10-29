//! P14: Branch reply-budget pages resume after returned items, and every
//! Workspace resolves its Branch even when the project has more than 64.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_bridge::sdk_values::hex;
use layerfs_history::{
    BranchId, CommitHistoryRequest, CommitId, CommitMeta, CommitOutcome, CommitVia, HistoryName,
    LayerStackId,
};
use layerfs_store_service::operator::{self, WorkspaceView};
use support::Fixture;

const REPLY_BUDGET: usize = 48 * 1024;

fn branch_name(n: usize) -> String {
    format!("b{n:03}-{}", "x".repeat(58))
}

fn list_branches(fixture: &Fixture, page_size: i64, page_pos: i64) -> SdkValue {
    fixture.call(
        "project.list_branches",
        vec![
            ("pageSize", SdkValue::Int(page_size)),
            ("pagePos", SdkValue::Int(page_pos)),
        ],
    )
}

fn branch_names(value: &SdkValue) -> Vec<String> {
    match value.get("items") {
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|item| item.get_text("name").unwrap().to_owned())
            .collect(),
        other => panic!("items: {other:?}"),
    }
}

fn expected_page(names: &[String], page_size: usize, page_pos: usize) -> (Vec<String>, bool) {
    let mut items = Vec::new();
    let mut spent = 0;
    for name in names.iter().skip(page_pos) {
        if items.len() == page_size || spent + name.len() + 512 > REPLY_BUDGET {
            return (items, true);
        }
        spent += name.len() + 512;
        items.push(name.clone());
    }
    (items, false)
}

fn assert_page(page: &SdkValue, names: &[String], page_size: usize, page_pos: usize) {
    let (items, more) = expected_page(names, page_size, page_pos);
    assert_eq!(branch_names(page), items, "pagePos={page_pos}");
    assert_eq!(page.get_bool("hasMore"), more, "pagePos={page_pos}");
    let next = if more {
        Some((page_pos + items.len()) as i64)
    } else {
        None
    };
    assert_eq!(page.get_int("nextPagePos"), next, "pagePos={page_pos}");
}

#[test]
fn branch_pages_include_main_and_resume_after_returned_items() {
    let fixture = Fixture::new("area-p14-branches", |_source| 0);
    let mut made = 0;
    for count in [1usize, 2, 3, 64, 65, 66, 130] {
        while made < count - 1 {
            fixture.fork(&branch_name(made));
            made += 1;
        }
        let expected = (0..count - 1)
            .map(branch_name)
            .chain(["main".to_owned()])
            .collect::<Vec<_>>();
        for page_size in [1usize, 2, 5, 50] {
            for page_pos in [0, 1, count - 1, count, count + 10] {
                let page = list_branches(&fixture, page_size as i64, page_pos as i64);
                assert_page(&page, &expected, page_size, page_pos);
            }
            let mut found = Vec::new();
            let mut page_pos = 0;
            let mut calls = 0;
            loop {
                calls += 1;
                assert!(calls <= count + 1, "pagination did not terminate");
                let page = list_branches(&fixture, page_size as i64, page_pos);
                found.extend(branch_names(&page));
                match page.get_int("nextPagePos") {
                    Some(next) => page_pos = next,
                    None => break,
                }
            }
            assert_eq!(found, expected, "pageSize={page_size}");
        }
    }

    let expected = (0..129)
        .map(branch_name)
        .chain(["main".to_owned()])
        .collect::<Vec<_>>();
    let first = list_branches(&fixture, 1000, 0);
    assert!(branch_names(&first).len() < 1000);
    assert!(first.get_bool("hasMore"));
    assert_eq!(
        first.get_int("nextPagePos"),
        Some(branch_names(&first).len() as i64)
    );
    let second = list_branches(&fixture, 1000, first.get_int("nextPagePos").unwrap());
    assert!(!second.get_bool("hasMore"));
    assert_eq!(second.get("nextPagePos"), Some(&SdkValue::Null));
    assert_eq!(
        [branch_names(&first), branch_names(&second)].concat(),
        expected
    );
    println!(
        "P14_BRANCHES first_page_items={} requested=1000 hasMore=true nextPagePos={} all_items=130 exactly_once=true",
        branch_names(&first).len(),
        first.get_int("nextPagePos").unwrap()
    );
    fixture.cleanup();
}

#[test]
fn operator_snapshot_discloses_oversized_units_within_the_control_budget() {
    let fixture = Fixture::new("area-p14-commit-budget", |_source| 0);
    fixture.fork("operator-budget");
    let branch = BranchId::named(
        LayerStackId::from_authority([23; 16]),
        &HistoryName::new("operator-budget").unwrap(),
    );
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
            candidate_root: layerfs_content::ObjectId::for_bytes(b"P14 oversized units"),
            profile: binding.profile,
            scope: binding.scope,
            second_parent: None,
            meta: layerfs_history::CommitMeta {
                units: vec![layerfs_content::merge::MergeUnit {
                    name: "oversized".into(),
                    patterns: vec![format!("/{}", "u".repeat(70 * 1024))],
                }],
                ..layerfs_history::CommitMeta::new(layerfs_history::CommitVia::Merge)
            },
            force: false,
        })
        .unwrap();
    assert!(matches!(
        outcome,
        layerfs_history::CommitOutcome::Committed { .. }
    ));
    let value = snapshot(&fixture, &[]);
    let answer = layerfs_bridge::control::Answer {
        id: 1,
        reply: layerfs_bridge::control::Reply::Sdk(value.clone()),
    };
    let encoded = answer.encode().unwrap();
    assert!(encoded.len() <= 65_519, "control bytes: {}", encoded.len());
    assert_eq!(
        layerfs_bridge::control::Answer::decode(&encoded).unwrap(),
        answer
    );
    let commits = match value.get("commits") {
        Some(SdkValue::List(commits)) => commits,
        other => panic!("commits: {other:?}"),
    };
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].get("units"), None);
    assert_eq!(
        commits[0].get("unitsTruncated"),
        Some(&SdkValue::Bool(true))
    );
    fixture.cleanup();
}

fn branch_id(name: &str) -> BranchId {
    BranchId::named(
        LayerStackId::from_authority([23; 16]),
        &HistoryName::new(name).unwrap(),
    )
}

fn small_meta() -> CommitMeta {
    CommitMeta::new(CommitVia::Merge)
}

fn big_meta() -> CommitMeta {
    CommitMeta {
        message: Some("m".repeat(4000)),
        author: Some("a".repeat(4000)),
        labels: vec!["l".repeat(1000); 3],
        ..CommitMeta::new(CommitVia::Merge)
    }
}

fn commit_to(fixture: &Fixture, name: &str, meta: CommitMeta, root: &[u8]) -> CommitId {
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
            candidate_root: layerfs_content::ObjectId::for_bytes(root),
            profile: binding.profile,
            scope: binding.scope,
            second_parent: None,
            meta,
            force: false,
        })
        .unwrap();
    match outcome {
        CommitOutcome::Committed { record, .. } => record.id,
        other => panic!("commit to {name} was not committed: {other:?}"),
    }
}

fn snapshot_commit_ids(value: &SdkValue) -> Vec<String> {
    match value.get("commits") {
        Some(SdkValue::List(commits)) => commits
            .iter()
            .map(|commit| commit.get_text("id").unwrap().to_owned())
            .collect(),
        other => panic!("commits: {other:?}"),
    }
}

/// Retro finding 1 review, BLOCKING: when a Branch's newest Commit does not
/// fit the remaining snapshot budget and older Commits remain, the snapshot
/// previously pushed both restart (Null) and the provider cursor for the
/// same Branch, a duplicate map key the control decoder rejects. The fix
/// pushes exactly one continuation: restart, which is exact here because
/// provider pages hold one Commit each, so nothing was emitted for the
/// Branch yet and restart yields the rejected Commit first.
#[test]
fn snapshot_rejected_commit_yields_one_restart_continuation() {
    let fixture = Fixture::new("area-p14-rejected-continuation", |_source| 0);
    fixture.fork("mmm-target");
    let old = commit_to(&fixture, "mmm-target", small_meta(), b"rejected-old");
    let rejected = commit_to(&fixture, "mmm-target", big_meta(), b"rejected-new");
    fixture.fork("zzz-control");
    let control = commit_to(&fixture, "zzz-control", small_meta(), b"rejected-control");
    // Add small-commit branches (name order before the target) until the
    // snapshot rejects the target's newest Commit while still emitting the
    // control Commit. Control's presence proves room >= 1 at the target's
    // turn, so the target's absence proves rejection, not a zero room.
    let mut added = 0;
    let value = loop {
        let value = snapshot(&fixture, &[]);
        let ids = snapshot_commit_ids(&value);
        let has_rejected = ids.iter().any(|id| id == &hex(rejected.as_slice()));
        let has_control = ids.iter().any(|id| id == &hex(control.as_slice()));
        if !has_rejected && has_control {
            break value;
        }
        // Control missing while the target is still emitted is transient:
        // the emitted target Commit itself consumes the room control needs.
        // Once the target is rejected, its cost stops accruing and control
        // is emitted again. Only both missing means room hit zero at the
        // target itself, i.e. a real overshoot.
        assert!(
            has_rejected || has_control,
            "overshot: neither target nor control emitted after {added} small branches"
        );
        let name = format!("aaa-s{added:03}");
        fixture.fork(&name);
        let root = format!("rejected-small-{added}");
        commit_to(&fixture, &name, small_meta(), root.as_bytes());
        added += 1;
        assert!(added <= 60, "never reached the rejection regime");
    };
    // 1. The whole reply decodes: no duplicate map key anywhere.
    let answer = layerfs_bridge::control::Answer {
        id: 1,
        reply: layerfs_bridge::control::Reply::Sdk(value.clone()),
    };
    let encoded = answer.encode().unwrap();
    assert_eq!(
        layerfs_bridge::control::Answer::decode(&encoded).unwrap(),
        answer
    );
    // 2. Exactly one continuation, for the target, and it is restart.
    let more = match value.get("more") {
        Some(SdkValue::Map(entries)) => entries,
        other => panic!("more: {other:?}"),
    };
    assert_eq!(more.len(), 1, "more: {more:?}");
    assert_eq!(more[0].0, "mmm-target");
    assert_eq!(more[0].1, SdkValue::Null);
    // 3. Restart yields the rejected Commit first, then the older one, each
    // exactly once: neither skipped nor repeated.
    let mut ids = Vec::new();
    let mut cursor = None;
    for _ in 0..16 {
        let page = fixture
            .service
            .store
            .history()
            .commit_history(&CommitHistoryRequest {
                branch: branch_id("mmm-target"),
                start: None,
                cursor,
                limit: 128,
            })
            .unwrap();
        ids.extend(page.records.iter().map(|record| record.id));
        match page.continuation {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(&ids[..2], &[rejected, old]);
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "restart repeated a commit");
    println!("P14_REJECTED target_more=Null restart_first_is_rejected=true small_branches={added}");
    fixture.cleanup();
}

fn workspace_view(fixture: &Fixture, name: &str, n: usize) -> WorkspaceView {
    let branch = if name == "main" {
        fixture.host.branch
    } else {
        BranchId::named(
            LayerStackId::from_authority([23; 16]),
            &HistoryName::new(name).unwrap(),
        )
    };
    let binding = fixture
        .service
        .store
        .history()
        .branch_snapshot(branch)
        .unwrap()
        .unwrap();
    WorkspaceView {
        workspace: format!("workspace-{n:03}"),
        namespace: n as i64,
        binding,
        activity: "idle",
        mounted: true,
        directory: Some(format!("/workspaces/{n:03}")),
        dirty: None,
        pending_merge: false,
        revision: None,
    }
}

fn snapshot(fixture: &Fixture, views: &[WorkspaceView]) -> SdkValue {
    let args = SdkValue::map([("project", SdkValue::text("installed"))]);
    operator::snapshot(&fixture.service.store, &args, views).unwrap()
}

fn workspace_branches(value: &SdkValue) -> Vec<Option<String>> {
    match value.get("workspaces") {
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|item| item.get_text("branch").map(str::to_owned))
            .collect(),
        other => panic!("workspaces: {other:?}"),
    }
}

/// Retro finding 2: a snapshot used to build the full Branch-name map
/// before answering, so a cheap status call paid one provider page per ~81
/// Branches. Names now resolve lazily, so the SQL work of a small-output
/// snapshot must not grow with the Branch count, while a name outside the
/// displayed page still resolves. The counters are SQLite statements and
/// returned rows from `Fixture::work`, not wall time: identical query
/// shapes give identical counts, so the bound is exact up to a small
/// slack for the statistics aggregates, which scan more Branch rows at
/// the larger size.
#[test]
fn snapshot_lazy_branch_names_bound_sql_work() {
    let fixture = Fixture::new("area-p14-lazy-names", |_source| 0);
    let names = (0..130).map(branch_name).collect::<Vec<_>>();
    for name in &names {
        fixture.fork(name);
    }
    let view130 = workspace_view(&fixture, &names[129], 129);
    let before = fixture.work();
    let value130 = snapshot(&fixture, &[view130]);
    let work130 = fixture.since(before);
    assert_eq!(
        workspace_branches(&value130),
        [Some(names[129].clone())],
        "late workspace name must resolve at 130 branches"
    );
    let more = (130..260).map(branch_name).collect::<Vec<_>>();
    for name in &more {
        fixture.fork(name);
    }
    let view260 = workspace_view(&fixture, &more[129], 259);
    let before = fixture.work();
    let value260 = snapshot(&fixture, &[view260]);
    let work260 = fixture.since(before);
    assert_eq!(
        workspace_branches(&value260),
        [Some(more[129].clone())],
        "late workspace name must resolve at 260 branches"
    );
    println!(
        "P14_LAZY rows_130={} rows_260={} statements_130={} statements_260={}",
        work130.rows, work260.rows, work130.statements, work260.statements
    );
    assert!(
        work260.rows <= work130.rows + 16,
        "snapshot rows grew with branch count: 130->{}, 260->{}",
        work130.rows,
        work260.rows
    );
    assert!(
        work260.statements <= work130.statements + 2,
        "snapshot statements grew with branch count: 130->{}, 260->{}",
        work130.statements,
        work260.statements
    );
    fixture.cleanup();
}

/// Retro finding 2, Layer site: a Layer published from a Branch outside
/// the displayed page must still show that Branch's name as its source.
#[test]
fn snapshot_lazy_layer_source_name() {
    let fixture = Fixture::new("area-p14-lazy-layer", |_source| 0);
    let names = (0..130).map(branch_name).collect::<Vec<_>>();
    for name in &names {
        fixture.fork(name);
    }
    let late = &names[129];
    let commit = commit_to(&fixture, late, small_meta(), b"lazy-layer-source");
    fixture.call(
        "branch.publish",
        vec![
            ("branch", SdkValue::text(late)),
            ("commit", SdkValue::text(hex(commit.as_slice()))),
        ],
    );
    let value = snapshot(&fixture, &[]);
    let layers = match value.get("layers") {
        Some(SdkValue::List(layers)) => layers,
        other => panic!("layers: {other:?}"),
    };
    assert!(
        !layers.is_empty(),
        "published layer must be the newest layer"
    );
    assert_eq!(
        layers[0].get_text("sourceBranch"),
        Some(late.as_str()),
        "newest layer source must resolve past the displayed page"
    );
    fixture.cleanup();
}

#[test]
fn workspace_branches_resolve_beyond_the_first_history_page() {
    let fixture = Fixture::new("area-p14-workspaces", |_source| 0);
    let names = (0..129).map(branch_name).collect::<Vec<_>>();
    for name in &names {
        fixture.fork(name);
    }
    let views = names
        .iter()
        .enumerate()
        .map(|(n, name)| workspace_view(&fixture, name, n))
        .collect::<Vec<_>>();
    for count in [0usize, 64, 65, 129] {
        let value = snapshot(&fixture, &views[..count]);
        let expected = names[..count]
            .iter()
            .map(|name| Some(name.clone()))
            .collect::<Vec<_>>();
        let actual = workspace_branches(&value);
        let missing = actual.iter().filter(|name| name.is_none()).count();
        println!("P14_WORKSPACES count={count} missing_branch={missing}");
        assert_eq!(actual, expected, "count={count}");
    }
    let main = snapshot(&fixture, &[workspace_view(&fixture, "main", 129)]);
    assert_eq!(workspace_branches(&main), [Some("main".to_owned())]);
    fixture.cleanup();
}
