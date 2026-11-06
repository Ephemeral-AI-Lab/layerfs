//! Each typed record's `to_sdk` gives the shape its calls answered before
//! the records were typed (#536 §10 "Result keys stay stable"), byte for
//! byte as JSON text, and reads past a layout-3 `v`.
use layerfs_bridge::control::SdkValue;
use layerfs_control::backup::record::BucketRecord;
use layerfs_control::project::record::ProjectRecord;
use layerfs_control::store::record::{StoreDerived, StoreRecord, StoreState};
use layerfs_home::json::{parse, print};
use layerfs_home::outcome::View;

fn value(text: &str) -> SdkValue {
    parse(text).unwrap()
}

fn projects(names: &[&str]) -> StoreDerived {
    StoreDerived {
        projects: names.iter().map(|n| n.to_string()).collect(),
        state: None,
    }
}

#[test]
fn a_store_shows_its_record_without_secrets_then_its_projects() {
    let record = StoreRecord::from_record(
        "shop",
        value(
            r#"{"v":1,"binding":"b","name":"old","cursor":"c","kind":"sqlite","machine":"box",
               "profile":"disposable","created":5,"sqlite":"3.50","backups":{"r2":{"file":"shop"}},
               "op":{"id":"op-1","name":"store.push","phase":"send","args":{"to":"box"},
                     "owner":{"pid":0},"started":3,"updated":4,"uncertain":true},
               "last_transfer":{"from":"native","to":"box"}}"#,
        ),
    );
    let want = r#"{"name":"shop","kind":"sqlite","machine":"box","profile":"disposable","created":5,"sqlite":"3.50","backups":{"r2":{"file":"shop"}},"op":{"id":"op-1","name":"store.push","phase":"send","started":3,"uncertain":true},"last_transfer":{"from":"native","to":"box"},"projects":["api","web"]}"#;
    for view in [View::Full, View::List] {
        let shown = record.to_sdk(view, &projects(&["api", "web"])).unwrap();
        assert_eq!(print(&shown), want);
    }
    // a list item ends with its state; a get shows none
    let mut derived = projects(&["api", "web"]);
    derived.state = Some(StoreState::InUse);
    let listed = record.to_sdk(View::List, &derived).unwrap();
    let mut want_listed = want.trim_end_matches('}').to_owned();
    want_listed.push_str(r#","state":"in_use"}"#);
    assert_eq!(print(&listed), want_listed);
    assert_eq!(print(&record.to_sdk(View::Full, &derived).unwrap()), want);
    // the record itself is written back as read, `v` and all
    assert_eq!(record.to_record().get("v"), Some(&SdkValue::Int(1)));
}

#[test]
fn a_store_record_of_an_unknown_kind_still_shows() {
    let record = StoreRecord::from_record("odd", value(r#"{"kind":"oracle"}"#));
    let shown = record.to_sdk(View::List, &projects(&[])).unwrap();
    assert_eq!(
        print(&shown),
        r#"{"name":"odd","kind":"oracle","projects":[]}"#
    );
    let broken = StoreRecord::from_record("odd", SdkValue::Text("x".into()));
    let refusal = broken.to_sdk(View::Full, &projects(&[])).unwrap_err();
    assert_eq!(refusal.code, "Failed");
    assert_eq!(refusal.message, "Store odd's record is not a record");
}

#[test]
fn a_project_shows_its_store_kind_machine_and_creation() {
    let sqlite = StoreRecord::from_record("shop", value(r#"{"kind":"sqlite"}"#));
    let project =
        ProjectRecord::from_record("shop", "api", value(r#"{"v":1,"name":"api","created":7}"#));
    assert_eq!(
        print(&project.to_sdk(View::Full, &sqlite)),
        r#"{"project":"shop/api","store":"shop","name":"api","kind":"sqlite","machine":"native","created":7}"#
    );
    let placed = StoreRecord::from_record("shop", value(r#"{"machine":"box"}"#));
    let missing = ProjectRecord::from_record("shop", "web", SdkValue::Null);
    assert_eq!(
        print(&missing.to_sdk(View::List, &placed)),
        r#"{"project":"shop/web","store":"shop","name":"web","kind":"sqlite","machine":"box","created":null}"#
    );
    let unknown = StoreRecord::from_record("shop", value(r#"{"kind":"oracle","machine":null}"#));
    assert_eq!(
        print(&missing.to_sdk(View::List, &unknown)),
        r#"{"project":"shop/web","store":"shop","name":"web","kind":null,"machine":null,"created":null}"#
    );
    let postgres = StoreRecord::from_record("pg", value(r#"{"kind":"postgresql"}"#));
    let project = ProjectRecord::from_record("pg", "api", value(r#"{"name":"api","stack":"s"}"#));
    assert_eq!(
        print(&project.to_sdk(View::Full, &postgres)),
        r#"{"project":"pg/api","store":"pg","name":"api","kind":"postgresql","machine":"native","created":null}"#
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
    use layerfs_control::sandbox::record::SandboxRecord;
    let text = r#"{"name":"t1","project":"shop/api","machine":"native","created":3,"id":"t1","platform":"macos","runtime":"native","image":null,"backend":"native","route":"nfs"}"#;
    // its state is derived, never stored (#536 H4), and shown last
    let shown = text.replacen('}', r#","state":"running"}"#, 1);
    let record = SandboxRecord::from_record("t1", value(text));
    assert_eq!(print(&record.to_sdk(View::List)), shown);
    assert_eq!(print(&record.to_sdk(View::Full)), shown);
    // layout 3: `v`, `runtime_id` and what reopens it (`daemon`) are not shown
    let hidden = text.replacen('{', r#"{"v":1,"#, 1).replacen(
        r#""route":"nfs""#,
        r#""route":"nfs","runtime_id":"c0ffee","daemon":{"pid":7}"#,
        1,
    );
    let versioned = SandboxRecord::from_record("t1", value(&hidden));
    assert_eq!(print(&versioned.to_sdk(View::List)), shown);
    assert_eq!(versioned.machine(), "native");
    assert_eq!(record.project(), Some("shop/api"));
    assert_eq!(record.id(), Some("t1"));
}

#[cfg(feature = "machine")]
mod machines {
    use super::*;
    use layerfs_control::machine::record::{MachineDerived, MachineRecord};

    fn keys(value: &SdkValue) -> Vec<&str> {
        match value {
            SdkValue::Map(fields) => fields.iter().map(|(n, _)| n.as_str()).collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn a_vm_shows_its_live_state_size_and_settings() {
        let record = MachineRecord::from_record(
            "dev",
            value(
                r#"{"v":1,"name":"dev","platform":"lima","cpus":2,"memory":4,"disk":20,"vm_name":"lfs-x-dev",
                   "state":"running","created":1,"image":"sha256:ab","nested":true,"error":"old",
                   "lasting":{"port":2222}}"#,
            ),
        );
        let derived = MachineDerived {
            live: Some(("stopped".into(), Some("its probe did not answer".into()))),
            stores: vec!["shop".into()],
            sandboxes: vec!["t1".into()],
        };
        let shown = record.to_sdk(View::List, &derived).unwrap();
        assert_eq!(
            keys(&shown),
            [
                "name",
                "platform",
                "backend",
                "supports",
                "state",
                "cpus",
                "memory",
                "disk",
                "stores",
                "sandboxes",
                "image",
                "nested",
                "error"
            ]
        );
        assert_eq!(shown.get_text("platform"), Some("lima"));
        assert_eq!(shown.get_text("state"), Some("stopped"));
        assert_eq!(shown.get_text("error"), Some("its probe did not answer"));
        assert_eq!(shown.get("cpus"), Some(&SdkValue::Int(2)));
        assert_eq!(print(shown.get("stores").unwrap()), r#"["shop"]"#);
        assert_eq!(print(shown.get("sandboxes").unwrap()), r#"["t1"]"#);
        assert_eq!(shown.get("nested"), Some(&SdkValue::Bool(true)));
        assert_eq!(record.state(Some("missing")).unwrap(), "missing");
        assert_eq!(record.state(None).unwrap(), "running");
        // without a live answer the record's state and error show
        let derived = MachineDerived::default();
        let shown = record.to_sdk(View::Full, &derived).unwrap();
        assert_eq!(shown.get_text("state"), Some("running"));
        assert_eq!(shown.get_text("error"), Some("old"));
    }

    #[test]
    fn a_built_in_machine_is_ready_and_unsized() {
        let record = MachineRecord::from_record(
            "docker",
            value(r#"{"name":"docker","platform":"docker","created":1,"docker":"/run/d.sock"}"#),
        );
        let shown = record
            .to_sdk(View::List, &MachineDerived::default())
            .unwrap();
        assert_eq!(
            keys(&shown),
            [
                "name",
                "platform",
                "backend",
                "supports",
                "state",
                "cpus",
                "memory",
                "disk",
                "stores",
                "sandboxes",
                "docker"
            ]
        );
        assert_eq!(shown.get_text("state"), Some("ready"));
        assert_eq!(shown.get("cpus"), Some(&SdkValue::Null));
        let vm = MachineRecord::from_record("vm", value(r#"{"platform":"lima"}"#));
        assert_eq!(vm.state(None).unwrap(), "stopped");
        // a connect in flight: its live `op` (this process) says `creating`
        let me = std::process::id();
        let ssh = MachineRecord::from_record(
            "box",
            value(&format!(
                r#"{{"name":"box","platform":"ssh","ssh":{{"host":"h","host_id":"k"}},
                   "op":{{"name":"machine.connect","owner":{{"pid":{me}}},"uncertain":false}}}}"#
            )),
        );
        let shown = ssh.to_sdk(View::Full, &MachineDerived::default()).unwrap();
        assert_eq!(keys(&shown).last(), Some(&"ssh"));
        assert_eq!(shown.get_text("state"), Some("creating"));
        assert!(shown.get("op").is_none(), "{shown:?}");
        assert!(ssh.in_flight());
        let unknown = MachineRecord::from_record("x", value(r#"{"platform":"vmware"}"#));
        let refusal = unknown
            .to_sdk(View::List, &MachineDerived::default())
            .unwrap_err();
        assert_eq!(refusal.code, "Failed");
        assert_eq!(refusal.message, "a machine record names no known platform");
    }

    /// Machine `dev` (lima, recorded `running`) holding intent `name` of
    /// process `pid`.
    fn held(name: &str, pid: u32, uncertain: bool) -> MachineRecord {
        MachineRecord::from_record(
            "dev",
            value(&format!(
                r#"{{"name":"dev","platform":"lima","state":"running",
                   "op":{{"name":"{name}","owner":{{"pid":{pid}}},"uncertain":{uncertain}}}}}"#
            )),
        )
    }

    #[test]
    fn a_machine_s_state_follows_its_op() {
        let me = std::process::id();
        // a live owner: its call, whatever the probe says
        let removing = held("machine.remove", me, false);
        assert_eq!(removing.state(Some("running")).unwrap(), "removing");
        assert_eq!(
            held("machine.disconnect", me, true).state(None).unwrap(),
            "removing"
        );
        assert_eq!(
            held("machine.create", me, false).state(None).unwrap(),
            "creating"
        );
        assert!(removing.in_flight());
        // a gone owner (id 0): unresolved when uncertain, else nothing
        let unresolved = held("machine.create", 0, true);
        assert_eq!(unresolved.state(Some("running")).unwrap(), "unresolved");
        assert!(unresolved.in_flight());
        let stale = held("machine.remove", 0, false);
        assert_eq!(stale.state(Some("missing")).unwrap(), "missing");
        assert_eq!(stale.state(None).unwrap(), "running");
        assert!(!stale.in_flight());
        stale.not_held().unwrap();
        assert_eq!(removing.not_held().unwrap_err().code, "Busy");
        assert_eq!(unresolved.not_held().unwrap_err().code, "Unresolved");
        // a remove clears an unresolved intent, never a live one
        unresolved.not_running().unwrap();
        assert_eq!(removing.not_running().unwrap_err().code, "Busy");
        assert!(stale.clone().without_op().op().is_none());
    }
}

#[test]
fn a_store_state_is_busy_unresolved_down_idle_or_in_use() {
    use layerfs_home::op::Op;
    let idle = StoreRecord::from_record("s", value(r#"{"kind":"sqlite"}"#));
    assert_eq!(idle.state(false, &[]), StoreState::Idle);
    assert_eq!(idle.state(false, &[42]), StoreState::InUse);
    assert_eq!(idle.state(true, &[42]), StoreState::Down);
    // a live operation (this process) wins over everything; a gone one's
    // uncertain outcome is unresolved, its certain one is ignored
    let live = Op::begin("store.push", "send", SdkValue::Null).unwrap();
    let with = |op: SdkValue| StoreRecord::from_record("s", SdkValue::map([("op", op)]));
    let busy = with(live.to_record().clone());
    assert_eq!(busy.state(true, &[42]), StoreState::Busy);
    let gone = |uncertain: bool| {
        value(&format!(
            r#"{{"id":"op-2","name":"store.remove","phase":"remove","owner":{{"pid":0}},"uncertain":{uncertain}}}"#
        ))
    };
    assert_eq!(with(gone(true)).state(false, &[]), StoreState::Unresolved);
    assert_eq!(with(gone(false)).state(false, &[]), StoreState::Idle);
    let names: Vec<_> = [
        StoreState::Idle,
        StoreState::InUse,
        StoreState::Busy,
        StoreState::Down,
        StoreState::Unresolved,
    ]
    .iter()
    .map(|s| s.as_str())
    .collect();
    assert_eq!(names, ["idle", "in_use", "busy", "down", "unresolved"]);
}

/// A record with an id or none and a `name` intent owned by `pid`.
fn sandbox_with(
    id: bool,
    op: Option<(&str, i64, bool)>,
) -> layerfs_control::sandbox::record::SandboxRecord {
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
    layerfs_control::sandbox::record::SandboxRecord::from_record("b", value(&text))
}

#[test]
fn a_sandbox_state_is_derived_from_its_record_and_intent() {
    let me = i64::from(std::process::id());
    assert_eq!(sandbox_with(true, None).state(false), "running");
    // the daemon answered that it does not run
    assert_eq!(sandbox_with(true, None).state(true), "gone");
    // no id: its create never recorded one
    assert_eq!(sandbox_with(false, None).state(false), "gone");
    assert_eq!(
        sandbox_with(false, Some(("sandbox.create", me, false))).state(false),
        "creating"
    );
    assert_eq!(
        sandbox_with(true, Some(("sandbox.close", me, false))).state(false),
        "closing"
    );
    // a gone owner (id 0): uncertain blocks, certain is ignored
    assert_eq!(
        sandbox_with(true, Some(("sandbox.close", 0, true))).state(false),
        "unresolved"
    );
    assert_eq!(
        sandbox_with(true, Some(("sandbox.close", 0, false))).state(false),
        "running"
    );
    // the intent shows as `{id, name, phase, started, uncertain}`, never its owner
    let shown = sandbox_with(true, Some(("sandbox.close", 0, true))).to_sdk(View::List);
    assert_eq!(
        print(&shown),
        r#"{"name":"b","id":"b","state":"unresolved","op":{"id":"op-00000000000000cc","name":"sandbox.close","phase":"stop","started":4,"uncertain":true}}"#
    );
}
