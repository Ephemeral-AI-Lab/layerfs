//! Tier 0 integration: an installed Store opened in this process.

#![cfg(target_os = "linux")]

mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_rutis::{LayerFs, LayerFsError, LayerFsPlugin};
use serde_json::json;
use std::os::unix::fs::{symlink, PermissionsExt};
use support::{fixture, write_control, write_record, PROJECT};

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
    layerfs.close().await.unwrap();
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
async fn a_missing_installed_key_is_no_store() {
    let (_root, config, manifest) = fixture();
    write_control(
        &config.home.join(PROJECT).join("project.record"),
        vec![(
            "manifest".to_owned(),
            SdkValue::Bytes(manifest.encode().unwrap()),
        )],
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
async fn an_empty_or_wrong_type_installed_value_is_invalid() {
    for installed in [SdkValue::Bytes(Vec::new()), SdkValue::Text("bad".into())] {
        let (_root, config, manifest) = fixture();
        write_control(
            &config.home.join(PROJECT).join("project.record"),
            vec![
                (
                    "manifest".to_owned(),
                    SdkValue::Bytes(manifest.encode().unwrap()),
                ),
                ("installed".to_owned(), installed),
            ],
        );
        let layerfs = LayerFs::new(config).unwrap();
        let error = layerfs
            .call(
                "fs.read".into(),
                json!({ "branch": "main", "path": "/README.md" }),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code(), "InvalidInput");
    }
}

#[tokio::test]
async fn undecodable_installed_bytes_fail() {
    let (_root, config, manifest) = fixture();
    write_control(
        &config.home.join(PROJECT).join("project.record"),
        vec![
            (
                "manifest".to_owned(),
                SdkValue::Bytes(manifest.encode().unwrap()),
            ),
            ("installed".to_owned(), SdkValue::Bytes(b"junk".to_vec())),
        ],
    );
    let layerfs = LayerFs::new(config).unwrap();
    let error = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "Failed");
}

#[tokio::test]
async fn an_installed_manifest_without_daemon_sqlite_fails() {
    let (_root, config, manifest) = fixture();
    write_record(
        &config.home.join(PROJECT).join("project.record"),
        &manifest,
        Some(&manifest),
    );
    let layerfs = LayerFs::new(config).unwrap();
    let error = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        LayerFsError::Control(message) if message.contains("no daemon SQLite")
    ));
}

#[tokio::test]
async fn info_does_not_open_the_store() {
    let (_root, config, _manifest) = fixture();
    let store = config.store_path.clone();
    let layerfs = LayerFs::new(config).unwrap();
    std::fs::remove_file(store).unwrap();
    assert_eq!(layerfs.info().await.project, PROJECT);
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
async fn unknown_method_fails_before_the_store_opens() {
    let (_root, config, _manifest) = fixture();
    let store = config.store_path.clone();
    let layerfs = LayerFs::new(config).unwrap();
    std::fs::remove_file(store).unwrap();

    let error = layerfs
        .call("workspace.mount".into(), json!({ "branch": "main" }))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "UnknownMethod");

    let error = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "NoStore");
}

#[test]
fn a_symlinked_record_is_rejected() {
    let (_root, config, _manifest) = fixture();
    let record = config.home.join(PROJECT).join("project.record");
    let target = config.home.join(PROJECT).join("moved.record");
    std::fs::rename(&record, &target).unwrap();
    symlink(&target, &record).unwrap();
    match LayerFs::new(config) {
        Ok(_) => panic!("a symlinked record must be rejected"),
        Err(error) => assert_eq!(error.code(), "Unauthorized"),
    }
}

#[tokio::test]
async fn a_mismatched_installed_manifest_fails() {
    let (_root, config, manifest) = fixture();
    let mut installed = manifest.clone();
    installed.daemon_sqlite = Some(installed.host_sqlite.clone());
    installed.root = [9; 32];
    write_record(
        &config.home.join(PROJECT).join("project.record"),
        &manifest,
        Some(&installed),
    );
    let layerfs = LayerFs::new(config).unwrap();
    let error = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        LayerFsError::Control(message)
            if message.contains("installed manifest does not match the project")
    ));
}

#[tokio::test]
async fn a_loosened_project_directory_fails_on_first_call() {
    let (_root, config, _manifest) = fixture();
    let project = config.home.join(PROJECT);
    let layerfs = LayerFs::new(config).unwrap();
    std::fs::set_permissions(&project, std::fs::Permissions::from_mode(0o755)).unwrap();
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
async fn a_record_replaced_with_a_symlink_fails_on_first_call() {
    let (_root, config, _manifest) = fixture();
    let record = config.home.join(PROJECT).join("project.record");
    let target = config.home.join(PROJECT).join("moved.record");
    let layerfs = LayerFs::new(config).unwrap();
    std::fs::rename(&record, &target).unwrap();
    symlink(&target, &record).unwrap();
    let error = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "Unauthorized");
}

#[test]
fn layerfs_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<LayerFs>();
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
    assert_eq!(service.info().await.project, PROJECT);
    let error = service
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        LayerFsError::Invalid(message) if message.contains("closed")
    ));
    ctx.shutdown().await.unwrap();
}
