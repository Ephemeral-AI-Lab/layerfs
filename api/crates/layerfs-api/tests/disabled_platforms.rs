//! Disabled platform policy through the shared public call dispatcher.
use layerfs_api::{call, json};
use layerfs_bridge::control::SdkValue;
use layerfs_isolation::platform_policy::disabled_reason;
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::path::Path;

const CASES: &str = include_str!("../../../test/disabled-platforms.json");

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
    for platform in ["firecracker", "ssh"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let home =
            std::env::temp_dir().join(format!("lfs-disabled-{platform}-{}", std::process::id()));
        std::fs::create_dir(&home).unwrap();
        let data = CASES
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
            let reason = disabled_reason(if name == "machine.connect" {
                "ssh"
            } else {
                platform
            })
            .unwrap();
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
                ("ws", SdkValue::text("box:one")),
                ("command", SdkValue::text("true")),
            ]),
            &mut |_| {},
        )
        .unwrap_err();
        assert_eq!(refusal.message, disabled_reason(platform).unwrap());
        std::fs::remove_dir_all(home).unwrap();
    }
}

#[test]
fn disabled_selection_does_not_create_a_home() {
    let home = std::env::temp_dir().join(format!("lfs-disabled-new-{}", std::process::id()));
    assert!(!home.exists());
    for platform in ["firecracker", "ssh"] {
        let args = SdkValue::map([
            ("name", SdkValue::text("new")),
            ("platform", SdkValue::text(platform)),
        ]);
        let refusal = call(Some(&home), "machine.create", args, &mut |_| {}).unwrap_err();
        assert_eq!(refusal.message, disabled_reason(platform).unwrap());
        assert!(!home.exists());
    }
}

#[cfg(feature = "machine")]
#[test]
fn existing_machines_can_be_inspected_without_probing_or_changing_their_records() {
    use layerfs_api::home::{paths, write_record};
    let home = std::env::temp_dir().join(format!("lfs-disabled-inspect-{}", std::process::id()));
    layerfs_api::home::open(&home).unwrap();
    for (platform, state) in [("firecracker", "running"), ("ssh", "stopped")] {
        let record = SdkValue::map([
            ("name", SdkValue::text(platform)),
            ("platform", SdkValue::text(platform)),
            ("state", SdkValue::text(state)),
        ]);
        let path = paths::machine_record(&home, platform);
        write_record(&path, &record).unwrap();
        let before = std::fs::read(&path).unwrap();
        // No host spec, ssh credentials or live tool is needed to inspect disabled records.
        for name in ["machine.get", "machine.status"] {
            let value = call(
                Some(&home),
                name,
                SdkValue::map([("name", SdkValue::text(platform))]),
                &mut |_| {},
            )
            .unwrap();
            assert_eq!(value.get_text("state"), Some(state));
            if name == "machine.status" {
                assert_eq!(value.get_text("down_reason"), disabled_reason(platform));
            } else {
                assert_eq!(
                    value.get("supports").unwrap().get_text("reason"),
                    disabled_reason(platform)
                );
            }
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn import_env_reports_disabled_settings_without_suggesting_setup() {
    let home = std::env::temp_dir().join(format!("lfs-disabled-import-{}", std::process::id()));
    let env = vec![
        ("LAYERFS_SSH_HOST".to_owned(), "example.invalid".to_owned()),
        ("LAYERFS_PLACEMENT".to_owned(), "vm:firecracker".to_owned()),
    ];
    let answer = layerfs_api::home::import_env(&home, &env).unwrap();
    let shown = json::print(&answer);
    assert!(shown.contains(disabled_reason("ssh").unwrap()));
    assert!(shown.contains(disabled_reason("firecracker").unwrap()));
    assert!(!shown.contains("machine connect"));
    assert!(!shown.contains("machine create"));
    assert!(!home.join("machine/ssh").exists());
    assert!(!home.join("machine/firecracker").exists());
    std::fs::remove_dir_all(home).unwrap();
}
