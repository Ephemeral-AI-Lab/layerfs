//! `backend.store.project.get_history`: the read model's history half, read from the
//! Store with no daemon, so the dsh project page starts no Sandbox to show a
//! project. It lists the Branches, their Commits and the Layers, holds no
//! Workspaces, and `since` its version answers `unchanged` until history
//! moves. Disposable profile.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkCall, SdkValue};
use layerfs_store_service::operator;
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
