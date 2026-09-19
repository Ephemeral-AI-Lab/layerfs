//! C2-3 `c2.delta.cdc-locality` and its registered sub-lane `c2.delta.boundaries`.
//!
//! The locality half proves the *route* through policy outcomes (`prefix_selected`,
//! `full_losses`, `no_candidate`, `work_exceeded`), never through speed. The
//! boundary sub-lane walks the chunk grammar's edges, declared in the product as
//! `dedup_cdc_locality::boundaries()`: `0, 1, 8,191, 8,192, 16,384, 32,768,
//! 32,769`, three seeds each. The sub-lane is a registered group of its own and
//! carries **no** `--smoke` representative: the twenty-family smoke lane counts it
//! under its parent family.

use super::{leak, CaseSpec, ENTRY_LADDER};
use crate::registry::{CacheState, Case, DeltaOp, Preparation, Shape, StoreState};

/// Family identifier.
pub const CDC_LOCALITY: &str = "c2.delta.cdc-locality";
/// Registered sub-lane identifier.
pub const BOUNDARIES: &str = "c2.delta.boundaries";

/// The product's declared chunk-grammar boundary lengths.
pub const BOUNDARY_LENGTHS: [u64; 7] = [0, 1, 8_191, 8_192, 16_384, 32_768, 32_769];

/// Declared seeds 1..3.
pub const BOUNDARY_SEEDS: [u8; 3] = [1, 2, 3];

/// C2-3 locality: five kinds over four tiers.
pub fn cdc_locality() -> Vec<Case> {
    let kinds = [
        (DeltaOp::Overwrite, "overwrite"),
        (DeltaOp::Insert, "insert"),
        (DeltaOp::Delete, "delete"),
        (DeltaOp::CommonBody, "common-body"),
        (DeltaOp::Scattered, "scattered"),
    ];
    let mut out = Vec::with_capacity(20);
    for (op, name) in kinds {
        for (index, (entries, label)) in ENTRY_LADDER.iter().enumerate() {
            out.push(
                CaseSpec::new(
                    leak(format!("dedup-cdc-{name}-{label}")),
                    CDC_LOCALITY,
                    Shape::Delta(op),
                )
                .entry_tier(index, *entries, label)
                .cache(CacheState::PreparedDewarmed)
                .store(StoreState::OpenedFromCopy)
                .prepared(Preparation::BaseStore)
                .smoke_if(index == 0 && name == "overwrite")
                .build(),
            );
        }
    }
    out
}

/// C2-3 sub-lane: seven grammar edges over three seeds.
pub fn boundaries() -> Vec<Case> {
    let mut out = Vec::with_capacity(21);
    for length in BOUNDARY_LENGTHS {
        for seed in BOUNDARY_SEEDS {
            out.push(
                CaseSpec::new(
                    leak(format!("dedup-cdc-boundary-{length}-s{seed}")),
                    BOUNDARIES,
                    Shape::Boundary { seed },
                )
                .byte_tier(0, length, "")
                .cache(CacheState::PreparedDewarmed)
                .store(StoreState::OpenedFromCopy)
                .build(),
            );
        }
    }
    out
}
