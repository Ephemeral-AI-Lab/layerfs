//! The three `component.primitives` diagnostic cases.
//!
//! **Not one of the 217.** `CONTRACT.md` section 3 registers them, runs them and
//! receipts them, and excludes them from admission and from every count. Under
//! `claim_kind = structural-complexity` (decision D1) their receipt is
//! diagnostic and cannot gate: `component.primitives` is the only library-matched
//! reference pair in the tree, and pairing it needs the reference arm, which lives
//! in the root `crates/` reference tree and is out of this harness's scope. The
//! rows therefore stay registered and report their measured state rather than
//! being folded into 217 or quietly dropped.

use super::{leak, CaseSpec};
use crate::registry::{CacheState, Case, PrimitiveOp, Shape};

/// Registry-group identifier. Deliberately not a `c1.*`/`c2.*` family.
pub const GROUP: &str = "component.primitives";

/// Three diagnostic rows.
pub fn cases() -> Vec<Case> {
    [
        (PrimitiveOp::Payload, "payload"),
        (PrimitiveOp::Filesystem, "filesystem"),
        (PrimitiveOp::Edit, "edit"),
    ]
    .iter()
    .map(|(op, name)| {
        CaseSpec::new(
            leak(format!("component-primitives-{name}")),
            GROUP,
            Shape::Primitives(*op),
        )
        .cache(CacheState::WarmInProcessFixture)
        .diagnostic()
        .build()
    })
    .collect()
}
