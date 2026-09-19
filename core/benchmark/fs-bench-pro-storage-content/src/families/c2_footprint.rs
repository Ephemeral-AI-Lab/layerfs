//! C2-5 `c2.footprint`: allocated versus apparent bytes.
//!
//! The reflink rung is **forbidden** here: a COW clone's `st_blocks` double-counts
//! blocks shared with the master, so `store_allocated_bytes` would be a fabricated
//! number. Every row therefore carries the byte-copy rung and
//! `allocation_attribution: exclusive`. The footprint SQL must not use `COALESCE`:
//! a missing `object_packs` table is `INCOMPLETE`, never a zero that passes
//! `pack_bodies <= database`.

use super::{leak, CaseSpec};
use crate::registry::{CacheState, Case, FootprintOp, Preparation, Shape, StoreState};

/// Family identifier.
pub const FAMILY: &str = "c2.footprint";

/// Six rows: three controls at 100k files / 500 MB, three low controls.
pub fn cases() -> Vec<Case> {
    let controls = [
        (FootprintOp::Unique, "store-footprint-unique-100000", 100_000u32, 524_288_000u64),
        (
            FootprintOp::MetadataCardinality,
            "store-footprint-metadata-cardinality-100000",
            100_000,
            524_288_000,
        ),
        (FootprintOp::LargeObject, "store-footprint-large-object-500m", 1, 524_288_000),
        (FootprintOp::Unique, "store-footprint-unique-100-low-v1", 100, 5_242_880),
        (
            FootprintOp::MetadataCardinality,
            "store-footprint-metadata-cardinality-100-low-v1",
            100,
            5_242_880,
        ),
        (
            FootprintOp::LargeObject,
            "store-footprint-large-object-10m-low-v1",
            1,
            10_485_760,
        ),
    ];
    controls
        .iter()
        .map(|(op, id, entries, bytes)| {
            let _ = leak(String::new());
            CaseSpec::new(id, FAMILY, Shape::Footprint(*op))
                .entry_tier(0, *entries, "")
                .byte_tier(0, *bytes, "")
                .profile(if id.ends_with("low-v1") { "low-v1" } else { "" })
                .cache(CacheState::PreparedDewarmed)
                .store(StoreState::OpenedFromCopy)
                .prepared(Preparation::ObjectSet)
                .smoke_if(id.ends_with("unique-100-low-v1"))
                .build()
        })
        .collect()
}
