//! `workspace.status` of a busy Workspace (conformance R1 F9): its `op`
//! names the operation, the phase the daemon reports and when the
//! operation began; a daemon that reports neither still answers busy with
//! the activity's own phase word and no time.
use layerfs_bridge::control::SdkValue;
use layerfs_sandbox::workspace::answers::status;

fn daemon(activity: &str, phase: Option<&str>, since: Option<i64>) -> SdkValue {
    SdkValue::map([
        ("activity", SdkValue::text(activity)),
        ("activityPhase", SdkValue::opt_text(phase)),
        (
            "activitySinceMs",
            since.map_or(SdkValue::Null, SdkValue::Int),
        ),
        ("changes", SdkValue::List(Vec::new())),
    ])
}

fn op(answer: &SdkValue) -> (Option<&str>, Option<&str>, SdkValue) {
    let op = answer.get("op").unwrap_or_else(|| panic!("{answer:?}"));
    (
        op.get_text("name"),
        op.get_text("phase"),
        op.get("started_at").cloned().unwrap_or_default(),
    )
}

#[test]
fn a_busy_op_carries_the_daemons_phase_and_start() {
    let busy = status(&daemon("Committing", Some("publish"), Some(0)), "s:w");
    assert_eq!(busy.get_text("state"), Some("busy"));
    let (name, phase, started) = op(&busy);
    assert_eq!((name, phase), (Some("commit"), Some("publish")));
    assert_eq!(started, SdkValue::text("1970-01-01T00:00:00Z"));
    let busy = status(&daemon("Attaching", Some("attach"), Some(0)), "s:w");
    assert_eq!(op(&busy).0, Some("mount"));
    // an older daemon: the activity's phase word, no time
    let busy = status(&daemon("Closing", None, None), "s:w");
    let (name, phase, started) = op(&busy);
    assert_eq!((name, phase), (Some("unmount"), Some("close")));
    assert_eq!(started, SdkValue::Null);
    // idle: no op at all
    let ready = status(&daemon("Idle", None, Some(5)), "s:w");
    assert_eq!(ready.get("op"), Some(&SdkValue::Null));
}
