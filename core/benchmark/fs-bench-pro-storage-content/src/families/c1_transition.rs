//! C1-6 `c1.transition.boundary`: the whole-file to chunked transition.
//!
//! Seven fixed rows across the frozen cutoff `DEFAULT_SMALL_FILE_THRESHOLD_BYTES`
//! (131,072). `BOUNDARY_{BELOW,EXACT,ABOVE}` are **harness-declared**, not product
//! constants: they appear nowhere under `core/crates/*/src` and there is no
//! `131_071` literal. The registry pins them here and marks them declared; the
//! constant-parity test can only anchor the cutoff itself.

use super::{leak, CaseSpec};
use crate::registry::{CacheState, Case, Shape, Transition};

/// Family identifier.
pub const FAMILY: &str = "c1.transition.boundary";

/// Declared boundary point below the cutoff.
pub const BOUNDARY_BELOW: u64 = 131_071;
/// Declared boundary point at the cutoff.
pub const BOUNDARY_EXACT: u64 = 131_072;
/// Declared boundary point above the cutoff.
pub const BOUNDARY_ABOVE: u64 = 131_073;
/// The family's small control.
pub const SMALL_CONTROL: u64 = 4_096;
/// The family's large control.
pub const LARGE_CONTROL: u64 = 1 << 20;

/// Seven fixed rows.
pub fn cases() -> Vec<Case> {
    let rows = [
        (Transition::SmallControl, SMALL_CONTROL),
        (Transition::Below, BOUNDARY_BELOW),
        (Transition::Exact, BOUNDARY_EXACT),
        (Transition::Above, BOUNDARY_ABOVE),
        (Transition::LargeControl, LARGE_CONTROL),
        (Transition::Roundtrip, BOUNDARY_ABOVE),
        (Transition::AliasRoundtrip, BOUNDARY_ABOVE),
    ];
    rows.iter()
        .enumerate()
        .map(|(index, (target, bytes))| {
            let name = match target {
                Transition::SmallControl => "small-control",
                Transition::Below => "below",
                Transition::Exact => "exact",
                Transition::Above => "above",
                Transition::LargeControl => "large-control",
                Transition::Roundtrip => "roundtrip",
                Transition::AliasRoundtrip => "alias-roundtrip",
            };
            let mut spec = CaseSpec::new(
                leak(format!("v016-boundary-{name}-v1")),
                FAMILY,
                Shape::Transition(*target),
            )
            .cache(CacheState::WarmInProcessFixture);
            // Fixed-config family: the tier columns stay empty and the smoke
            // representative is the exact-cutoff row, the boundary itself.
            spec = spec.smoke_if(*target == Transition::Exact);
            let _ = (index, bytes);
            spec.build()
        })
        .collect()
}
