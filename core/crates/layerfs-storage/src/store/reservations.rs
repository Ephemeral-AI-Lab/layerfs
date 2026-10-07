//! Startup allocation windows, independent from total Save size.
use crate::{policy, StorageError, StorageResult};

/// IDs requested in the initial combined reservation and later refills.
/// A larger bounded demand can request a larger refill within the port limit.
/// Blocks never limit total objects/bytes accepted by a Save.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReservationBlocks {
    /// Pack ids per ordinary refill, in `1..=TRANSACTION_ROW_LIMIT`.
    pub packs: usize,
    /// Pooled value ordinals per refill, in `1..=METADATA_INDEX_VALUES`.
    pub ordinals: usize,
}
impl Default for ReservationBlocks {
    fn default() -> Self {
        Self {
            packs: 4096,
            ordinals: 16384,
        }
    }
}
impl ReservationBlocks {
    /// Validates allocation windows without touching provider state.
    pub fn validate(self) -> StorageResult<Self> {
        if self.packs == 0
            || self.packs > policy::TRANSACTION_ROW_LIMIT as usize
            || self.ordinals == 0
            || self.ordinals > policy::METADATA_INDEX_VALUES
        {
            return Err(StorageError::UnsupportedPolicy {
                field: "Save reservation blocks",
            });
        }
        Ok(self)
    }
}
