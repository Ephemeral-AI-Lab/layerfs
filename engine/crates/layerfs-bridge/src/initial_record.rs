//! One exact discriminator for the first authenticated application record.
use crate::{
    control::{Call, ControlError},
    provision::StoreManifest,
};
/// Owned first request; installation payload records are never routed through this.
#[derive(Clone, Debug, Eq, PartialEq)]
// Pinned 1.85.1 Clippy loses layout through recursive boxed Request and reports
// Call/InitialRecord as zero bytes. Actual ARM64 layout is Request112, Call120,
// StoreManifest208, InitialRecord208: the variant gap is 88, below its 200 gate.
// External observed_records::actual_inline_initial_record_layout_has_no_large_variant_gap
// guards the real layout. Expectation becomes unfulfilled if the heuristic is fixed.
#[expect(
    clippy::large_enum_variant,
    reason = "pinned Clippy recursive-Request layout false positive; external layout guard verifies the actual 88-byte gap"
)]
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
