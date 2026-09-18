//! C1-4 `c1.edit.length-preserving` and C1-5 `c1.edit.length-changing`.
//!
//! Two families over one byte ladder. The preserving family is three 4 KiB
//! overwrites (head, middle, tail) whose result must be byte-exactly as long as
//! its base; the changing family is eight ops whose result must satisfy
//! `final = base - removed + replacement` exactly. Both hand the edit an
//! authenticated provider and a consumer, and both assert the root identity and a
//! byte-exact readback, so the oracle never reads the mutation's own output.

use super::{leak, CaseSpec, BYTE_LADDER};
use crate::registry::{CacheState, Case, EditOp, Shape};

/// Family identifier.
pub const LENGTH_PRESERVING: &str = "c1.edit.length-preserving";
/// Family identifier.
pub const LENGTH_CHANGING: &str = "c1.edit.length-changing";

/// The 4 KiB replacement every edit family uses.
pub const REPLACEMENT_LEN: u64 = 4_096;

/// The preserving family's rotation.
pub const PRESERVING_ROTATION: [u8; 5] = [0, 5, 10, 3, 8];
/// The changing family's rotation.
pub const CHANGING_ROTATION: [u8; 5] = [0, 13, 26, 7, 20];

/// C1-4: three ops over four sizes.
pub fn length_preserving() -> Vec<Case> {
    let ops = [
        (EditOp::OverwriteHead, "head"),
        (EditOp::OverwriteMiddle, "middle"),
        (EditOp::OverwriteTail, "tail"),
    ];
    let mut out = Vec::with_capacity(12);
    for (op, name) in ops {
        for (index, (bytes, label)) in BYTE_LADDER.iter().enumerate() {
            out.push(
                CaseSpec::new(
                    leak(format!("overwrite-{name}-4k-{label}")),
                    LENGTH_PRESERVING,
                    Shape::Edit(op),
                )
                .byte_tier(index, *bytes, label)
                .cache(CacheState::WarmInProcessFixture)
                .smoke_if(index == 0 && name == "middle")
                .build(),
            );
        }
    }
    out
}

/// C1-5: eight ops over four sizes.
pub fn length_changing() -> Vec<Case> {
    let ops = [
        (EditOp::InsertMiddle, "insert-middle-4k"),
        (EditOp::DeleteMiddle, "delete-middle-4k"),
        (EditOp::AppendTail, "append-tail-4k"),
        (EditOp::PrependHead, "prepend-head-4k"),
        (EditOp::Grow, "grow"),
        (EditOp::Shrink, "shrink"),
        (EditOp::Truncate, "truncate"),
        (EditOp::ZeroExtend, "zero-extend"),
    ];
    let mut out = Vec::with_capacity(32);
    for (op, name) in ops {
        for (index, (bytes, label)) in BYTE_LADDER.iter().enumerate() {
            out.push(
                CaseSpec::new(
                    leak(format!("{name}-{label}")),
                    LENGTH_CHANGING,
                    Shape::Edit(op),
                )
                .byte_tier(index, *bytes, label)
                .cache(CacheState::WarmInProcessFixture)
                .smoke_if(index == 0 && name == "insert-middle-4k")
                .build(),
            );
        }
    }
    out
}
