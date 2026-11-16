//! The end-to-end mount: bind a Workspace from the support fixture
//! (an installed Store with its owner), mount it over WinFsp, and
//! answer through the mount. Skips when the WinFsp driver service is
//! absent (fsptool install).
#![cfg(windows)]
#[path = "../../layerfs-store-service/tests/support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "../../layerfs-store-service/tests/support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_history::{BranchId, HistoryName, LayerStackId, WorkspaceId};
use layerfs_store_service::store::BindRequest;

#[test]
fn binds_and_mounts_a_workspace() {
    let driver = std::process::Command::new("sc")
        .args(["query", "WinFsp"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !driver {
        eprintln!("skipped: the WinFsp driver service is not installed (run fsptool install as admin)");
        return;
    }
    let sdk = support::Fixture::new("winfsp-e2e", |source| {
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
                branch: BranchId::named(
                    LayerStackId::from_authority([23; 16]),
                    &HistoryName::new("main").unwrap(),
                ),
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
