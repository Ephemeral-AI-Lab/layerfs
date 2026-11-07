//! Product platform restrictions, shared by host discovery and guest builds.
//! Adapter code and existing records remain. There is no environment or feature
//! override; re-enabling requires an explicit owner decision and source change.

/// The authoritative Firecracker refusal (layerfs-project#7).
pub const FIRECRACKER_DISABLED: &str =
    "firecracker is permanently disabled by product policy (layerfs-project#7)";
/// The authoritative SSH refusal (layerfs-project#7).
pub const SSH_DISABLED: &str = "ssh is permanently disabled by product policy (layerfs-project#7)";

/// Why new work on this platform is disabled, on every host and Store kind.
pub fn disabled_reason(platform: &str) -> Option<&'static str> {
    match platform {
        "firecracker" => Some(FIRECRACKER_DISABLED),
        "ssh" => Some(SSH_DISABLED),
        _ => None,
    }
}
