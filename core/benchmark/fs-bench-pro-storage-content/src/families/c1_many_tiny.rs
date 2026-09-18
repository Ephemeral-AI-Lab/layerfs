//! C1-7 `c1.many-tiny`: the cutoff straddle for many small objects.
//!
//! Twenty rows: three per-file operations and two bulk operations over the entry
//! ladder. `c1-families.md` section 3.1 writes the profile as a bracketed list
//! (`[-compact-v2|-mixed-v4]`), which the frozen parsing rule makes **one** case
//! per tier; `super::profile_for_tier` fixes which variant each tier takes and the
//! golden TSV records the choice per row.

use super::{profile_for_tier, CaseSpec, ENTRY_LADDER};
use crate::registry::{CacheState, Case, Shape, TinyOp};
use crate::families::leak;

/// Family identifier.
pub const FAMILY: &str = "c1.many-tiny";

/// Twenty rows.
pub fn cases() -> Vec<Case> {
    let per_file = [
        (TinyOp::Create, "create"),
        (TinyOp::Stat, "stat"),
        (TinyOp::Unlink, "unlink"),
    ];
    let bulk = [(TinyOp::BulkCreate, "create"), (TinyOp::BulkDelete, "delete")];
    let mut out = Vec::with_capacity(20);
    for (op, name) in per_file {
        for (index, (entries, label)) in ENTRY_LADDER.iter().enumerate() {
            let profile = profile_for_tier(index, "mixed-v4", "compact-v2");
            out.push(
                CaseSpec::new(
                    leak(format!("tiny-{name}-{label}-{profile}")),
                    FAMILY,
                    Shape::ManyTiny(op),
                )
                .entry_tier(index, *entries, label)
                .profile(profile)
                .cache(CacheState::WarmInProcessFixture)
                .smoke_if(index == 0 && name == "create")
                .build(),
            );
        }
    }
    for (op, name) in bulk {
        for (index, (entries, label)) in ENTRY_LADDER.iter().enumerate() {
            out.push(
                CaseSpec::new(
                    leak(format!("tiny-bulk-{name}-{label}-mixed-v3")),
                    FAMILY,
                    Shape::ManyTiny(op),
                )
                .entry_tier(index, *entries, label)
                .profile("mixed-v3")
                .cache(CacheState::WarmInProcessFixture)
                .build(),
            );
        }
    }
    out
}

/// The retained `local_snapshot` fixture constant.
pub const FILE_COUNT: u32 = 25_000;
