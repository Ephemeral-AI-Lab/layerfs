//! Public identities and observations; runtime ownership lives in layerfs-sandbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SandboxId(pub [u8; 16]);

impl std::fmt::Display for SandboxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxStatus {
    Ready,
    Unconfirmed,
    Stale,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxInfo {
    pub id: SandboxId,
    pub name: String,
    pub status: SandboxStatus,
}

/// One owned sandbox removal outcome.
///
/// Confirmed removal is reported per resource, so a caller can record partial
/// cleanup and make an explicit later attempt with the same Sandbox ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteError {
    pub sandbox: SandboxId,
    pub cause: layerfs_bridge::contract::Failure,
    pub container_removed: bool,
    pub volume_removed: bool,
}
