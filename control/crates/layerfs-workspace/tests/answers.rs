//! `workspace.mount`, `workspace.commit` and `workspace.status` long forms
//! name the Branch's head as `branch get` does: a fresh fork has no Commit
//! yet, so the mount's `commit`, its first Commit's `parent` and a status's
//! `commit` and `branch_head` are the Layer the Branch was forked at, not
//! null (RF re-check N-B for status).
use layerfs_protocol::provision::install::SdkValue;
use layerfs_workspace::answers;
use layerfs_workspace::route::send::Mounted;

const LAYER: &str = "02aa";
const COMMIT: &str = "01bb";

fn mount(base: SdkValue) -> SdkValue {
    let answer = SdkValue::map([
        ("kind", SdkValue::text("Mounted")),
        ("path", SdkValue::text("/w/1")),
        ("workspace", SdkValue::text("ff")),
        ("namespace", SdkValue::Int(1)),
        ("branch", SdkValue::text("agent")),
        ("base", base),
        ("baseLayer", SdkValue::text(LAYER)),
        ("root", SdkValue::text("cc")),
    ]);
    answers::mounted(&Mounted {
        ws: "box:agent".to_owned(),
        answer,
    })
}

#[test]
fn a_mount_names_the_head_commit_else_the_fork_layer() {
    assert_eq!(mount(SdkValue::Null).get_text("commit"), Some(LAYER));
    assert_eq!(
        mount(SdkValue::text(COMMIT)).get_text("commit"),
        Some(COMMIT)
    );
}

fn commit(parent: SdkValue) -> SdkValue {
    let made = SdkValue::map([
        ("id", SdkValue::text("01dd")),
        ("parent", parent),
        ("base", SdkValue::text(LAYER)),
        ("root", SdkValue::text("cc")),
        ("message", SdkValue::Null),
        ("time", SdkValue::Int(0)),
    ]);
    answers::committed(&SdkValue::map([
        ("kind", SdkValue::text("Committed")),
        ("head", SdkValue::text("01dd")),
        ("commit", made),
        ("displacedHead", SdkValue::Null),
        ("merged", SdkValue::Null),
        ("lww", SdkValue::List(Vec::new())),
        ("lwwTruncated", SdkValue::Bool(false)),
    ]))
}

#[test]
fn a_commit_names_its_parent_commit_else_the_fork_layer() {
    assert_eq!(commit(SdkValue::Null).get_text("parent"), Some(LAYER));
    assert_eq!(
        commit(SdkValue::text(COMMIT)).get_text("parent"),
        Some(COMMIT)
    );
    // no Commit made: nothing it follows
    let unchanged = answers::committed(&SdkValue::map([
        ("kind", SdkValue::text("UpToDate")),
        ("head", SdkValue::text(COMMIT)),
        ("root", SdkValue::text("cc")),
        ("merged", SdkValue::Null),
        ("lww", SdkValue::List(Vec::new())),
        ("lwwTruncated", SdkValue::Bool(false)),
    ]));
    assert_eq!(unchanged.get("parent"), Some(&SdkValue::Null));
    assert_eq!(unchanged.get_text("commit"), Some(COMMIT));
}

/// An unchanged fresh fork has no head Commit: its Commit answers the
/// Layer the Branch was forked at as `commit` (RF follow-up N-H).
#[test]
fn an_unchanged_fresh_fork_names_its_fork_layer() {
    let unchanged = answers::committed(&SdkValue::map([
        ("kind", SdkValue::text("UpToDate")),
        ("head", SdkValue::Null),
        ("root", SdkValue::text("cc")),
        ("merged", SdkValue::Null),
        ("lww", SdkValue::List(Vec::new())),
        ("lwwTruncated", SdkValue::Bool(false)),
        ("baseLayer", SdkValue::text(LAYER)),
    ]));
    assert_eq!(unchanged.get_text("commit"), Some(LAYER));
    assert_eq!(unchanged.get("unchanged"), Some(&SdkValue::Bool(true)));
}

fn status(base: SdkValue, head: SdkValue, moved: SdkValue) -> SdkValue {
    answers::status(
        &SdkValue::map([
            ("base", base),
            ("baseLayer", SdkValue::text(LAYER)),
            ("branchHead", head),
            ("headMovedSinceMount", moved),
            ("activity", SdkValue::text("Idle")),
            ("changes", SdkValue::List(Vec::new())),
        ]),
        "box:agent",
    )
}

#[test]
fn a_status_names_the_head_commit_else_the_fork_layer() {
    let fresh = status(SdkValue::Null, SdkValue::Null, SdkValue::Bool(false));
    assert_eq!(fresh.get_text("commit"), Some(LAYER));
    assert_eq!(fresh.get_text("branch_head"), Some(LAYER));
    let committed = SdkValue::text(COMMIT);
    let moved = status(SdkValue::Null, committed.clone(), SdkValue::Bool(true));
    assert_eq!(moved.get_text("commit"), Some(LAYER));
    assert_eq!(moved.get_text("branch_head"), Some(COMMIT));
    let at = status(committed.clone(), committed, SdkValue::Bool(false));
    assert_eq!(at.get_text("commit"), Some(COMMIT));
    assert_eq!(at.get_text("branch_head"), Some(COMMIT));
    // the head could not be read: no head is named
    let unread = status(SdkValue::Null, SdkValue::Null, SdkValue::Null);
    assert_eq!(unread.get("branch_head"), Some(&SdkValue::Null));
}

/// `workspace commit --expected` compares with the head `branch get`
/// answers (RC-A F2): the listed Branch's `commit`, which is the Layer it
/// was forked at while it has no Commit of its own, and only for the
/// Branch the Workspace is bound to.
#[test]
fn expected_reads_the_head_branch_get_answers() {
    use layerfs_history::BranchId;
    use layerfs_workspace::machine::serve::listed_head;
    let mut bytes = [7; layerfs_history::identity::BRANCH_BYTES];
    bytes[0] = layerfs_history::identity::BRANCH_TAG;
    let id = BranchId::from_slice(&bytes).unwrap();
    let hex = layerfs_protocol::sdk::hex(id.as_slice());
    let listed = |id: &str| {
        let item = SdkValue::map([
            ("branch", SdkValue::text("w3")),
            ("commit", SdkValue::text(LAYER)),
            ("id", SdkValue::text(id)),
            ("head", SdkValue::Null),
        ]);
        SdkValue::map([("items", SdkValue::List(vec![item]))])
    };
    assert_eq!(listed_head(&listed(&hex), id).as_deref(), Some(LAYER));
    assert_eq!(listed_head(&listed("other"), id), None);
    let none = SdkValue::map([("items", SdkValue::List(Vec::new()))]);
    assert_eq!(listed_head(&none, id), None);
}

fn change(path: &str, kind: &str) -> SdkValue {
    SdkValue::map([
        ("path", SdkValue::text(path)),
        ("change", SdkValue::text("added")),
        ("type", SdkValue::text(kind)),
    ])
}

fn dirty(entries: Vec<SdkValue>, partial: bool) -> SdkValue {
    let refusal = layerfs_home::outcome::refused("Dirty", "1 uncommitted change(s)");
    let read = SdkValue::map([
        ("entries", SdkValue::List(entries)),
        ("partial", SdkValue::Bool(partial)),
    ]);
    let refusal = layerfs_workspace::machine::serve::dirty_fields(refusal, Some(&read));
    refusal.detail.get("next").cloned().unwrap()
}

/// A Dirty refusal lists its changes as `workspace status`'s short form
/// does (RC-A F3 and NOTE): `type` left out for a file, and
/// `changes_count` only beside a list cut at its first 20.
#[test]
fn a_dirty_refusal_counts_only_a_cut_list_and_lists_as_status_does() {
    let next = dirty(vec![change("/a.txt", "file"), change("/d", "dir")], false);
    assert_eq!(next.get("changes_count"), None, "{next:?}");
    assert_eq!(
        next.get("changes"),
        Some(&SdkValue::List(vec![
            SdkValue::map([
                ("path", SdkValue::text("/a.txt")),
                ("change", SdkValue::text("added")),
            ]),
            change("/d", "dir"),
        ]))
    );
    assert_eq!(next.get("pending_merge"), Some(&SdkValue::Bool(false)));
    let many = (0..25).map(|i| change(&format!("/{i}"), "file")).collect();
    let next = dirty(many, false);
    let Some(SdkValue::List(shown)) = next.get("changes") else {
        panic!("{next:?}")
    };
    assert_eq!(shown.len(), 20);
    assert_eq!(next.get_int("changes_count"), Some(25));
    // a read that hit its bound is cut too
    let next = dirty(vec![change("/a.txt", "file")], true);
    assert_eq!(next.get_int("changes_count"), Some(1));
}
