//! `store.upgrade` (#804): brings an older-format Store in place, in one
//! transaction, to the latest format (skeleton; W2 implements).
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// The worker op that upgrades a sqlite Store on that machine:
/// `{ store, store_record, op }`; its phases go out as events.
pub const WORKER_OP: &str = "store.upgrade";
/// The worker op that cancels a running upgrade there: `{ op }`.
pub const CANCEL_OP: &str = "store.cancel";
/// The worker op that reads a sqlite Store's format there: `{ store }`.
pub const FORMAT_OP: &str = "store.format";
/// The formats this build writes, as `--version --json` shows them.
pub const FORMATS: [(&str, i64); 2] = [
    ("sqlite", layerfs_persistence::upgrade::SQLITE_LATEST),
    (
        "postgresql",
        layerfs_persistence::upgrade::POSTGRESQL_LATEST,
    ),
];

/// `store.upgrade(store)`: answers `{ outcome, store, engine, format_before,
/// format_after, steps, warnings }`; `cancel` is read until `commit`.
pub fn upgrade(home: &Path, events: &mut Events<'_>, store: &str, cancel: &AtomicBool) -> Outcome {
    let _ = (home, events, store, cancel);
    Err(refused("Failed", "store upgrade is not built yet"))
}

/// The worker half of [`upgrade`], under the worker's `root`.
pub fn in_worker(
    root: &Path,
    args: &SdkValue,
    events: &mut dyn FnMut(SdkValue) -> Result<(), layerfs_bridge::control::SdkRefusal>,
) -> Outcome {
    let _ = (root, args, events);
    Err(refused("Failed", "store upgrade is not built yet"))
}

/// The worker's `store.cancel { op }`: sets the running upgrade's flag.
pub fn cancel_in_worker(args: &SdkValue) -> Outcome {
    let _ = args;
    Err(refused("Failed", "store upgrade is not built yet"))
}

/// The worker's `store.format { store }`: the file's format section.
pub fn format_in_worker(root: &Path, args: &SdkValue) -> Outcome {
    let _ = (root, args);
    Err(refused("Failed", "store upgrade is not built yet"))
}
