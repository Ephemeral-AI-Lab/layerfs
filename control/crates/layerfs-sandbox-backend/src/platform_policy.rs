//! Platforms removed from LayerFS, shared by host discovery and guest builds.
//! A removed platform has no code left; a record that still names one is
//! shown, refused and forgotten, never run. No platform is disabled with its
//! code kept. There is no environment or feature override; bringing a
//! platform back requires an explicit owner decision and source change.

/// The refusal of a record naming the removed Firecracker platform.
pub const FIRECRACKER_REMOVED: &str = "platform `firecracker` was removed in LayerFS 2.0";
/// The refusal of a record naming the removed Microsandbox platform.
pub const MICROSANDBOX_REMOVED: &str = "platform `microsandbox` was removed in LayerFS 2.0";
/// The refusal of a record naming the removed SSH platform.
pub const SSH_REMOVED: &str = "platform `ssh` was removed in LayerFS 2.0";

/// Why a platform name LayerFS once had is gone: `firecracker`,
/// `microsandbox` and `ssh` were removed in 2.0 (owner, 2026-11-08). A
/// machine or Sandbox record may still name one.
pub fn removed_reason(platform: &str) -> Option<&'static str> {
    match platform {
        "firecracker" => Some(FIRECRACKER_REMOVED),
        "microsandbox" => Some(MICROSANDBOX_REMOVED),
        "ssh" => Some(SSH_REMOVED),
        _ => None,
    }
}

/// Why new work on this platform is refused, on every host and Store kind:
/// the platform was removed ([`removed_reason`]). The call-level policy
/// asks this of every platform a call or a record names.
pub fn disabled_reason(platform: &str) -> Option<&'static str> {
    removed_reason(platform)
}
