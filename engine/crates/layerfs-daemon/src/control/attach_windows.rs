//! The Windows Attach face: bind one admitted Workspace over the
//! in-process WinFsp mount (#454). The control Service itself is
//! the unix registry face; the Windows control registry is the
//! follow-up this skeleton names. Today the call answers its
//! boundary; the driver install flips it to the real mount.
use super::{Failure, Success};

/// Binds the token Workspace over WinFsp at one directory under the
/// mounts root. The registry entry move lands with the Windows
/// control registry batch; the mount itself needs the driver.
pub(super) fn attach_winfsp() -> Result<Success, Failure> {
    Err(Failure::Rejected(
        layerfs_bridge::control::ControlCode::Invalid,
        "WinFsp attach lands with the driver install and the Windows control registry (#454)",
    ))
}
