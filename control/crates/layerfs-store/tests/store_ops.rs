//! The engine method and arguments each typed Store call sends (#109 R2):
//! the same names and order the call's args map gave before, an absent
//! option not sent.
use layerfs_bridge::control::SdkValue;
use layerfs_store::calls::branch::{BranchFork, BranchMerge, BranchPublish};
use layerfs_store::calls::commit::{CommitGet, CommitPage, DiffPage};
use layerfs_store::calls::{Names, Page, StoreOp};

fn names(op: &StoreOp<'_>) -> Vec<String> {
    op.engine_args().into_iter().map(|(name, _)| name).collect()
}

fn t(text: &str) -> String {
    text.to_owned()
}

#[test]
fn absent_options_are_not_sent_and_names_keep_their_order() {
    let fork = BranchFork {
        name: t("b"),
        from: Some(t("main")),
        ..BranchFork::default()
    };
    let op = StoreOp::Fork(&fork);
    assert_eq!(op.method(), "branch.fork");
    assert_eq!(
        op.engine_args(),
        vec![
            (t("name"), SdkValue::text("b")),
            (t("from"), SdkValue::text("main")),
        ]
    );
    let fork = BranchFork {
        at: Some(t("abc")),
        ..fork
    };
    assert_eq!(names(&StoreOp::Fork(&fork)), ["name", "from", "at"]);
    // into an existing Branch: no `name`; into a new one: `name`
    let merge = BranchMerge {
        branch: t("x"),
        from: t("y"),
        units: Some(SdkValue::Null),
        ..BranchMerge::default()
    };
    assert_eq!(names(&StoreOp::Merge(&merge)), ["branch", "with"]);
    let merge = BranchMerge {
        into: Some(t("m")),
        expected: Some(t("c0ffee00")),
        ..merge
    };
    assert_eq!(
        names(&StoreOp::Merge(&merge)),
        ["branch", "with", "name", "expected"]
    );
    let page = StoreOp::ListBranches {
        name: None,
        page: Page {
            number: None,
            size: Some(5),
            pos: None,
        },
    };
    assert_eq!(page.method(), "project.list_branches");
    assert_eq!(names(&page), ["limit"]);
    let names_page = StoreOp::Branches {
        names: Names {
            prefix: Some("dsh."),
            after: Some("dsh.a"),
            ..Names::default()
        },
        limit: 100,
    };
    assert_eq!(names_page.method(), "project.list_branches");
    assert_eq!(names(&names_page), ["prefix", "after", "limit"]);
}

#[test]
fn a_diff_sends_its_sides_and_a_later_page_its_pins() {
    let first = DiffPage {
        reference: Some(t("agent")),
        path: Some(t("/src")),
        limit: 100,
        ..DiffPage::default()
    };
    let op = StoreOp::Diff(&first);
    assert_eq!(op.method(), "commit.diff");
    assert_eq!(names(&op), ["ref", "path", "hunks", "limit"]);
    let later = DiffPage {
        hunks: true,
        after: Some(t("/src/a")),
        limit: 100,
        pins: Some((t("to"), None)),
        ..DiffPage::default()
    };
    assert_eq!(
        names(&StoreOp::Diff(&later)),
        ["hunks", "after", "limit", "pinned", "pinTo"]
    );
}

#[test]
fn history_pages_are_forwarded_without_resolving_heads() {
    let history = CommitPage {
        branch: t("main"),
        at: Some(t("saved")),
        after: Some(t("last")),
        limit: 5,
    };
    assert_eq!(
        StoreOp::ListCommits(&history).method(),
        "branch.list_commits"
    );
    assert_eq!(
        StoreOp::ListCommits(&history).engine_args(),
        vec![
            (t("branch"), SdkValue::text("main")),
            (t("at"), SdkValue::text("saved")),
            (t("after"), SdkValue::text("last")),
            (t("limit"), SdkValue::Int(5)),
        ]
    );
    let detail = CommitGet {
        project: t("s/p"),
        commit: t("saved"),
        branch: None,
    };
    assert_eq!(StoreOp::GetCommit(&detail).method(), "branch.get_commit");
    assert_eq!(names(&StoreOp::GetCommit(&detail)), ["commit"]);
}

#[test]
fn a_publish_sends_its_commit_expected_head_and_force() {
    // The API layer's snake_case options reach the Store as the names its
    // method reads; an absent expectation is not sent.
    let publish = BranchPublish {
        branch: t("work"),
        at: Some(t("cc")),
        expected: Some(t("aa")),
        force: Some(false),
        ..BranchPublish::default()
    };
    let op = StoreOp::Publish(&publish);
    assert_eq!(op.method(), "branch.publish");
    assert_eq!(
        op.engine_args(),
        vec![
            (t("branch"), SdkValue::text("work")),
            (t("at"), SdkValue::text("cc")),
            (t("expected"), SdkValue::text("aa")),
            (t("force"), SdkValue::Bool(false)),
        ]
    );
    let plain = BranchPublish {
        branch: t("work"),
        ..BranchPublish::default()
    };
    assert_eq!(
        StoreOp::Publish(&plain).engine_args(),
        vec![(t("branch"), SdkValue::text("work"))]
    );
}
