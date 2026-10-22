//! Tier 1: Workspace calls over the shared session layer.

#![cfg(target_os = "linux")]

mod support;

use layerfs_rutis::{Config, Deploy, LayerFs, LayerFsError};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use support::{deploy, fixture};

fn deployed(mut config: Config, root: &Path) -> Config {
    config.deploy = Some(deploy(root));
    config
}

#[tokio::test]
async fn workspace_calls_without_a_deployment_are_invalid_input() {
    let (_root, config, _manifest) = fixture();
    let layerfs = LayerFs::new(config).unwrap();
    let cases = [
        layerfs.workspace_mount(json!({ "branch": "main" })).await,
        layerfs
            .workspace_status(json!({ "sandbox": "1", "path": "/ws" }))
            .await,
        layerfs
            .workspace_commit(json!({ "sandbox": "1", "path": "/ws" }))
            .await,
        layerfs
            .workspace_unmount(json!({ "sandbox": "1", "path": "/ws" }))
            .await,
        layerfs
            .workspace_exec(json!({ "sandbox": "1", "path": "/ws", "command": "true" }))
            .await,
    ];
    for result in cases {
        assert_eq!(result.unwrap_err().code(), "InvalidInput");
    }
    // Store calls still work without a deployment.
    let value = layerfs
        .call(
            "fs.read".into(),
            json!({ "branch": "main", "path": "/README.md" }),
        )
        .await
        .unwrap();
    assert_eq!(value["text"], "hello\n");
    layerfs.close().await.unwrap();
}

#[test]
fn deploy_shape_is_validated_at_apply() {
    type Change = fn(&mut Deploy);
    let cases: Vec<Change> = vec![
        |deploy| deploy.docker = "docker.sock".into(),
        |deploy| deploy.image = "sha256:short".into(),
        |deploy| deploy.image = format!("sha256:{}", "A".repeat(64)),
        |deploy| deploy.volume = "bad volume".into(),
        |deploy| deploy.daemon = "layerfs-daemon".into(),
        |deploy| deploy.container_prefix = Some("-bad".into()),
        |deploy| deploy.daemon_host = Some(String::new()),
    ];
    for change in cases {
        let (root, config, _manifest) = fixture();
        let mut deploy = deploy(root.path());
        change(&mut deploy);
        let config = Config {
            deploy: Some(deploy),
            ..config
        };
        match LayerFs::new(config) {
            Ok(_) => panic!("an invalid deployment must be refused"),
            Err(error) => assert_eq!(error.code(), "InvalidInput"),
        }
    }
}

#[tokio::test]
async fn unknown_sandbox_is_refused_before_any_engine_call() {
    let (root, config, _manifest) = fixture();
    let layerfs = LayerFs::new(deployed(config, root.path())).unwrap();
    let error = layerfs
        .workspace_status(json!({ "sandbox": "1", "path": "/ws" }))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "NoSuchSandbox");
    let error = layerfs
        .workspace_commit(json!({ "sandbox": "1", "path": "/ws" }))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "NoSuchSandbox");
    let error = layerfs
        .workspace_exec(json!({ "sandbox": "1", "path": "/ws", "command": "true" }))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "NoSuchSandbox");
    let error = layerfs
        .workspace_unmount(json!({ "sandbox": "1", "path": "/ws" }))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "NoSuchSandbox");
    layerfs.close().await.unwrap();
}

#[tokio::test]
async fn exec_sink_requires_absolute_paths() {
    let (root, config, _manifest) = fixture();
    let layerfs = LayerFs::new(deployed(config, root.path())).unwrap();
    let error = layerfs
        .workspace_exec(json!({
            "sandbox": "1",
            "path": "/ws",
            "command": "true",
            "sink": { "stdout": "relative.log" }
        }))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "InvalidInput");
    let error = layerfs
        .workspace_exec(json!({
            "sandbox": "1",
            "path": "/ws",
            "command": "true",
            "sink": {}
        }))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "InvalidInput");
    let error = layerfs
        .workspace_exec(json!({
            "sandbox": "1",
            "path": "/ws",
            "command": "true",
            "sink": "/tmp/out.log"
        }))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "InvalidInput");
}

#[tokio::test]
async fn a_public_project_directory_is_refused_on_the_first_workspace_call() {
    let (root, config, _manifest) = fixture();
    let project = config.home.join(support::PROJECT);
    let layerfs = LayerFs::new(deployed(config, root.path())).unwrap();
    std::fs::set_permissions(&project, std::fs::Permissions::from_mode(0o755)).unwrap();
    let error = layerfs
        .workspace_status(json!({ "sandbox": "1", "path": "/ws" }))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "Unauthorized");
    assert!(matches!(error, LayerFsError::Refused { .. }));
}
