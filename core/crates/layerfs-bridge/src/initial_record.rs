//! One exact discriminator for the first authenticated application record.
use crate::{
    control::{Call, ControlError},
    provision::StoreManifest,
};
/// Owned first request; installation payload records are never routed through this.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InitialRecord {
    /// One original correlated control request.
    Control(Call),
    /// One original provisioning request, decoded before the receiver is reused.
    Install(StoreManifest),
}
impl InitialRecord {
    /// Selects exactly one existing decoder. A failed decoder has no fallback.
    pub fn decode(bytes: &[u8]) -> Result<Self, ControlError> {
        if bytes.starts_with(crate::control_request::MAGIC) {
            Call::decode(bytes).map(Self::Control)
        } else if bytes.starts_with(crate::provision_wire::MAGIC) {
            StoreManifest::decode(bytes).map(Self::Install)
        } else {
            Err(ControlError("initial record kind or version"))
        }
    }
}
