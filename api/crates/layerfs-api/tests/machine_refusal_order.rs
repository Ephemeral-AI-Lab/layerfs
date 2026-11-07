//! Which refusal wins when a machine call breaks several rules
//! (layerfs-project#109): a bad name is the name check's own text, create
//! refuses any `nested` off lima before it checks the name, and update
//! checks size, then the built-in settings, then the daemon, then wsl.
#![cfg(feature = "machine")]
use layerfs_api::home::{paths, write_record};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::MachineName;
use std::path::{Path, PathBuf};

/// A temporary home, removed when the test ends, failing or not.
struct TempHome(PathBuf);

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tempdir() -> TempHome {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-machine-order-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    TempHome(dir)
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn refusal(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> (String, String) {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    let refusal = layerfs_api::call(Some(home), name, args, &mut |_| {}).unwrap_err();
    (refusal.code, refusal.message)
}

#[test]
fn the_first_broken_rule_is_the_refusal() {
    let temp = tempdir();
    let home = temp.0.as_path();
    let bad = MachineName::parse("-x").unwrap_err().message;
    for call in [
        "machine.get",
        "machine.status",
        "machine.start",
        "machine.stop",
        "machine.remove",
        "machine.update",
    ] {
        let got = refusal(home, call, &[("name", t("-x"))]);
        assert_eq!(got, ("InvalidInput".to_owned(), bad.clone()), "{call}");
    }

    let nested = |value: bool| {
        refusal(
            home,
            "machine.create",
            &[
                ("name", t("-x")),
                ("platform", t("apple-container")),
                ("nested", SdkValue::Bool(value)),
            ],
        )
    };
    for value in [false, true] {
        assert_eq!(nested(value).1, "only a lima machine is nested");
    }
    let unknown = refusal(
        home,
        "machine.create",
        &[("name", t("-x")), ("platform", t("nope"))],
    );
    assert_eq!(unknown.1, "no platform named nope");

    let update = |args: &[(&str, SdkValue)]| refusal(home, "machine.update", args).1;
    assert_eq!(
        update(&[
            ("name", t("native")),
            ("cpus", SdkValue::Int(2)),
            ("docker", t("/tmp/docker.sock")),
        ]),
        "native is built in; it has no size of its own"
    );
    write_record(
        &paths::machine_record(home, "box"),
        &SdkValue::map([
            ("name", t("box")),
            ("platform", t("lima")),
            ("state", t("stopped")),
        ]),
    )
    .unwrap();
    assert_eq!(
        update(&[("name", t("box")), ("wsl", t("Ubuntu"))]),
        "docker, wsl and daemon are the built-in machines' settings"
    );
    let daemon_and_wsl = update(&[
        ("name", t("native")),
        ("daemon", t("relative")),
        ("wsl", t("Ubuntu")),
    ]);
    assert_eq!(
        daemon_and_wsl,
        "daemon must be the absolute path of a file; relative is not one"
    );
    #[cfg(not(windows))]
    assert_eq!(
        update(&[("name", t("native")), ("wsl", t("Ubuntu"))]),
        "wsl is a Windows host's setting"
    );
}
