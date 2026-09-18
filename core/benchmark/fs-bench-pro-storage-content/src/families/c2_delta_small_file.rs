//! C2-6 `c2.delta.small-file`: the delta policy below the cutoff.
//!
//! Owner decision D5 defines this family rather than deferring it: four cases, one
//! per size tier, exercising the delta policy below the 131,072 cutoff.
//! `benchmark_rules.md` section 7 forbids accepting favourable members while moving
//! unfavourable siblings to a later release, so dropping the family would do
//! exactly that. The claim is the policy outcome only: no pack-read claim is made.

use super::{leak, CaseSpec};
use crate::registry::{CacheState, Case, Shape, StoreState};

/// Family identifier.
pub const FAMILY: &str = "c2.delta.small-file";

/// The declared size tiers.
pub const TIERS: [u64; 4] = [1_024, 16_384, 65_536, 131_071];

/// Four rows.
pub fn cases() -> Vec<Case> {
    TIERS
        .iter()
        .enumerate()
        .map(|(index, bytes)| {
            CaseSpec::new(
                leak(format!("small-file-delta-{bytes}")),
                FAMILY,
                Shape::SmallFile,
            )
            .byte_tier(index, *bytes, "")
            .cache(CacheState::PreparedDewarmed)
            .store(StoreState::OpenedFromCopy)
            .smoke_if(index == 0)
            .build()
        })
        .collect()
}
