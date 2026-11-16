//! One real mount with the first callbacks: the volume answers.
//! Skips when the WinFsp driver service is absent (fsptool install).
#![cfg(windows)]
use layerfs_mount_ports::ports::Fence;
use layerfs_mount_ports::ports::{MountServices, RequestServices, ServiceError};
use layerfs_winfsp::ffi::FSP_FILE_SYSTEM_INTERFACE;
use layerfs_winfsp::mount;
use layerfs_winfsp::services::Context;
use std::sync::Arc;

/// An empty services face: requests open, nothing answers.
struct Empty;
impl MountServices for Empty {
    fn request(&self, _fence: &Fence) -> Result<Arc<dyn RequestServices>, ServiceError> {
        Err("empty".into())
    }
}

#[test]
fn mounts_with_the_first_callbacks() {
    let installed = std::process::Command::new("sc")
        .args(["query", "WinFsp"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !installed {
        eprintln!("skipped: the WinFsp driver service is not installed (run fsptool install as admin)");
        return;
    }
    let interface = FSP_FILE_SYSTEM_INTERFACE {
        GetVolumeInfo: Some(layerfs_winfsp::callbacks::get_volume_info),
        GetSecurityByName: Some(layerfs_winfsp::callbacks::get_security_by_name),
        ..Default::default()
    };
    let context = Context {
        services: Arc::new(Empty),
        fence: Fence::default(),
        mount: layerfs_winfsp::services::mint_mount(),
    };
    let mounted = mount("Y:", interface, context).expect("mount");
    let meta = std::fs::metadata("Y:").expect("the mounted root answers");
    assert!(meta.is_dir());
    drop(mounted);
}
