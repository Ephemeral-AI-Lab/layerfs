//! B5 (#548): `sandbox.close` refuses a stale generation without side
//! effects, blocks on a live exec lease, marks an expired lease `Lost`,
//! and is idempotent under replay. No runtime is started.
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-b5-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn sandbox(home: &Path, name: &str, record: &str) {
    write_record(
        &paths::sandbox_record(home, name),
        &layerfs_api::json::parse(record).unwrap(),
    )
    .unwrap();
}

/// A process that has ended (and been reaped).
fn dead_pid() -> i64 {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    child.wait().unwrap();
    i64::from(child.id())
}

/// The `sandbox.list` item of `name`: the record and its derived state,
/// without reaching a machine.
fn listed(home: &Path, name: &str) -> SdkValue {
    let list = call(home, "sandbox.list", &[]).unwrap();
    match list.get("items") {
        Some(SdkValue::List(items)) => items
            .iter()
            .find(|item| item.get_text("name") == Some(name))
            .cloned()
            .unwrap_or_else(|| panic!("{name} not listed: {list:?}")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_stale_generation_close_changes_nothing() {
    let home = tempdir("stale");
    home::open(&home).unwrap();
    sandbox(
        &home,
        "s",
        r#"{"name":"s","project":"shop/app","machine":"native","generation":5}"#,
    );

    let (code, message) = call(
        &home,
        "sandbox.close",
        &[("name", t("s")), ("generation", SdkValue::Int(4))],
    )
    .unwrap_err();
    assert_eq!(code, "StaleGeneration", "{message}");
    assert!(paths::sandbox_dir(&home, "s").exists());
    let got = call(&home, "sandbox.get", &[("name", t("s"))]).unwrap();
    assert_eq!(got.get_int("generation"), Some(5), "{got:?}");
    assert_eq!(got.get_text("state"), Some("closed"), "{got:?}");

    let closed = call(
        &home,
        "sandbox.close",
        &[("name", t("s")), ("generation", SdkValue::Int(5))],
    )
    .unwrap();
    assert_eq!(closed.get_text("was"), Some("closed"), "{closed:?}");
    assert!(!paths::sandbox_dir(&home, "s").exists());

    // a replay is a quiet no-op, not a second close
    let replay = call(
        &home,
        "sandbox.close",
        &[("name", t("s")), ("generation", SdkValue::Int(5))],
    )
    .unwrap();
    assert!(replay.get_bool("quiet"), "{replay:?}");
    assert_eq!(replay.get_int("generation"), Some(5), "{replay:?}");

    // a generation the tombstone does not know is stale
    let (code, _) = call(
        &home,
        "sandbox.close",
        &[("name", t("s")), ("generation", SdkValue::Int(6))],
    )
    .unwrap_err();
    assert_eq!(code, "StaleGeneration");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_live_exec_lease_blocks_the_close() {
    let home = tempdir("lease");
    home::open(&home).unwrap();
    sandbox(
        &home,
        "s",
        r#"{"name":"s","project":"shop/app","machine":"native","generation":3,
            "exec_leases":[{"id":"lease-0000000000000001","deadline":9999999999999}]}"#,
    );
    let (code, message) = call(
        &home,
        "sandbox.close",
        &[("name", t("s")), ("generation", SdkValue::Int(3))],
    )
    .unwrap_err();
    assert_eq!(code, "LeaseActive", "{message}");
    assert!(message.contains("lease-0000000000000001"), "{message}");
    assert!(paths::sandbox_dir(&home, "s").exists());
    let record = read_record(&paths::sandbox_record(&home, "s"))
        .unwrap()
        .unwrap();
    assert_eq!(record.get_int("generation"), Some(3));
    assert!(record.get("op").is_none(), "no close intent was written");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn an_expired_exec_lease_is_lost_and_observable() {
    let home = tempdir("expired");
    home::open(&home).unwrap();
    sandbox(
        &home,
        "s",
        r#"{"name":"s","project":"shop/app","machine":"native","generation":8,
            "exec_leases":[{"id":"lease-0000000000000002","deadline":1}]}"#,
    );
    let (code, message) = call(
        &home,
        "sandbox.close",
        &[("name", t("s")), ("generation", SdkValue::Int(8))],
    )
    .unwrap_err();
    assert_eq!(code, "Lost", "{message}");
    assert!(paths::sandbox_dir(&home, "s").exists());
    let got = call(&home, "sandbox.get", &[("name", t("s"))]).unwrap();
    assert_eq!(got.get_text("state"), Some("lost"), "{got:?}");
    assert_eq!(
        got.get_text("close_reason"),
        Some("an exec lease expired"),
        "{got:?}"
    );
    // the expired lease is gone, so a second close reclaims the record
    let reclaimed = call(
        &home,
        "sandbox.close",
        &[("name", t("s")), ("generation", SdkValue::Int(8))],
    )
    .unwrap();
    assert_eq!(reclaimed.get_text("was"), Some("lost"), "{reclaimed:?}");
    assert!(!paths::sandbox_dir(&home, "s").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn an_exec_lease_holds_a_close_and_is_released_on_drop() {
    let home = tempdir("held");
    home::open(&home).unwrap();
    sandbox(
        &home,
        "s",
        r#"{"name":"s","project":"shop/app","machine":"native","generation":1}"#,
    );
    // B5 (#548): a running exec holds the lease the close reads. The close
    // must not kill it; dropping the guard releases the lease.
    let lease = layerfs_control::sandbox::exec_lease::ExecLease::begin(&home, "s").unwrap();
    let held = read_record(&paths::sandbox_record(&home, "s"))
        .unwrap()
        .unwrap();
    assert!(held.get("exec_leases").is_some(), "{held:?}");
    let (code, message) = call(
        &home,
        "sandbox.close",
        &[("name", t("s")), ("generation", SdkValue::Int(1))],
    )
    .unwrap_err();
    assert_eq!(code, "LeaseActive", "{message}");
    drop(lease);
    let released = read_record(&paths::sandbox_record(&home, "s"))
        .unwrap()
        .unwrap();
    assert!(released.get("exec_leases").is_none(), "{released:?}");
    let closed = call(
        &home,
        "sandbox.close",
        &[("name", t("s")), ("generation", SdkValue::Int(1))],
    )
    .unwrap();
    assert_eq!(closed.get_text("was"), Some("closed"), "{closed:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_dry_run_close_decides_and_changes_nothing() {
    let home = tempdir("dry");
    home::open(&home).unwrap();
    sandbox(
        &home,
        "s",
        r#"{"name":"s","project":"shop/app","machine":"native","id":"s","generation":5}"#,
    );
    let before = read_record(&paths::sandbox_record(&home, "s")).unwrap();
    let preview = call(
        &home,
        "sandbox.close",
        &[("name", t("s")), ("dry_run", SdkValue::Bool(true))],
    )
    .unwrap();
    assert!(preview.get_bool("dry_run"), "{preview:?}");
    assert_eq!(preview.get_text("decision"), Some("drain"), "{preview:?}");
    assert_eq!(preview.get_int("generation"), Some(5), "{preview:?}");
    assert!(preview.get_text("reason").is_some(), "{preview:?}");
    // nothing was written: no CAS, no intent, no tombstone
    assert_eq!(
        read_record(&paths::sandbox_record(&home, "s")).unwrap(),
        before
    );
    assert!(paths::sandbox_dir(&home, "s").exists());
    assert!(!home.join("sandbox/s.closed.json").exists());

    // an unstarted record is previewed, not removed
    sandbox(
        &home,
        "u",
        r#"{"name":"u","project":"shop/app","machine":"native","generation":2}"#,
    );
    let preview = call(
        &home,
        "sandbox.close",
        &[("name", t("u")), ("dry_run", SdkValue::Bool(true))],
    )
    .unwrap();
    assert_eq!(preview.get_text("decision"), Some("reclaim"), "{preview:?}");
    assert!(paths::sandbox_dir(&home, "u").exists());
    assert!(!home.join("sandbox/u.closed.json").exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_close_that_died_while_draining_is_unresolved_until_one_close_clears_it() {
    let home = tempdir("crashed-close");
    home::open(&home).unwrap();
    let dead = dead_pid();
    // The fault: the process that won the generation CAS ended before it
    // could stop the runtime or settle the record.
    sandbox(
        &home,
        "c",
        &format!(
            r#"{{"name":"c","project":"shop/app","machine":"native","generation":6,
                "close_reason":"an operator asked to close it",
                "op":{{"id":"op-00000000000000c1","name":"sandbox.close","phase":"draining",
                       "args":null,"owner":{{"pid":{dead}}},"started":1,"updated":2,
                       "uncertain":true}}}}"#
        ),
    );
    let got = call(&home, "sandbox.get", &[("name", t("c"))]).unwrap();
    assert_eq!(got.get_text("state"), Some("unresolved"), "{got:?}");
    assert_eq!(
        got.get_text("close_reason"),
        Some("an operator asked to close it"),
        "{got:?}"
    );
    assert_eq!(
        got.get("op").and_then(|op| op.get_text("phase")),
        Some("draining"),
        "{got:?}"
    );
    // No unbounded retry: the read above settled nothing, and a workspace
    // call is refused until one explicit close clears the intent.
    let (code, message) = call(
        &home,
        "workspace.exec",
        &[("ws", t("c:/w")), ("command", t("true"))],
    )
    .unwrap_err();
    assert_eq!(code, "Unresolved", "{message}");
    assert!(message.contains("sandbox get c"), "{message}");
    assert!(message.contains("sandbox close c"), "{message}");
    let again = call(&home, "sandbox.get", &[("name", t("c"))]).unwrap();
    assert_eq!(again.get_text("state"), Some("unresolved"), "{again:?}");

    // One close clears it and records why; a replay stays a quiet no-op.
    let closed = call(&home, "sandbox.close", &[("name", t("c"))]).unwrap();
    assert_eq!(closed.get_text("was"), Some("unresolved"), "{closed:?}");
    assert!(
        closed.get_text("left").unwrap_or("").contains("close"),
        "{closed:?}"
    );
    assert!(!paths::sandbox_dir(&home, "c").exists());
    let replay = call(&home, "sandbox.close", &[("name", t("c"))]).unwrap();
    assert!(replay.get_bool("quiet"), "{replay:?}");
    assert_eq!(replay.get_text("reason"), Some("unresolved"), "{replay:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_started_crashed_close_is_unresolved_with_its_reason() {
    let home = tempdir("crashed-started");
    home::open(&home).unwrap();
    let dead = dead_pid();
    sandbox(
        &home,
        "s",
        &format!(
            r#"{{"name":"s","project":"shop/app","machine":"native","id":"s","generation":7,
                "close_reason":"the close outcome is unknown; the runtime may or may not be stopped",
                "op":{{"id":"op-00000000000000c2","name":"sandbox.close","phase":"draining",
                       "args":null,"owner":{{"pid":{dead}}},"started":1,"updated":2,
                       "uncertain":true}}}}"#
        ),
    );
    let got = listed(&home, "s");
    assert_eq!(got.get_text("state"), Some("unresolved"), "{got:?}");
    assert_eq!(
        got.get_text("close_reason"),
        Some("the close outcome is unknown; the runtime may or may not be stopped"),
        "{got:?}"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_stopped_reclaim_is_kept_lost_with_the_brake_as_its_reason() {
    let home = tempdir("reclaim-brake");
    home::open(&home).unwrap();
    let reason = "automatic reclamation was stopped: reclaim is frozen (LAYERFS_RECLAIM_FREEZE)";
    sandbox(
        &home,
        "s",
        &format!(
            r#"{{"name":"s","project":"shop/app","machine":"native","id":"s","generation":9,
                "lost":true,"close_reason":"{reason}"}}"#
        ),
    );
    // A Lost record whose runtime was recorded is never deleted blindly: the
    // close refuses and leaves the evidence for the provider probe.
    let (code, message) = call(&home, "sandbox.close", &[("name", t("s"))]).unwrap_err();
    assert_eq!(code, "Lost", "{message}");
    assert!(paths::sandbox_dir(&home, "s").exists());
    let got = listed(&home, "s");
    assert_eq!(got.get_text("state"), Some("lost"), "{got:?}");
    assert_eq!(got.get_text("close_reason"), Some(reason), "{got:?}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn an_exec_between_the_close_decision_and_the_kill_is_refused() {
    let home = tempdir("between");
    home::open(&home).unwrap();
    let me = i64::from(std::process::id());
    // The close won its generation CAS: the record is Draining and the
    // runtime has not been stopped yet.
    sandbox(
        &home,
        "s",
        &format!(
            r#"{{"name":"s","project":"shop/app","machine":"native","generation":4,
                "op":{{"id":"op-00000000000000c3","name":"sandbox.close","phase":"draining",
                       "args":null,"owner":{{"pid":{me}}},"started":1,"updated":2,
                       "uncertain":true}}}}"#
        ),
    );
    // An exec that starts now is refused before it can take a lease, so the
    // close can never kill a command that began after its decision.
    let refused = match layerfs_control::sandbox::exec_lease::ExecLease::begin(&home, "s") {
        Ok(_) => panic!("an exec took a lease while the Sandbox was draining"),
        Err(refusal) => refusal,
    };
    assert_eq!(refused.code, "Busy", "{}", refused.message);
    let record = read_record(&paths::sandbox_record(&home, "s"))
        .unwrap()
        .unwrap();
    assert!(record.get("exec_leases").is_none(), "{record:?}");
    std::fs::remove_dir_all(home).unwrap();
}
