//! C2-1 `c2.lifecycle`: the Store lifecycle as its own case.
//!
//! `Store::create` is fixed-cost and large — Phase 0's diagnostic single sample
//! put `store.create` at 4,606 µs of a 6,952 µs small save — so creation must be
//! its own case or small saves measure SQLite's DDL. `lifecycle-create` is the
//! only row in the registry with `store_state: created-in-sample`; every other C2
//! row opens a per-sample copy.

use super::{leak, CaseSpec};
use crate::registry::{CacheState, Case, LifecycleStep, Shape, StoreState};

/// Family identifier.
pub const FAMILY: &str = "c2.lifecycle";

/// Five fixed rows.
pub fn cases() -> Vec<Case> {
    [
        (LifecycleStep::Create, "create", StoreState::CreatedInSample),
        (LifecycleStep::Open, "open", StoreState::OpenedFromCopy),
        (LifecycleStep::BeginSave, "begin-save", StoreState::OpenedFromCopy),
        (
            LifecycleStep::FinishEmpty,
            "finish-empty",
            StoreState::OpenedFromCopy,
        ),
        (LifecycleStep::Abort, "abort", StoreState::OpenedFromCopy),
    ]
    .iter()
    .map(|(step, name, store)| {
        let cache = if *step == LifecycleStep::Create {
            CacheState::CreatedInSample
        } else {
            CacheState::PreparedDewarmed
        };
        CaseSpec::new(
            leak(format!("lifecycle-{name}")),
            FAMILY,
            Shape::Lifecycle(*step),
        )
        .cache(cache)
        .store(*store)
        .smoke_if(*step == LifecycleStep::Create)
        .build()
    })
    .collect()
}
