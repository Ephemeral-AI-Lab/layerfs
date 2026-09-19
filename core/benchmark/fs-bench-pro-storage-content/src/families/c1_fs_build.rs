//! C1-11 `c1.fs.build-scale`: filesystem build only, no traversal, no save.
//!
//! Eight rows: four build tiers, each with and without the `-text-v1` fixture
//! variant. The declared ladder is `100 f / 5 MB / 1 dir`, `1,000 / 20 MB / 10`,
//! `10,000 / 300 MB / 100`, `100,000 / 500 MB / 1,000`. The per-file size is the
//! declared total divided by the declared file count, so the row's `bytes` field
//! is that quotient and the total stays the declared one.

use super::{leak, CaseSpec};
use crate::registry::{CacheState, Case, Preparation, Shape};

/// Family identifier.
pub const FAMILY: &str = "c1.fs.build-scale";

/// Declared build ladder: (files, total bytes, directories).
pub const LADDER: [(u32, u64, u32); 4] = [
    (100, 5_242_880, 1),
    (1_000, 20_971_520, 10),
    (10_000, 314_572_800, 100),
    (100_000, 524_288_000, 1_000),
];

/// Eight rows.
pub fn cases() -> Vec<Case> {
    let mut out = Vec::with_capacity(8);
    for (index, (files, total, _dirs)) in LADDER.iter().enumerate() {
        for text in [false, true] {
            let label = if text {
                format!("{files}-text-v1")
            } else {
                format!("{files}")
            };
            let name = if *files == 100 || *files == 1_000 {
                format!("namespace-{label}-compact-v3")
            } else {
                format!("namespace-{label}")
            };
            out.push(
                CaseSpec::new(leak(name), FAMILY, Shape::FsBuild { text })
                    .entry_tier(index, *files, if text { "text" } else { "binary" })
                    .profile(if text { "text-v1" } else { "" })
                    .cache(CacheState::WarmInProcessFixture)
                    .prepared(Preparation::InputTree)
                    .smoke_if(index == 0 && !text)
                    .build(),
            );
            let _ = total;
        }
    }
    out
}
