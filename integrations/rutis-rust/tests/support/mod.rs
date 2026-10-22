//! Shared fixtures for the rutis integration tests.
#![allow(dead_code)]
use layerfs_api::{InitRequest, ProjectApi};
use layerfs_bridge::control::{Answer, Reply, SdkValue};
use layerfs_bridge::provision::StoreManifest;
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_rutis::{Config, Deploy};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::TempDir;

pub const PROJECT: &str = "website";

pub fn private_dir(path: &Path) {
    std::fs::DirBuilder::new().mode(0o700).create(path).unwrap();
}

pub fn write_control(path: &Path, fields: Vec<(String, SdkValue)>) {
    let bytes = Answer {
        id: 1,
        reply: Reply::Sdk(SdkValue::Map(fields)),
    }
    .encode()
    .unwrap();
    if path.exists() {
        std::fs::remove_file(path).unwrap();
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(&bytes).unwrap();
}

pub fn write_record(path: &Path, manifest: &StoreManifest, installed: Option<&StoreManifest>) {
    let fields = vec![
        (
            "manifest".to_owned(),
            SdkValue::Bytes(manifest.encode().unwrap()),
        ),
        (
            "installed".to_owned(),
            installed.map_or(SdkValue::Null, |manifest| {
                SdkValue::Bytes(manifest.encode().unwrap())
            }),
        ),
    ];
    write_control(path, fields);
}

pub fn fixture() -> (TempDir, Config, StoreManifest) {
    let root = tempfile::Builder::new()
        .prefix("layerfs-tier0-")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap();
    let home = root.path().join("home");
    private_dir(&home);
    let source = root.path().join("source");
    private_dir(&source);
    std::fs::write(source.join("README.md"), "hello\n").unwrap();
    private_dir(&home.join(PROJECT));
    let volume = root.path().join("volume");
    private_dir(&volume);
    private_dir(&volume.join("global"));
    let store_path = volume.join("global").join("store.sqlite");

    let request = InitRequest {
        source,
        store: PersistenceConfig::sqlite(home.join(PROJECT).join("sealed.sqlite"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        locator: "/layerfs-store/global/store.sqlite".into(),
        policy: StoragePolicy::frozen_default(),
        catalog: HistoryCatalogConfig {
            binding_key: vec![1; 32],
            incarnation: 1,
            cursor_key: [2; 32],
        },
        stack: LayerStackId::from_authority([3; 16]),
        stack_name: HistoryName::new(PROJECT).unwrap(),
        branch: BranchId::from_authority([4; 16]),
        branch_name: HistoryName::new("main").unwrap(),
        scope_seed: [5; 32],
        deadline: Instant::now() + Duration::from_secs(60),
    };
    let sealed = Timing::disabled("test", |timer| ProjectApi::new().init(request, timer))
        .0
        .unwrap();
    // install_local runs as the daemons' user (root); a test copies the
    // sealed Store to the installed path instead of exercising installation.
    std::fs::copy(&sealed.store.path, &store_path).unwrap();
    let mut installed = sealed.manifest.clone();
    installed.daemon_sqlite = Some(installed.host_sqlite.clone());
    write_record(
        &home.join(PROJECT).join("project.record"),
        &sealed.manifest,
        Some(&installed),
    );
    let config = Config {
        project: PROJECT.into(),
        home,
        store_path,
        deploy: None,
    };
    (root, config, sealed.manifest)
}

/// Removes the short-lived Engine socket a fixture made.
pub fn cleanup(config: &Config) {
    if let Some(deploy) = &config.deploy {
        let _ = std::fs::remove_file(&deploy.docker);
    }
}

/// A deployment whose socket and daemon path exist; no Engine answers.
pub fn deploy(root: &Path) -> Deploy {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    // A Unix socket path is short; the fixture's CARGO_TARGET_TMPDIR is not.
    let docker = std::path::PathBuf::from(format!(
        "/tmp/lfs-{}-{}.sock",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&docker);
    std::os::unix::net::UnixListener::bind(&docker).unwrap();
    let daemon = root.join("layerfs-daemon");
    std::fs::write(&daemon, b"#!/bin/true\n").unwrap();
    Deploy {
        docker,
        image: format!("sha256:{}", "a".repeat(64)),
        volume: "lfs-test".into(),
        daemon,
        daemon_host: None,
        container_prefix: None,
    }
}
