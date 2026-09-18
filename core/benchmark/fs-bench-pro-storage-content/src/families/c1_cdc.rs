//! C1-3 `c1.cdc.chunk-count`: three chunk-count effects over the byte ladder.
//!
//! The frozen configuration is `START = 147,456`, `LEN = 65,536` and a rotation of
//! `[0, 5, 10, 3, 8]`. Each row asserts the pinned initial count, final count,
//! file root and map digest, and the direction its own ID names. The direction is
//! a claim, not a label: a row whose measured direction disagrees with its ID is
//! a finding and reports `FAIL`, not a renamed case.

use super::{leak, CaseSpec, BYTE_LADDER};
use crate::registry::{CacheState, Case, CountOp, Shape};

/// Family identifier.
pub const FAMILY: &str = "c1.cdc.chunk-count";

/// Frozen edit start offset.
pub const START: u64 = 147_456;
/// Frozen edit length.
pub const LEN: u64 = 65_536;
/// Frozen rotation of the replacement alphabet.
pub const ROTATIONS: [u8; 5] = [0, 5, 10, 3, 8];

/// Twelve rows: three ops over four sizes.
pub fn cases() -> Vec<Case> {
    let ops = [CountOp::Preserve, CountOp::Increase, CountOp::Decrease];
    let mut out = Vec::with_capacity(12);
    for op in ops {
        for (index, (bytes, label)) in BYTE_LADDER.iter().enumerate() {
            let op_name = match op {
                CountOp::Preserve => "preserve",
                CountOp::Increase => "increase",
                CountOp::Decrease => "decrease",
            };
            out.push(
                CaseSpec::new(
                    leak(format!("overwrite-fixed-64k-chunk-count-{op_name}-{label}")),
                    FAMILY,
                    Shape::ChunkCount(op),
                )
                .byte_tier(index, *bytes, label)
                .cache(CacheState::WarmInProcessFixture)
                .smoke_if(index == 0 && op == CountOp::Preserve)
                .build(),
            );
        }
    }
    out
}
