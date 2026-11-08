//! Saved alias diagnostics preserve records, discriminate permissions and never write without safe cleanup capability.
mod target {
    pub use layerfs_control::doctor::target::{Budget, Checks};
}
#[path = "../src/doctor/backup_target.rs"]
mod backup_target;
#[path = "../../layerfs-bucket/tests/support/doctor_s3.rs"]
mod peer;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, paths, write_entity};
use peer::{Mode, Server};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    home: PathBuf,
    server: Server,
}
impl Fixture {
    fn new(mode: Mode) -> Self {
        let home = std::env::temp_dir().join(format!(
            "layerfs-doctor-backup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        home::ensure(&home).unwrap();
        let credentials = home.join("credentials");
        std::fs::write(
            &credentials,
            "access_key=mock-access-key\nsecret_key=mock-secret-key\n",
        )
        .unwrap();
        std::fs::set_permissions(&credentials, std::fs::Permissions::from_mode(0o600)).unwrap();
        let server = Server::new(mode);
        write_entity(
            &paths::bucket_record(&home, "production"),
            &SdkValue::map([
                ("name", SdkValue::text("production")),
                ("provider", SdkValue::text("minio")),
                ("endpoint", SdkValue::text(&server.endpoint)),
                ("bucket", SdkValue::text("bucket")),
                ("prefix", SdkValue::text("saved/prefix")),
                ("region", SdkValue::text("us-east-1")),
                (
                    "key",
                    SdkValue::text(format!("file:{}", credentials.display())),
                ),
            ]),
        )
        .unwrap();
        Self { home, server }
    }
    fn report(&self, write: bool) -> SdkValue {
        self.report_with_budget(write, Duration::from_secs(10))
    }
    fn report_with_budget(&self, write: bool, timeout: Duration) -> SdkValue {
        let before = contents(&self.home);
        let mut checks = target::Checks::new("backup", "production");
        backup_target::diagnose(
            &self.home,
            "production",
            write,
            &target::Budget::new(Some(timeout)),
            &mut checks,
        );
        assert_eq!(before, contents(&self.home));
        let report = checks.to_sdk();
        let text = format!("{report:?}");
        assert!(!text.contains("mock-access-key") && !text.contains("mock-secret-key"));
        report
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}
fn contents(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(contents(&path));
        } else {
            result.insert(path.clone(), std::fs::read(path).unwrap());
        }
    }
    result
}
fn check<'a>(report: &'a SdkValue, id: &str) -> &'a SdkValue {
    let Some(SdkValue::List(checks)) = report.get("checks") else {
        panic!("missing checks")
    };
    checks
        .iter()
        .find(|row| row.get_text("id") == Some(id))
        .unwrap_or_else(|| panic!("missing {id}: {report:?}"))
}
fn outcome(report: &SdkValue, id: &str, value: &str) {
    assert_eq!(
        check(report, id).get_text("outcome"),
        Some(value),
        "{report:?}"
    );
}
#[test]
fn default_is_one_read_only_prefix_page() {
    let f = Fixture::new(Mode::default());
    let report = f.report(false);
    outcome(&report, "backup.list", "pass");
    assert_eq!(f.server.state.lock().unwrap().requests.len(), 1);
    outcome(&report, "backup.put", "skipped");
}
#[test]
fn unversioned_and_capability_denied_destinations_receive_no_put() {
    for mode in [
        Mode {
            unversioned: true,
            ..Mode::default()
        },
        Mode {
            versioning_denied: true,
            ..Mode::default()
        },
    ] {
        let f = Fixture::new(mode);
        let report = f.report(true);
        outcome(&report, "backup.write_capability", "unknown");
        outcome(&report, "backup.put", "blocked");
        assert_eq!(f.server.state.lock().unwrap().requests.len(), 2);
        assert!(f
            .server
            .state
            .lock()
            .unwrap()
            .requests
            .iter()
            .all(|request| request.method == "GET"));
    }
}
#[test]
fn list_denied_does_not_block_independent_write_permission_probe() {
    let f = Fixture::new(Mode {
        list_denied: true,
        ..Mode::default()
    });
    let report = f.report(true);
    outcome(&report, "backup.list", "fail");
    for id in ["backup.put", "backup.get", "backup.delete"] {
        outcome(&report, id, "pass");
    }
    assert!(f.server.state.lock().unwrap().versions.is_empty());
}
#[test]
fn list_permission_does_not_imply_write_permission() {
    let f = Fixture::new(Mode {
        put_denied: true,
        ..Mode::default()
    });
    let report = f.report(true);
    outcome(&report, "backup.list", "pass");
    outcome(&report, "backup.put", "fail");
    outcome(&report, "backup.delete", "skipped");
    assert_eq!(f.server.state.lock().unwrap().requests.len(), 3);
}
#[test]
fn lost_put_and_denied_cleanup_keep_ownership_evidence_without_mutation_retries() {
    let f = Fixture::new(Mode {
        lost_put: true,
        denied_delete: true,
        ..Mode::default()
    });
    let report = f.report(true);
    outcome(&report, "backup.put", "unknown");
    outcome(&report, "backup.reconcile", "pass");
    outcome(&report, "backup.get", "pass");
    outcome(&report, "backup.delete", "fail");
    let evidence = check(&report, "backup.delete").get("evidence").unwrap();
    assert!(evidence
        .get_text("possible_retained_key")
        .unwrap()
        .starts_with("saved/prefix/_layerfs_doctor/"));
    assert_eq!(evidence.get_text("owned_version"), Some("owned-v1"));
    let state = f.server.state.lock().unwrap();
    assert_eq!(
        state
            .requests
            .iter()
            .filter(|request| request.method == "PUT")
            .count(),
        1
    );
    assert_eq!(
        state
            .requests
            .iter()
            .filter(|request| request.method == "DELETE")
            .count(),
        1
    );
    assert!(state.versions.contains_key("owned-v1"));
}
#[test]
fn read_corruption_does_not_hide_failed_cleanup() {
    let f = Fixture::new(Mode {
        bad_download: true,
        denied_delete: true,
        ..Mode::default()
    });
    let report = f.report(true);
    outcome(&report, "backup.get", "fail");
    outcome(&report, "backup.delete", "fail");
}

#[test]
fn denied_credential_file_permissions_block_all_network_without_disclosing_values() {
    let f = Fixture::new(Mode::default());
    std::fs::set_permissions(
        f.home.join("credentials"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let report = f.report(true);
    outcome(&report, "backup.configuration", "fail");
    outcome(&report, "backup.list", "blocked");
    assert!(f.server.state.lock().unwrap().requests.is_empty());
}

#[test]
fn signature_and_region_refusals_are_precise_without_echoing_error_bodies() {
    for (mode, code, status) in [
        (
            Mode {
                signature_failure: true,
                ..Mode::default()
            },
            "SignatureDoesNotMatch",
            403,
        ),
        (
            Mode {
                region_failure: true,
                ..Mode::default()
            },
            "AuthorizationHeaderMalformed",
            400,
        ),
    ] {
        let f = Fixture::new(mode);
        let report = f.report(false);
        outcome(&report, "backup.transport", "pass");
        outcome(&report, "backup.list", "fail");
        let evidence = check(&report, "backup.list").get("evidence").unwrap();
        assert_eq!(evidence.get_text("s3_code"), Some(code));
        assert_eq!(evidence.get_int("http_status"), Some(status));
        assert_eq!(f.server.state.lock().unwrap().requests.len(), 1);
    }
}

#[test]
fn deadline_after_put_retains_exact_owned_version_and_never_sends_late_delete() {
    let f = Fixture::new(Mode {
        stall_get: true,
        ..Mode::default()
    });
    let started = std::time::Instant::now();
    let report = f.report_with_budget(true, Duration::from_millis(250));
    assert!(started.elapsed() < Duration::from_secs(2));
    outcome(&report, "backup.put", "pass");
    outcome(&report, "backup.get", "unknown");
    outcome(&report, "backup.delete", "unknown");
    let evidence = check(&report, "backup.delete").get("evidence").unwrap();
    assert_eq!(evidence.get_text("owned_version"), Some("owned-v1"));
    assert!(evidence
        .get_text("possible_retained_key")
        .unwrap()
        .starts_with("saved/prefix/_layerfs_doctor/"));
    let state = f.server.state.lock().unwrap();
    assert_eq!(
        state
            .requests
            .iter()
            .filter(|request| request.method == "PUT")
            .count(),
        1
    );
    assert!(!state
        .requests
        .iter()
        .any(|request| request.method == "DELETE"));
    assert!(state.versions.contains_key("owned-v1"));
}
