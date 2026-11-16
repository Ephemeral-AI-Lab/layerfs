//! The end-to-end mount: bind a Workspace from the support fixture
//! (an installed Store with its owner), mount it over WinFsp, and
//! answer through the mount. Skips when the WinFsp control device is
//! not live (the driver not installed as admin).
#![cfg(windows)]
#[path = "../../layerfs-store-service/tests/support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "../../layerfs-store-service/tests/support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_history::WorkspaceId;
use layerfs_store_service::store::BindRequest;

#[test]
fn binds_and_mounts_a_workspace() {
    if !layerfs_winfsp::device_live() {
        eprintln!("skipped: the WinFsp control device is not live (install the driver as admin)");
        return;
    }
    let label = format!("winfsp-e2e-{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    let sdk = support::Fixture::new(&label, |source| {
        std::fs::write(source.join("seed.txt"), b"seed").unwrap();
        1
    });
    let bound = sdk
        .service
        .store
        .bind(
            sdk.owner.client(),
            BindRequest {
                workspace: WorkspaceId::from_authority([7; 32]).unwrap(),
                branch: sdk.host.branch,
            },
        )
        .expect("bind");
    let mounted =
        layerfs_winfsp::mount_workspace("Y:", std::sync::Arc::new(bound.workspace))
            .expect("mount");
    let entries: Vec<_> = std::fs::read_dir("Y:").expect("read the mounted root").collect();
    assert!(!entries.is_empty(), "the seed answers through the mount");
    drop(mounted);
}
