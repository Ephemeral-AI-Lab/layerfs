//! #818 W2 (lane W): a file call binds its private Workspace by project and
//! Branch name in one read, and still refuses in today's order with today's
//! texts: `NoSuchProject` before a refused `branch` argument, before
//! `ReadOnly` for `main`, before `NoSuchBranch`, before any change's own
//! refusal. Runs on SQLite, and on PostgreSQL with
//! `LAYERFS_TEST_POSTGRES_URL` (NOT_RUN without it).
#[path = "support/round_trips.rs"]
mod rt;

use layerfs_protocol::provision::install::{SdkCall, SdkRefusal, SdkValue};
use rt::{t, Opened, Store};

/// `fs.write` with exactly `fields` (no project added).
fn write(opened: &Opened, fields: Vec<(&str, SdkValue)>) -> Result<SdkValue, SdkRefusal> {
    let call = SdkCall {
        method: "fs.write".into(),
        args: SdkValue::Map(
            fields
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        ),
    };
    layerfs_store_service::methods::call(&opened.service, &call)
}

fn refused(opened: &Opened, fields: Vec<(&str, SdkValue)>) -> (String, String) {
    let refusal = write(opened, fields).expect_err("a refusal");
    (refusal.code, refusal.message)
}

fn pair(code: &str, message: &str) -> (String, String) {
    (code.to_owned(), message.to_owned())
}

fn scenario(store: &mut Store) {
    let source = store.dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.txt"), "a\n").unwrap();
    store.init(&source, &mut |_, _| {});
    let opened = store.open("named");
    rt::fork_work(&opened);
    let good = || ("content", t("new\n"));
    let bad = || ("content", SdkValue::Int(1));
    let path = || ("path", t("/a.txt"));
    let no_project = |name: &str| pair("NoSuchProject", &format!("project {name} does not exist"));
    let no_branch = |name: &str| pair("NoSuchBranch", &format!("Branch {name} does not exist"));

    // The project first, whatever else is wrong.
    for branch in [Some("work"), Some("main"), Some("ghost"), Some("a b"), None] {
        for project in ["ghost", "not a name"] {
            let mut fields = vec![("project", t(project)), path(), bad()];
            if let Some(branch) = branch {
                fields.push(("branch", t(branch)));
            }
            assert_eq!(
                refused(&opened, fields),
                no_project(project),
                "{project} / {branch:?}"
            );
        }
    }
    // Then the `branch` argument, then `main`, then the Branch.
    let refusal = refused(&opened, vec![("project", t(rt::PROJECT)), path(), good()]);
    assert_eq!(
        refusal,
        pair("InvalidInput", "missing text argument `branch`")
    );
    let main = vec![
        ("project", t(rt::PROJECT)),
        ("branch", t("main")),
        path(),
        bad(),
    ];
    assert_eq!(
        refused(&opened, main),
        pair("ReadOnly", "main changes only through publish")
    );
    for branch in ["ghost", "a b", "a/b"] {
        let fields = vec![
            ("project", t(rt::PROJECT)),
            ("branch", t(branch)),
            path(),
            bad(),
        ];
        assert_eq!(refused(&opened, fields), no_branch(branch), "{branch}");
    }
    // Then the change.
    let fields = vec![
        ("project", t(rt::PROJECT)),
        ("branch", t(rt::BRANCH)),
        path(),
        bad(),
    ];
    assert_eq!(
        refused(&opened, fields).1,
        "write content must be text or bytes"
    );
    let fields = vec![
        ("project", t(rt::PROJECT)),
        ("branch", t(rt::BRANCH)),
        path(),
        good(),
    ];
    let written = write(&opened, fields).expect("a write");
    assert_eq!(written.get_text("kind"), Some("Committed"), "{written:?}");
    opened.stop();
}

/// A bind that fails before it reads the project and Branch (its reader
/// does not open: the Store's Workspace readers are all held) still answers
/// the lookups first, as before W2: `NoSuchProject`, `NoSuchBranch`, and
/// only then `Failed` with the bind's text, never `Unknown` (M1 review).
/// SQLite only: holding every reader on PostgreSQL holds `CALLS`
/// connections of the shared test server.
#[test]
fn sqlite_reader_refusal_keeps_the_lookups_first() {
    use layerfs_history::WorkspaceId;
    let mut store = Store::sqlite("w2-reader-full");
    let source = store.dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.txt"), "a\n").unwrap();
    store.init(&source, &mut |_, _| {});
    let opened = store.open("full");
    rt::fork_work(&opened);
    let held: Vec<WorkspaceId> = (0..layerfs_store_service::CALLS)
        .map(|n| WorkspaceId::from_authority([n as u8 + 1; 32]).unwrap())
        .collect();
    for workspace in &held {
        opened.service.store.mount_reader(*workspace).unwrap();
    }
    let fields = |project: &str, branch: &str| {
        vec![
            ("project", t(project)),
            ("branch", t(branch)),
            ("path", t("/a.txt")),
            ("content", t("new\n")),
        ]
    };
    assert_eq!(
        refused(&opened, fields("ghost", rt::BRANCH)),
        pair("NoSuchProject", "project ghost does not exist")
    );
    assert_eq!(
        refused(&opened, fields(rt::PROJECT, "ghost")),
        pair("NoSuchBranch", "Branch ghost does not exist")
    );
    let (code, message) = refused(&opened, fields(rt::PROJECT, rt::BRANCH));
    assert_eq!(code, "Failed", "{message}");
    assert!(message.contains("Capacity"), "{message}");
    for workspace in &held {
        opened.service.store.close_reader(*workspace).unwrap();
    }
    let written = write(&opened, fields(rt::PROJECT, rt::BRANCH)).expect("a write");
    assert_eq!(written.get_text("kind"), Some("Committed"), "{written:?}");
    opened.stop();
}

#[test]
fn sqlite_named_bind_keeps_the_refusal_order() {
    let mut store = Store::sqlite("w2-named-sqlite");
    scenario(&mut store);
}

#[test]
fn postgresql_named_bind_keeps_the_refusal_order() {
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql("w2named") else {
        return;
    };
    scenario(&mut store);
}
