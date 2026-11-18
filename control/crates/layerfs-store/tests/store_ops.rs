//! The engine method and arguments each typed Store call sends (#109 R2):
//! the same names and order the call's args map gave before, an absent
//! option not sent.
use layerfs_bridge::control::SdkValue;
use layerfs_store::calls::branch::{BranchFork, BranchMerge, BranchPublish};
use layerfs_store::calls::commit::{CommitGet, CommitPage, DiffPage};
use layerfs_store::calls::file::{FileApply, FileDiff, FileRead, FileWrite, Side};
use layerfs_store::calls::{Change, Content, Names, Page, StoreOp};

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
    let read = FileRead {
        branch: t("main"),
        path: t("/a"),
        commit: Some(t("c")),
        end_line: Some(3),
        ..FileRead::default()
    };
    assert_eq!(
        names(&StoreOp::Read(&read)),
        ["branch", "commit", "path", "endLine"]
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
    // a file diff's side is its Commit, else its Branch; an empty side null
    let files = FileDiff {
        left: Side {
            branch: Some(t("main")),
            commit: Some(t("c1")),
            path: Some(t("/a")),
        },
        ..FileDiff::default()
    };
    let args = StoreOp::DiffFiles(&files).engine_args();
    assert_eq!(
        args[0].1,
        SdkValue::map([
            ("commit", SdkValue::text("c1")),
            ("path", SdkValue::text("/a"))
        ])
    );
    assert_eq!(
        args[1].1,
        SdkValue::map([("commit", SdkValue::Null), ("path", SdkValue::Null)])
    );
}

#[test]
fn bytes_content_is_sent_as_bytes() {
    let write = FileWrite {
        project: t("s/p"),
        branch: t("main"),
        path: t("/a"),
        content: Content::Bytes(vec![1, 2]),
        message: None,
        mode: Some(0o644),
    };
    let args = StoreOp::Write(&write).engine_args();
    assert_eq!(
        args,
        vec![
            (t("branch"), SdkValue::text("main")),
            (t("path"), SdkValue::text("/a")),
            (t("mode"), SdkValue::Int(0o644)),
            (t("bytes"), SdkValue::Bytes(vec![1, 2])),
        ]
    );
    let apply = FileApply {
        branch: t("main"),
        changes: vec![Change {
            fields: vec![
                (t("op"), SdkValue::text("write")),
                (t("to_path"), SdkValue::text("/b")),
                (t("content"), SdkValue::Bytes(vec![3])),
                (t("old"), SdkValue::text("x")),
            ],
        }],
        ..FileApply::default()
    };
    let args = StoreOp::Apply(&apply).engine_args();
    assert_eq!(
        args[1].1,
        SdkValue::List(vec![SdkValue::map([
            ("op", SdkValue::text("write")),
            ("to", SdkValue::text("/b")),
            ("bytes", SdkValue::Bytes(vec![3])),
            ("oldText", SdkValue::text("x")),
        ])])
    );
}

#[test]
fn history_pages_and_file_selectors_are_forwarded_without_resolving_heads() {
    use layerfs_store::calls::file::{FileBlame, FileRevision};
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
    let numbered = Page {
        number: Some(2),
        size: Some(5),
        pos: None,
    };
    let revision = FileRevision {
        branch: t("feature"),
        path: t("/src/a"),
        at_commit: Some(t("a")),
        traversal: Some(t("provenance")),
        page: numbered,
        ..FileRevision::default()
    };
    assert_eq!(StoreOp::Revision(&revision).method(), "fs.revision");
    assert_eq!(
        names(&StoreOp::Revision(&revision)),
        ["branch", "commit", "path", "traversal", "pageSize", "page"]
    );
    let blame = FileBlame {
        line_count: Some(7),
        ..FileBlame::default()
    };
    assert_eq!(
        names(&StoreOp::Blame(&blame)),
        ["branch", "path", "lineCount"]
    );
    let file_diff = FileDiff {
        include_attribution: Some(true),
        allow_missing: Some(true),
        page: numbered,
        ..FileDiff::default()
    };
    assert_eq!(
        names(&StoreOp::DiffFiles(&file_diff)),
        [
            "left",
            "right",
            "includeAttribution",
            "allowMissing",
            "pageSize",
            "page"
        ]
    );
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
