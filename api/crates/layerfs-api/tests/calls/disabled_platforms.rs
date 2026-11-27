//! Removed-platform policy through the shared public call dispatcher: the
//! platforms removed in 2.0 (`firecracker`, `ssh`, `microsandbox`), whose
//! records may remain in a home.
use layerfs_machine::table::removed::removed_reason;
use layerfs_protocol::sdk::SdkValue;
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::path::Path;
use {layerfs_api::call, layerfs_home::json};

const CASES: &str = include_str!("../support/disabled-platforms.json");

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().display().to_string(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn every_new_work_route_refuses_before_writes_or_worker_and_daemon_connections() {
    for platform in ["firecracker", "ssh", "microsandbox"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let home =
            std::env::temp_dir().join(format!("lfs-disabled-{platform}-{}", std::process::id()));
        std::fs::create_dir(&home).unwrap();
        let data = CASES
            .replace("$host", layerfs_home::names::HOST_MACHINE)
            .replace("$platform", platform)
            .replace("$address", &listener.local_addr().unwrap().to_string());
        let data = json::parse(&data).unwrap();
        let SdkValue::Map(files) = data.get("files").unwrap() else {
            panic!("files")
        };
        for (path, value) in files {
            let path = home.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, json::print(value)).unwrap();
        }
        let before = snapshot(&home);
        let SdkValue::List(cases) = data.get("cases").unwrap() else {
            panic!("cases")
        };
        for case in cases {
            let name = case.get_text("call").unwrap();
            let reason = removed_reason(platform).unwrap();
            let refusal = call(
                Some(&home),
                name,
                case.get("args").unwrap().clone(),
                &mut |_| panic!("event before refusal"),
            )
            .unwrap_err();
            assert_eq!(
                (refusal.code.as_str(), refusal.message.as_str()),
                ("Unsupported", reason),
                "{name}"
            );
            assert_eq!(snapshot(&home), before, "{name} changed existing state");
            assert!(
                matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
                "{name} dialed a worker/daemon"
            );
        }
        // A forwarded Sandbox still carries its platform when its machine record is absent.
        std::fs::remove_file(home.join("machine/legacy/record.json")).unwrap();
        let refusal = call(
            Some(&home),
            "workspace.exec",
            SdkValue::map([
                ("workspace", SdkValue::text("box:one")),
                ("command", SdkValue::text("true")),
            ]),
            &mut |_| {},
        )
        .unwrap_err();
        assert_eq!(refusal.message, removed_reason(platform).unwrap());
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[test]
fn disabled_selection_does_not_create_a_home() {
    let home = std::env::temp_dir().join(format!("lfs-disabled-new-{}", std::process::id()));
    assert!(!home.exists());
    for platform in ["firecracker", "ssh", "microsandbox"] {
        let args = SdkValue::map([
            ("name", SdkValue::text("new")),
            ("platform", SdkValue::text(platform)),
        ]);
        let refusal = call(Some(&home), "machine.create", args, &mut |_| {}).unwrap_err();
        assert_eq!(refusal.message, removed_reason(platform).unwrap());
        assert!(!home.exists());
    }
}
