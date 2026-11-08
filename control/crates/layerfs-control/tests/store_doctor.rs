//! Store target record validation and actual observational worker transport.
use layerfs_bridge::control::SdkValue;
use layerfs_history::HistoryCatalogConfig;
use layerfs_home::{
    home::{paths, write_entity, write_record},
    outcome::refused,
};
use layerfs_machine::worker::{diagnostic_connect, Endpoint};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
mod target {
    pub use layerfs_control::doctor::target::{Budget, Checks};
}
#[path = "../src/doctor/store_target.rs"]
mod store_target;
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("layerfs-store-doctor-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
fn fields(name: &str) -> SdkValue {
    SdkValue::map([
        ("name", SdkValue::text(name)),
        ("kind", SdkValue::text("sqlite")),
        ("machine", SdkValue::text("native")),
        ("binding", SdkValue::Bytes(b"doctor-worker".to_vec())),
        ("cursor", SdkValue::Bytes(vec![9; 32])),
    ])
}
#[test]
fn malformed_saved_identity_is_rejected_before_database_access() {
    let f = Fixture::new();
    let path = paths::store_record(&f.root, "one");
    for value in [
        fields("two"),
        layerfs_home::home::with_field(fields("one"), "v", SdkValue::Int(999)),
    ] {
        write_record(&path, &value).unwrap();
        let mut checks = target::Checks::new("store", "one");
        store_target::diagnose_integrity(
            &f.root,
            "one",
            false,
            &target::Budget::new(None),
            &mut checks,
        );
        let report = checks.to_sdk();
        assert!(format!("{report:?}").contains("saved Store record name or version is invalid"));
        assert!(!paths::store_file(&f.root, "one").exists());
    }
    // Correctly versioned records proceed to the unavailable database finding.
    write_entity(&path, &fields("one")).unwrap();
    let mut checks = target::Checks::new("store", "one");
    store_target::diagnose_connection(&f.root, "one", &target::Budget::new(None), &mut checks);
    assert!(format!("{:?}", checks.to_sdk())
        .contains("connection diagnostics require a PostgreSQL Store"));
    // Exercise the included handler's cancellation path without changing visibility.
    let args = SdkValue::map([("store", SdkValue::text("../escape"))]);
    assert!(store_target::in_worker(&f.root, args, &mut |_| Ok(())).is_err());
}
struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
#[ignore]
fn worker_child() {
    if let Ok(root) = std::env::var("LAYERFS_DOCTOR_WORKER_FIXTURE_ROOT") {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        std::fs::write(
            PathBuf::from(&root).join("endpoint"),
            listener.local_addr().unwrap().to_string(),
        )
        .unwrap();
        layerfs_control::worker_server::serve(
            listener,
            "fixture-secret".to_owned(),
            PathBuf::from(root),
        )
        .unwrap();
    }
}
#[test]
fn actual_worker_audit_streams_and_cancelled_peer_does_not_poison_next_call() {
    let f = Fixture::new();
    let path = paths::store_file(&f.root, "one");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let authority = HistoryCatalogConfig {
        binding_key: b"doctor-worker".to_vec(),
        cursor_key: [9; 32],
        incarnation: 1,
    };
    let writer = SqliteHandles::create(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &authority,
    )
    .unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "worker_child"])
        .env("LAYERFS_DOCTOR_WORKER_FIXTURE_ROOT", &f.root)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    let endpoint = loop {
        if let Ok(text) = std::fs::read_to_string(f.root.join("endpoint")) {
            break Endpoint {
                address: text.parse().unwrap(),
                secret: "fixture-secret".to_owned(),
            };
        }
        assert!(Instant::now() < until);
        assert!(child.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(5));
    };
    let _server = Server(child);
    let args = || {
        SdkValue::map([
            ("store", SdkValue::text("one")),
            ("binding", SdkValue::Bytes(authority.binding_key.clone())),
            ("cursor", SdkValue::Bytes(authority.cursor_key.to_vec())),
            ("quick", SdkValue::Bool(false)),
            ("timeout_ms", SdkValue::Int(2000)),
        ])
    };
    let mut observed = Vec::new();
    let value = diagnostic_connect(&endpoint, Instant::now() + Duration::from_secs(2))
        .unwrap()
        .diagnose_store(
            args(),
            Some(Instant::now() + Duration::from_secs(2)),
            &mut |e| {
                observed.push(e);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(value.get("completed"), Some(&SdkValue::Bool(true)));
    assert!(observed
        .iter()
        .any(|e| e.get_text("id") == Some("store.snapshot")));
    assert!(observed
        .iter()
        .any(|e| e.get_text("id") == Some("store.objects.summary")));
    let error = diagnostic_connect(&endpoint, Instant::now() + Duration::from_secs(2))
        .unwrap()
        .diagnose_store(
            args(),
            Some(Instant::now() + Duration::from_secs(2)),
            &mut |_| Err(refused("Cancelled", "fixture stops receiving")),
        )
        .unwrap_err();
    assert_eq!(error.code, "Cancelled");
    // A subsequent real audit succeeds after the interrupted peer; no source
    // staging ownership or server-session state is retained across calls.
    let value = diagnostic_connect(&endpoint, Instant::now() + Duration::from_secs(2))
        .unwrap()
        .diagnose_store(
            args(),
            Some(Instant::now() + Duration::from_secs(2)),
            &mut |_| Ok(()),
        )
        .unwrap();
    assert_eq!(value.get("completed"), Some(&SdkValue::Bool(true)));
    // An independent SQLite connection must be able to truncate the WAL after
    // the cancelled peer, demonstrating no worker read snapshot remains held.
    let checkpoint=Command::new("python3").args(["-c",r#"import sqlite3,sys; c=sqlite3.connect(sys.argv[1],timeout=1); r=c.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchone(); assert r[0]==0,r"#,path.to_str().unwrap()]).output().unwrap();
    assert!(
        checkpoint.status.success(),
        "{}",
        String::from_utf8_lossy(&checkpoint.stderr)
    );
    drop(writer);
}
