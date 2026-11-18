//! `workspace.mount` and `workspace.commit` long forms name the Branch's
//! head as `branch get` does: a fresh fork has no Commit yet, so the
//! mount's `commit` and its first Commit's `parent` are the Layer the
//! Branch was forked at, not null.
use layerfs_bridge::control::SdkValue;
use layerfs_sandbox::workspace::answers;
use layerfs_sandbox::workspace::calls::Mounted;

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
}
