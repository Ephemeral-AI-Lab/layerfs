//! A Firecracker machine's Lima host is started before the microVM (#533
//! review P0-6): the host's own worker may have stopped it when idle, and
//! the microVM cannot boot inside a stopped host. Here the start of each
//! fails at the missing Linux bundle; the host's `start.lock` shows that
//! its start ran first. One test, because it sets the process's environment.
#![cfg(feature = "machine")]
use layerfs_bridge::control::SdkValue;
use layerfs_control::common::home::{self, paths};
use layerfs_control::machine;
use std::path::PathBuf;

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-hoststart-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

#[test]
fn a_microvm_s_stopped_host_is_started_first() {
    let cache = tempdir("cache");
    std::env::set_var("XDG_CACHE_HOME", &cache);
    std::env::remove_var("LAYERFS_BUNDLE");
    let home = tempdir("home");
    home::open(&home).unwrap();
    let t = SdkValue::text;
    home::write_record(
        &paths::machine_record(&home, "host"),
        &SdkValue::map([("name", t("host")), ("platform", t("lima"))]),
    )
    .unwrap();
    home::write_record(
        &paths::machine_record(&home, "fc"),
        &SdkValue::map([
            ("name", t("fc")),
            ("platform", t("firecracker")),
            ("host", t("host")),
        ]),
    )
    .unwrap();
    let refusal = machine::running(&home, "fc").unwrap_err();
    assert!(refusal.message.contains("bundle"), "{}", refusal.message);
    assert!(
        paths::machine_dir(&home, "host")
            .join("start.lock")
            .exists(),
        "the host's start ran"
    );
    assert!(
        !paths::machine_dir(&home, "fc").join("start.lock").exists(),
        "the microVM waits for its host"
    );
    for dir in [home, cache] {
        std::fs::remove_dir_all(dir).unwrap();
    }
}
