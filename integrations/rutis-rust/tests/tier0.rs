//! Tier 0 integration: an installed Store opened in this process.

#![cfg(target_os = "linux")]

use layerfs_api::{InitRequest, ProjectApi};
use layerfs_bridge::control::{Answer, Reply, SdkValue};
use layerfs_bridge::provision::StoreManifest;
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_rutis::{Config, LayerFs, LayerFsError, LayerFsPlugin};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use serde_json::json;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const PROJECT: &str = "website";

fn private_dir(path: &Path) {
    std::fs::DirBuilder::new().mode(0o700).create(path).unwrap();
}

fn write_record(path: &Path, manifest: &StoreManifest, installed: Option<&StoreManifest>) {
    let mut fields = vec![(
        "manifest".to_owned(),
        SdkValue::Bytes(manifest.encode().unwrap()),
    )];
    if let Some(installed) = installed {
        fields.push((
            "installed".to_owned(),
            SdkValue::Bytes(installed.encode().unwrap()),
        ));
    }
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

fn fixture() -> (TempDir, Config, StoreManifest) {
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

    (
        root,
        Config {
            project: PROJECT.into(),
            home,
            store_path,
        },
        sealed.manifest,
    )
}

#[tokio::test]
async fn opens_an_installed_store_and_runs_the_public_table() {
    let (_root, config, _manifest) = fixture();
    let layerfs = LayerFs::new(config).unwrap();

    let value = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap();
    assert_eq!(value["text"], "hello\n");
    assert_eq!(layerfs.info().await.project, PROJECT);

    layerfs
        .call(
            "branch.fork".into(),
            json!({ "name": "feature", "from": "main" }),
        )
        .await
        .unwrap();
    layerfs
        .call(
            "fs.write".into(),
            json!({
                "branch": "feature",
                "path": "/README.md",
                "content": "updated\n",
                "message": "test write"
            }),
        )
        .await
        .unwrap();
    let value = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "feature", "path": "/README.md" }),
        )
        .await
        .unwrap();
    assert_eq!(value["text"], "updated\n");

    let commits = layerfs
        .call(
            "branch.list_commits".into(),
            json!({ "branch": "feature", "pageSize": 10, "pagePos": 0 }),
        )
        .await
        .unwrap();
    assert!(!commits["items"].as_array().unwrap().is_empty());
    assert_eq!(commits["items"][0]["message"], "test write");

    // main must still be the old content before publish.
    let value = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap();
    assert_eq!(value["text"], "hello\n");

    layerfs
        .call("branch.publish".into(), json!({ "branch": "feature" }))
        .await
        .unwrap();
    let value = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap();
    assert_eq!(value["text"], "updated\n");

    let error = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/missing" }),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "NoSuchPath");
    assert!(matches!(
        error,
        LayerFsError::Refused { data, .. } if data["path"] == "/missing"
    ));

    for method in ["workspace.mount", "fs.bogus"] {
        let error = layerfs
            .call(method.into(), json!({ "branch": "main" }))
            .await
            .unwrap_err();
        assert_eq!(error.code(), "UnknownMethod");
    }
}

#[tokio::test]
async fn missing_installed_manifest_is_no_store() {
    let (_root, config, manifest) = fixture();
    write_record(
        &config.home.join(PROJECT).join("project.record"),
        &manifest,
        None,
    );
    let layerfs = LayerFs::new(config).unwrap();
    let error = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "NoStore");
}

#[tokio::test]
async fn a_public_record_is_unauthorized() {
    let (_root, config, _manifest) = fixture();
    let record = config.home.join(PROJECT).join("project.record");
    std::fs::set_permissions(&record, std::fs::Permissions::from_mode(0o644)).unwrap();
    let layerfs = LayerFs::new(config).unwrap();
    let error = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "Unauthorized");
}

#[tokio::test]
async fn a_rutis_host_requires_and_calls_the_service() {
    let (_root, config, _manifest) = fixture();
    let ctx = rutis::Ctx::root().unwrap();
    let view = ctx.plugin(LayerFsPlugin::new(config));
    (&view).await.unwrap();
    let service = ctx.get::<LayerFs>().unwrap();
    let value = service
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap();
    assert_eq!(value["text"], "hello\n");

    view.dispose().await.unwrap();
    assert!(ctx.get::<LayerFs>().is_none());
    let error = service
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "InvalidInput");
    ctx.shutdown().await.unwrap();
}
