//! C1-10 `c1.change-locality`: the copy-on-write claim.
//!
//! The oracle is O4 (the three-tuple) **and** O5 as the claim itself:
//! `untouched_subtrees > 0`. `pages_reused` alone is *not* the claim — it means a
//! page was re-encoded to identical bytes, which a full rewrite can also produce.
//! The family is the renamed `workspace_change_locality` with the SDK-surface kind
//! dropped, so three kinds remain over four tiers.

use super::{leak, profile_for_tier, CaseSpec, ENTRY_LADDER};
use crate::registry::{CacheState, Case, LocalityOp, Shape};

/// Family identifier.
pub const FAMILY: &str = "c1.change-locality";

/// Twelve rows.
pub fn cases() -> Vec<Case> {
    let kinds = [
        (LocalityOp::CleanCommit, "clean-commit"),
        (LocalityOp::FixedMove, "fixed-move"),
        (LocalityOp::DenseRewrite, "dense-rewrite"),
    ];
    let mut out = Vec::with_capacity(12);
    for (op, name) in kinds {
        for (index, (entries, label)) in ENTRY_LADDER.iter().enumerate() {
            let profile = profile_for_tier(index, "mixed-v4", "compact-v2");
            out.push(
                CaseSpec::new(
                    leak(format!("workspace-{name}-{label}-{profile}")),
                    FAMILY,
                    Shape::Locality(op),
                )
                .entry_tier(index, *entries, label)
                .profile(profile)
                .cache(CacheState::WarmInProcessFixture)
                .smoke_if(index == 0 && name == "clean-commit")
                .build(),
            );
        }
    }
    out
}
