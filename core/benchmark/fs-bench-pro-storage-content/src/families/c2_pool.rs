//! C2-8 `c2.pool.cold-warm`: the pooled metadata lane, cold against warm.
//!
//! `c2-families.md` writes the configuration as `24 / 128 / 512 leaves × 100 rows`.
//! The harness declares the case configuration as **512 leaves × 100 rows**: the
//! largest listed leaf count, so the row measures the pooling claim at the point
//! where it matters, and the declaration is recorded here rather than inferred by
//! a driver. Cold and warm are reported separately and are never pooled.

use super::CaseSpec;
use crate::registry::{CacheState, Case, Shape, StoreState};

/// Family identifier.
pub const FAMILY: &str = "c2.pool.cold-warm";

/// Declared leaves of the case configuration.
pub const LEAVES: u32 = 512;
/// Declared pooled rows per leaf.
pub const ROWS: u32 = 100;

/// Two rows: cold and warm.
pub fn cases() -> Vec<Case> {
    [true, false]
        .iter()
        .map(|cold| {
            CaseSpec::new(
                if *cold {
                    "pooled-lane-cold"
                } else {
                    "pooled-lane-warm"
                },
                FAMILY,
                Shape::Pool { cold: *cold },
            )
            .entry_tier(0, LEAVES, "")
            .cache(CacheState::PreparedDewarmed)
            .store(StoreState::OpenedFromCopy)
            .smoke_if(*cold)
            .build()
        })
        .collect()
}
