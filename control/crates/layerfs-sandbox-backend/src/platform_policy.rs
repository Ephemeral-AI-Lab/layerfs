//! Product platform restrictions, shared by host discovery and guest builds.
//! Disabled platforms keep their adapter code and records; removed ones have
//! no code left, and a record that still names one is shown, refused and
//! forgotten, never run. There is no environment or feature override;
//! re-enabling requires an explicit owner decision and source change.

/// The authoritative Firecracker refusal (layerfs-project#7).
pub const FIRECRACKER_DISABLED: &str =
    "firecracker is permanently disabled by product policy (layerfs-project#7)";
/// The refusal of a record naming the removed Microsandbox platform.
pub const MICROSANDBOX_REMOVED: &str = "platform `microsandbox` was removed in LayerFS 2.0";
/// The refusal of a record naming the removed SSH platform.
pub const SSH_REMOVED: &str = "platform `ssh` was removed in LayerFS 2.0";

/// Why a platform name LayerFS once had is gone: `microsandbox` and `ssh`
/// were removed in 2.0 (owner, 2026-11-08). A machine or Sandbox record may
/// still name one.
pub fn removed_reason(platform: &str) -> Option<&'static str> {
    match platform {
        "microsandbox" => Some(MICROSANDBOX_REMOVED),
        "ssh" => Some(SSH_REMOVED),
        _ => None,
    }
}

/// Why new work on this platform is refused, on every host and Store kind:
/// it is disabled, or it was removed.
pub fn disabled_reason(platform: &str) -> Option<&'static str> {
    match platform {
        "firecracker" => Some(FIRECRACKER_DISABLED),
        _ => removed_reason(platform),
    }
}
