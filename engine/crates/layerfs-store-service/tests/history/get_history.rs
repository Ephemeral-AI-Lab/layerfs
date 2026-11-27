//! `backend.store.project.get_history` and the `snapshot` behind
//! `sandbox.list_workspaces`: the read model, read from the Store with no
//! daemon, so the dsh project page starts no Sandbox to show a project. It
//! lists the Branches, their Commits and the Layers, names the Branches the
//! Workspaces are mounted on, and `since` its version answers `unchanged`
//! until what it shows moves. Normal profile.
use crate::support;

use layerfs_history::HistoryName;
use layerfs_protocol::sdk::{SdkCall, SdkValue};
use layerfs_store_service::families::operator::{self, WorkspaceView};
use std::fs;
use support::{write, Fixture};

fn history(fixture: &Fixture, since: Option<&str>) -> SdkValue {
    let mut args = vec![("project".to_owned(), SdkValue::text("installed"))];
    args.extend(since.map(|v| ("since".to_owned(), SdkValue::text(v))));
    let call = SdkCall {
        method: "backend.store.project.get_history".into(),
        args: SdkValue::Map(args),
    };
    operator::dispatch(&fixture.service, &call).unwrap()
}

fn names(value: &SdkValue, list: &str, field: &str) -> Vec<String> {
    match value.get(list) {
        Some(SdkValue::List(items)) => items
            .iter()
            .filter_map(|item| item.get_text(field).map(str::to_owned))
            .collect(),
        other => panic!("{list}: {other:?}"),
    }
}

#[test]
fn history_is_read_from_the_store_alone() {
    let fixture = Fixture::new("operator-history", |source| {
        fs::write(source.join("seed.txt"), b"seed\n").unwrap();
        1
    });
    let first = history(&fixture, None);
    assert_eq!(first.get_text("kind"), Some("ok"));
    assert_eq!(first.get_text("project"), Some("installed"));
    assert_eq!(names(&first, "branches", "name"), ["main"]);
    assert_eq!(first.get("workspaces"), Some(&SdkValue::List(Vec::new())));
    let version = first.get_text("version").unwrap().to_owned();
    let same = history(&fixture, Some(&version));
    assert_eq!(same.get_text("kind"), Some("unchanged"));

    fixture.fork("feature");
    fixture.apply("feature", vec![write("/feature.txt", "new\n")]);
    let moved = history(&fixture, Some(&version));
    assert_eq!(moved.get_text("kind"), Some("ok"));
    assert_eq!(names(&moved, "branches", "name"), ["main", "feature"]);
    assert_eq!(names(&moved, "commits", "branch"), ["feature"]);
    assert_eq!(moved.get("workspaces"), Some(&SdkValue::List(Vec::new())));
    fixture.cleanup();
}

#[test]
fn workspaces_name_branches_beyond_the_first_page() {
    let fixture = Fixture::new("operator-history-branches", |source| {
        fs::write(source.join("seed.txt"), b"seed\n").unwrap();
        1
    });
    for n in 1..=70 {
        fixture.fork(&format!("b{n:02}"));
    }
    let history = fixture.service.store.history();
    let installed = HistoryName::new("installed").unwrap();
    let stack = history
        .layer_stack_named(&installed)
        .unwrap()
        .expect("installed stack")
        .id;
    let branch = history
        .branch_named(stack, &HistoryName::new("b70").unwrap())
        .unwrap()
        .expect("b70 exists");
    let binding = history
        .branch_snapshot(branch.id)
        .unwrap()
        .expect("b70 snapshot");
    // Move the head after the Workspace's binding was taken, so the answer
    // also proves it reads the Branch's current head and not the page's.
    fixture.apply("b70", vec![write("/after-mount.txt", "moved\n")]);
    let view = WorkspaceView {
        workspace: "workspace-4".into(),
        namespace: 4,
        binding,
        activity: "idle",
        mounted: true,
        directory: Some("/workspaces/4".into()),
        dirty: None,
        pending_merge: false,
        revision: None,
    };
    let args = SdkValue::Map(vec![("project".to_owned(), SdkValue::text("installed"))]);
    let answer = operator::snapshot(&fixture.service.store, &args, &[view]).unwrap();
    assert_eq!(answer.get("moreBranches"), Some(&SdkValue::Bool(true)));
    let workspaces = match answer.get("workspaces") {
        Some(SdkValue::List(items)) => items,
        other => panic!("workspaces: {other:?}"),
    };
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].get_text("branch"), Some("b70"));
    assert_eq!(
        workspaces[0].get("headMovedSinceMount"),
        Some(&SdkValue::Bool(true))
    );
    fixture.cleanup();
}

/// The Workspaces-only answer a daemon gives a caller that sent
/// `history: false` (layerfs-project#181): the same Workspace items as the
/// whole model, on main and on a Branch beyond the first page, and nothing
/// of the history half.
#[test]
fn the_workspaces_alone_are_the_snapshot_s_workspaces() {
    let fixture = Fixture::new("operator-workspaces-only", |source| {
        fs::write(source.join("seed.txt"), b"seed\n").unwrap();
        1
    });
    for n in 1..=70 {
        fixture.fork(&format!("b{n:02}"));
    }
    let history = fixture.service.store.history();
    let installed = HistoryName::new("installed").unwrap();
    let stack = history
        .layer_stack_named(&installed)
        .unwrap()
        .expect("installed stack")
        .id;
    let binding = |name: &str| {
        let branch = history
            .branch_named(stack, &HistoryName::new(name).unwrap())
            .unwrap()
            .unwrap_or_else(|| panic!("{name} exists"));
        history
            .branch_snapshot(branch.id)
            .unwrap()
            .unwrap_or_else(|| panic!("{name} snapshot"))
    };
    let view = |n: i64, name: &str| WorkspaceView {
        workspace: format!("workspace-{n}"),
        namespace: n,
        binding: binding(name),
        activity: "idle",
        mounted: true,
        directory: Some(format!("/workspaces/{n}")),
        dirty: Some((n as u64, 2)),
        pending_merge: false,
        revision: Some(n),
    };
    // two Workspaces on one Branch, one beyond the first Branch page with
    // its head moved since, and one on main
    let mut views = vec![view(1, "b01"), view(2, "b01"), view(3, "b70")];
    fixture.apply("b70", vec![write("/after-mount.txt", "moved\n")]);
    views.push(view(4, "main"));
    let args = SdkValue::Map(vec![("project".to_owned(), SdkValue::text("installed"))]);
    let whole = operator::snapshot(&fixture.service.store, &args, &views).unwrap();
    let alone = operator::workspaces(&fixture.service.store, &args, &views).unwrap();
    assert_eq!(alone.get_text("kind"), Some("ok"));
    assert_eq!(alone.get("workspaces"), whole.get("workspaces"));
    assert_eq!(names(&alone, "workspaces", "branch").len(), 4, "{alone:?}");
    assert_eq!(names(&alone, "workspaces", "branch")[2], "b70");
    let SdkValue::Map(fields) = &alone else {
        panic!("{alone:?}")
    };
    let keys: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(keys, ["kind", "version", "workspaces"]);
    // `since` its own version answers `unchanged`, as the whole model does
    let version = alone.get_text("version").unwrap().to_owned();
    let since = SdkValue::Map(vec![
        ("project".to_owned(), SdkValue::text("installed")),
        ("since".to_owned(), SdkValue::Text(version)),
    ]);
    let same = operator::workspaces(&fixture.service.store, &since, &views).unwrap();
    assert_eq!(same.get_text("kind"), Some("unchanged"));
    // no Workspace of the project: an empty list, still no history
    let none = operator::workspaces(&fixture.service.store, &args, &[]).unwrap();
    assert_eq!(none.get("workspaces"), Some(&SdkValue::List(Vec::new())));
    fixture.cleanup();
}
