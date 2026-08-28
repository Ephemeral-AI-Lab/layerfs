//! Platform projection-driver selection.

#[cfg(not(target_os = "macos"))]
use std::path::Path;

use crate::driver;

#[cfg(target_os = "macos")]
pub type HostDriver = crate::apfs::AppleDriver;

#[cfg(not(target_os = "macos"))]
#[derive(Default)]
pub struct HostDriver;

#[cfg(not(target_os = "macos"))]
impl driver::ProjectionDriver for HostDriver {
    fn open_workspace(
        &self,
        _path: &Path,
        _policy: driver::WorkspacePolicy,
        _store_id: [u8; 32],
    ) -> driver::Result<Box<dyn driver::ProjectionWorkspace>> {
        Err(driver::DriverError::Unsupported)
    }
}

pub fn host_driver() -> std::sync::Arc<dyn driver::ProjectionDriver> {
    #[cfg(target_os = "macos")]
    let driver = HostDriver::default();
    #[cfg(not(target_os = "macos"))]
    let driver = HostDriver;
    std::sync::Arc::new(driver)
}
