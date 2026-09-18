//! C1-1 `c1.construct.whole-file` and C1-2 `c1.construct.chunked`.
//!
//! Both families run the frozen 1 / 10 / 100 / 500 MiB payload ladder; they differ
//! in which construction entry point the row drives. `c1-families.md` section 3.1
//! gives one shared ID list for the pair and the cardinality rule makes that four
//! cases per family, but case IDs are globally unique, so the second family's IDs
//! carry a `-chunked` infix. The reading is recorded in the harness README: the
//! distinction is the real product one between `construct_bytes` (the payload is
//! an in-memory slice) and `construct_stream` (the payload arrives through
//! `impl Read`).

use super::{leak, CaseSpec, BYTE_LADDER};
use crate::registry::{CacheState, Case, Route, Shape};

/// Family identifier.
pub const WHOLE_FILE: &str = "c1.construct.whole-file";
/// Family identifier.
pub const CHUNKED: &str = "c1.construct.chunked";

/// C1-1: four cases over the byte ladder through `construct_bytes`.
pub fn whole_file() -> Vec<Case> {
    BYTE_LADDER
        .iter()
        .enumerate()
        .map(|(index, (bytes, label))| {
            let profile = if *bytes < 100 << 20 { "compact-v2" } else { "" };
            let id = if profile.is_empty() {
                leak(format!("payload-create-{label}"))
            } else {
                leak(format!("payload-create-{label}-{profile}"))
            };
            CaseSpec::new(id, WHOLE_FILE, Shape::Construct(Route::Bytes))
                .byte_tier(index, *bytes, label)
                .profile(profile)
                .cache(CacheState::WarmInProcessFixture)
                .smoke_if(index == 0)
                .build()
        })
        .collect()
}

/// C1-2: the same ladder through `construct_stream`.
pub fn chunked() -> Vec<Case> {
    BYTE_LADDER
        .iter()
        .enumerate()
        .map(|(index, (bytes, label))| {
            let profile = if *bytes < 100 << 20 { "compact-v2" } else { "" };
            let id = if profile.is_empty() {
                leak(format!("payload-create-chunked-{label}"))
            } else {
                leak(format!("payload-create-chunked-{label}-{profile}"))
            };
            CaseSpec::new(id, CHUNKED, Shape::Construct(Route::Stream))
                .byte_tier(index, *bytes, label)
                .profile(profile)
                .cache(CacheState::WarmInProcessFixture)
                .smoke_if(index == 0)
                .build()
        })
        .collect()
}
