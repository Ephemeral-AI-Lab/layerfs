//! C2-7 `c2.read.waves`: independent bounded reads over a stored ladder.
//!
//! The O(1) claim gated on `opens`: one on the opening wave and zero afterwards,
//! and `pages` had to equal `ceil(ids / 128)`. Both were counters of the removed
//! `StoreProvider::read_wave`. The current reader (`Storage::reader()`) publishes
//! neither, so the driver still runs and times the two waves and reports the two
//! gates `INCOMPLETE` (`ops::store::UNAVAILABLE_COUNTERS`).

use super::{leak, CaseSpec, BYTE_LADDER};
use crate::registry::{CacheState, Case, Preparation, Shape, StoreState};

/// Family identifier.
pub const FAMILY: &str = "c2.read.waves";

/// Four rows over the byte ladder.
pub fn cases() -> Vec<Case> {
    BYTE_LADDER
        .iter()
        .enumerate()
        .map(|(index, (bytes, label))| {
            let profile = if *bytes < 100 << 20 { "compact-v2" } else { "" };
            let id = if profile.is_empty() {
                leak(format!("payload-random-read-{label}"))
            } else {
                leak(format!("payload-random-read-{label}-{profile}"))
            };
            CaseSpec::new(id, FAMILY, Shape::ReadWave)
                .byte_tier(index, *bytes, label)
                .profile(profile)
                .cache(CacheState::PreparedDewarmed)
                .store(StoreState::OpenedFromCopy)
                .prepared(Preparation::BaseStore)
                .smoke_if(index == 0)
                .build()
        })
        .collect()
}
