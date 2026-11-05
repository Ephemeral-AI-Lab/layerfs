//! CLI uses the same disabled-platform fixture as the SDKs.
mod common;
use layerfs_api::{json, Kind};
use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;

#[test]
fn cli_refuses_every_new_work_route_before_contacting_the_machine() {
    for platform in ["firecracker", "ssh"] {
        let home = common::tempdir(&format!("disabled-{platform}"));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let fixture = include_str!("../../../test/disabled-platforms.json")
            .replace("$platform", platform)
            .replace("$address", &listener.local_addr().unwrap().to_string());
        let data = json::parse(&fixture).unwrap();
        let SdkValue::Map(files) = data.get("files").unwrap() else {
            panic!("files")
        };
        for (file, value) in files {
            let path = home.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, json::print(value)).unwrap();
        }
        let SdkValue::List(cases) = data.get("cases").unwrap() else {
            panic!("cases")
        };
        for row in cases {
            let name = row.get_text("call").unwrap();
            let (family, call) = name.split_once('.').unwrap();
            let spec = layerfs_api::calls()
                .into_iter()
                .find(|s| s.family == family && s.call == call)
                .unwrap();
            let args = row.get("args").unwrap();
            let mut words = vec![family.to_owned(), call.to_owned()];
            let word = |value: &SdkValue| match value {
                SdkValue::Text(text) => text.clone(),
                other => json::print(other),
            };
            let mut input = String::new();
            for arg in spec.args {
                if let Some(value) = args.get(arg.name) {
                    if matches!(arg.kind, Kind::Content | Kind::Json) {
                        input = word(value);
                    } else {
                        words.push(word(value));
                    }
                }
            }
            for opt in spec.options {
                if let Some(value) = args.get(opt.name) {
                    words.push(format!("--{}", opt.name.replace('_', "-")));
                    if opt.kind != Kind::Bool {
                        words.push(word(value));
                    }
                }
            }
            let out = common::run_with_stdin(&mut common::layerfs(&home, &words), input.as_bytes());
            assert_eq!(
                out.status.code(),
                Some(125),
                "{name}: {}",
                common::text(&out.stderr)
            );
            let refusal = json::parse(common::text(&out.stderr).trim()).unwrap();
            assert_eq!(refusal.get_text("code"), Some("Unsupported"), "{name}");
            let disabled = if name == "machine.connect" {
                "ssh"
            } else {
                platform
            };
            assert_eq!(
                refusal.get_text("message"),
                Some(
                    format!(
                        "{disabled} is permanently disabled by product policy (layerfs-project#7)"
                    )
                    .as_str()
                )
            );
            assert!(
                matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock)
            );
            for (file, value) in files {
                assert_eq!(
                    std::fs::read_to_string(home.join(file)).unwrap(),
                    json::print(value),
                    "{name}: {file}"
                );
            }
            assert!(!home.join("home.json").exists());
        }
        std::fs::remove_dir_all(home).unwrap();
    }
}
