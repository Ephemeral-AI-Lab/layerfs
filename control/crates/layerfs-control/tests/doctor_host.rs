//! Explicit host diagnostics observe records without opening or changing a home.
use layerfs_bridge::control::SdkValue;
use layerfs_control::doctor::{doctor_target, DoctorRequest, DoctorTarget};
use layerfs_home::home::{paths, private_dir, write_entity, write_record};
use layerfs_home::op::Op;
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);
fn temp() -> PathBuf {
    std::env::temp_dir().join(format!(
        "doctor-host-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}
fn request(timeout: Duration) -> DoctorRequest {
    DoctorRequest {
        target: DoctorTarget::Host,
        timeout: Some(timeout),
        quick: false,
        write_test: false,
    }
}
fn run(home: &Path) -> SdkValue {
    doctor_target(home, &request(Duration::from_secs(5))).unwrap()
}
fn home(layout: i64) -> PathBuf {
    let home = temp();
    private_dir(&home).unwrap();
    write_record(
        &home.join("layerfs.json"),
        &SdkValue::map([
            ("layout", SdkValue::Int(layout)),
            ("id", t("0123456789abcdef0123456789abcdef")),
        ]),
    )
    .unwrap();
    home
}
fn tree(home: &Path) -> BTreeMap<PathBuf, (u64, i64, i64, u32)> {
    let mut result = BTreeMap::new();
    let mut pending = vec![home.to_owned()];
    while let Some(path) = pending.pop() {
        let meta = std::fs::symlink_metadata(&path).unwrap();
        result.insert(
            path.clone(),
            (meta.len(), meta.mtime(), meta.mtime_nsec(), meta.mode()),
        );
        if meta.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                pending.push(entry.unwrap().path());
            }
        }
    }
    result
}
fn contains(report: &SdkValue, needle: &str) -> bool {
    layerfs_home::json::print(report).contains(needle)
}
#[test]
fn absent_home_stays_absent_and_reports_uninitialized() {
    let home = temp();
    let report = run(&home);
    assert!(!home.exists());
    assert!(contains(&report, "not initialized"));
    assert!(contains(&report, "host.native_daemon"));
    assert!(contains(&report, "host.artifacts"));
}
#[test]
fn current_and_previous_layouts_are_observed_without_conversion_or_files() {
    for layout in [2, 3] {
        let home = home(layout);
        let before = tree(&home);
        let report = run(&home);
        assert_eq!(before, tree(&home));
        assert!(contains(
            &report,
            if layout == 2 {
                "requires conversion"
            } else {
                "Current home layout"
            }
        ));
        std::fs::remove_dir_all(home).unwrap();
    }
}
#[test]
fn malformed_layout_has_a_blocked_record_check_and_changes_nothing() {
    let home = home(3);
    write_record(
        &home.join("layerfs.json"),
        &SdkValue::map([("layout", t("bad"))]),
    )
    .unwrap();
    let before = tree(&home);
    let report = run(&home);
    assert!(contains(&report, "blocked"));
    assert!(contains(&report, "host.layout"));
    assert_eq!(before, tree(&home));
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn local_records_detect_missing_references_and_permissions_without_worker_dial() {
    let home = home(3);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    write_entity(
        &paths::machine_record(&home, "vm1"),
        &SdkValue::map([("platform", t("lima"))]),
    )
    .unwrap();
    write_record(
        &paths::worker_record(&home, "vm1"),
        &SdkValue::map([
            ("address", t(&listener.local_addr().unwrap().to_string())),
            ("secret", t("never-reveal")),
        ]),
    )
    .unwrap();
    write_entity(
        &paths::store_record(&home, "broken"),
        &SdkValue::map([("kind", t("sqlite")), ("machine", t("missing"))]),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        paths::store_record(&home, "broken"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let before = tree(&home);
    let report = run(&home);
    assert_eq!(before, tree(&home));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(contains(&report, "machine missing"));
    assert!(contains(&report, "0644"));
    assert!(!contains(&report, "never-reveal"));
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn stale_lease_and_dead_op_remain_while_live_op_is_not_stale() {
    let home = home(3);
    let live = Op::begin("store.remove", "retire", SdkValue::Null).unwrap();
    for (name, op) in [
        ("live", live.to_record().clone()),
        (
            "dead",
            SdkValue::map([
                ("id", t("op-00000000000000aa")),
                ("name", t("store.remove")),
                ("phase", t("retire")),
                ("owner", SdkValue::map([("pid", SdkValue::Int(0))])),
                ("uncertain", SdkValue::Bool(true)),
            ]),
        ),
    ] {
        write_entity(
            &paths::store_record(&home, name),
            &SdkValue::map([("kind", t("sqlite")), ("machine", t("native")), ("op", op)]),
        )
        .unwrap();
    }
    private_dir(&paths::lease_dir(&home, "dead")).unwrap();
    std::fs::write(paths::lease_dir(&home, "dead").join("99999999"), "1\n").unwrap();
    let before = tree(&home);
    let report = run(&home);
    assert_eq!(before, tree(&home));
    assert!(contains(&report, "Store dead holds a stale lease"));
    assert!(contains(&report, "store dead has an unresolved"));
    assert!(!contains(&report, "store live has an unresolved"));
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn arbitrary_version_value_is_not_echoed_and_timeout_is_incomplete() {
    let home = home(3);
    write_record(
        &paths::store_record(&home, "bad"),
        &SdkValue::map([("v", t("secret-token-value"))]),
    )
    .unwrap();
    let report = run(&home);
    assert!(!contains(&report, "secret-token-value"));
    assert!(contains(&report, "non-integer"));
    let invalid = doctor_target(&home, &request(Duration::ZERO)).unwrap_err();
    assert_eq!(invalid.code, "InvalidInput");
    // Positive budgets are valid even when exhausted before local inspection.
    let timed = doctor_target(&home, &request(Duration::from_nanos(1))).unwrap();
    assert!(!timed.get_bool("complete"));
    std::fs::remove_dir_all(home).unwrap();
}
