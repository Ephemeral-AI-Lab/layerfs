//! The typed Sandbox, daemon and Workspace records (layerfs-project#108)
//! keep their JSON as read: field order, null against absent, unknown and
//! mistyped fields; a changed field keeps its place and a new one comes
//! last, as the untyped records did.
use layerfs_bridge::control::SdkValue;
use layerfs_home::json::{parse, print};
use layerfs_home::op::Op;
use layerfs_sandbox::host::Started;
use layerfs_sandbox::record::{DaemonRecord, SandboxRecord, WorkspaceRecord};

fn value(text: &str) -> SdkValue {
    parse(text).unwrap()
}

/// A fresh, opened home of its own under the temp directory.
fn scratch_home(tag: &str) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!("lfs-codec-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    layerfs_home::home::layout::open(&home).expect("a new home");
    home
}

#[test]
fn a_sandbox_record_reads_and_writes_back_the_same_json() {
    for text in [
        // a native record: `daemon` came mid-create, before `id`
        r#"{"v":1,"name":"t1","project":"shop/api","machine":"native","created":3,"backend":"native","route":"nfs","runtime_id":"c0","daemon":{"platform":"mac","name":"lfs-0-t1","container":"c0","runtime":"native","uid":501,"gid":20,"directory":"/r/lfs-0-t1","pid":7,"start":"9","boot":"00000000000000000000000000000001","port":5,"image":"local","labels":[{"key":"layerfs.role","value":"sandbox"}]},"id":"t1","platform":"mac","runtime":"native","image":"local"}"#,
        // nulls, a mistyped field, an unknown one and a forward
        r#"{"name":"b","id":null,"created":"soon","left":"ended","extra":[1,{"a":null}],"daemon":{"forward":{"address":"127.0.0.1:9","machine":"lima","pid":3,"pid_start":"4","pid_boot":"ff"},"name":null,"runtime":"namespaces","platform":"lima","image":null,"docker":"/var/run/docker.sock"}}"#,
        // labels of another shape and a daemon that is not a map
        r#"{"daemon":{"labels":[{"value":"v","key":"k","more":1}]},"image":7}"#,
        r#"{"daemon":"none","op":"not an intent"}"#,
    ] {
        let record = SandboxRecord::from_record("b", value(text));
        assert_eq!(print(&record.to_record()), text);
    }
}

#[test]
fn typed_fields_read_as_the_untyped_record_read_them() {
    let text = r#"{"name":"b","id":null,"image":7,"machine":"docker","daemon":{"labels":[{"value":"v","key":"k"}],"name":"lfs-x-b"}}"#;
    let record = SandboxRecord::from_record("b", value(text));
    assert_eq!(record.id(), None);
    assert_eq!(record.image(), None);
    assert_eq!(record.machine(), "docker");
    let daemon = record.daemon().unwrap();
    assert_eq!(daemon.name(), Some("lfs-x-b"));
    assert!(daemon.retired());
    // a Sandbox of the retired docker backend names the container its
    // record keeps, and none when it keeps none: no name is computed
    assert_eq!(record.retired_container(), None);
    let retired = |daemon: &str| {
        let text = format!(r#"{{"name":"b","backend":"docker","daemon":{daemon}}}"#);
        SandboxRecord::from_record("b", value(&text)).retired_container()
    };
    assert_eq!(
        retired(r#"{"name":"lfs-x-b"}"#),
        Some(Some("lfs-x-b".to_owned()))
    );
    assert_eq!(retired("{}"), Some(None));
    // labels of another shape than `[{ key, value }]` are refused
    let refusal = daemon.labels().unwrap_err();
    assert_eq!(refusal.code, "InvalidInput");
    assert_eq!(refusal.message, "local Sandbox record labels");
    // an item without its key is refused
    let broken = DaemonRecord::from_record(&value(r#"{"labels":[{"value":"v"}]}"#));
    let refusal = broken.labels().unwrap_err();
    assert_eq!(refusal.code, "InvalidInput");
    // `[{ key, value }]` reads as it is
    let good = DaemonRecord::from_record(&value(r#"{"labels":[{"key":"k","value":"v"}]}"#));
    assert_eq!(
        good.labels().unwrap(),
        vec![("k".to_owned(), "v".to_owned())]
    );
}

#[test]
fn a_started_record_keeps_its_fields_in_place_and_adds_the_rest_last() {
    let native = r#"{"v":1,"name":"t1","project":"p","machine":"native","created":3,"op":{"id":"op-1"},"backend":"native","route":"nfs","runtime_id":"c0","daemon":{"pid":7}}"#;
    let started = Started::read(&value(
        r#"{"sandbox":"t1","name":"lfs-0-t1","platform":"mac","runtime":"native","route":"nfs","runtime_id":"c0"}"#,
    ));
    let record = SandboxRecord::from_record("t1", value(native)).started(&started);
    // the intent is dropped; `image` was not answered and is null
    assert_eq!(
        print(&record.to_record()),
        r#"{"v":1,"name":"t1","project":"p","machine":"native","created":3,"backend":"native","route":"nfs","runtime_id":"c0","daemon":{"pid":7},"id":"t1","platform":"mac","runtime":"native","image":null}"#
    );
}

#[test]
fn a_workspace_record_is_written_in_its_order() {
    let named = WorkspaceRecord::mounted("ab".to_owned(), Some("w2"), "dev", "/w/2", 2);
    let dir = std::env::temp_dir().join(format!("lfs-record-codec-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("ab.json");
    named.write(&file).unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    let created = text.split("\"created\":").nth(1).unwrap().trim_end();
    assert_eq!(
        text.trim_end(),
        format!(
            r#"{{"v":1,"id":"ab","name":"w2","branch":"dev","path":"/w/2","namespace":2,"created":{created}"#
        )
    );
    let read = WorkspaceRecord::read(&file).unwrap().unwrap();
    assert_eq!(
        (read.name(), read.path(), read.namespace()),
        (Some("w2"), Some("/w/2"), Some(2))
    );
    let plain = WorkspaceRecord::mounted("cd".to_owned(), None, "dev", "/w/1", 1);
    plain.write(&file).unwrap();
    assert!(!std::fs::read_to_string(&file).unwrap().contains("\"name\""));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unmount_guard_forceable_rejects_crashed_and_already_forced_records() {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::mpsc;
    use std::time::Duration;

    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let dir = std::env::temp_dir().join(format!("lfs-unmount-guard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let ws = dir.join("workspace");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(&ws, std::fs::Permissions::from_mode(0o700)).unwrap();

        let write_rec = |name: &str, text: &str| {
            let p = ws.join(name);
            let bytes = layerfs_bridge::control::Answer {
                id: 1,
                reply: layerfs_bridge::control::Reply::Sdk(value(text)),
            }
            .encode()
            .unwrap();
            std::fs::write(&p, bytes).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
        };
        // 1. Crashed force unmount (state=Unmounting, forced=true, no active lock -> code=Unknown)
        write_rec(
            "01.unmount",
            r#"{"v":1,"state":"Unmounting","startedMs":1,"path":"/w/1","forced":true}"#,
        );
        let r1 = layerfs_sandbox::unmount_guard::find_in(&dir, "/w/1")
            .unwrap()
            .unwrap();
        assert_eq!(r1.get_text("code"), Some("Unknown"));
        assert_eq!(r1.get("forced"), Some(&SdkValue::Bool(true)));
        assert!(!layerfs_sandbox::unmount_guard::forceable(&r1));

        // 2. Crashed normal unmount (state=Unmounting, forced=false, no active lock -> code=Unknown)
        write_rec(
            "02.unmount",
            r#"{"v":1,"state":"Unmounting","startedMs":1,"path":"/w/2","forced":false}"#,
        );
        let r2 = layerfs_sandbox::unmount_guard::find_in(&dir, "/w/2")
            .unwrap()
            .unwrap();
        assert_eq!(r2.get_text("code"), Some("Unknown"));
        assert!(!layerfs_sandbox::unmount_guard::forceable(&r2));

        // 3. Completed forced unmount that remained Retained (forced=true, code=Failed)
        write_rec(
            "03.unmount",
            r#"{"v":1,"state":"Retained","startedMs":1,"path":"/w/3","forced":true,"code":"Failed","message":"m","detail":null}"#,
        );
        let r3 = layerfs_sandbox::unmount_guard::find_in(&dir, "/w/3")
            .unwrap()
            .unwrap();
        assert!(!layerfs_sandbox::unmount_guard::forceable(&r3));

        // 4. Completed normal unmount the daemon answered Retained at Detach
        //    (forced=false, code=Failed, stage=Detach) -> forceable!
        write_rec(
            "04.unmount",
            r#"{"v":1,"state":"Retained","startedMs":1,"path":"/w/4","forced":false,"stage":"Detach","code":"Failed","message":"m","detail":null}"#,
        );
        let r4 = layerfs_sandbox::unmount_guard::find_in(&dir, "/w/4")
            .unwrap()
            .unwrap();
        assert!(layerfs_sandbox::unmount_guard::forceable(&r4));

        // 5. The same completed `Retained` answer without the daemon's stage:
        //    the machine wrote it after a send/connect failure, so force must
        //    not replace it.
        write_rec(
            "05.unmount",
            r#"{"v":1,"state":"Retained","startedMs":1,"path":"/w/5","forced":false,"code":"Failed","message":"m","detail":null}"#,
        );
        let r5 = layerfs_sandbox::unmount_guard::find_in(&dir, "/w/5")
            .unwrap()
            .unwrap();
        assert!(!layerfs_sandbox::unmount_guard::forceable(&r5));

        // 6. A version-0 record (no `v`): answered, never forceable.
        write_rec(
            "06.unmount",
            r#"{"state":"Retained","startedMs":1,"path":"/w/6","code":"Failed","message":"m","detail":null}"#,
        );
        let r6 = layerfs_sandbox::unmount_guard::find_in(&dir, "/w/6")
            .unwrap()
            .unwrap();
        assert!(!layerfs_sandbox::unmount_guard::forceable(&r6));

        // 7. A record from a newer version: answered, never forceable.
        write_rec(
            "07.unmount",
            r#"{"v":2,"state":"Retained","startedMs":1,"path":"/w/7","forced":false,"stage":"Detach","code":"Failed","message":"m","detail":null}"#,
        );
        let r7 = layerfs_sandbox::unmount_guard::find_in(&dir, "/w/7")
            .unwrap()
            .unwrap();
        assert!(!layerfs_sandbox::unmount_guard::forceable(&r7));

        std::fs::remove_dir_all(dir).unwrap();
        let _ = send.send(());
    });
    receive
        .recv_timeout(Duration::from_secs(5))
        .expect("bounded test timed out");
    worker.join().unwrap();
}

/// §3.1: a create assigns an ordered generation the durable tombstone names.
/// It is a record field only: it does not appear in a call result, and a
/// record from before generations has none.
#[test]
fn a_created_sandbox_records_one_generation_that_round_trips() {
    let home = scratch_home("codec");
    let op = Op::begin("sandbox.create", "create", SdkValue::Null).expect("intent");
    let record = SandboxRecord::creating(&home, "t1", "shop/api", "native", &op).expect("record");
    let generation = record.generation().expect("a generation").to_owned();
    assert!(!generation.is_empty());
    let id = layerfs_home::home::opened(&home).expect("home").id;
    let (label, _, _) =
        layerfs_history::ordered_token_parts(&generation).expect("a new generation is ordered");
    assert_eq!(label, format!("{id}.sandbox"), "it names the minting home");
    let _ = std::fs::remove_dir_all(&home);
    assert!(record.to_record().get("generation").is_some());

    let read = SandboxRecord::from_record("t1", record.to_record().clone());
    assert_eq!(read.generation(), Some(generation.as_str()));

    // a Sandbox recorded before generations has none, and a write keeps it so
    let old = SandboxRecord::from_record("t1", value(r#"{"name":"t1","machine":"native"}"#));
    assert_eq!(old.generation(), None);
    assert_eq!(
        print(&old.to_record()),
        r#"{"name":"t1","machine":"native"}"#
    );
}

/// A wall clock stepped backwards never mints a generation older than the
/// one the name's tombstone retired: the new value is past the floor.
#[test]
fn a_generation_is_minted_past_the_retired_one_after_a_backwards_clock() {
    use layerfs_sandbox::record::SandboxTombstone;
    let home = scratch_home("floor");
    let id = layerfs_home::home::opened(&home).expect("home").id;
    // A retirement minted far in the future, as a clock stepped back after.
    let future = format!("p1:{id}.sandbox:{:016}:aa", u64::MAX / 4);
    SandboxTombstone {
        name: "t1".to_owned(),
        machine: "native".to_owned(),
        generation: future.clone(),
        retired_ms: 0,
    }
    .write(&home)
    .expect("tombstone");
    let op = Op::begin("sandbox.create", "create", SdkValue::Null).expect("intent");
    let record = SandboxRecord::creating(&home, "t1", "shop/api", "native", &op).expect("record");
    let generation = record.generation().expect("a generation");
    assert_eq!(
        layerfs_history::compare_place(&future, generation),
        layerfs_history::PlaceOrder::RecordedOlder,
        "{generation} follows {future}"
    );
    let _ = std::fs::remove_dir_all(&home);
}
