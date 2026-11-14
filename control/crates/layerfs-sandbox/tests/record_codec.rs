//! The typed Sandbox, daemon and Workspace records (layerfs-project#108)
//! keep their JSON as read: field order, null against absent, unknown and
//! mistyped fields; a changed field keeps its place and a new one comes
//! last, as the untyped records did.
use layerfs_bridge::control::SdkValue;
use layerfs_home::json::{parse, print};
use layerfs_sandbox::host::Started;
use layerfs_sandbox::record::{DaemonRecord, SandboxRecord, WorkspaceRecord};

fn value(text: &str) -> SdkValue {
    parse(text).unwrap()
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
    assert_eq!(
        daemon.labels().unwrap(),
        vec![("k".to_owned(), "v".to_owned())]
    );
    // an item without its key is refused as before
    let broken = DaemonRecord::from_record(&value(r#"{"labels":[{"value":"v"}]}"#));
    let refusal = broken.labels().unwrap_err();
    assert_eq!(refusal.code, "InvalidInput");
    assert_eq!(refusal.message, "missing text argument `key`");
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
            r#"{"state":"Unmounting","startedMs":1,"path":"/w/1","forced":true}"#,
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
            r#"{"state":"Unmounting","startedMs":1,"path":"/w/2","forced":false}"#,
        );
        let r2 = layerfs_sandbox::unmount_guard::find_in(&dir, "/w/2")
            .unwrap()
            .unwrap();
        assert_eq!(r2.get_text("code"), Some("Unknown"));
        assert!(!layerfs_sandbox::unmount_guard::forceable(&r2));

        // 3. Completed forced unmount that remained Retained (forced=true, code=Failed)
        write_rec(
            "03.unmount",
            r#"{"state":"Retained","startedMs":1,"path":"/w/3","forced":true,"code":"Failed","message":"m","detail":null}"#,
        );
        let r3 = layerfs_sandbox::unmount_guard::find_in(&dir, "/w/3")
            .unwrap()
            .unwrap();
        assert!(!layerfs_sandbox::unmount_guard::forceable(&r3));

        // 4. Completed normal unmount the daemon answered Retained at Detach
        //    (forced=false, code=Failed, stage=Detach) -> forceable!
        write_rec(
            "04.unmount",
            r#"{"state":"Retained","startedMs":1,"path":"/w/4","forced":false,"stage":"Detach","code":"Failed","message":"m","detail":null}"#,
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
            r#"{"state":"Retained","startedMs":1,"path":"/w/5","forced":false,"code":"Failed","message":"m","detail":null}"#,
        );
        let r5 = layerfs_sandbox::unmount_guard::find_in(&dir, "/w/5")
            .unwrap()
            .unwrap();
        assert!(!layerfs_sandbox::unmount_guard::forceable(&r5));

        std::fs::remove_dir_all(dir).unwrap();
        let _ = send.send(());
    });
    receive
        .recv_timeout(Duration::from_secs(5))
        .expect("bounded test timed out");
    worker.join().unwrap();
}
