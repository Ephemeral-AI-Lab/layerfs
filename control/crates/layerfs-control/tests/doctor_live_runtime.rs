//! Opt-in negative observations against exclusively owned, real runtime fixtures.
#![cfg(all(unix, feature = "machine"))]
use layerfs_bridge::control::SdkValue;
#[path = "support/doctor_own.rs"]
mod doctor_own;
use layerfs_control::doctor::{Kind, Options};
use layerfs_home::home::{paths, private_dir, read_record, write_record};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

struct CopyHome(PathBuf);
impl Drop for CopyHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn records(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(path) = pending.pop() {
        if path == root.join("machine/native/run") {
            continue;
        }
        for entry in std::fs::read_dir(&path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|x| x == "json")
                || path
                    .parent()
                    .is_some_and(|p| p.file_name().is_some_and(|n| n == "open"))
            {
                result.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    result
}
fn copy(source: &Path, n: u32) -> CopyHome {
    let root =
        std::env::temp_dir().join(format!("doctor-live-negative-{}-{n}", std::process::id()));
    private_dir(&root).unwrap();
    for (relative, bytes) in records(source) {
        let p = root.join(relative);
        private_dir(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    CopyHome(root)
}
fn report(home: &Path, kind: Kind, name: &str) -> SdkValue {
    let before = records(home);
    let options = Options::default();
    let budget = Some(Duration::from_secs(3));
    let result = doctor_own::own_within(home, kind, name, &options, budget);
    assert!(
        records(home) == before,
        "diagnosis changed logical fixture records or leases"
    );
    result
}
/// Check `id`'s outcome in today's words; none when it did not apply (a
/// check a failed check of the same thing kept from running).
fn outcome(report: &SdkValue, id: &str) -> Option<String> {
    doctor_own::outcome(report, id)
}
fn field(value: &mut SdkValue, key: &str, new: SdkValue) {
    let SdkValue::Map(map) = value else {
        panic!("record map")
    };
    map.retain(|(name, _)| name != key);
    map.push((key.into(), new));
}
fn keys(home: &Path, mutate: impl FnOnce(&mut SdkValue)) {
    let path = paths::sandbox_control(home, "b6-native");
    let mut value = read_record(&path).unwrap().unwrap();
    mutate(&mut value);
    write_record(&path, &value).unwrap();
}
#[test]
#[ignore = "requires explicitly owned B6 native/Docker/Lima runtime fixture"]
fn actual_runtime_negative_observations_preserve_records_and_ownership() {
    assert_eq!(
        std::env::var("LAYERFS_OWNED_DOCTOR_FIXTURE").as_deref(),
        Ok("1")
    );
    let source =
        PathBuf::from(std::env::var_os("LAYERFS_DOCTOR_FIXTURE_HOME").expect("owned fixture home"));
    let before = records(&source);
    let stale = copy(&source, 1);
    keys(&stale.0, |value| {
        field(value, "instance", SdkValue::Bytes(vec![0; 32]))
    });
    let r = report(&stale.0, Kind::Sandbox, "b6-native");
    assert_eq!(
        outcome(&r, "sandbox.daemon_instance").as_deref(),
        Some("fail")
    );
    assert_eq!(outcome(&r, "sandbox.daemon_ready"), None);
    assert_eq!(outcome(&r, "sandbox.session").as_deref(), Some("pass"));

    let auth = copy(&source, 2);
    keys(&auth.0, |value| {
        field(value, "secret", SdkValue::Bytes(vec![0xA5; 32]))
    });
    let r = report(&auth.0, Kind::Sandbox, "b6-native");
    assert_eq!(
        outcome(&r, "sandbox.daemon_auth").as_deref(),
        Some("unknown"),
        "raw authentication close remains ambiguous"
    );
    assert!(!doctor_own::complete(&r));

    let carrier = copy(&source, 3);
    let path = paths::sandbox_record(&carrier.0, "b6-lima-box");
    let mut value = read_record(&path).unwrap().unwrap();
    let SdkValue::Map(root) = &mut value else {
        panic!("record")
    };
    let Some(SdkValue::Map(daemon)) = root
        .iter_mut()
        .find(|(name, _)| name == "daemon")
        .map(|(_, value)| value)
    else {
        panic!("daemon")
    };
    let Some(SdkValue::Map(forward)) = daemon
        .iter_mut()
        .find(|(name, _)| name == "forward")
        .map(|(_, value)| value)
    else {
        panic!("forward")
    };
    assert!(
        forward.iter().any(|(name, _)| name == "pid"),
        "real SSH forward has a carrier PID"
    );
    forward.retain(|(name, _)| name != "pid_start");
    forward.push(("pid_start".into(), SdkValue::text("0")));
    write_record(&path, &value).unwrap();
    let r = report(&carrier.0, Kind::Sandbox, "b6-lima-box");
    assert_eq!(
        outcome(&r, "sandbox.forward_process").as_deref(),
        Some("fail")
    );
    assert_eq!(outcome(&r, "sandbox.daemon_connect"), None);

    let missing = copy(&source, 4);
    std::fs::remove_file(paths::worker_record(&missing.0, "b6-vm")).unwrap();
    let r = report(&missing.0, Kind::Sandbox, "b6-lima-box");
    assert_eq!(outcome(&r, "sandbox.route").as_deref(), Some("unknown"));
    assert_eq!(outcome(&r, "sandbox.daemon_connect"), None);

    let denied = copy(&source, 5);
    let path = paths::worker_record(&denied.0, "docker");
    let mut value = read_record(&path).unwrap().unwrap();
    field(
        &mut value,
        "secret",
        SdkValue::text("intentionally-wrong-b6-fixture-secret"),
    );
    write_record(&path, &value).unwrap();
    let r = report(&denied.0, Kind::Machine, "docker");
    assert_eq!(outcome(&r, "machine.worker.auth").as_deref(), Some("fail"));
    assert_eq!(outcome(&r, "machine.worker.ping"), None);
    // A separately owned temporary runtime establishes a genuinely gone PID,
    // without killing or altering the primary qualification daemon.
    let cli = std::env::var_os("LAYERFS_DOCTOR_FIXTURE_CLI").expect("absolute fixture CLI");
    let result = std::process::Command::new(&cli)
        .args(["sandbox", "create", "b6-store/demo", "b6-gone", "native"])
        .env("LAYERFS_HOME", &source)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "temporary owned daemon setup failed"
    );
    let gone = copy(&source, 6);
    let closed = std::process::Command::new(&cli)
        .args(["sandbox", "remove", "b6-gone"])
        .env("LAYERFS_HOME", &source)
        .output()
        .unwrap();
    assert!(
        closed.status.success(),
        "temporary owned daemon close failed; no replay"
    );
    let r = report(&gone.0, Kind::Sandbox, "b6-gone");
    assert_eq!(outcome(&r, "sandbox.runtime").as_deref(), Some("fail"));
    assert_eq!(outcome(&r, "sandbox.daemon_connect"), None);
    assert!(
        records(&source) == before,
        "real source runtime records/leases changed"
    );
    println!("actual_native_stale=true actual_auth_close_ambiguous=true actual_lima_carrier_identity=true missing_worker_blocked=true actual_worker_denial=true actual_gone_runtime=true source_records_unchanged=true");
}
