//! C1-8 `c1.tree.construct-traverse` and C1-9 `c1.tree.namespace-mutation`.
//!
//! Both families assert the O4 three-tuple — directory root, inode table and
//! filesystem root — plus a listing that equals the fixture manifest. The mutation
//! family additionally asserts `released` equals its declared count, which is the
//! mechanism (O5) evidence that the release actually happened.

use super::{leak, profile_for_tier, CaseSpec, ENTRY_LADDER};
use crate::registry::{CacheState, Case, Shape, TreeOp};
use crate::families::leak as leak_id;

/// Family identifier.
pub const CONSTRUCT_TRAVERSE: &str = "c1.tree.construct-traverse";
/// Family identifier.
pub const NAMESPACE_MUTATION: &str = "c1.tree.namespace-mutation";

/// C1-8: three kinds over four tiers.
pub fn construct_traverse() -> Vec<Case> {
    let kinds = [
        (TreeOp::Construct, "construct"),
        (TreeOp::MetadataScan, "metadata-scan"),
        (TreeOp::ContentScan, "content-scan"),
    ];
    let mut out = Vec::with_capacity(12);
    for (op, name) in kinds {
        for (index, (entries, label)) in ENTRY_LADDER.iter().enumerate() {
            let profile = profile_for_tier(index, "mixed-v4", "compact-v2");
            out.push(
                CaseSpec::new(
                    leak(format!("directory-{name}-{label}-{profile}")),
                    CONSTRUCT_TRAVERSE,
                    Shape::Tree(op),
                )
                .entry_tier(index, *entries, label)
                .profile(profile)
                .cache(CacheState::WarmInProcessFixture)
                .smoke_if(index == 0 && name == "construct")
                .build(),
            );
        }
    }
    out
}

/// C1-9: one kind over four tiers.
pub fn namespace_mutation() -> Vec<Case> {
    ENTRY_LADDER
        .iter()
        .enumerate()
        .map(|(index, (entries, label))| {
            let profile = profile_for_tier(index, "mixed-v4", "compact-v2");
            let _ = leak_id;
            CaseSpec::new(
                leak(format!("namespace-subtree-relocate-delete-{label}-{profile}")),
                NAMESPACE_MUTATION,
                Shape::Namespace,
            )
            .entry_tier(index, *entries, label)
            .profile(profile)
            .cache(CacheState::WarmInProcessFixture)
            .smoke_if(index == 0)
            .build()
        })
        .collect()
}
