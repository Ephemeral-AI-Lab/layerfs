//! P14: Branch reply-budget pages resume after returned items, and every
//! Workspace resolves its Branch even when the project has more than 64.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_history::{BranchId, HistoryName, LayerStackId};
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
    assert_eq!(first.get_bool("hasMore"), true);
    assert_eq!(
        first.get_int("nextPagePos"),
        Some(branch_names(&first).len() as i64)
    );
    let second = list_branches(&fixture, 1000, first.get_int("nextPagePos").unwrap());
    assert_eq!(second.get_bool("hasMore"), false);
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
