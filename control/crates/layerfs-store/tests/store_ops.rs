//! The engine method and arguments each typed Store call sends (#109 R2):
//! the same names and order the call's args map gave before, an absent
//! option not sent.
use layerfs_bridge::control::SdkValue;
use layerfs_store::calls::branch::{BranchDiff, BranchFork, BranchMerge};
use layerfs_store::calls::file::{FileApply, FileDiff, FileRead, FileWrite, Side};
use layerfs_store::calls::{Change, Content, DiffOptions, Page, StoreOp};

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
        from_branch: t("main"),
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
        from_commit: Some(t("abc")),
        ..fork
    };
    assert_eq!(
        StoreOp::Fork(&fork).engine_args()[1].1,
        SdkValue::text("abc")
    );
    let merge = BranchMerge {
        name: t("m"),
        a: t("x"),
        b: t("y"),
        units: Some(SdkValue::Null),
        ..BranchMerge::default()
    };
    assert_eq!(names(&StoreOp::Merge(&merge)), ["branch", "with", "name"]);
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
            size: Some(5),
            pos: None,
        },
    };
    assert_eq!(page.method(), "project.list_branches");
    assert_eq!(names(&page), ["pageSize"]);
}

#[test]
fn a_diff_sends_hunks_as_the_opposite_of_paths_only() {
    let diff = BranchDiff {
        from_branch: t("a"),
        to_branch: t("b"),
        options: DiffOptions {
            page: Page {
                size: Some(10),
                pos: Some(0),
            },
            cursor: Some(t("k")),
            paths_only: Some(true),
        },
        ..BranchDiff::default()
    };
    let args = StoreOp::DiffBranches(&diff).engine_args();
    let names: Vec<&str> = args.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        ["from", "to", "pageSize", "pagePos", "cursor", "hunks"]
    );
    assert_eq!(args[5].1, SdkValue::Bool(false));
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
