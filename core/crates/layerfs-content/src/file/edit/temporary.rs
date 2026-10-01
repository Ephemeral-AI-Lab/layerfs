//! Moved ownership of one bounded temporary subtree name; no implicit cleanup.
use crate::file::mapping::NodeSummary;

/// One temporary-summary pin issued by the selected draft authority.
/// Dropping this value performs no SQL or cleanup; failure retains the owner.
#[derive(Debug)]
#[repr(transparent)]
pub(crate) struct TemporarySummary(pub(crate) NodeSummary);
impl TemporarySummary {
    /// Read the subtree's immutable checked coverage without releasing its pin.
    pub(crate) const fn summary(&self) -> NodeSummary {
        self.0
    }
}
impl std::ops::Deref for TemporarySummary {
    type Target = NodeSummary;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
