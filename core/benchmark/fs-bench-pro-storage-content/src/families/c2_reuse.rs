//! C2-2 `c2.reuse.cross-file` and C2-4 `c2.reuse.workspace`.
//!
//! Both measure exact canonical reuse against a stored base. The mechanism gate is
//! the declared equation, not a speed: an `identical` profile must reuse for every
//! member after the first, and a `unique` profile must reuse none. The workspace
//! family adds the two `base128` controls, whose base file count is declared as
//! 128 rather than the tier.

use super::{leak, CaseSpec, ENTRY_LADDER};
use crate::registry::{CacheState, Case, ReuseOp, Shape, StoreState};

/// Family identifier.
pub const CROSS_FILE: &str = "c2.reuse.cross-file";
/// Family identifier.
pub const WORKSPACE: &str = "c2.reuse.workspace";

/// Declared base file count of the `base128-v3` controls.
pub const BASE128_FILES: u32 = 128;

/// C2-2: one anchor plus three profiles over three tiers.
pub fn cross_file() -> Vec<Case> {
    let mut out = Vec::with_capacity(10);
    out.push(
        CaseSpec::new(
            "dedup-cross-file-anchor-1",
            CROSS_FILE,
            Shape::Reuse(ReuseOp::Identical),
        )
        .entry_tier(0, 1, "1")
        .cache(CacheState::PreparedDewarmed)
        .store(StoreState::OpenedFromCopy)
        .smoke()
        .build(),
    );
    let kinds = [
        (ReuseOp::Unique, "unique"),
        (ReuseOp::Identical, "identical"),
        (ReuseOp::Local, "mixed"),
    ];
    for (op, name) in kinds {
        for (index, (entries, label)) in ENTRY_LADDER.iter().enumerate().skip(1) {
            out.push(
                CaseSpec::new(
                    leak(format!("dedup-cross-file-{name}-{label}")),
                    CROSS_FILE,
                    Shape::Reuse(op),
                )
                .entry_tier(index, *entries, label)
                .cache(CacheState::PreparedDewarmed)
                .store(StoreState::OpenedFromCopy)
                .build(),
            );
        }
    }
    out
}

/// C2-4: three kinds over four tiers plus two `base128` controls.
pub fn workspace() -> Vec<Case> {
    let mut out = Vec::with_capacity(14);
    let kinds = [
        (ReuseOp::Identical, "exact"),
        (ReuseOp::Local, "local"),
        (ReuseOp::Unique, "unique"),
    ];
    for (op, name) in kinds {
        for (index, (entries, label)) in ENTRY_LADDER.iter().enumerate() {
            out.push(
                CaseSpec::new(
                    leak(format!("dedup-workspace-{name}-{label}-compact-v2")),
                    WORKSPACE,
                    Shape::Workspace(op),
                )
                .entry_tier(index, *entries, label)
                .profile("compact-v2")
                .cache(CacheState::PreparedDewarmed)
                .store(StoreState::OpenedFromCopy)
                .smoke_if(index == 0 && name == "exact")
                .build(),
            );
        }
    }
    for (index, (entries, label)) in ENTRY_LADDER.iter().enumerate().take(2) {
        out.push(
            CaseSpec::new(
                leak(format!("dedup-workspace-unique-{label}-base128-v3")),
                WORKSPACE,
                Shape::Workspace(ReuseOp::Base128),
            )
            .entry_tier(index, *entries, label)
            .profile("base128-v3")
            .cache(CacheState::PreparedDewarmed)
            .store(StoreState::OpenedFromCopy)
            .build(),
        );
    }
    out
}
