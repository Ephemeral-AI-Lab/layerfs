//! C2-7 `c2.read.waves`: independent bounded reads over a stored ladder.
//!
//! The O(1) claim gates on `opens`: one on the opening wave and zero afterwards,
//! **but only through `StoreProvider::read_wave`**. The direct `Store::read_batch`
//! route reports `opens: 1` unconditionally, so this cell is ungateable on that
//! route and the case is driven through the provider. `pages` must equal
//! `ceil(ids / 128)`.

use super::{leak, CaseSpec, BYTE_LADDER};
use crate::registry::{CacheState, Case, Shape, StoreState};

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
                .smoke_if(index == 0)
                .build()
        })
        .collect()
}
