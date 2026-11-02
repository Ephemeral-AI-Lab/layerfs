//! Internal snapshot method.
use crate::{internal::Control, SdkResult};
use layerfs_bridge::control::SdkValue;

/// Reads the daemon's read model once on its backend method table.
pub fn snapshot(control: &mut Control, args: SdkValue) -> SdkResult {
    super::super::call(control, "sandbox.list_workspaces", args)
}
