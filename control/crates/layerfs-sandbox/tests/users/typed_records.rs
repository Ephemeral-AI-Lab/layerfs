//! Each typed record's `to_sdk` gives the shape its calls answered before
//! the records were typed (#536 §10 "Result keys stay stable"), byte for
//! byte as JSON text, and reads past a layout-3 `v`; a project's record
//! gives the fields every project answer carries (call surface 2.0).
use layerfs_home::json::{parse, print};
use layerfs_home::outcome::View;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::backup::record::BucketRecord;
use layerfs_store::project::record::ProjectRecord;
use layerfs_store::record::StoreRecord;

fn value(text: &str) -> SdkValue {
    parse(text).unwrap()
}

#[test]
fn a_project_answers_its_store_engine_machine_and_creation() {
    let print_answer = |fields: Vec<(&str, SdkValue)>| {
        print(&SdkValue::Map(
            fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        ))
    };
    let sqlite = StoreRecord::from_record("shop", value(r#"{"kind":"sqlite"}"#));
    let project = ProjectRecord::from_record(
        "shop",
        "api",
        value(r#"{"v":1,"name":"api","created":7000}"#),
    );
    assert_eq!(
        print_answer(project.answer(&sqlite)),
        r#"{"project":"shop/api","store":"shop","engine":"sqlite","machine":"HOST_MACHINE","created_at":"1970-01-01T00:00:07Z"}"#.replace("HOST_MACHINE", layerfs_home::names::HOST_MACHINE)
    );
    let placed = StoreRecord::from_record("shop", value(r#"{"kind":"sqlite","machine":"box"}"#));
    let missing = ProjectRecord::from_record("shop", "web", SdkValue::Null);
    assert_eq!(
        print_answer(missing.answer(&placed)),
        r#"{"project":"shop/web","store":"shop","engine":"sqlite","machine":"box","created_at":null}"#
    );
    let unknown = StoreRecord::from_record("shop", value(r#"{"kind":"oracle","machine":null}"#));
    assert_eq!(
        print_answer(missing.answer(&unknown)),
        r#"{"project":"shop/web","store":"shop","engine":null,"machine":null,"created_at":null}"#
    );
    // a PostgreSQL Store is on no machine
    let postgres = StoreRecord::from_record("pg", value(r#"{"kind":"postgresql"}"#));
    let project = ProjectRecord::from_record("pg", "api", value(r#"{"name":"api","stack":"s"}"#));
    assert_eq!(
        print_answer(project.answer(&postgres)),
        r#"{"project":"pg/api","store":"pg","engine":"postgresql","machine":null,"created_at":null}"#
    );
}

#[test]
fn a_bucket_shows_its_record() {
    let text = r#"{"name":"r","provider":"r2","account":"a","jurisdiction":"","bucket":"b","prefix":"p","region":"auto","key":"env:K","part_size":8}"#;
    let record = BucketRecord::from_record("r", value(text));
    assert_eq!(print(&record.to_sdk(View::Full)), text);
    let versioned = BucketRecord::from_record("r", value(&text.replacen('{', r#"{"v":1,"#, 1)));
    assert_eq!(print(&versioned.to_sdk(View::Full)), text);
}

#[test]
fn a_sandbox_shows_its_record() {
    use layerfs_home::state::State;
    use layerfs_sandbox::record::SandboxRecord;
    let text = r#"{"name":"t1","project":"shop/api","machine":"macos-host","created":3,"id":"t1","platform":"macos","runtime":"process","image":null,"backend":"process","route":"nfs"}"#;
    let record = SandboxRecord::from_record("t1", value(text));
    let read = |record: &SandboxRecord| {
        (
            record.name().to_owned(),
            record.project().map(str::to_owned),
            record.machine().to_owned(),
            record.id().map(str::to_owned),
            record.platform().map(str::to_owned),
            record.image().map(str::to_owned),
            record.backend().map(str::to_owned),
            record.route().map(str::to_owned),
            record.created_ms(),
        )
    };
    let shown = (
        "t1".to_owned(),
        Some("shop/api".to_owned()),
        "macos-host".to_owned(),
        Some("t1".to_owned()),
        Some("macos".to_owned()),
        None,
        Some("process".to_owned()),
        Some("nfs".to_owned()),
        Some(3),
    );
    assert_eq!(read(&record), shown);
    // its state is derived, never stored (#536 H4): ready on a ready machine
    assert_eq!(record.status(&State::Ready), State::Ready);
    // layout 3: `v`, `runtime_id` and what reopens it (`daemon`) read the same
    let hidden = text.replacen('{', r#"{"v":1,"#, 1).replacen(
        r#""route":"nfs""#,
        r#""route":"nfs","runtime_id":"c0ffee","daemon":{"pid":7}"#,
        1,
    );
    let versioned = SandboxRecord::from_record("t1", value(&hidden));
    assert_eq!(read(&versioned), shown);
    assert_eq!(versioned.status(&State::Ready), State::Ready);
}

#[cfg(feature = "machine")]
mod machines {
    use super::*;
    use layerfs_home::state::{Reason, State};
    use layerfs_machine::record::MachineRecord;
    use layerfs_machine::state::unrecorded;

    fn machine(name: &str, record: &str) -> MachineRecord {
        MachineRecord::from_record(name, value(record))
    }

    fn broken(state: &State) -> (Reason, Option<&str>) {
        match state {
            State::Broken { reason, fix } => (*reason, fix.as_deref()),
            other => panic!("not broken: {other:?}"),
        }
    }

    #[test]
    fn a_machine_s_state_comes_from_its_record_alone() {
        let dev = |state: &str| {
            machine(
                "dev",
                &format!(r#"{{"platform":"lima","vm_name":"layerfs-0-dev","state":"{state}"}}"#),
            )
        };
        assert_eq!(dev("running").recorded(0), State::Ready);
        assert_eq!(dev("stopped").recorded(0), State::Stopped);
        assert_eq!(
            machine("vm", r#"{"platform":"lima","vm_name":"layerfs-0-vm"}"#).recorded(0),
            State::Stopped
        );
        // an earlier build's record, with no provider name: only its record
        // can go
        let old = machine("docker", r#"{"platform":"docker","state":"running"}"#);
        assert_eq!(
            broken(&old.recorded(0)),
            (Reason::Outdated, Some("layerfs machine remove docker"))
        );
        assert!(old
            .message(&old.recorded(0), 0)
            .unwrap()
            .contains("has no vm_name"));
        // a `failed` word with no reason is no 2.0 record: it reads stopped
        assert_eq!(dev("failed").recorded(0), State::Stopped);
        assert_eq!(
            machine(layerfs_home::names::HOST_MACHINE, r#"{"platform":"host"}"#).recorded(0),
            State::Ready
        );
        // what a live read or a start found stays until it changes
        let outdated = dev("running").broken_as(Reason::Outdated, "worker of another build");
        assert_eq!(
            broken(&outdated.recorded(0)),
            (
                Reason::Outdated,
                Some("layerfs machine stop dev && layerfs machine start dev")
            )
        );
        let on_it = outdated.recorded(2);
        assert_eq!(
            broken(&on_it).1,
            Some("layerfs machine stop dev --force && layerfs machine start dev")
        );
        assert_eq!(
            outdated.message(&on_it, 2).as_deref(),
            Some("worker of another build; 2 Sandboxes are on it")
        );
        let down = dev("stopped").broken_as(Reason::EngineDown, "Docker does not answer");
        let fix = if cfg!(target_os = "macos") {
            "open -a Docker"
        } else {
            "sudo systemctl start docker"
        };
        assert_eq!(broken(&down.recorded(0)), (Reason::EngineDown, Some(fix)));
        assert_eq!(down.mended().recorded(0), State::Stopped);
        // a platform removed in 2.0, whatever its record says
        let ssh = machine("box", r#"{"platform":"ssh","state":"running"}"#);
        assert_eq!(
            broken(&ssh.recorded(0)),
            (Reason::Unsupported, Some("layerfs machine remove box"))
        );
        assert_eq!(
            ssh.platform().unwrap_err().message,
            "platform `ssh` was removed in LayerFS 2.0"
        );
        let unknown = machine("x", r#"{"platform":"vmware"}"#);
        assert_eq!(unknown.platform().unwrap_err().code, "Failed");
        // a machine a Store or Sandbox names with no record of its own
        assert_eq!(unrecorded(layerfs_home::names::HOST_MACHINE), State::Ready);
        assert_eq!(unrecorded("docker"), State::Stopped);
        assert_eq!(broken(&unrecorded("gone")).0, Reason::Missing);
    }

    /// Machine `dev` (lima, recorded `running`) holding intent `name` of
    /// process `pid`.
    fn held(name: &str, pid: u32, uncertain: bool) -> MachineRecord {
        machine(
            "dev",
            &format!(
                r#"{{"name":"dev","platform":"lima","vm_name":"layerfs-0-dev","state":"running",
                   "op":{{"name":"{name}","phase":"boot","owner":{{"pid":{pid}}},"uncertain":{uncertain}}}}}"#
            ),
        )
    }

    #[test]
    fn a_machine_s_state_follows_its_op() {
        let me = std::process::id();
        // a live owner: busy, the op named by its call word
        let starting = held("machine.start", me, false);
        assert_eq!(
            starting.recorded(0),
            State::Busy {
                name: "start".into(),
                phase: "boot".into(),
                started_ms: None
            }
        );
        // a gone owner (id 0): unresolved when uncertain, else ignored
        let unresolved = held("machine.create", 0, true);
        assert_eq!(
            broken(&unresolved.recorded(0)),
            (Reason::Unresolved, Some("layerfs machine remove dev"))
        );
        let lost_start = held("machine.start", 0, true);
        assert_eq!(
            broken(&lost_start.recorded(0)),
            (
                Reason::Unresolved,
                Some("layerfs machine get dev --with live")
            )
        );
        let stale = held("machine.remove", 0, false);
        assert_eq!(stale.recorded(0), State::Ready);
        stale.not_held().unwrap();
        let busy = starting.not_held().unwrap_err();
        assert_eq!(busy.code, "Busy");
        let refused = lost_start.not_held().unwrap_err();
        assert_eq!(refused.code, "Unresolved");
        assert_eq!(
            refused.detail.get_text("fix"),
            Some("layerfs machine get dev --with live")
        );
        // a remove clears an unresolved intent, never a live one
        unresolved.not_running().unwrap();
        assert_eq!(starting.not_running().unwrap_err().code, "Busy");
        assert!(stale.clone().without_op().op().is_none());
    }
}

#[test]
fn a_store_state_from_its_records_is_busy_unresolved_blocked_broken_or_ready() {
    use layerfs_home::op::Op;
    use layerfs_home::state::{Reason, State};
    use layerfs_store::state::{held, recorded};
    let machines = &layerfs_sandbox::users::machine_state;
    let home = std::env::temp_dir().join(format!("layerfs-store-state-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let on = |machine: &str, more: &str| {
        let text = format!(r#"{{"kind":"sqlite","machine":"{machine}"{more}}}"#);
        StoreRecord::from_record("s", value(&text))
    };
    assert_eq!(
        recorded(&home, &on(layerfs_home::names::HOST_MACHINE, ""), machines).unwrap(),
        State::Ready
    );
    // a machine the home does not record blocks the Store, naming it
    assert_eq!(
        recorded(&home, &on("box", ""), machines).unwrap(),
        State::blocked_by("machine", "box", "missing")
    );
    // what a live read found and wrote is answered from the record
    assert_eq!(
        recorded(
            &home,
            &on(layerfs_home::names::HOST_MACHINE, r#","broken":"missing""#),
            machines
        )
        .unwrap(),
        State::broken(Reason::Missing, Some("layerfs store remove s"))
    );
    // a missing file with a backup link has no in-place restore fix:
    // `backup restore` protects the recorded Store from replacement it
    // cannot prove unchanged
    assert_eq!(
        recorded(
            &home,
            &on(
                layerfs_home::names::HOST_MACHINE,
                r#","broken":"missing","backups":{"b1":{"file":"s","heads":null}}"#
            ),
            machines
        )
        .unwrap(),
        State::broken(Reason::Missing, None)
    );
    // an unreachable server has no fix either: no command makes it answer,
    // and re-running the doctor only observes it again
    assert_eq!(
        recorded(
            &home,
            &on(
                layerfs_home::names::HOST_MACHINE,
                r#","broken":"unreachable""#
            ),
            machines
        )
        .unwrap(),
        State::broken(Reason::Unreachable, None)
    );
    // a live operation (this process) wins over everything, named by its
    // call word; a gone one's uncertain outcome is unresolved with the call
    // that clears it, its certain one is ignored
    let live = Op::begin("store.move", "send", SdkValue::Null).unwrap();
    let with = |op: SdkValue| StoreRecord::from_record("s", SdkValue::map([("op", op)]));
    let busy = held(&with(live.to_record().clone())).unwrap();
    assert!(matches!(&busy, State::Busy { name, phase, .. } if name == "move" && phase == "send"));
    let gone = |uncertain: bool| {
        value(&format!(
            r#"{{"id":"op-2","name":"store.remove","phase":"remove","owner":{{"pid":0}},"uncertain":{uncertain}}}"#
        ))
    };
    assert_eq!(
        held(&with(gone(true))),
        Some(State::broken(
            Reason::Unresolved,
            Some("layerfs store remove s")
        ))
    );
    assert_eq!(held(&with(gone(false))), None);
    // a name this build has no call for (a build before 2.0's `store.push`)
    // is cleared by the live read, which is the fix it shows
    let old = value(
        r#"{"id":"op-3","name":"store.push","phase":"send","args":{"to":"box"},"owner":{"pid":0},"uncertain":true}"#,
    );
    assert_eq!(
        held(&with(old)),
        Some(State::broken(
            Reason::Unresolved,
            Some("layerfs store get s --with live")
        ))
    );
    std::fs::remove_dir_all(&home).unwrap();
}

/// A record with an id or none and a `name` intent owned by `pid`.
fn sandbox_with(id: bool, op: Option<(&str, i64, bool)>) -> layerfs_sandbox::record::SandboxRecord {
    let mut text = String::from(r#"{"name":"b""#);
    if id {
        text.push_str(r#","id":"b""#);
    }
    if let Some((name, pid, uncertain)) = op {
        text.push_str(&format!(
            r#","op":{{"id":"op-00000000000000cc","name":"{name}","phase":"stop","args":null,"owner":{{"pid":{pid}}},"started":4,"updated":5,"uncertain":{uncertain}}}"#
        ));
    }
    text.push('}');
    layerfs_sandbox::record::SandboxRecord::from_record("b", value(&text))
}

#[test]
fn a_sandbox_state_is_derived_from_its_record_and_intent() {
    use layerfs_home::state::{Reason, State};
    let me = i64::from(std::process::id());
    let ready = State::Ready;
    let broken = |reason| State::broken(reason, Some("layerfs sandbox remove b"));
    assert_eq!(sandbox_with(true, None).status(&ready), State::Ready);
    // its machine is not ready: blocked by it
    let blocked = sandbox_with(true, None).status(&State::Stopped);
    assert_eq!(blocked.word(), "blocked", "{blocked:?}");
    // the daemon answered that it does not run: a live read recorded `lost`
    let lost = sandbox_with(true, None).observed_as(Some(("lost", "it does not run")));
    assert_eq!(lost.status(&ready), broken(Reason::Lost));
    assert_eq!(lost.reason_word(), Some("lost"));
    // no id: its create never recorded one
    assert_eq!(
        sandbox_with(false, None).status(&ready),
        broken(Reason::Missing)
    );
    // a live owner's op is busy, by its call word
    let busy = |id: bool, op: &str| match sandbox_with(id, Some((op, me, false))).status(&ready) {
        State::Busy { name, phase, .. } => (name, phase),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        busy(false, "sandbox.create"),
        ("create".into(), "stop".into())
    );
    assert_eq!(
        busy(true, "sandbox.remove"),
        ("remove".into(), "stop".into())
    );
    // a gone owner (id 0): uncertain is unresolved, certain is ignored
    assert_eq!(
        sandbox_with(true, Some(("sandbox.remove", 0, true))).status(&ready),
        broken(Reason::Unresolved)
    );
    assert_eq!(
        sandbox_with(true, Some(("sandbox.remove", 0, false))).status(&ready),
        State::Ready
    );
    // the op shows as `{name, phase, started_at}`, never its owner
    let record = sandbox_with(true, Some(("sandbox.remove", 0, true)));
    let shown = record.op().unwrap().shown();
    let text = print(&shown);
    assert!(text.contains(r#""name":"remove""#), "{text}");
    assert!(
        text.contains(r#""phase":"stop""#) && !text.contains("owner"),
        "{text}"
    );
}
