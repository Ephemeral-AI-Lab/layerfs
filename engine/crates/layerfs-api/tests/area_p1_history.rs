//! P1 history graph and changed-path coverage over a real disposable Store.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::fs;
use support::{write, Fixture};

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn graph(fixture: &Fixture, branch: &str, limit: i64, pos: i64) -> SdkValue {
    fixture.call(
        "branch.graph",
        vec![
            ("branch", text(branch)),
            ("pageSize", SdkValue::Int(limit)),
            ("pagePos", SdkValue::Int(pos)),
        ],
    )
}

fn changed(fixture: &Fixture, commit: &str, limit: i64, pos: i64) -> SdkValue {
    fixture.call(
        "branch.changed_paths",
        vec![
            ("commit", text(commit)),
            ("pageSize", SdkValue::Int(limit)),
            ("pagePos", SdkValue::Int(pos)),
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

fn apply_message(fixture: &Fixture, branch: &str, path: &str, message: &str) -> String {
    fixture
        .call(
            "fs.apply",
            vec![
                ("branch", text(branch)),
                ("changes", SdkValue::List(vec![write(path, path)])),
                ("message", text(message)),
            ],
        )
        .get_text("head")
        .unwrap()
        .to_owned()
}

#[test]
fn graph_bounds_forks_merges_main_and_publication() {
    let fixture = Fixture::new("area-p1-graph", |source| {
        fs::write(source.join("base.txt"), b"base\n").unwrap();
        1
    });
    fixture.fork("layer");
    let empty = graph(&fixture, "layer", 50, 0);
    assert!(graph_ids(&empty).is_empty());
    let layer = empty
        .get("branch")
        .unwrap()
        .get_text("base")
        .unwrap()
        .to_owned();
    assert_eq!(
        empty.get("forkPoint").unwrap().get_text("kind"),
        Some("layer")
    );
    assert_eq!(
        empty.get("forkPoint").unwrap().get_text("id"),
        Some(layer.as_str())
    );

    fixture.apply("layer", vec![write("/first.txt", "first\n")]);
    fixture.apply("layer", vec![write("/second.txt", "second\n")]);
    let before = graph(&fixture, "layer", 50, 0);
    let ids = graph_ids(&before);
    assert_eq!(ids.len(), 2);
    assert_eq!(published(&before), [false, false]);
    let newest = ids[0].to_owned();
    fixture.call("branch.publish", vec![("branch", text("layer"))]);
    let after = graph(&fixture, "layer", 50, 0);
    assert_eq!(graph_ids(&after), ids);
    assert_eq!(
        published(&after),
        [true, false],
        "publish is derived per call"
    );

    fixture.call(
        "branch.fork",
        vec![("name", text("commit-fork")), ("from", text(&newest))],
    );
    let fresh = graph(&fixture, "commit-fork", 50, 0);
    assert!(graph_ids(&fresh).is_empty(), "the Commit fork is exclusive");
    assert_eq!(
        fresh.get("forkPoint").unwrap().get_text("kind"),
        Some("commit")
    );
    assert_eq!(
        fresh.get("forkPoint").unwrap().get_text("id"),
        Some(newest.as_str())
    );
    fixture.apply("commit-fork", vec![write("/own.txt", "own\n")]);
    let own = graph(&fixture, "commit-fork", 50, 0);
    assert_eq!(graph_ids(&own).len(), 1);
    assert_ne!(graph_ids(&own)[0], newest);
    assert!(graph_ids(&graph(&fixture, "commit-fork", 50, 1)).is_empty());

    fixture.fork("left");
    fixture.fork("right");
    fixture.apply("left", vec![write("/left.txt", "left\n")]);
    fixture.apply("right", vec![write("/right.txt", "right\n")]);
    let merged = fixture.call(
        "branch.merge",
        vec![
            ("branch", text("left")),
            ("with", text("right")),
            ("name", text("merged")),
        ],
    );
    let merge_id = merged.get_text("head").unwrap();
    let merge_graph = graph(&fixture, "merged", 50, 0);
    assert_eq!(
        graph_ids(&merge_graph),
        [merge_id],
        "stop before the left node"
    );
    assert_eq!(
        merge_graph.get("forkPoint").unwrap().get_text("kind"),
        Some("commit")
    );

    let main = graph(&fixture, "main", 50, 0);
    assert_eq!(graph_ids(&main), [newest]);
    assert_eq!(published(&main), [true]);
    assert_eq!(main.get("forkPoint"), Some(&SdkValue::Null));
    assert_eq!(main.get("branch").unwrap().get("forkedFrom"), None);
    assert_eq!(main.get("commits").unwrap().get("branch"), None);
    match main.get("commits") {
        Some(SdkValue::List(commits)) => {
            assert_eq!(commits[0].get_text("branch"), Some("main"));
        }
        other => panic!("commits: {other:?}"),
    }

    let listed = fixture
        .try_call("branch.list_commits", vec![("branch", text("ghost"))])
        .unwrap_err();
    let graphed = fixture
        .try_call("branch.graph", vec![("branch", text("ghost"))])
        .unwrap_err();
    assert_eq!(graphed.code, listed.code);
    assert_eq!(graphed.message, listed.message);
    assert_eq!(graphed.detail, listed.detail);
    fixture.cleanup();
}

#[test]
fn graph_budget_short_pages_traverse_the_bounded_walk() {
    let fixture = Fixture::new("area-p1-graph-budget", |_source| 0);
    fixture.fork("budget");
    let message = "x".repeat(4096);
    let mut expected = Vec::new();
    for n in 0..12 {
        expected.push(apply_message(
            &fixture,
            "budget",
            &format!("/f{n:02}.txt"),
            &message,
        ));
    }
    expected.reverse();
    let first = graph(&fixture, "budget", 1000, 0);
    assert!(graph_ids(&first).len() < 1000);
    assert_eq!(first.get_bool("hasMore"), true);
    assert_eq!(
        first.get_int("nextPagePos"),
        Some(graph_ids(&first).len() as i64)
    );
    let second = graph(
        &fixture,
        "budget",
        1000,
        first.get_int("nextPagePos").unwrap(),
    );
    assert_eq!(second.get_bool("hasMore"), false);
    assert_eq!(second.get("nextPagePos"), Some(&SdkValue::Null));
    assert_eq!([graph_ids(&first), graph_ids(&second)].concat(), expected);
    fixture.cleanup();
}

#[test]
fn changed_paths_uses_parent_base_layer_or_empty_tree() {
    let fixture = Fixture::new("area-p1-changed", |source| {
        fs::write(source.join("base.txt"), b"base\n").unwrap();
        1
    });
    fixture.fork("feature");
    fixture.apply(
        "feature",
        vec![
            write("/root.txt", "root\n"),
            write("/base.txt", "changed\n"),
        ],
    );
    let history = fixture.call("branch.list_commits", vec![("branch", text("feature"))]);
    let root = support::items(&history)[0]
        .get_text("id")
        .unwrap()
        .to_owned();
    let root_changed = changed(&fixture, &root, 50, 0);
    assert_eq!(root_changed.get("parent"), Some(&SdkValue::Null));
    assert_eq!(support::paths(&root_changed), ["/base.txt", "/root.txt"]);
    assert!(support::items(&root_changed)
        .iter()
        .all(|item| item.get_text("kind") == Some("Added")));

    fixture.apply("feature", vec![write("/second.txt", "second\n")]);
    let history = fixture.call("branch.list_commits", vec![("branch", text("feature"))]);
    let newest = support::items(&history)[0]
        .get_text("id")
        .unwrap()
        .to_owned();
    let parent_changed = changed(&fixture, &newest, 50, 0);
    assert_eq!(parent_changed.get_text("parent"), Some(root.as_str()));
    let diff = fixture.call(
        "branch.diff_commits",
        vec![
            ("from", text(&root)),
            ("to", text(&newest)),
            ("hunks", SdkValue::Bool(false)),
        ],
    );
    assert_eq!(parent_changed.get("items"), diff.get("items"));
    assert_eq!(support::paths(&parent_changed), ["/second.txt"]);

    fixture.fork("main-right");
    fixture.apply("main-right", vec![write("/right.txt", "right\n")]);
    fixture.fork("publisher");
    fixture.apply("publisher", vec![write("/main.txt", "main\n")]);
    fixture.call("branch.publish", vec![("branch", text("publisher"))]);
    let merged = fixture.call(
        "branch.merge",
        vec![
            ("branch", text("main")),
            ("with", text("main-right")),
            ("name", text("parentless")),
        ],
    );
    assert_eq!(merged.get_bool("fastForward"), false);
    let merge_id = merged.get_text("head").unwrap();
    let merge_changed = changed(&fixture, merge_id, 50, 0);
    assert_eq!(merge_changed.get("parent"), Some(&SdkValue::Null));
    assert_eq!(
        support::paths(&merge_changed),
        ["/right.txt"],
        "a parentless merge diffs its base Layer, not an empty tree"
    );

    let unknown = format!(
        "{}{}",
        &newest[..newest.len() - 1],
        if newest.ends_with('0') { '1' } else { '0' }
    );
    for (input, code) in [("not-a-commit", "InvalidInput"), (&unknown, "NoSuchCommit")] {
        let refusal = fixture
            .try_call("branch.changed_paths", vec![("commit", text(input))])
            .unwrap_err();
        assert_eq!(refusal.code, code, "{refusal:?}");
    }
    let layer = graph(&fixture, "feature", 1, 0)
        .get("branch")
        .unwrap()
        .get_text("base")
        .unwrap()
        .to_owned();
    let refusal = fixture
        .try_call("branch.changed_paths", vec![("commit", text(&layer))])
        .unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{refusal:?}");
    fixture.cleanup();
}

#[test]
fn changed_paths_budget_short_pages_return_every_root_path() {
    let fixture = Fixture::new("area-p1-changed-budget", |_source| 0);
    fixture.fork("root");
    let expected = (0..220)
        .map(|n| format!("/f{n:03}-{}", "x".repeat(220)))
        .collect::<Vec<_>>();
    fixture.apply(
        "root",
        expected.iter().map(|path| write(path, "x\n")).collect(),
    );
    let history = fixture.call("branch.list_commits", vec![("branch", text("root"))]);
    let commit = support::items(&history)[0]
        .get_text("id")
        .unwrap()
        .to_owned();
    let first = changed(&fixture, &commit, 1000, 0);
    assert!(support::items(&first).len() < 1000);
    assert_eq!(first.get_bool("hasMore"), true);
    assert_eq!(
        first.get_int("nextPagePos"),
        Some(support::items(&first).len() as i64)
    );
    let second = changed(
        &fixture,
        &commit,
        1000,
        first.get_int("nextPagePos").unwrap(),
    );
    assert_eq!(second.get_bool("hasMore"), false);
    assert_eq!(second.get("nextPagePos"), Some(&SdkValue::Null));
    assert_eq!(
        [support::paths(&first), support::paths(&second)].concat(),
        expected
    );
    fixture.cleanup();
}
