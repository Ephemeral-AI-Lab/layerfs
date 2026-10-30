//! P11 storage statistics over real disposable Stores and the public SDK values.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{SdkCall, SdkRefusal, SdkValue};
use layerfs_store_service::operator;
use std::fs;
use support::{write, Change, Fixture};

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn stats(fixture: &Fixture, commits: Option<bool>) -> SdkValue {
    let mut args = vec![("project".to_owned(), SdkValue::text("installed"))];
    if let Some(commits) = commits {
        args.push(("commits".to_owned(), SdkValue::Bool(commits)));
    }
    let call = SdkCall {
        method: "backend.store.project.get_storage".into(),
        args: SdkValue::Map(args),
    };
    operator::dispatch(&fixture.service, &call)
        .unwrap_or_else(|refusal| panic!("get_storage refused: {refusal:?}"))
}

fn try_stats(fixture: &Fixture, fields: Vec<(&str, SdkValue)>) -> Result<SdkValue, SdkRefusal> {
    let mut args = vec![("project".to_owned(), SdkValue::text("installed"))];
    for (k, v) in fields {
        args.push((k.to_owned(), v));
    }
    let call = SdkCall {
        method: "backend.store.project.get_storage".into(),
        args: SdkValue::Map(args),
    };
    operator::dispatch(&fixture.service, &call)
}

fn kind_map<'a>(commits: &'a SdkValue, kind: &str) -> &'a SdkValue {
    commits
        .get(kind)
        .unwrap_or_else(|| panic!("missing kind {kind} in {commits:?}"))
}

fn kind_figures(commits: &SdkValue, kind: &str) -> (u64, u64, u64, u64) {
    let map = kind_map(commits, kind);
    let get = |field| {
        map.get_int(field)
            .unwrap_or_else(|| panic!("missing {field}")) as u64
    };
    (
        get("count"),
        get("newCanonical"),
        get("newStored"),
        get("newMetadata"),
    )
}

#[test]
fn empty_store_statistics_and_defaults() {
    let fixture = Fixture::new("p11-empty", |_| 0);

    // Default call: disk present, commits absent
    let default_stats = stats(&fixture, None);
    let disk = default_stats.get("disk").expect("disk must be present");
    let main = disk.get_int("main").expect("main") as u64;
    let wal = disk.get_int("wal").expect("wal") as u64;
    let total = disk.get_int("total").expect("total") as u64;

    assert!(main > 0, "main sqlite file has bytes");
    assert_eq!(total, main + wal, "disk.total is main + wal");
    assert_eq!(
        default_stats.get("commits"),
        None,
        "commits absent by default"
    );

    // Independent filesystem check
    let main_meta = fs::metadata(&fixture.host.config.path).unwrap().len();
    assert_eq!(main, main_meta, "disk.main matches filesystem metadata");

    // Explicit commits: false also omits commits
    let false_stats = stats(&fixture, Some(false));
    assert_eq!(
        false_stats.get("commits"),
        None,
        "commits absent when commits: false"
    );

    // Explicit commits: true returns all three kinds, zero-filled for an empty store
    let true_stats = stats(&fixture, Some(true));
    let commits = true_stats
        .get("commits")
        .expect("commits present when requested");
    assert_eq!(kind_figures(commits, "Workspace"), (0, 0, 0, 0));
    assert_eq!(kind_figures(commits, "Files"), (0, 0, 0, 0));
    assert_eq!(kind_figures(commits, "Merge"), (0, 0, 0, 0));
}

#[test]
fn commit_kind_statistics_independent_counts() {
    let fixture = Fixture::new("p11-kinds", |source| {
        fs::write(source.join("base.txt"), b"base\n").unwrap();
        1
    });
    fixture.fork("p11-branch");

    // 1 Workspace commit on p11-branch
    fixture.workspace_commit(
        "p11-branch",
        &[Change::Create("/from-ws.txt", b"workspace-data-bytes\n")],
        1,
        || {},
    );

    // 1 Files commit on p11-branch
    fixture.apply(
        "p11-branch",
        vec![write("/from-files.txt", "files-data-bytes\n")],
    );

    // Fork feature branch from p11-branch
    fixture.call(
        "branch.fork",
        vec![("name", text("feature")), ("from", text("p11-branch"))],
    );
    // Make 1 Files commit on feature
    fixture.apply(
        "feature",
        vec![write("/feature-file.txt", "feature-data-bytes\n")],
    );
    // Make 1 Files commit on p11-branch so the two branches diverge (requires true merge commit)
    fixture.apply(
        "p11-branch",
        vec![write("/diverge.txt", "diverge-data-bytes\n")],
    );

    // 1 Merge commit: merge feature with p11-branch into merged-branch
    fixture.call(
        "branch.merge",
        vec![
            ("branch", text("p11-branch")),
            ("with", text("feature")),
            ("name", text("merged-branch")),
            ("message", text("Merge feature")),
        ],
    );

    // Query stats with commits: true
    let result = stats(&fixture, Some(true));
    let commits = result.get("commits").expect("commits present");

    let (ws_count, ws_canon, ws_stored, ws_meta) = kind_figures(commits, "Workspace");
    let (files_count, files_canon, files_stored, files_meta) = kind_figures(commits, "Files");
    let (merge_count, merge_canon, merge_stored, merge_meta) = kind_figures(commits, "Merge");

    assert_eq!(ws_count, 1, "exactly 1 Workspace commit");
    assert!(ws_canon > 0, "Workspace commit added canonical bytes");
    assert!(ws_stored > 0, "Workspace commit added stored bytes");
    assert!(ws_meta > 0, "Workspace commit added metadata bytes");

    assert_eq!(
        files_count, 3,
        "exactly 3 Files commits across all branches"
    );
    assert!(files_canon > 0, "Files commits added canonical bytes");
    assert!(files_stored > 0, "Files commits added stored bytes");
    assert!(files_meta > 0, "Files commits added metadata bytes");

    assert_eq!(merge_count, 1, "exactly 1 Merge commit");
    assert!(
        merge_canon > 0,
        "Merge commit added canonical bytes for merged directory root"
    );
    assert!(merge_stored > 0, "Merge commit added stored bytes");
    assert!(merge_meta > 0, "Merge commit added metadata bytes");

    // Total count across all 3 kinds in the whole catalog equals 5
    assert_eq!(ws_count + files_count + merge_count, 5);
}

#[test]
fn refusal_on_invalid_options() {
    let fixture = Fixture::new("p11-refusals", |_| 0);

    // Non-boolean commits option
    let refusal =
        try_stats(&fixture, vec![("commits", SdkValue::Int(1))]).expect_err("int commits refused");
    assert_eq!(refusal.code, "InvalidInput");
    assert!(refusal.message.contains("commits must be a boolean"));

    let refusal = try_stats(&fixture, vec![("commits", SdkValue::text("true"))])
        .expect_err("text commits refused");
    assert_eq!(refusal.code, "InvalidInput");

    // Unknown option key
    let refusal = try_stats(&fixture, vec![("unknownOption", SdkValue::Bool(true))])
        .expect_err("unknown option refused");
    assert_eq!(refusal.code, "InvalidInput");
    assert!(refusal.message.contains("unknown option"));
}
