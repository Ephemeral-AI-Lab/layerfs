//! Saved PostgreSQL integrity admission through the actual Control family.
#![cfg(all(unix, feature = "postgresql"))]
use layerfs_bridge::control::SdkValue;
use layerfs_history::HistoryCatalogConfig;
use layerfs_home::home::{self, paths};
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_store::record::StoreRecord;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::Duration,
};
mod target {
    pub use layerfs_control::doctor::target::{Budget, Checks};
}
#[allow(dead_code)]
#[path = "../src/doctor/store_target.rs"]
mod store_target;
const BINDING: &[u8] = b"doctor-saved-pg-admission";
fn command(container: &str, args: &[&str]) -> Output {
    let out = Command::new("docker")
        .args(["exec", "-u", "postgres", container])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "owned fixture infrastructure command failed; output retained privately"
    );
    out
}
fn sql(container: &str, sql: &str) {
    command(
        container,
        &["psql", "-XAt", "-v", "ON_ERROR_STOP=1", "-c", sql],
    );
}
struct Fixture {
    root: PathBuf,
    container: String,
    database: String,
    empty: String,
    writer: Option<PostgresHandles>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.writer.take();
        for db in [&self.database, &self.empty] {
            sql(&self.container, &format!("DROP DATABASE {db} WITH (FORCE)"));
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    fn walk(root: &Path, at: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out)
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    walk(root, root, &mut out);
    out
}
fn report(home: &Path) -> SdkValue {
    let before = files(home);
    let mut checks = target::Checks::new("store", "saved");
    store_target::diagnose_integrity(
        home,
        "saved",
        true,
        &target::Budget::new(Some(Duration::from_secs(5))),
        &mut checks,
    );
    assert_eq!(
        before,
        files(home),
        "Doctor did not change home/records/password reference"
    );
    checks.to_sdk()
}
fn outcome(report: &SdkValue, id: &str, status: &str) -> bool {
    let Some(SdkValue::List(checks)) = report.get("checks") else {
        return false;
    };
    checks
        .iter()
        .any(|c| c.get_text("id") == Some(id) && c.get_text("outcome") == Some(status))
}
fn dump(container: &str, db: &str) -> Vec<String> {
    String::from_utf8(
        command(
            container,
            &["pg_dump", "--data-only", "--schema=layerfs", db],
        )
        .stdout,
    )
    .unwrap()
    .lines()
    .filter(|line| !line.starts_with("\\restrict") && !line.starts_with("\\unrestrict"))
    .map(str::to_owned)
    .collect()
}
#[test]
fn saved_integrity_distinguishes_authority_authentication_and_schema_without_mutation() {
    let Ok(container) = std::env::var("LAYERFS_TEST_DOCTOR_PG_DISPOSABLE_CONTAINER") else {
        return;
    };
    assert!(container.starts_with("doctor-pg116-"));
    let label = Command::new("docker")
        .args([
            "inspect",
            "--format",
            "{{index .Config.Labels \"owner\"}}",
            &container,
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(label.stdout).unwrap().trim(),
        "layerfs-doctor-pg-116"
    );
    let database = format!("doctor_control_{}", std::process::id());
    let empty = format!("{database}_empty");
    sql(&container, &format!("CREATE DATABASE {database}"));
    sql(&container, &format!("CREATE DATABASE {empty}"));
    let root = std::env::temp_dir().join(format!("doctor-control-pg-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let password = std::env::var("LAYERFS_TEST_POSTGRES_PASSWORD").unwrap();
    let source = std::env::var("LAYERFS_TEST_POSTGRES_URL").unwrap();
    let url = source.replace("/postgres?", &format!("/{database}?"));
    let selection = PostgresSelection::new(&url, "layerfs");
    assert_eq!(
        selection.synchronous_commit,
        layerfs_persistence::SynchronousCommit::Off
    );
    let writer = PostgresHandles::create(
        &selection,
        layerfs_storage::StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .unwrap();
    let f = Fixture {
        root,
        container,
        database,
        empty,
        writer: Some(writer),
    };
    let home = f.root.join("home");
    home::ensure(&home).unwrap();
    let secret = f.root.join("password");
    std::fs::write(&secret, &password).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
    let public_url = url.replace(&format!(":{password}@"), "@");
    let record = StoreRecord::connected(
        "saved",
        &public_url,
        "layerfs",
        "off",
        Some(&format!("file:{}", secret.display())),
    )
    .with("binding", SdkValue::Bytes(BINDING.to_vec()))
    .with("cursor", SdkValue::Bytes(vec![71; 32]));
    record.write(&home).unwrap();
    let before = dump(&f.container, &f.database);
    let out = report(&home);
    assert!(
        outcome(&out, "store.connection.identity", "pass"),
        "{out:?}"
    );
    assert!(
        outcome(&out, "store.schema.objects.summary", "pass"),
        "{out:?}"
    );
    record
        .clone()
        .with("binding", SdkValue::Bytes(b"wrong-saved-binding".to_vec()))
        .write(&home)
        .unwrap();
    let out = report(&home);
    assert!(
        outcome(&out, "store.connection.identity", "fail"),
        "{out:?}"
    );
    assert!(!outcome(&out, "store.snapshot", "pass"));
    record.write(&home).unwrap();
    let marker = "wrong-fixture-password-must-stay-private";
    std::fs::write(&secret, marker).unwrap();
    let out = report(&home);
    assert!(
        outcome(&out, "store.connection.authentication", "fail"),
        "{out:?}"
    );
    assert!(!format!("{out:?}").contains(marker));
    assert!(!format!("{out:?}").contains(&password));
    std::fs::write(&secret, &password).unwrap();
    record
        .clone()
        .with(
            "url",
            SdkValue::text(
                public_url
                    .replace(&format!("/{}/", f.database), &format!("/{}/", f.empty))
                    .replace(&format!("/{}?", f.database), &format!("/{}?", f.empty)),
            ),
        )
        .write(&home)
        .unwrap();
    let out = report(&home);
    assert!(outcome(&out, "store.connection.schema", "fail"), "{out:?}");
    assert!(!outcome(&out, "store.snapshot", "pass"));
    assert_eq!(before, dump(&f.container, &f.database));
    assert!(!paths::store_file(&home, "saved").exists());
}
